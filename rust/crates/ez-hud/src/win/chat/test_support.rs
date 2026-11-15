//! Fakes and helpers shared by the tests of the chat window.

use std::sync::Arc;

use ndarray::Array3;
use parking_lot::Mutex;

use crate::model::chat::view::test_support::{
    sample_view as model_sample_view, FakeHost, FakeInput, FakeModel,
};
use crate::model::chat::view::ChatView;

use super::window::{ChatEvent, ChatEvents, ChatUi, SendView};

/// UI with plain fields and the Python defaults.
pub struct FakeUi {
    pub scale: Mutex<f64>,
    pub bar_scale: f64,
    pub form_size: (f64, f64),
    pub anchor: Mutex<Option<(f64, f64)>>,
    pub layout: &'static str,
    pub reduced: bool,
    pub theme: &'static str,
    pub pose: Mutex<String>,
    pub in_call: bool,
    pub chat_hotkey: String,
    pub chat_size: Mutex<Option<(f64, f64)>>,
    pub body: [u8; 3],
    pub fluid: bool,
    pub form_hidden: Mutex<bool>,
    pub chat_open: Mutex<bool>,
    pub view: Mutex<Option<SendView>>,
    /// Seconds `make_view` sleeps: stands in for a real history that takes
    /// longer to paint than the morph.
    pub slow_view: Mutex<f64>,
}

impl Default for FakeUi {
    fn default() -> Self {
        Self {
            scale: Mutex::new(1.0),
            bar_scale: 1.0,
            form_size: (100.0, 36.0),
            anchor: Mutex::new(Some((500.0, 500.0))),
            layout: "pill",
            reduced: false,
            theme: "dark",
            pose: Mutex::new(String::from("idle")),
            in_call: false,
            chat_hotkey: String::new(),
            chat_size: Mutex::new(None),
            body: [13, 22, 26],
            fluid: false,
            form_hidden: Mutex::new(false),
            chat_open: Mutex::new(false),
            view: Mutex::new(None),
            slow_view: Mutex::new(0.0),
        }
    }
}

impl ChatUi for FakeUi {
    fn scale(&self) -> f64 {
        *self.scale.lock()
    }

    fn bar_scale(&self) -> f64 {
        self.bar_scale
    }

    fn form_size(&self) -> (f64, f64) {
        self.form_size
    }

    fn form_anchor(&self) -> Option<(f64, f64)> {
        *self.anchor.lock()
    }

    fn layout(&self) -> &str {
        self.layout
    }

    fn reduced(&self) -> bool {
        self.reduced
    }

    fn theme_name(&self) -> String {
        self.theme.to_string()
    }

    fn chat_pose(&self) -> String {
        self.pose.lock().clone()
    }

    fn in_call(&self) -> bool {
        self.in_call
    }

    fn chat_hotkey(&self) -> String {
        self.chat_hotkey.clone()
    }

    fn chat_size(&self) -> Option<(f64, f64)> {
        *self.chat_size.lock()
    }

    fn remember_chat_size(&self, width: f64, height: f64) {
        *self.chat_size.lock() = Some((width, height));
    }

    fn chat_body_rgb(&self) -> [u8; 3] {
        self.body
    }

    fn chat_fluid(&self, width: usize, height: usize, _now: f64) -> Option<Array3<f32>> {
        self.fluid.then(|| {
            Array3::from_shape_fn((height, width, 3), |(_, _, channel)| match channel {
                0 => 1.0,
                1 => 0.0,
                _ => 0.0,
            })
        })
    }

    fn set_form_hidden(&self, hidden: bool) {
        *self.form_hidden.lock() = hidden;
    }

    fn set_chat_open(&self, open: bool) {
        *self.chat_open.lock() = open;
    }

    fn set_scale(&self, scale: f64) {
        *self.scale.lock() = scale;
    }

    fn make_view(&self) -> Option<Box<ChatView>> {
        let slow = *self.slow_view.lock();
        if slow > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(slow));
        }
        self.view.lock().take().map(SendView::into_inner)
    }
}

/// Event sink that records every callback.
#[derive(Default)]
pub struct Recorder {
    pub events: Mutex<Vec<ChatEvent>>,
}

impl ChatEvents for Recorder {
    fn emit(&self, event: ChatEvent) {
        self.events.lock().push(event);
    }
}

/// UI with the Python defaults.
pub fn fake_ui() -> Arc<FakeUi> {
    Arc::new(FakeUi::default())
}

/// View with one message and a dark palette, configured for the panel.
pub fn sample_view() -> ChatView {
    let mut view = model_sample_view();
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    view
}

/// Empty view for tests that only need the trait objects in place.
pub fn empty_view() -> ChatView {
    ChatView::new(
        Box::new(FakeModel::default()),
        Box::new(FakeHost),
        Box::new(FakeInput::default()),
    )
}
