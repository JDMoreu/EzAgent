//! Lifecycle of the chat panel: open, animate, close and tear down.

use std::sync::Arc;
use std::thread;

use ez_core::metrics::monotonic;

use crate::geometry::{panel_rect, WorkArea, MORPH_SECS, SCREEN_PAD};
use crate::model::chat::view::ChatView;
use crate::theme::card_palette;
use crate::win::monitor::work_area;

use super::anim::elapsed_for;
use super::{hotkey_label, Anim, AnimState, ChatPanel, ChatWindow, SendView, CLOSE_SECS};

impl ChatPanel {
    // -------------------------------------------------------- lifecycle
    /// Whether the panel is opening or already open.
    pub fn is_open(&self) -> bool {
        matches!(
            self.state.lock().anim.state,
            AnimState::Opening | AnimState::Open
        )
    }

    /// Opens the chat if closed and closes it otherwise. `false` if it could
    /// not open.
    pub fn toggle(self: &Arc<Self>) -> bool {
        if self.is_open() {
            self.close();
            true
        } else {
            self.open()
        }
    }

    /// Opens the chat, morphing the bar shape into the panel.
    pub fn open(self: &Arc<Self>) -> bool {
        let Some(anchor) = self.ui.form_anchor() else {
            return false;
        };
        if self.resume_closing(monotonic()) {
            return true;
        }
        {
            // DECISION: Python rebuilds the view and window if `open` is called
            // again while open; that leaks the old native window here (its paint
            // thread keeps the Arc alive). Reopening is a no-op instead.
            let state = self.state.lock();
            if matches!(state.anim.state, AnimState::Opening | AnimState::Open)
                && state.view.is_some()
            {
                return true;
            }
        }
        let Some(view) = self.ui.make_view() else {
            return false;
        };
        let view = Box::new(view.with_hotkey(hotkey_label(&self.ui.chat_hotkey())));
        let form = form_around(anchor, self.ui.bar_scale(), self.ui.form_size());
        let margin = self.margin();
        let work = panel_work_area(work_area(), self.ui.scale(), margin);
        let panel = panel_rect(form, work, self.ui.scale(), self.ui.chat_size());
        let (x, y, width, height) = window_rect(panel, margin);
        let window = Arc::new(ChatWindow::new(self, x, y, width, height));
        let mut view = view;
        self.prepare_view(&mut view, panel);
        *self.window.lock() = Some(Arc::clone(&window));
        self.ui.set_chat_open(true);
        if !window.start() {
            self.teardown(true);
            return false;
        }
        // The morph clock starts here, not before: creating the native window
        // can take longer than MORPH_SECS with a real conversation, and the
        // first painted frame was already the open panel. Python sets
        // `_Anim("opening", time.monotonic(), 0.0)` after `view.render` and
        // after building the window for the same reason. The history is no
        // longer painted here (DX-1, paso 1): that work is now the paint
        // thread's, and it must not delay the morph.
        let now = monotonic();
        self.install_view(SendView(view), form, now);
        window.wake();
        window.show();
        window.set_keyboard_enabled(true);
        true
    }

    /// Reopening mid-close resumes where it was; the existing view is kept.
    // DECISION: Python re-reads the model and then keeps the view; here the
    // existing view proves the model is alive, so the factory is not called
    // again (that would consume a fresh view just to drop it).
    fn resume_closing(&self, now: f64) -> bool {
        let mut state = self.state.lock();
        if state.anim.state != AnimState::Closing || state.view.is_none() {
            return false;
        }
        let elapsed = elapsed_for(self.ui.reduced(), state.anim.p, MORPH_SECS);
        state.anim = Anim {
            state: AnimState::Opening,
            t0: now - elapsed,
            p: state.anim.p,
        };
        true
    }

    /// Configures the view before the morph starts.
    ///
    /// DECISION: the history is **not** laid out here (DX-1, paso 1). Doing it
    /// cost ~89 ms per message and it ran in the thread that opens the chat —
    /// the app's main thread when the panel is opened with the shortcut — so
    /// the whole interface froze while the history was drawn (the audio kept
    /// going). The first content frame is painted by the paint thread, which
    /// is where that work belongs.
    fn prepare_view(&self, view: &mut ChatView, panel: crate::geometry::Rect) {
        self.configure_view(
            view,
            (panel.2 - panel.0) / self.ui.scale(),
            (panel.3 - panel.1) / self.ui.scale(),
        );
    }

    fn install_view(&self, view: SendView, form: crate::geometry::Rect, now: f64) {
        let mut state = self.state.lock();
        state.view = Some(view);
        state.anim = Anim {
            state: AnimState::Opening,
            t0: now,
            p: 0.0,
        };
        state.form = form;
        state.finished = false;
        state.base = None;
        state.content = crate::win::chat::compose::ContentCache::default();
        state.resize = None;
        state.act = 0.0;
        state.last = now;
    }

    /// Starts closing: the panel shrinks back into the bar.
    pub fn close(&self) {
        let mut state = self.state.lock();
        if matches!(state.anim.state, AnimState::Closed | AnimState::Closing) {
            return;
        }
        drop(state);
        self.end_resize();
        if let Some(anchor) = self.ui.form_anchor() {
            let bar = self.ui.bar_scale();
            let (form_w, form_h) = self.ui.form_size();
            state = self.state.lock();
            state.form = (
                anchor.0 - form_w * bar / 2.0,
                anchor.1 - form_h * bar / 2.0,
                anchor.0 + form_w * bar / 2.0,
                anchor.1 + form_h * bar / 2.0,
            );
            drop(state);
        }
        let now = monotonic();
        let mut state = self.state.lock();
        let elapsed = elapsed_for(self.ui.reduced(), 1.0 - state.anim.p, CLOSE_SECS);
        state.anim = Anim {
            state: AnimState::Closing,
            t0: now - elapsed,
            p: state.anim.p,
        };
        drop(state);
        self.ui.set_form_hidden(false);
        // Sin el lock de la ventana: sus métodos despiertan a su hilo de pintura.
        let window = self.window.lock().clone();
        if let Some(window) = window {
            window.set_keyboard_enabled(false);
            // The paint loop may be asleep on the 30 fps interval of the open
            // panel: wake it so the shrink starts with the panel still whole.
            window.wake();
        }
    }

    /// Closes without animating (bar hidden or closed).
    pub fn close_now(&self) {
        self.teardown(true);
    }

    /// Called by the bar every frame: removes the window when closing ends.
    pub fn poll(&self, _now: f64) {
        if self.state.lock().finished {
            self.teardown(false);
        }
    }

    fn teardown(&self, wait: bool) {
        let window = self.window.lock().take();
        {
            let mut state = self.state.lock();
            state.view = None;
            state.anim = Anim::default();
            state.finished = false;
            state.base = None;
            state.content = crate::win::chat::compose::ContentCache::default();
            state.resize = None;
        }
        if let Some(window) = window {
            if wait {
                window.stop();
            } else {
                // The bar paint thread asks for this: stop() waits for the
                // window thread (~25-40 ms) and would jank the closing frame.
                let _ = thread::Builder::new()
                    .name(String::from("ezagent-chat-stop"))
                    .spawn(move || window.stop());
            }
        }
        self.ui.set_chat_open(false);
        self.ui.set_form_hidden(false);
    }

    pub(super) fn configure_view(&self, view: &mut ChatView, width: f64, height: f64) {
        let theme = self.ui.theme_name();
        view.configure(width, height, self.ui.scale(), &theme, card_palette(&theme));
    }

    // ------------------------------------------------------- live state
}

/// `_form`: physical rectangle of the colour shape around its anchor.
fn form_around(anchor: (f64, f64), bar: f64, size: (f64, f64)) -> crate::geometry::Rect {
    (
        anchor.0 - size.0 * bar / 2.0,
        anchor.1 - size.1 * bar / 2.0,
        anchor.0 + size.0 * bar / 2.0,
        anchor.1 + size.1 * bar / 2.0,
    )
}

/// Área para el panel que deja sitio al margen de sombra de la ventana.
///
/// `panel_rect` ya aplica `SCREEN_PAD`; se descuenta solo la diferencia con el
/// margen de la ventana, para que `panel + margen` quepa entero en `work`.
fn panel_work_area(work: WorkArea, scale: f64, margin: i32) -> WorkArea {
    let pad = SCREEN_PAD * scale;
    let inset = (f64::from(margin) - pad).max(0.0).ceil() as i32;
    (
        work.0 + inset,
        work.1 + inset,
        work.2 - inset,
        work.3 - inset,
    )
}

/// Window rectangle around the panel, leaving room for the shadow.
fn window_rect(panel: crate::geometry::Rect, margin: i32) -> (i32, i32, i32, i32) {
    (
        (panel.0.round_ties_even() as i32) - margin,
        (panel.1.round_ties_even() as i32) - margin,
        (panel.2 - panel.0).round_ties_even() as i32 + 2 * margin,
        (panel.3 - panel.1).round_ties_even() as i32 + 2 * margin,
    )
}

impl Drop for ChatPanel {
    fn drop(&mut self) {
        if let Some(window) = self.window.lock().take() {
            window.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::chat::view::test_support::view_with;
    use crate::model::chat::view::{ChatMessage, Role};
    use crate::win::chat::test_support::{fake_ui, Recorder};

    /// Abrir el chat no maqueta el historial en el hilo que abre.
    ///
    /// DECISION: el maquetado es trabajo del hilo de pintura (DX-1, paso 1).
    /// Antes costaba ~89 ms por mensaje y lo pagaba el hilo que abre el chat
    /// (el principal de la app): con 33 mensajes eran unos tres segundos de
    /// interfaz congelida.
    #[test]
    fn opening_the_chat_does_not_lay_out_the_history() {
        let panel = ChatPanel::new(fake_ui(), Arc::new(Recorder::default()));
        let messages: Vec<ChatMessage> = (1..=40)
            .map(|id| ChatMessage::new(id, Role::User, format!("mensaje {id}")))
            .collect();
        let mut view = view_with(messages, Vec::new());
        panel.prepare_view(&mut view, (0.0, 0.0, 440.0, 600.0));
        assert_eq!(
            view.layouts(),
            0,
            "the thread that opens the chat laid out the history"
        );
        // The paint thread is the one that does it.
        view.render(0.0);
        assert!(view.layouts() > 0, "the painted frame laid out nothing");
    }

    #[test]
    fn the_window_sits_around_the_panel_with_the_shadow_margin() {
        let panel = (100.4, 200.6, 500.0, 800.0);
        assert_eq!(window_rect(panel, 12), (88, 189, 424, 623));
        let form = form_around((500.0, 500.0), 1.0, (100.0, 36.0));
        assert_eq!(form, (450.0, 482.0, 550.0, 518.0));
        let scaled = form_around((500.0, 500.0), 1.5, (100.0, 36.0));
        assert_eq!(scaled.2 - scaled.0, 150.0);
        assert_eq!(scaled.3 - scaled.1, 54.0);
    }

    /// DX-7c: el humo real dejaba el borde inferior de la ventana en 1528,
    /// un píxel fuera de `DISPLAY2` (hasta 1527), por la sombra.
    #[test]
    fn the_chat_window_with_its_shadow_stays_inside_the_monitor() {
        let work = (-1080, -393, 0, 1527);
        let form = (-671.0, 1469.0, -409.0, 1512.0);
        let panel = panel_rect(
            form,
            panel_work_area(work, 1.0, 16),
            1.0,
            Some((434.0, 704.0)),
        );
        let (x, y, width, height) = window_rect(panel, 16);
        assert_eq!((x, y, x + width, y + height), (-773, 791, -307, 1527));
    }

    #[test]
    fn a_maximum_panel_with_its_shadow_still_fits() {
        let work = (-1080, -393, 0, 1527);
        let panel = panel_rect(
            (0.0, 0.0, 1.0, 1.0),
            panel_work_area(work, 1.0, 16),
            1.0,
            Some((10_000.0, 10_000.0)),
        );
        let (x, y, width, height) = window_rect(panel, 16);
        assert_eq!((x, y, x + width, y + height), work);
    }
}
