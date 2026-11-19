//! Lifecycle of the chat panel: open, animate, close and tear down.

use std::sync::Arc;
use std::thread;

use ez_core::metrics::monotonic;

use crate::geometry::{panel_rect, MORPH_SECS};
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
        let panel = panel_rect(form, work_area(), self.ui.scale(), self.ui.chat_size());
        let (x, y, width, height) = window_rect(panel, self.margin());
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
}
