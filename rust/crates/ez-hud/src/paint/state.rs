//! Port of `src/ezagent/voice/hud/state.py` (F5-02).
//!
//! Máquina de estados visual del HUD. `display_state` es pura y fija la
//! prioridad `off > error > speak > think > notice > mute > listen > idle`;
//! [`UiState`] añade el comportamiento temporal: permanencia mínima de 120 ms,
//! gracia de 250 ms en `vad_end`, aviso de 1000 ms con vuelta a `idle` si no
//! llega TTS en 1500 ms, y error transitorio de 1500 ms.
//!
//! Los hilos del pipeline publican eventos; el hilo de render los vacía en
//! [`UiState::update`]. Aquí solo hay eventos lógicos: ni Tk, ni Win32.

use std::collections::VecDeque;

use parking_lot::Mutex;

use ez_core::metrics::monotonic;

/// Permanencia mínima de un estado mostrado (s).
pub const MIN_DWELL: f64 = 0.120;
/// Gracia tras `vad_end` antes de dejar de escuchar (s).
pub const VAD_HANGOVER: f64 = 0.250;
/// Ventana que protege la permanencia del aviso (s).
pub const NOTICE_HOLD: f64 = 1.000;
/// Sin TTS en este plazo, el aviso vuelve a `idle` (s).
pub const NOTICE_TTS_GRACE: f64 = 1.500;
/// Duración de un error transitorio (s).
pub const TRANSIENT_ERROR: f64 = 1.500;

/// Estados base que acepta `set_base`.
pub const BASE_STATES: [&str; 5] = ["idle", "listen", "think", "speak", "notice"];

/// Palabra que se muestra bajo cada estado.
pub const STATE_WORD: [(&str, &str); 10] = [
    ("idle", "ESPERA"),
    ("listen", "ESCUCHO"),
    ("think", "PIENSO"),
    ("speak", "HABLO"),
    ("notice", "AVISO"),
    ("mute", "PAUSA"),
    ("error", "ERROR"),
    ("open", "ABRE"),
    ("close", "CIERRA"),
    ("off", ""),
];

/// Pose de la barra para cada estado mostrado.
pub const POSE_FOR_STATE: [(&str, &str); 10] = [
    ("idle", "idle"),
    ("listen", "listen"),
    ("think", "think"),
    ("speak", "speak"),
    ("notice", "notice"),
    ("mute", "mute"),
    ("error", "error"),
    ("open", "open"),
    ("close", "close"),
    ("off", "off"),
];

/// Estados del pipeline real (`app.py`) mapeados al estado base lógico.
pub const APP_STATE_TO_BASE: [(&str, Option<&str>); 8] = [
    ("idle", Some("idle")),
    ("listening", Some("listen")),
    ("thinking", Some("think")),
    ("transcribing", Some("think")),
    ("speaking", Some("speak")),
    ("notice", Some("notice")),
    ("error", None),
    ("cancelled", None),
];

fn lookup(
    table: &[(&'static str, &'static str)],
    key: &str,
    fallback: &'static str,
) -> &'static str {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map_or(fallback, |(_, value)| *value)
}

/// Palabra del estado mostrado (`STATE_WORD.get(state, "")`).
pub fn word_for(state: &str) -> &'static str {
    lookup(&STATE_WORD, state, "")
}

/// Pose del estado mostrado (`POSE_FOR_STATE.get(state, "idle")`).
pub fn pose_for(state: &str) -> &'static str {
    lookup(&POSE_FOR_STATE, state, "idle")
}

/// Función pura de la sección 9.2 / 12.5.
pub fn display_state(visible: bool, error_active: bool, base: &str, muted: bool) -> &'static str {
    if !visible {
        return "off";
    }
    if error_active {
        return "error";
    }
    if base == "speak" {
        return "speak";
    }
    if base == "think" {
        return "think";
    }
    if base == "notice" {
        return "notice";
    }
    if muted {
        return "mute";
    }
    if base == "listen" {
        return "listen";
    }
    "idle"
}

#[derive(Clone, Debug)]
enum Event {
    Visible(bool),
    InCall(bool),
    Muted(bool),
    Base(String),
    VadEnd(f64),
    TurnEnd,
    TtsStart,
    Notice,
    Error(String),
    ErrorClear,
}

#[derive(Clone, Debug)]
struct Pending {
    at: f64,
    event: Event,
}

struct Engine {
    queue: VecDeque<Pending>,
    visible: bool,
    in_call: bool,
    muted: bool,
    base: String,
    error_active: bool,
    menu_open: bool,
    displayed: &'static str,
    displayed_at: f64,
    vad_end_at: Option<f64>,
    notice_until: Option<f64>,
    notice_tts_deadline: Option<f64>,
    error_until: Option<f64>,
}

impl Engine {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            visible: false,
            in_call: false,
            muted: false,
            base: "idle".to_string(),
            error_active: false,
            menu_open: false,
            displayed: "off",
            displayed_at: 0.0,
            vad_end_at: None,
            notice_until: None,
            notice_tts_deadline: None,
            error_until: None,
        }
    }

    fn apply(&mut self, pending: &Pending) {
        let at = pending.at;
        match &pending.event {
            Event::Visible(value) => self.visible = *value,
            Event::InCall(value) => self.in_call = *value,
            Event::Muted(value) => self.muted = *value,
            Event::Base(base) => self.queue_base(base),
            Event::VadEnd(end) => self.vad_end_at = Some(*end),
            Event::TurnEnd => {
                self.vad_end_at = None;
                self.queue_base("think");
            }
            Event::TtsStart => {
                self.notice_until = None;
                self.notice_tts_deadline = None;
                self.queue_base("speak");
            }
            Event::Notice => {
                self.queue_base("notice");
                self.notice_until = Some(at + NOTICE_HOLD);
                self.notice_tts_deadline = Some(at + NOTICE_TTS_GRACE);
            }
            Event::Error(kind) => {
                self.error_active = true;
                self.error_until = if kind == "transient" {
                    Some(at + TRANSIENT_ERROR)
                } else {
                    None
                };
            }
            Event::ErrorClear => {
                self.error_active = false;
                self.error_until = None;
            }
        }
    }

    fn queue_base(&mut self, base: &str) {
        if base == self.base {
            return;
        }
        // AVISO tiene prioridad sobre el nivel del micrófono y sobre el regreso
        // a idle mientras dura su ventana. Sin esto, cada bloque de audio
        // llamaba a set_base('listen') y borraba el aviso en un frame.
        if self.base == "notice"
            && (base == "listen" || base == "idle")
            && self.notice_until.is_some()
        {
            return;
        }
        self.base = base.to_string();
    }

    fn update_timers(&mut self, now: f64) {
        if self.vad_end_at.is_some_and(|end| now >= end) {
            self.vad_end_at = None;
            if self.base == "listen" {
                self.base = "idle".to_string();
            }
        }
        if self.notice_until.is_some_and(|until| now >= until) {
            // El AVISO no cae a idle hasta el deadline de 1500 ms sin TTS
            // (T-11); los 1000 ms solo protegen la permanencia.
            self.notice_until = None;
        }
        if self
            .notice_tts_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.notice_tts_deadline = None;
            if self.base == "notice" {
                self.base = "idle".to_string();
            }
        }
        if self.error_until.is_some_and(|until| now >= until) {
            self.error_until = None;
            self.error_active = false;
        }
    }

    fn recompute(&mut self, now: f64) {
        let target = display_state(self.visible, self.error_active, &self.base, self.muted);
        if target == self.displayed {
            return;
        }
        // Excepción de la spec: error y off no esperan permanencia.
        if target != "off"
            && target != "error"
            && self.displayed != "off"
            && self.displayed != "error"
            && now - self.displayed_at < MIN_DWELL
        {
            return;
        }
        self.displayed = target;
        self.displayed_at = now;
    }
}

type Clock = Box<dyn Fn() -> f64 + Send + Sync>;

/// Estado visual compartido entre el pipeline y el hilo de render.
pub struct UiState {
    clock: Clock,
    engine: Mutex<Engine>,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    /// Con `time.monotonic` como reloj.
    pub fn new() -> Self {
        Self::with_clock(monotonic)
    }

    /// Con un reloj inyectado (el `clock=` del constructor de Python).
    pub fn with_clock<F>(clock: F) -> Self
    where
        F: Fn() -> f64 + Send + Sync + 'static,
    {
        Self {
            clock: Box::new(clock),
            engine: Mutex::new(Engine::new()),
        }
    }

    fn now(&self, now: Option<f64>) -> f64 {
        now.unwrap_or_else(|| (self.clock)())
    }

    fn push(&self, event: Event, at: f64) {
        self.engine.lock().queue.push_back(Pending { at, event });
    }

    // ------------------------------------------------------------ lectura

    /// Estado mostrado.
    pub fn displayed(&self) -> &'static str {
        self.engine.lock().displayed
    }

    /// Palabra del estado mostrado.
    pub fn word(&self) -> &'static str {
        word_for(self.displayed())
    }

    /// Pose de la barra del estado mostrado.
    pub fn pose(&self) -> &'static str {
        pose_for(self.displayed())
    }

    /// Actualiza temporizadores y devuelve `(estado mostrado, palabra)`.
    pub fn snapshot(&self, now: Option<f64>) -> (&'static str, &'static str) {
        let displayed = self.update(now);
        (displayed, word_for(displayed))
    }

    /// Si la barra debe verse.
    pub fn visible(&self) -> bool {
        self.engine.lock().visible
    }

    /// Si hay una llamada en curso.
    pub fn in_call(&self) -> bool {
        self.engine.lock().in_call
    }

    /// Si el micrófono está en pausa.
    pub fn muted(&self) -> bool {
        self.engine.lock().muted
    }

    /// Estado base lógico.
    pub fn base(&self) -> String {
        self.engine.lock().base.clone()
    }

    /// Si hay un error activo.
    pub fn error_active(&self) -> bool {
        self.engine.lock().error_active
    }

    /// Si el menú está abierto (hoy nadie publica este evento, igual que en Python).
    pub fn menu_open(&self) -> bool {
        self.engine.lock().menu_open
    }

    // ------------------------------------------------------------ eventos

    pub fn set_visible(&self, visible: bool, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::Visible(visible), now);
    }

    pub fn set_in_call(&self, in_call: bool, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::InCall(in_call), now);
    }

    pub fn set_muted(&self, muted: bool, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::Muted(muted), now);
    }

    /// Cambia el estado base. Respeta la permanencia mínima de 120 ms.
    pub fn set_base(&self, base: &str, now: Option<f64>) {
        if !BASE_STATES.contains(&base) {
            return;
        }
        let now = self.now(now);
        self.push(Event::Base(base.to_string()), now);
    }

    /// Mapea el estado real de `VoiceApp` al modelo lógico.
    pub fn set_app_state(&self, state: &str, now: Option<f64>) {
        if state == "error" || state == "cancelled" {
            // El Python no pasa `now` aquí: el evento usa el reloj.
            self.raise_error(
                if state == "cancelled" {
                    "transient"
                } else {
                    "persistent"
                },
                None,
            );
            return;
        }
        // Cualquier estado normal implica que el error visual ya se resolvió.
        self.clear_error(now);
        if let Some((_, Some(base))) = APP_STATE_TO_BASE.iter().find(|(name, _)| *name == state) {
            self.set_base(base, now);
        }
    }

    /// Cierra la escucha tras la gracia de 250 ms, salvo `turn_end`.
    pub fn vad_end(&self, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::VadEnd(now + VAD_HANGOVER), now);
    }

    pub fn turn_end(&self, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::TurnEnd, now);
    }

    pub fn tts_start(&self, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::TtsStart, now);
    }

    pub fn notice(&self, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::Notice, now);
    }

    pub fn raise_error(&self, kind: &str, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::Error(kind.to_string()), now);
    }

    pub fn clear_error(&self, now: Option<f64>) {
        let now = self.now(now);
        self.push(Event::ErrorClear, now);
    }

    pub fn hide(&self, now: Option<f64>) {
        self.set_visible(false, now);
    }

    // ------------------------------------------------------------ motor

    /// Vacía la cola, avanza temporizadores y devuelve el estado mostrado.
    pub fn update(&self, now: Option<f64>) -> &'static str {
        let now = self.now(now);
        let mut engine = self.engine.lock();
        while let Some(pending) = engine.queue.pop_front() {
            engine.apply(&pending);
        }
        engine.update_timers(now);
        engine.recompute(now);
        engine.displayed
    }
}

#[cfg(test)]
mod tests;
