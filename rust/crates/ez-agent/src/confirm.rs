//! Port of `src/ezagent/agent/confirm.py` (F3-04).
//!
//! Confirmaciones por voz o clic para operaciones de nivel 2. El hilo de
//! herramientas llama a `ask` y queda bloqueado hasta que la interfaz entrega
//! una respuesta o vence el plazo. El hilo de UI nunca se bloquea: usa
//! `submit_text`, `submit_click` o `cancel`.
//!
//! El plazo no empieza al plantear la pregunta, sino cuando la interfaz llama a
//! `arm` tras terminar de leerla por TTS. Cada confirmación tiene un token: una
//! respuesta con otro token (una transcripción atrasada) no la resuelve.
//!
//! DECISION (del Python): una respuesta solo resuelve si contiene alguna
//! palabra completa de `accept` o de `reject`. Una frase ajena o vacía devuelve
//! `None` y deja la confirmación pendiente. Si aparecen palabras de ambas
//! listas gana el rechazo.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use ez_core::pyvalue::{fold_accents, python_int};
use regex::Regex;

pub const CONFIRM_WORD: &str = "confirmo";

/// Si la interfaz nunca arma la pregunta (TTS caído), el plazo cuenta igual
/// después de esta gracia para no bloquear el hilo de herramientas para siempre.
const ARM_GRACE_S: f64 = 30.0;

/// `yes` | `no` | `timeout` | `cancelled` | `unavailable`.
pub type Outcome = &'static str;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Palabras completas (letras y dígitos), sin puntuación ni tildes.
pub fn words(text: &str) -> Vec<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| Regex::new(r"[^\W_]+").expect("patrón válido"));
    pattern
        .find_iter(&fold_accents(text))
        .map(|found| found.as_str().to_string())
        .collect()
}

fn phrases(values: &[String]) -> Vec<Vec<String>> {
    values
        .iter()
        .map(|value| words(value))
        .filter(|phrase| !phrase.is_empty())
        .collect()
}

fn contains_phrase(tokens: &[String], phrase: &[String]) -> bool {
    match phrase.len() {
        0 => false,
        1 => tokens.contains(&phrase[0]),
        size => tokens.windows(size).any(|window| window == phrase),
    }
}

const UNITS: [(&str, i64); 31] = [
    ("cero", 0),
    ("uno", 1),
    ("un", 1),
    ("una", 1),
    ("dos", 2),
    ("tres", 3),
    ("cuatro", 4),
    ("cinco", 5),
    ("seis", 6),
    ("siete", 7),
    ("ocho", 8),
    ("nueve", 9),
    ("diez", 10),
    ("once", 11),
    ("doce", 12),
    ("trece", 13),
    ("catorce", 14),
    ("quince", 15),
    ("dieciseis", 16),
    ("diecisiete", 17),
    ("dieciocho", 18),
    ("diecinueve", 19),
    ("veinte", 20),
    ("veintiuno", 21),
    ("veintidos", 22),
    ("veintitres", 23),
    ("veinticuatro", 24),
    ("veinticinco", 25),
    ("veintiseis", 26),
    ("veintisiete", 27),
    ("veintiocho", 28),
];
const MORE_UNITS: [(&str, i64); 1] = [("veintinueve", 29)];
const TENS: [(&str, i64); 7] = [
    ("treinta", 30),
    ("cuarenta", 40),
    ("cincuenta", 50),
    ("sesenta", 60),
    ("setenta", 70),
    ("ochenta", 80),
    ("noventa", 90),
];
const HUNDREDS: [(&str, i64); 10] = [
    ("cien", 100),
    ("ciento", 100),
    ("doscientos", 200),
    ("trescientos", 300),
    ("cuatrocientos", 400),
    ("quinientos", 500),
    ("seiscientos", 600),
    ("setecientos", 700),
    ("ochocientos", 800),
    ("novecientos", 900),
];

fn lookup(word: &str) -> Option<i64> {
    UNITS
        .iter()
        .chain(MORE_UNITS.iter())
        .chain(TENS.iter())
        .chain(HUNDREDS.iter())
        .find(|(name, _)| *name == word)
        .map(|(_, value)| *value)
}

/// Interpreta «doce», «treinta y dos» o «mil doscientos».
pub fn number_from_words(words: &[String]) -> Option<i64> {
    let (mut total, mut current, mut found) = (0_i64, 0_i64, false);
    for word in words {
        if let Some(value) = lookup(word) {
            current += value;
            found = true;
        } else if word == "mil" {
            total += (if current == 0 { 1 } else { current }) * 1000;
            current = 0;
            found = true;
        }
    }
    found.then_some(total + current)
}

#[derive(Default)]
struct PendingState {
    armed: bool,
    result: bool,
    resolved: bool,
    outcome: Option<Outcome>,
}

struct Pending {
    strong: bool,
    require_number: Option<i64>,
    token: String,
    state: Mutex<PendingState>,
    condition: Condvar,
}

#[derive(Default)]
struct ManagerState {
    pending: Option<Arc<Pending>>,
    last_outcome: Option<Outcome>,
}

type OnAsk = Box<dyn Fn(&str, bool) + Send + Sync>;

/// Coordina una confirmación pendiente entre el hilo de herramientas y la UI.
pub struct ConfirmationManager {
    accept: Vec<Vec<String>>,
    reject: Vec<Vec<String>>,
    timeout_s: f64,
    arm_grace_s: f64,
    on_ask: Option<OnAsk>,
    state: Mutex<ManagerState>,
}

impl ConfirmationManager {
    pub fn new(
        accept: &[String],
        reject: &[String],
        timeout_s: f64,
        on_ask: Option<OnAsk>,
    ) -> Self {
        Self {
            accept: phrases(accept),
            reject: phrases(reject),
            timeout_s: timeout_s.max(0.0),
            arm_grace_s: ARM_GRACE_S,
            on_ask,
            state: Mutex::new(ManagerState::default()),
        }
    }

    /// Cambia la gracia de armado (pruebas y perfiles sin TTS).
    pub fn with_arm_grace(mut self, seconds: f64) -> Self {
        self.arm_grace_s = seconds.max(0.0);
        self
    }

    fn live(state: &ManagerState) -> Option<Arc<Pending>> {
        let pending = state.pending.clone()?;
        let resolved = lock(&pending.state).resolved;
        (!resolved).then_some(pending)
    }

    pub fn pending(&self) -> bool {
        Self::live(&lock(&self.state)).is_some()
    }

    /// Token de la confirmación vigente, o `None` si no hay ninguna.
    pub fn pending_token(&self) -> Option<String> {
        Self::live(&lock(&self.state)).map(|pending| pending.token.clone())
    }

    /// Último resultado; `unavailable` si ya había una pendiente.
    pub fn last_outcome(&self) -> Option<Outcome> {
        lock(&self.state).last_outcome
    }

    /// Empieza a contar el plazo cuando la pregunta ya se terminó de leer.
    ///
    /// Con `token`, un armado atrasado de otra pregunta no arma esta.
    pub fn arm(&self, token: Option<&str>) {
        let state = lock(&self.state);
        let Some(request) = Self::live(&state) else {
            return;
        };
        if token.is_some_and(|token| token != request.token) {
            return;
        }
        lock(&request.state).armed = true;
        request.condition.notify_all();
    }

    /// Muestra el resumen y espera la respuesta. Devuelve `true` si acepta.
    ///
    /// Bloquea solo al hilo que ejecuta herramientas. Espera a `arm` antes de
    /// contar el plazo; un timeout equivale a rechazo.
    pub fn ask(&self, summary: &str, strong: bool, require_number: Option<i64>) -> bool {
        let request = Arc::new(Pending {
            strong,
            require_number,
            token: uuid_hex(),
            state: Mutex::new(PendingState::default()),
            condition: Condvar::new(),
        });
        {
            let mut state = lock(&self.state);
            // DECISION (del Python): no se encolan varias confirmaciones.
            if Self::live(&state).is_some() {
                state.last_outcome = Some("unavailable");
                return false;
            }
            state.pending = Some(Arc::clone(&request));
        }
        if let Some(on_ask) = &self.on_ask {
            // DECISION (del Python): un fallo de la UI no debe tumbar la herramienta.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                on_ask(summary, strong);
            }));
        }
        let (accepted, outcome) = {
            let mut guard = lock(&request.state);
            if self.on_ask.is_none() {
                // DECISION (del Python): sin interfaz no hay TTS que esperar.
                guard.armed = true;
            }
            guard = self.wait_armed(&request, guard);
            if !guard.resolved {
                guard = self.wait_answer(&request, guard);
            }
            (guard.result, guard.outcome)
        };
        let mut state = lock(&self.state);
        // Si otra confirmación ya ocupó el lugar, no pises su resultado.
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, &request))
        {
            if outcome.is_some() {
                state.last_outcome = outcome;
            }
            state.pending = None;
        }
        accepted
    }

    /// Resuelve la confirmación pendiente con una respuesta de voz.
    ///
    /// `Some(bool)` si la respuesta es clara; `None` si no había una pendiente,
    /// el token no coincide o el texto no es ni aceptación ni rechazo.
    pub fn submit_text(&self, text: &str, token: Option<&str>) -> Option<bool> {
        let tokens = words(text);
        let mut state = lock(&self.state);
        let request = Self::live(&state)?;
        if token.is_some_and(|token| token != request.token) {
            return None;
        }
        let outcome = self.intent(&request, &tokens)?;
        let accepted = outcome == "yes";
        Self::resolve(&mut state, &request, accepted, outcome);
        state.pending = None;
        Some(accepted)
    }

    /// Resuelve la confirmación pendiente con un clic en la interfaz.
    pub fn submit_click(&self, yes: bool) {
        let mut state = lock(&self.state);
        let Some(request) = Self::live(&state) else {
            return;
        };
        Self::resolve(&mut state, &request, yes, if yes { "yes" } else { "no" });
        state.pending = None;
    }

    /// Cierra la confirmación pendiente como rechazo.
    pub fn cancel(&self, _reason: &str) {
        let mut state = lock(&self.state);
        let Some(request) = Self::live(&state) else {
            return;
        };
        Self::resolve(&mut state, &request, false, "cancelled");
        state.pending = None;
    }

    /// Cierra `request` y despierta a `ask`. Se llama con el cerrojo del gestor tomado.
    fn resolve(state: &mut ManagerState, request: &Pending, accepted: bool, outcome: Outcome) {
        let mut guard = lock(&request.state);
        guard.resolved = true;
        guard.result = accepted;
        guard.outcome = Some(outcome);
        state.last_outcome = Some(outcome);
        request.condition.notify_all();
    }

    fn wait_armed<'a>(
        &self,
        request: &'a Pending,
        mut guard: MutexGuard<'a, PendingState>,
    ) -> MutexGuard<'a, PendingState> {
        let deadline = Instant::now() + Duration::from_secs_f64(self.arm_grace_s);
        while !guard.armed && !guard.resolved {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                // DECISION (del Python): sin armado, el plazo cuenta desde que vence la gracia.
                guard.armed = true;
                break;
            };
            if remaining.is_zero() {
                guard.armed = true;
                break;
            }
            guard = request
                .condition
                .wait_timeout(guard, remaining)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        guard
    }

    fn wait_answer<'a>(
        &self,
        request: &'a Pending,
        mut guard: MutexGuard<'a, PendingState>,
    ) -> MutexGuard<'a, PendingState> {
        let deadline = Instant::now() + Duration::from_secs_f64(self.timeout_s);
        while !guard.resolved {
            let remaining = deadline.checked_duration_since(Instant::now());
            match remaining.filter(|remaining| !remaining.is_zero()) {
                None => {
                    guard.resolved = true;
                    guard.result = false;
                    guard.outcome = Some("timeout");
                    break;
                }
                Some(remaining) => {
                    guard = request
                        .condition
                        .wait_timeout(guard, remaining)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        }
        guard
    }

    /// `yes`/`no` para una respuesta clara, o `None` si es ajena.
    fn intent(&self, request: &Pending, tokens: &[String]) -> Option<Outcome> {
        let folded = tokens.join(" ");
        let unsure = [
            "no se",
            "no estoy seguro",
            "no sabria",
            "no sabría",
            "quizas",
        ];
        if unsure.iter().any(|phrase| folded.contains(phrase)) {
            return None;
        }
        if self
            .reject
            .iter()
            .any(|phrase| contains_phrase(tokens, phrase))
        {
            return Some("no");
        }
        if !self
            .accept
            .iter()
            .any(|phrase| contains_phrase(tokens, phrase))
        {
            return None;
        }
        if request.strong {
            if !tokens.iter().any(|word| word == CONFIRM_WORD) {
                // Una frase sin «confirmo» no es un rechazo: es ambigua y se
                // repregunta, igual que el ruido (F48).
                return None;
            }
            if let Some(required) = request.require_number {
                let mut numbers: Vec<i64> = tokens.iter().filter_map(|w| python_int(w)).collect();
                if let Some(spoken) = number_from_words(tokens) {
                    numbers.push(spoken);
                }
                if numbers.is_empty() {
                    return None; // «confirmo» sin número: aún no es respuesta
                }
                if !numbers.contains(&required) {
                    return Some("no");
                }
            }
        }
        Some("yes")
    }
}

fn uuid_hex() -> String {
    // `uuid4().hex`: 32 dígitos hexadecimales sin guiones.
    format!("{:032x}", uuid::Uuid::new_v4().as_u128())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> ConfirmationManager {
        let accept = vec!["sí".to_string(), "confirmo".to_string(), "dale".to_string()];
        let reject = vec!["no".to_string(), "cancela".to_string()];
        ConfirmationManager::new(&accept, &reject, 0.2, None)
    }

    #[test]
    fn words_and_numbers_fold_accents() {
        assert_eq!(words("¡Sí, CONFIRMO!"), vec!["si", "confirmo"]);
        let spoken = |text: &str| number_from_words(&words(text));
        assert_eq!(spoken("treinta y dos"), Some(32));
        assert_eq!(spoken("mil doscientos"), Some(1200));
        assert_eq!(spoken("mil"), Some(1000));
        assert_eq!(spoken("hola"), None);
    }

    #[test]
    fn times_out_as_rejection() {
        let manager = manager();
        assert!(!manager.ask("¿Borro?", false, None));
        assert_eq!(manager.last_outcome(), Some("timeout"));
        assert!(!manager.pending());
    }

    #[test]
    fn answers_from_another_thread() {
        let manager = Arc::new(manager());
        let worker = Arc::clone(&manager);
        let handle = std::thread::spawn(move || worker.ask("¿Borro?", false, None));
        while manager.pending_token().is_none() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let token = manager.pending_token().unwrap_or_default();
        assert_eq!(manager.submit_text("quizás", Some(&token)), None);
        assert_eq!(manager.submit_text("sí", Some("otro")), None);
        assert_eq!(manager.submit_text("sí, dale", Some(&token)), Some(true));
        assert!(handle.join().unwrap_or(false));
        assert_eq!(manager.last_outcome(), Some("yes"));
    }

    #[test]
    fn strong_confirmation_needs_the_word_and_number() {
        let manager = Arc::new(manager());
        let worker = Arc::clone(&manager);
        let handle = std::thread::spawn(move || worker.ask("¿Borro 12?", true, Some(12)));
        while !manager.pending() {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(manager.submit_text("sí", None), None);
        assert_eq!(manager.submit_text("confirmo", None), None);
        assert_eq!(manager.submit_text("confirmo trece", None), Some(false));
        assert!(!handle.join().unwrap_or(true));
        let second = Arc::new(self::tests::manager());
        let worker = Arc::clone(&second);
        let handle = std::thread::spawn(move || worker.ask("¿Borro 12?", true, Some(12)));
        while !second.pending() {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(second.submit_text("confirmo doce", None), Some(true));
        assert!(handle.join().unwrap_or(false));
    }

    #[test]
    fn click_and_cancel_resolve() {
        let manager = Arc::new(manager());
        let worker = Arc::clone(&manager);
        let handle = std::thread::spawn(move || worker.ask("¿?", false, None));
        while !manager.pending() {
            std::thread::sleep(Duration::from_millis(5));
        }
        manager.cancel("");
        assert!(!handle.join().unwrap_or(true));
        assert_eq!(manager.last_outcome(), Some("cancelled"));
    }
}
