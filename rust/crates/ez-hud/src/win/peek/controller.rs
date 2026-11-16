//! Controlador de la vista rápida: cuándo se ve la tarjeta y qué muestra.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use ez_core::metrics::monotonic;
use ez_core::pytime::timestamp;
use parking_lot::Mutex;

use crate::paint::marks::WORKING;
use crate::paint::peek::{
    card_height, ease_out, meta_line, paint_marks, paint_rows, toast_secs, IconPainter, PeekBuffer,
    PeekItem, CLOSE_SECS, HOVER_DELAY, LEAVE_GRACE, MARGIN, MAX_ROWS, OPEN_SECS, PEEK_W,
    REVEAL_SECS,
};
use crate::tokens::palette_version;

use super::frame::{compose, place_card};
use super::ui::PeekUi;
use super::window::PeekWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Hover,
    Toast,
}

pub(super) struct PeekState {
    pub(super) items: Vec<PeekItem>,
    pub(super) mode: Option<Mode>, // "hover" | "toast" | None
    toast: Option<(String, f64)>,  // pedido desde el hilo de Tk
    toast_key: String,
    until: f64,
    hover_since: Option<f64>,
    out_since: Option<f64>,
    pub(super) shown_at: f64,
    closing: Option<(f64, f64)>, // (instante, progreso al cerrar)
    above: bool,
    hover_row: Option<usize>,
    pub(super) rows: (Vec<PeekItem>, usize),
}

impl PeekState {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            mode: None,
            toast: None,
            toast_key: String::new(),
            until: 0.0,
            hover_since: None,
            out_since: None,
            shown_at: 0.0,
            closing: None,
            above: true,
            hover_row: None,
            rows: (Vec::new(), 0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct StaticKey {
    rows: Vec<(String, String, String, String, bool, String)>,
    more: usize,
    hover: Option<usize>,
    theme: String,
    palette: i64,
    scale: u64,
    width: i32,
    height: i32,
}

enum Plan {
    Hide,
    Place((i32, i32), f64),
    Frame(f64),
}

/// Decide cuándo se ve la tarjeta y qué muestra; la ventana se crea al primer uso.
pub struct Peek {
    ui: Arc<dyn PeekUi>,
    icons: Arc<dyn IconPainter>,
    pub(super) lock: Mutex<PeekState>,
    static_layer: Mutex<Option<(StaticKey, PeekBuffer)>>,
    pub(super) win: Mutex<Option<Arc<PeekWindow>>>,
}

impl Peek {
    /// Controlador con su UI y su proveedor de iconos.
    pub fn new(ui: Arc<dyn PeekUi>, icons: Arc<dyn IconPainter>) -> Self {
        Self {
            ui,
            icons,
            lock: Mutex::new(PeekState::new()),
            static_layer: Mutex::new(None),
            win: Mutex::new(None),
        }
    }

    // ---- datos (hilo de Tk)
    pub fn refresh(&self, items: Vec<PeekItem>) {
        self.lock.lock().items = items;
    }

    pub fn toast(&self, item: &PeekItem, now: f64) {
        self.lock.lock().toast = Some((item.key.clone(), now + toast_secs(&item.state)));
    }

    /// Muestra la lista completa un rato (pedida por voz), sin esperar al puntero.
    pub fn reveal(&self, now: f64) {
        self.lock.lock().toast = Some((String::new(), now + REVEAL_SECS));
    }

    /// Escala de la barra (pantalla × tamaño elegido): la tarjeta crece y encoge con ella.
    fn scale(&self) -> f64 {
        self.ui
            .bar_scale()
            .filter(|value| *value != 0.0)
            .unwrap_or_else(|| self.ui.scale())
    }

    pub fn is_open(&self) -> bool {
        self.lock.lock().mode.is_some()
    }

    /// Oculta la tarjeta sin animación (la barra se oculta).
    pub fn hide_now(&self) {
        {
            let mut state = self.lock.lock();
            state.mode = None;
            state.closing = None;
            state.toast = None;
        }
        // Sin el lock: `hide` espera al hilo de la vista rápida (ver `place`).
        let window = self.win.lock().clone();
        if let Some(window) = window {
            window.hide();
        }
    }

    pub fn stop(&self) {
        let window = self.win.lock().take();
        if let Some(window) = window {
            window.stop();
        }
    }

    // ---- estado (hilo de la barra)
    pub fn tick(self: &Arc<Self>, now: f64, over_form: bool, blocked: bool) {
        let plan = {
            let mut state = self.lock.lock();
            if blocked {
                state.hover_since = None;
                state.toast = None;
                self.close(&mut state, now);
            } else {
                self.advance(&mut state, now, over_form);
            }
            let rows = visible_rows(&state);
            self.plan(&mut state, now, rows)
        };
        self.apply(plan);
    }

    fn advance(&self, state: &mut PeekState, now: f64, over_form: bool) {
        if state.toast.is_some() && state.mode != Some(Mode::Hover) {
            let (key, until) = state.toast.take().expect("el aviso existe");
            state.toast_key = key;
            state.until = until;
            self.open(state, now, Mode::Toast);
        }
        state.toast = None;
        if !over_form {
            state.hover_since = None;
        } else if state.hover_since.is_none() {
            state.hover_since = Some(now);
        }
        let ready = state
            .hover_since
            .is_some_and(|since| now - since >= HOVER_DELAY);
        if state.mode.is_none() && ready && !state.items.is_empty() {
            self.open(state, now, Mode::Hover);
        }
        let inside = over_form || self.pointer_in_card();
        if state.mode == Some(Mode::Toast) {
            if inside {
                state.mode = Some(Mode::Hover);
            } else if now >= state.until {
                self.close(state, now);
            }
        }
        if state.mode == Some(Mode::Hover) {
            if inside {
                state.out_since = None;
            } else if state.out_since.is_none() {
                state.out_since = Some(now);
            }
            if state.items.is_empty()
                || state
                    .out_since
                    .is_some_and(|since| now - since > LEAVE_GRACE)
            {
                self.close(state, now);
            }
        }
        state.hover_row = if state.mode.is_some() {
            self.pointer_row()
        } else {
            None
        };
    }

    fn open(&self, state: &mut PeekState, now: f64, mode: Mode) {
        let current = progress(state, now);
        state.mode = Some(mode);
        state.out_since = None;
        state.shown_at = now - OPEN_SECS * current; // retoma desde donde iba
        state.closing = None;
    }

    fn close(&self, state: &mut PeekState, now: f64) {
        if state.mode.is_some() {
            state.closing = Some((now, progress(state, now)));
        }
        state.mode = None;
    }

    // ---- ventana
    fn window_size(&self, rows: usize, more: usize) -> (i32, i32) {
        let scale = self.scale();
        (
            ((PEEK_W + 2.0 * MARGIN) * scale).round_ties_even() as i32,
            ((card_height(rows.max(1), more > 0) + 2.0 * MARGIN) * scale).round_ties_even() as i32,
        )
    }

    /// Qué hacer con la ventana (con el lock tomado); lo aplica `apply` sin él.
    fn plan(&self, state: &mut PeekState, now: f64, rows: (Vec<PeekItem>, usize)) -> Option<Plan> {
        let window = self.win.lock().clone();
        if state.mode.is_none() && progress(state, now) <= 0.0 {
            state.closing = None;
            return window
                .as_ref()
                .filter(|window| window.is_visible())
                .map(|_| Plan::Hide);
        }
        if rows.0.is_empty() {
            return None;
        }
        let size = self.window_size(rows.0.len(), rows.1);
        let fresh = rows != state.rows
            || window.is_none()
            || !window.as_ref().is_some_and(|window| window.is_visible());
        state.rows = rows.clone();
        let animating = state.closing.is_some() || now - state.shown_at < OPEN_SECS;
        let spin = rows.0.iter().any(|item| item.state == WORKING);
        let interval = 1.0
            / if animating {
                60.0
            } else if spin {
                30.0
            } else {
                4.0
            };
        if fresh || window.as_ref().map(|window| window.size()) != Some(size) {
            return Some(Plan::Place(size, interval));
        }
        Some(Plan::Frame(interval))
    }

    fn apply(self: &Arc<Self>, plan: Option<Plan>) {
        // DECISION: nunca con el lock tomado. ShowWindow/SetWindowPos sobre una
        // ventana de otro hilo espera a que ese hilo atienda el mensaje, y ese
        // hilo puede estar en `render` esperando el lock: un interbloqueo que
        // congelaba la barra (y luego Tk en `refresh`) — «la app no responde».
        let interval = match plan {
            None => return,
            Some(Plan::Hide) => {
                let window = self.win.lock().clone();
                if let Some(window) = window {
                    window.hide();
                }
                return;
            }
            Some(Plan::Place(size, interval)) => {
                self.place(size);
                interval
            }
            Some(Plan::Frame(interval)) => interval,
        };
        let window = self.win.lock().clone();
        if let Some(window) = window {
            window.set_interval(interval);
            window.mark_dirty();
        }
    }

    /// Todo el hub en pantalla; sin él (pruebas), un cuadrado alrededor de la forma.
    fn hub(&self) -> Option<(f64, f64, f64, f64)> {
        if let Some(hub) = self.ui.hub_extent() {
            return Some(hub);
        }
        let (ax, ay, half) = self.ui.form_extent()?;
        Some((ax - half, ay - half, ax + half, ay + half))
    }

    fn place(self: &Arc<Self>, size: (i32, i32)) {
        let Some(hub) = self.hub() else {
            return;
        };
        let (width, height) = size;
        let (x, y, above) = place_card(hub, size, self.ui.work_area(), self.scale());
        self.lock.lock().above = above;
        let mut slot = self.win.lock();
        if slot.is_none() {
            let window = Arc::new(PeekWindow::new(self, x, y, width, height));
            *slot = Some(Arc::clone(&window));
            drop(slot);
            // start() espera a que Windows cree la ventana: fuera del hilo de la barra,
            // que si no se salta cuadros (medido: hasta 60 ms) justo al aparecer.
            let result = std::thread::Builder::new()
                .name(String::from("ezagent-peek-start"))
                .spawn(move || window.start());
            if let Err(error) = result {
                tracing::warn!("no se pudo arrancar el hilo de la vista rápida: {error}");
            }
            return;
        }
        let Some(window) = slot.clone() else {
            return;
        };
        // DECISION: soltar `win` antes de mover la ventana. `resize`/`set_pos`/`show`
        // esperan a que el hilo de la vista rápida atienda el mensaje, y ese hilo
        // pide `win` en `render`: con el lock tomado, la barra y la vista rápida se
        // esperaban para siempre (la app se congelaba al aparecer la actividad).
        drop(slot);
        window.resize(width, height);
        window.set_pos(x, y);
        if !window.is_visible() {
            window.show();
        }
    }

    fn cursor_client(&self) -> Option<(f64, f64)> {
        let window = self.win.lock().clone()?;
        if !window.is_visible() {
            return None;
        }
        let (sx, sy) = crate::win::layered::screen_cursor();
        let (wx, wy) = window.position();
        Some((f64::from(sx - wx), f64::from(sy - wy)))
    }

    fn pointer_in_card(&self) -> bool {
        let Some((x, y)) = self.cursor_client() else {
            return false;
        };
        let margin = MARGIN * self.scale();
        let (width, height) = self
            .win
            .lock()
            .as_ref()
            .map_or((1, 1), |window| window.size());
        margin <= x
            && x <= f64::from(width) - margin
            && margin <= y
            && y <= f64::from(height) - margin
    }

    fn pointer_row(&self) -> Option<usize> {
        let (x, y) = self.cursor_client()?;
        self.row_at(x, y)
    }

    /// Fila bajo un punto de la ventana (px físicos), o `None`.
    pub fn row_at(&self, x: f64, y: f64) -> Option<usize> {
        let count = self.lock.lock().rows.0.len();
        crate::paint::peek::row_at(count, x, y, self.scale())
    }

    /// Clave de la fila cuya × está bajo el punto, o `None`.
    pub fn dismiss_at(&self, x: f64, y: f64) -> Option<String> {
        let rows = self.lock.lock().rows.0.clone();
        crate::paint::peek::dismiss_at(&rows, x, y, self.scale())
    }

    /// Quita la fila al instante; la app la olvida de verdad en su hilo.
    pub fn dismiss(&self, key: &str) {
        let now = monotonic();
        {
            let mut state = self.lock.lock();
            state.items.retain(|item| item.key.as_str() != key);
            let more = state.rows.1;
            state.rows = (
                state
                    .rows
                    .0
                    .iter()
                    .filter(|item| item.key.as_str() != key)
                    .cloned()
                    .collect(),
                more,
            );
            if state.mode == Some(Mode::Toast) && state.toast_key == key {
                self.close(&mut state, now);
            }
        }
        let ui = Arc::clone(&self.ui);
        let _ = catch_unwind(AssertUnwindSafe(|| ui.on_activity_dismiss(key)));
    }

    pub fn click(&self, x: f64, y: f64) {
        if let Some(key) = self.dismiss_at(x, y) {
            self.dismiss(&key);
            return;
        }
        let Some(index) = self.row_at(x, y) else {
            return;
        };
        let key = self
            .lock
            .lock()
            .rows
            .0
            .get(index)
            .map(|item| item.key.clone());
        if let Some(key) = key {
            let ui = Arc::clone(&self.ui);
            let _ = catch_unwind(AssertUnwindSafe(|| ui.on_activity_open(&key)));
        }
    }

    // ---- dibujo (hilo de la tarjeta)
    pub fn render(&self) -> PeekBuffer {
        let now = monotonic();
        let (rows, more, hover, p, above) = {
            let state = self.lock.lock();
            (
                state.rows.0.clone(),
                state.rows.1,
                state.hover_row,
                progress(&state, now),
                state.above,
            )
        };
        let (width, height) = self
            .win
            .lock()
            .as_ref()
            .map_or((1, 1), |window| window.size());
        if p <= 0.0 || rows.is_empty() {
            return PeekBuffer::new(width.max(1) as usize, height.max(1) as usize);
        }
        let theme = self.ui.theme_name();
        let scale = self.scale();
        let reduced = self.ui.reduced();
        let wall = timestamp();
        let key = static_key(&rows, more, hover, &theme, scale, width, height, wall);
        let mut buffer = self.static_layer(&key, &rows, width, height, scale, &theme, hover, wall);
        paint_marks(
            &mut buffer,
            &rows,
            scale,
            &theme,
            now,
            reduced,
            self.icons.as_ref(),
        );
        compose(&buffer, p, above, scale, reduced)
    }

    #[allow(clippy::too_many_arguments)]
    fn static_layer(
        &self,
        key: &StaticKey,
        rows: &[PeekItem],
        width: i32,
        height: i32,
        scale: f64,
        theme: &str,
        hover: Option<usize>,
        wall: f64,
    ) -> PeekBuffer {
        let cached = {
            let cache = self.static_layer.lock();
            cache
                .as_ref()
                .filter(|(cached, _)| cached == key)
                .map(|(_, buffer)| buffer.clone())
        };
        if let Some(buffer) = cached {
            return buffer;
        }
        let buffer = paint_rows(
            rows,
            key.more,
            width.max(1) as usize,
            height.max(1) as usize,
            scale,
            theme,
            hover,
            wall,
            self.icons.as_ref(),
        );
        *self.static_layer.lock() = Some((key.clone(), buffer.clone()));
        buffer
    }
}

#[allow(clippy::too_many_arguments)]
fn static_key(
    rows: &[PeekItem],
    more: usize,
    hover: Option<usize>,
    theme: &str,
    scale: f64,
    width: i32,
    height: i32,
    wall: f64,
) -> StaticKey {
    StaticKey {
        rows: rows
            .iter()
            .map(|item| {
                (
                    item.key.clone(),
                    item.state.clone(),
                    item.title.clone(),
                    item.agent.clone(),
                    item.unseen,
                    meta_line(item, wall),
                )
            })
            .collect(),
        more,
        hover,
        theme: theme.to_string(),
        palette: palette_version(),
        scale: scale.to_bits(),
        width,
        height,
    }
}

impl Drop for Peek {
    fn drop(&mut self) {
        let window = self.win.lock().take();
        if let Some(window) = window {
            window.stop();
        }
    }
}

fn progress(state: &PeekState, now: f64) -> f64 {
    if let Some((at, start)) = state.closing {
        return start * (1.0 - ease_out((now - at) / CLOSE_SECS));
    }
    if state.mode.is_none() {
        return 0.0;
    }
    ease_out((now - state.shown_at) / OPEN_SECS)
}

pub(super) fn visible_rows(state: &PeekState) -> (Vec<PeekItem>, usize) {
    if state.mode == Some(Mode::Toast) && !state.toast_key.is_empty() {
        let matched: Vec<PeekItem> = state
            .items
            .iter()
            .filter(|item| item.key == state.toast_key)
            .cloned()
            .collect();
        return if matched.is_empty() {
            (state.items.iter().take(1).cloned().collect(), 0)
        } else {
            (matched, 0)
        };
    }
    if state.mode.is_none() {
        return state.rows.clone(); // se mantienen mientras se desvanece
    }
    let rows = state.items.iter().take(MAX_ROWS).cloned().collect();
    (rows, state.items.len().saturating_sub(MAX_ROWS))
}
