//! Cuadros del morph forma → panel (`ChatPanel._compose`) contra los PNG.

use std::path::Path;

use ndarray::Array3;
use parking_lot::Mutex;

use ez_hud::geometry::{panel_rect, WorkArea, MARGIN};
use ez_hud::model::chat::view::ChatView;
use ez_hud::model::drop::{DropModel, Update, PILL_H, PILL_W};
use ez_hud::paint::drop::render_fluid as drop_fluid;
use ez_hud::win::chat::compose::{compose_frame, fluid_array, Frame, Scene, WindowRect};
use ez_hud::win::chat::{AnimState, ChatUi};
use ez_parity::fixtures::FixtureCase;

use super::panel::model_of;
use super::support::{flag, line, num};

struct FakeUi {
    scale: f64,
    layout: &'static str,
    theme: &'static str,
    reduced: bool,
    model: Mutex<DropModel>,
    fluid: Mutex<Option<(f64, Array3<u8>)>>,
}

impl FakeUi {
    fn new(case: &FixtureCase, scale: f64) -> Self {
        let layout = if line(case, "layout") == "circle" {
            "circle"
        } else {
            "pill"
        };
        let theme = if line(case, "theme") == "dark" {
            "dark"
        } else {
            "light"
        };
        let mut model = DropModel::new(layout, None);
        for index in 0..48 {
            model.update(&Update {
                now: index as f64 / 30.0,
                dt: 1.0 / 30.0,
                pose: "idle",
                rms: 0.35,
                theme,
                ..Update::default()
            });
        }
        Self {
            scale,
            layout,
            theme,
            reduced: flag(case, "reduced"),
            model: Mutex::new(model),
            fluid: Mutex::new(None),
        }
    }
}

impl ChatUi for FakeUi {
    fn scale(&self) -> f64 {
        self.scale
    }

    fn bar_scale(&self) -> f64 {
        self.scale
    }

    fn form_size(&self) -> (f64, f64) {
        (PILL_W, PILL_H)
    }

    fn form_anchor(&self) -> Option<(f64, f64)> {
        None
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
        "idle".to_string()
    }

    fn in_call(&self) -> bool {
        false
    }

    fn chat_hotkey(&self) -> String {
        String::new()
    }

    fn chat_size(&self) -> Option<(f64, f64)> {
        None
    }

    fn remember_chat_size(&self, _: f64, _: f64) {}

    fn chat_body_rgb(&self) -> [u8; 3] {
        let pal = ez_hud::tokens::palette(self.theme).expect("la paleta del tema");
        pal["BODY"].channels()
    }

    fn chat_fluid(&self, width: usize, height: usize, now: f64) -> Option<Array3<f32>> {
        let model = self.model.lock();
        let mut cache = self.fluid.lock();
        let fresh = cache.as_ref().is_none_or(|(instant, _)| *instant != now);
        if fresh {
            let rgb = drop_fluid(&model, now);
            *cache = Some((now, rgb));
        }
        fluid_array(cache.as_ref().map(|(_, rgb)| rgb), width, height)
    }

    fn set_form_hidden(&self, _: bool) {}

    fn set_chat_open(&self, _: bool) {}

    fn set_scale(&self, _: f64) {}

    fn make_view(&self) -> Option<Box<ChatView>> {
        None
    }
}

pub(super) fn morph(case: &FixtureCase) {
    let scale = num(case, "scale");
    let ui = FakeUi::new(case, scale);
    let theme = line(case, "theme");
    let anchor = (960.0, 1000.0);
    let (form_w, form_h) = (PILL_W, PILL_H);
    let form = (
        anchor.0 - form_w / 2.0,
        anchor.1 - form_h / 2.0,
        anchor.0 + form_w / 2.0,
        anchor.1 + form_h / 2.0,
    );
    let work: WorkArea = (0, 0, 1920, 1080);
    let rect = panel_rect(form, work, scale, None);
    let margin = (MARGIN * scale).round_ties_even() as i32;
    let window = WindowRect {
        x: rect.0.round_ties_even() as i32 - margin,
        y: rect.1.round_ties_even() as i32 - margin,
        width: (rect.2 - rect.0).round_ties_even() as usize + 2 * margin as usize,
        height: (rect.3 - rect.1).round_ties_even() as usize + 2 * margin as usize,
    };
    let mut view = super::view::view(model_of(case));
    view.configure(
        (rect.2 - rect.0) / scale,
        (rect.3 - rect.1) / scale,
        scale,
        &theme,
        ez_hud::tokens::palette(&theme).expect("la paleta del tema"),
    );
    view.set_focus_instant(0.0);
    let state = if flag(case, "closing") {
        AnimState::Closing
    } else {
        AnimState::Opening
    };
    let mut scene = Scene {
        ui: &ui,
        view: &mut view,
        anim: state,
        form,
        act: 0.0,
        now: 0.0,
    };
    let frame = compose_frame(&mut scene, num(case, "p"), false, window);
    compare_png(case, &frame);
}

fn compare_png(case: &FixtureCase, frame: &Frame) {
    let Some(png) = case.output.get("png").and_then(|v| v.as_str()) else {
        return;
    };
    let path = Path::new("../../../parity/golden").join(png);
    let expected = ez_parity::image::decode(&path).expect("PNG dorado");
    let rgba = frame.to_rgba_frame();
    let produced = image::RgbaImage::from_raw(rgba.width, rgba.height, rgba.data)
        .expect("búfer RGBA del panel");
    let delta = ez_parity::image::compare(&produced, &expected);
    let thresholds =
        ez_parity::image::Thresholds::load(Path::new("../../../parity/thresholds.toml"))
            .expect("umbrales")
            .pixels
            .text;
    if !ez_parity::image::within(&delta, &thresholds) {
        super::png_failures()
            .lock()
            .expect("sin envenenar")
            .push(format!(
                "{} ({png}): max={} p995={} p999={} ssim={}",
                case.case, delta.max, delta.p995, delta.p999, delta.ssim
            ));
    }
}
