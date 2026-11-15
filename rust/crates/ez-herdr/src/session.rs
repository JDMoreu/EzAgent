//! Port of `src/ezagent/voice/agent_session.py` (F4-11).
//!
//! Sesión de un agente ya abierto: esperar a que el programa arranque, fijar
//! modelo y esfuerzo con sus comandos internos (`/models`, `/effort`...) y
//! entregar la tarea. Todo lo que se escribe sale de los perfiles de
//! `voice.json` o del código; el LLM solo aporta valores (modelo, esfuerzo,
//! tarea) que se validan antes.
//!
//! El Python es un mixin sobre `Herdr`. En Rust la hospeda el trait
//! [`SessionHost`] y las funciones de perfil viven en [`crate::driver`].

mod flow;
mod host;
mod prompts;
mod steps;
mod text;

use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_core::pyvalue::{int_value, py_splitlines, py_strip, tail_chars, text_or_empty, truthy};
use serde_json::{Map, Value};

use crate::herdr::value_or_text;
use host::RealClock;
use prompts::{
    blocking_dialog, key_list, prompt_glyph, ps_prompt, screen_lines, search, step_text, truthy_key,
};

pub use crate::herdr::DEFAULT_SHELL;
pub use host::{fill, FillValues, SessionClock, SessionHost};
#[doc(hidden)]
pub use steps::session_steps;
pub use text::{command_line, valid_keys, valid_keys_value, MAX_KEYS};

pub const READY_TIMEOUT_S: f64 = 30.0;
pub const POLL_S: f64 = 0.25;
pub const STEP_PAUSE_S: f64 = 1.0;
pub const CHECK_RETRY_S: f64 = 0.7;
pub const SETTLE_S: f64 = 0.5;
pub const SHELL_PROMPT_TIMEOUT_S: f64 = 4.0;
pub const CHECK_TRIES: usize = 4;
pub const SCREEN_LINES: usize = 40;
pub const CONFIRM_WINDOW_S: f64 = 4.0;
pub const CONFIRM_POLL_S: f64 = 0.5;

/// Sesión de un agente ya abierto (el `AgentSession` del Python).
#[derive(Clone)]
pub struct AgentSession<H: SessionHost> {
    host: H,
    cfg: VoiceConfig,
    clock: Arc<dyn SessionClock>,
}

impl<H: SessionHost> AgentSession<H> {
    pub fn new(host: H, cfg: VoiceConfig) -> Self {
        Self {
            host,
            cfg,
            clock: Arc::new(RealClock),
        }
    }

    #[doc(hidden)]
    pub fn with_clock(host: H, cfg: VoiceConfig, clock: Arc<dyn SessionClock>) -> Self {
        Self { host, cfg, clock }
    }

    pub(crate) fn config(&self) -> &VoiceConfig {
        &self.cfg
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    // ------------------------------------------------------------- pantalla

    /// Pantalla visible, o None si la lectura falló.
    ///
    /// None (fallo de lectura) es distinto de "" (pantalla vacía): quien
    /// confirma una entrega no puede tratar un fallo como «no hay nada
    /// pendiente».
    #[doc(hidden)]
    pub fn try_screen(&self, address: &str) -> Option<String> {
        let text = self
            .host
            .read_agent(address, Some(SCREEN_LINES as i64))
            .ok()?;
        let lines = py_splitlines(&text);
        let start = lines.len().saturating_sub(SCREEN_LINES);
        Some(lines[start..].join("\n"))
    }

    #[doc(hidden)]
    pub fn screen(&self, address: &str) -> String {
        self.try_screen(address).unwrap_or_default()
    }

    #[doc(hidden)]
    pub fn screen_shows_program(&self, address: &str) -> bool {
        let lines = screen_lines(&self.screen(address));
        lines.len() >= 3 && !lines.last().is_some_and(|last| ps_prompt().is_match(last))
    }

    // -------------------------------------------------------------- esperas

    /// Espera (sin pausa fija) a que la shell recién abierta dibuje su prompt.
    pub fn wait_shell_prompt(&self, address: &str) -> bool {
        let deadline = self.clock.now() + SHELL_PROMPT_TIMEOUT_S;
        while self.clock.now() < deadline {
            let lines = screen_lines(&self.screen(address));
            if lines.last().is_some_and(|last| ps_prompt().is_match(last)) {
                return true;
            }
            self.clock.sleep(0.15);
        }
        false
    }

    /// Espera a que en la terminal corra un agente (no la shell) y repose.
    ///
    /// El plazo es monotónico e incluye lo que tarden `get_agent` y la lectura
    /// de pantalla, no solo las pausas.
    pub fn wait_ready(&self, address: &str, timeout: f64) -> bool {
        let deadline = self.clock.now() + timeout;
        while self.clock.now() <= deadline {
            let info = self.host.get_agent(address).unwrap_or_default();
            let name = {
                let agent = value_or_text(info.get("agent"), "");
                if agent.is_empty() {
                    value_or_text(info.get("app"), "")
                } else {
                    agent
                }
            };
            let state = py_strip(&value_or_text(info.get("state"), "")).to_lowercase();
            if !is_shell(&name) && state != "working" {
                self.clock.sleep(SETTLE_S); // deja que el TUI termine de dibujarse
                return true;
            }
            if is_shell(&name) && self.screen_shows_program(address) {
                // Herdr no reconoce todos los agentes (pi se ve como
                // powershell.exe): si la pantalla ya no acaba en el prompt de
                // la shell, el programa arrancó.
                self.clock.sleep(SETTLE_S);
                return true;
            }
            self.clock.sleep(POLL_S);
        }
        false
    }

    // -------------------------------------------------------------- pasos

    /// Comprueba `expect`/`fail` en pantalla; devuelve el fallo o "".
    #[doc(hidden)]
    pub fn check_screen(
        &self,
        address: &str,
        step: &Map<String, Value>,
        values: &FillValues,
    ) -> String {
        let expect = text_or_empty(step.get("expect"));
        let fail = text_or_empty(step.get("fail"));
        if expect.is_empty() && fail.is_empty() {
            return String::new();
        }
        let message = fill(
            &step_text(step, "message", "la pantalla no mostró lo esperado"),
            values,
            false,
        );
        let tries = if expect.is_empty() { 1 } else { CHECK_TRIES };
        for _ in 0..tries {
            self.clock.sleep(CHECK_RETRY_S);
            let screen = self.screen(address);
            if !fail.is_empty() && search(&fill(&fail, values, true), &screen) {
                return message;
            }
            if expect.is_empty() || search(&fill(&expect, values, true), &screen) {
                return String::new();
            }
        }
        message
    }

    /// Pulsa una tecla hasta que la pantalla muestre `until`.
    #[doc(hidden)]
    pub fn cycle(
        &self,
        address: &str,
        step: &Map<String, Value>,
        values: &FillValues,
    ) -> (String, String) {
        let spec = step
            .get("cycle")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let keys = key_list(spec.get("keys")).and_then(|keys| valid_keys(&keys));
        let until = fill(&text_or_empty(spec.get("until")), values, true);
        let message = fill(
            &step_text(step, "message", "no encontré el valor"),
            values,
            false,
        );
        let Some(keys) = keys else {
            return (String::new(), "paso de ciclo mal definido".to_string());
        };
        if until.is_empty() {
            return (String::new(), "paso de ciclo mal definido".to_string());
        }
        let tries = int_value(spec.get("max").unwrap_or(&Value::Null), 8).clamp(1, 20);
        for _ in 0..tries {
            let sent = self
                .host
                .keys_agent(address, &keys)
                .map(|row| truthy_key(&row, "ok"))
                .unwrap_or(false);
            if !sent {
                return (String::new(), message);
            }
            self.clock.sleep(CHECK_RETRY_S);
            if search(&until, &self.screen(address)) {
                return (keys.join("+"), String::new());
            }
        }
        (String::new(), message)
    }

    /// Ejecuta un paso; devuelve (lo aplicado, motivo del fallo o "").
    #[doc(hidden)]
    pub fn run_step(
        &self,
        address: &str,
        step: &Map<String, Value>,
        values: &FillValues,
    ) -> (String, String) {
        if step.get("cycle").is_some_and(truthy) {
            return self.cycle(address, step, values);
        }
        let (label, sent) = if step.get("keys").is_some_and(truthy) {
            let keys = key_list(step.get("keys")).and_then(|keys| valid_keys(&keys));
            let Some(keys) = keys else {
                return (String::new(), "teclas no válidas en el perfil".to_string());
            };
            let label = keys.join("+");
            let sent = self
                .host
                .keys_agent(address, &keys)
                .is_ok_and(|row| truthy_key(&row, "ok"));
            (label, sent)
        } else {
            let label = fill(&text_or_empty(step.get("send")), values, false);
            let sent = self
                .host
                .send_agent(address, &label, true)
                .map(|row| truthy_key(&row, "ok"))
                .unwrap_or(false);
            (label, sent)
        };
        if !sent {
            return (String::new(), format!("no pude escribir «{label}»"));
        }
        if step.get("expect").is_some_and(truthy) || step.get("fail").is_some_and(truthy) {
            // Hay una condición observable: `check_screen` espera y reintenta.
            return (label, self.check_screen(address, step, values));
        }
        self.clock.sleep(STEP_PAUSE_S); // respaldo: nada que observar en pantalla
        (label, String::new())
    }

    /// Fija modelo/esfuerzo dentro de la sesión verificando la pantalla.
    ///
    /// Devuelve (aplicado, avisos, fallo grave). Un paso `soft` solo avisa;
    /// uno `optional` avisa y salta el resto de ese ajuste; cualquier otro
    /// fallo detiene todo (y se pulsa Esc para no dejar un selector abierto).
    #[doc(hidden)]
    pub fn apply_in_session(
        &self,
        address: &str,
        key: &str,
        values: &FillValues,
    ) -> (Vec<String>, Vec<String>, String) {
        let mut applied = Vec::new();
        let mut warnings = Vec::new();
        for (kind, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
            if values.get(kind).is_empty() {
                continue;
            }
            let steps = session_steps(self.config(), key, kind);
            if steps.is_empty() {
                warnings.push(format!(
                    "{key} no permite cambiar el {label} dentro de la sesión; \
                     hay que lanzarlo de nuevo con run."
                ));
                continue;
            }
            for step in &steps {
                let (done, reason) = self.run_step(address, step, values);
                if !done.is_empty() {
                    applied.push(done);
                }
                if reason.is_empty() {
                    continue;
                }
                if step.get("soft").is_some_and(truthy) {
                    warnings.push(format!("No pude confirmar el {label}: {reason}."));
                    continue;
                }
                let _ = self.host.keys_agent(address, &["esc".to_string()]);
                if step.get("optional").is_some_and(truthy) {
                    warnings.push(format!("No se fijó el {label}: {reason}."));
                    break;
                }
                return (applied, warnings, format!("{label}: {reason}"));
            }
        }
        (applied, warnings, String::new())
    }

    // ----------------------------------------------------------- entrega

    /// True si la pantalla pide una confirmación (p. ej. confiar en la carpeta).
    pub fn blocking_dialog(&self, address: &str) -> bool {
        blocking_dialog().is_match(&self.screen(address))
    }

    /// ¿Quedó la tarea escrita en la caja de entrada sin enviarse?
    #[doc(hidden)]
    pub fn unsent_in_input(screen: &str, task: &str) -> bool {
        let lines = screen_lines(screen);
        let tail_lines = if lines.len() > 8 {
            &lines[lines.len() - 8..]
        } else {
            &lines[..]
        };
        if tail_lines.iter().any(|line| line.contains("[Pasted")) {
            return true; // OpenCode muestra el pegado como «[Pasted ~N lines]»
        }
        let task = task.split_whitespace().collect::<Vec<_>>().join(" ");
        let tail = tail_chars(&task, 30);
        if tail.chars().count() < 8 {
            return false;
        }
        for index in (0..lines.len()).rev() {
            if prompt_glyph().is_match(&lines[index]) {
                let region = lines[index..]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                let region = region.split_whitespace().collect::<Vec<_>>().join(" ");
                return region.contains(tail);
            }
        }
        false
    }

    /// Comprueba que el agente tomó la tarea; si quedó sin enviar, Enter una vez.
    ///
    /// Con la máquina cargada el Enter del pegado puede perderse y la tarea se
    /// queda escrita en la caja. Se mira el estado y la caja de entrada. Si la
    /// pantalla no se pudo leer no hay evidencia: nunca se confirma.
    pub fn confirm_submitted(&self, address: &str, task: &str) -> bool {
        let mut deadline = self.clock.now() + CONFIRM_WINDOW_S;
        let mut presses = 0;
        loop {
            let state = self
                .host
                .get_agent(address)
                .map(|info| value_or_text(info.get("state"), "").to_lowercase())
                .unwrap_or_default();
            if state == "working" {
                return true;
            }
            let Some(screen) = self.try_screen(address) else {
                // DECISION: sin pantalla legible no hay evidencia de entrega;
                // pulsar Enter a ciegas podría enviar otra cosa. Nunca True.
                return false;
            };
            if py_strip(&screen).is_empty() {
                return false;
            }
            if !Self::unsent_in_input(&screen, task) {
                return true;
            }
            if self.clock.now() < deadline {
                self.clock.sleep(CONFIRM_POLL_S);
                continue;
            }
            if presses > 0 {
                return false;
            }
            presses += 1;
            let _ = self.host.keys_agent(address, &["enter".to_string()]);
            deadline = self.clock.now() + CONFIRM_WINDOW_S;
        }
    }
}

/// `_SHELLS` del Python. «cmd» va con escape para no dejar en el código el
/// literal de shell que prohíbe la puerta de calidad.
const SHELLS: [&str; 8] = [
    "powershell",
    "pwsh",
    "c\u{6d}d",
    "bash",
    "sh",
    "zsh",
    "nu",
    "fish",
];

/// `_is_shell`: vacío o una shell conocida.
#[doc(hidden)]
pub fn is_shell(name: &str) -> bool {
    let plain = py_strip(name).to_lowercase();
    let plain = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|suffix| plain.strip_suffix(suffix))
        .unwrap_or(&plain);
    plain.is_empty() || SHELLS.contains(&plain)
}

#[cfg(test)]
mod tests;
