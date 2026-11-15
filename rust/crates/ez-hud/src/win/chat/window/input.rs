//! Pointer, keyboard, wheel, drop and resize routing of the chat panel.

use crate::geometry::{edge_cursor, edges_at_grip, resize_rect, Edges, GRIP};
use crate::win::monitor::work_area;

use super::anim::clamp_to_work;
use super::compose::ContentCache;
use super::{AnimState, ChatPanel, ChatWindow, Resize};

impl ChatPanel {
    // ------------------------------------------------------------ input
    fn local(&self, x: i32, y: i32) -> (f64, f64) {
        let scale = self.ui.scale();
        let margin = self.margin();
        (f64::from(x - margin) / scale, f64::from(y - margin) / scale)
    }

    fn edges(&self, window: &ChatWindow, x: i32, y: i32) -> Option<Edges> {
        let (lx, ly) = self.local(x, y);
        let (width, height) = self.logical_size(window);
        edges_at_grip(lx, ly, width, height, GRIP)
    }

    /// Mouse press. `true` consumes the event; `false` lets the window drag.
    pub fn pointer_down(&self, x: i32, y: i32) -> bool {
        let Some(window) = self.window.lock().clone() else {
            return true;
        };
        let mut resize = None;
        let action = {
            let mut state = self.state.lock();
            if state.anim.state != AnimState::Open {
                return true;
            }
            let Some(view) = state.view.as_mut() else {
                return true;
            };
            let (lx, ly) = self.local(x, y);
            if let Some(edges) = self.edges(&window, x, y) {
                resize = Some(edges);
                None
            } else {
                view.pointer_move(lx, ly);
                if view.hover.is_none() {
                    return false;
                }
                view.pointer_down(lx, ly)
            }
        };
        if let Some(edges) = resize {
            self.begin_resize(edges);
            return true;
        }
        self.perform(action);
        true
    }

    /// Pointer motion with the button down inside the window (resizing).
    pub fn pointer_move(&self, x: i32, y: i32) -> bool {
        if self.state.lock().resize.is_some() {
            self.drag_resize();
            return true;
        }
        self.hover(x, y)
    }

    /// Pointer release.
    pub fn pointer_up(&self) -> bool {
        self.end_resize();
        true
    }

    /// Hover: updates the view and says whether the hand cursor belongs here.
    pub fn hover(&self, x: i32, y: i32) -> bool {
        let Some(window) = self.window.lock().clone() else {
            return false;
        };
        let mut state = self.state.lock();
        if state.anim.state != AnimState::Open {
            return false;
        }
        let Some(view) = state.view.as_mut() else {
            return false;
        };
        let (lx, ly) = self.local(x, y);
        if self.edges(&window, x, y).is_some() {
            view.pointer_move(-1.0, -1.0);
            return false;
        }
        view.pointer_move(lx, ly);
        view.cursor_is_hand(lx, ly)
    }

    /// Windows cursor identifier for the borders (`IDC_*`), or 0.
    pub fn cursor_ident(&self, x: i32, y: i32) -> u16 {
        let (open, resize) = {
            let state = self.state.lock();
            (state.anim.state == AnimState::Open, state.resize)
        };
        let edges = match resize {
            Some(resize) => Some(resize.edges),
            None if open => self
                .window
                .lock()
                .clone()
                .and_then(|window| self.edges(&window, x, y)),
            None => None,
        };
        edges.map_or(0, |edges| edge_cursor(edges) as u16)
    }

    /// Mouse wheel: `delta > 0` goes up, towards older messages.
    pub fn wheel(&self, delta: i32) -> bool {
        let mut state = self.state.lock();
        if state.anim.state != AnimState::Open {
            return false;
        }
        let Some(view) = state.view.as_mut() else {
            return false;
        };
        view.wheel(delta);
        true
    }

    /// One key (Windows VK code). Swallows keys while the panel animates.
    pub fn key(&self, vk: i32, ctrl: bool) -> bool {
        let mut state = self.state.lock();
        if state.anim.state != AnimState::Open {
            return true;
        }
        let Some(view) = state.view.as_mut() else {
            return true;
        };
        let shift = crate::win::settings_window::os_modifiers().contains("shift");
        let action = view.key(vk, ctrl, shift);
        drop(state);
        self.perform(action);
        true
    }

    /// A typed character.
    pub fn char(&self, value: &str) -> bool {
        let mut state = self.state.lock();
        if state.anim.state == AnimState::Open {
            if let Some(view) = state.view.as_mut() {
                view.char(value);
            }
        }
        true
    }

    /// Pasted text.
    pub fn paste(&self, text: &str) -> bool {
        let mut state = self.state.lock();
        if state.anim.state == AnimState::Open {
            if let Some(view) = state.view.as_mut() {
                view.paste(text);
            }
        }
        true
    }

    /// Files dropped on the window: they enter as links the chat shows as cards.
    pub fn files(&self, paths: Vec<String>) {
        if let Some(view) = self.state.lock().view.as_mut() {
            view.add_files(&paths);
        }
    }

    /// Allowed window position: the whole panel on screen (the shadow may hang out).
    pub fn clamp(&self, x: i32, y: i32) -> (i32, i32) {
        let (width, height) = self
            .window
            .lock()
            .as_ref()
            .map_or((0, 0), |window| window.size());
        let margin = self.margin();
        clamp_to_work(x, y, width, height, work_area(), margin)
    }

    /// DPI change reported by the window (`on_dpi_changed`).
    pub fn dpi_changed(&self, dpi: u32) {
        self.ui.set_scale(1.0f64.max(f64::from(dpi) / 96.0));
    }

    // ----------------------------------------------------------- resize
    fn begin_resize(&self, edges: Edges) {
        let Some(window) = self.window.lock().clone() else {
            return;
        };
        let cursor = crate::win::layered::screen_cursor();
        self.state.lock().resize = Some(Resize {
            edges,
            cursor,
            rect: self.panel(&window),
        });
    }

    fn drag_resize(&self) {
        let Some(window) = self.window.lock().clone() else {
            return;
        };
        let Some(resize) = self.state.lock().resize else {
            return;
        };
        let cursor = crate::win::layered::screen_cursor();
        let scale = self.ui.scale();
        let rect = resize_rect(
            resize.edges,
            resize.rect,
            f64::from(cursor.0 - resize.cursor.0),
            f64::from(cursor.1 - resize.cursor.1),
            work_area(),
            scale,
        );
        let width = (rect.2 - rect.0).round_ties_even() as i32;
        let height = (rect.3 - rect.1).round_ties_even() as i32;
        let margin = self.margin();
        window.set_pos(
            (rect.0.round_ties_even() as i32) - margin,
            (rect.1.round_ties_even() as i32) - margin,
        );
        window.resize(width + 2 * margin, height + 2 * margin);
        let mut state = self.state.lock();
        if let Some(view) = state.view.as_mut() {
            self.configure_view(view, f64::from(width) / scale, f64::from(height) / scale);
        }
        state.base = None;
        state.content = ContentCache::default();
    }

    pub(super) fn end_resize(&self) {
        let was_resizing = self.state.lock().resize.take().is_some();
        if !was_resizing {
            return;
        }
        if let Some(window) = self.window.lock().clone() {
            let (width, height) = self.logical_size(&window);
            self.ui.remember_chat_size(width, height);
        }
    }
}
