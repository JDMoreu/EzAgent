//! Port of `run_demo` (`app.py` 3873-3984) (F6-07).
//!
//! Recorre los estados del HUD sin micrófono, whisper, Kokoro ni Herdr. No
//! crea una app real: el menú devuelve siempre los mismos ajustes de ejemplo.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::config::VoiceConfig;
use ez_hud::model::settings::{MenuSettings, OptionSource, SettingValue, SettingsValues};
use ez_hud::win::drop::{create_ui, DropHost, DropUI, MenuSettingsProvider};

use super::core::{cfg_text, MENU_VOICES};
use super::live::LiveIcons;

/// Pasos de la demo, en orden (`steps` del Python).
pub const DEMO_STEPS: [&str; 14] = [
    "open", "idle", "listen", "think", "speak", "notice", "mute", "idle", "error", "idle", "menu",
    "menu", "idle", "close",
];

/// Duración de cada paso (`root.after(2000, step)`).
const STEP_SECS: f64 = 2.0;
/// Cuadro de la animación (`root.after(33, tick)`).
const FRAME_SECS: f64 = 0.033;

/// Host mínimo de la demo: guardar ajustes siempre funciona.
struct DemoHost;

impl DropHost for DemoHost {
    fn save_settings(&self, _values: &SettingsValues) -> Option<bool> {
        Some(true)
    }
}

/// Ajustes de ejemplo de la demo (`MenuSettings.from_app`).
fn demo_settings() -> MenuSettings {
    let mut values = SettingsValues::new();
    values.insert("input_device_index", SettingValue::None);
    values.insert("min_rms", SettingValue::Float(0.0006));
    values.insert("noise_multiplier", SettingValue::Float(1.3));
    values.insert("silence_secs", SettingValue::Float(1.3));
    values.insert("voice", SettingValue::Text("em_alex".to_string()));
    values.insert("speed", SettingValue::Float(0.95));
    values.insert("barge_in", SettingValue::Bool(true));
    values.insert("confirm_actions", SettingValue::Bool(false));
    let voices = OptionSource::Pairs(
        MENU_VOICES
            .iter()
            .map(|(id, label)| ((*id).to_string(), (*label).to_string()))
            .collect(),
    );
    MenuSettings::from_app(&values, &[], Some(&voices), &[], None, None)
}

/// `enter(state)`: la pose de cada paso.
fn enter(hud: &DropUI, state: &str) {
    match state {
        "open" => hud.show(super::core::IDLE),
        "close" => hud.hide(),
        "menu" => hud.toggle_menu(),
        "notice" => {
            hud.set_muted(false);
            hud.notice();
        }
        "mute" => {
            hud.set_muted(true);
            hud.set_state(super::core::IDLE);
        }
        "error" => hud.set_state("error"),
        other => {
            let state = match other {
                "listen" => super::core::LISTENING,
                "think" => super::core::THINKING,
                "speak" => super::core::SPEAKING,
                _ => super::core::IDLE,
            };
            hud.set_muted(false);
            hud.set_state(state);
        }
    }
}

/// Estado que anima el medidor en cada paso (`tick`).
fn animated_state(index: usize) -> &'static str {
    if index == 0 {
        return "idle";
    }
    match DEMO_STEPS[index.min(DEMO_STEPS.len()) - 1] {
        "listen" => "listen",
        "speak" => "speak",
        _ => "idle",
    }
}

/// `run_demo`: barra, menú y estados animados, sin servicios.
pub fn run_demo() {
    let hud = create_ui(
        Arc::new(DemoHost),
        None,
        Some(Arc::new(LiveIcons) as Arc<dyn ez_hud::paint::peek::IconPainter>),
    );
    let theme = cfg_text(&VoiceConfig::new(), "theme", "auto");
    hud.set_theme(&theme);
    let provider: MenuSettingsProvider = Arc::new(|| Some(demo_settings()));
    hud.set_menu_settings_provider(Some(provider));
    run_loop(&hud);
    hud.close();
}

/// El bucle de `step` y `tick`, con el reloj real.
fn run_loop(hud: &DropUI) {
    let started = Instant::now();
    let mut index = 0usize;
    loop {
        let elapsed = started.elapsed().as_secs_f64();
        while index < DEMO_STEPS.len() && elapsed >= index as f64 * STEP_SECS {
            enter(hud, DEMO_STEPS[index]);
            index += 1;
        }
        if index >= DEMO_STEPS.len() && elapsed >= DEMO_STEPS.len() as f64 * STEP_SECS {
            return;
        }
        let state = animated_state(index);
        if state == "listen" || state == "speak" {
            let t = elapsed;
            let rms = 0.004 + 0.016 * (0.5 + 0.5 * (t * 5.1).sin());
            let shown = if state == "listen" {
                super::core::LISTENING
            } else {
                super::core::SPEAKING
            };
            hud.draw(rms, Some(shown));
        }
        std::thread::sleep(Duration::from_secs_f64(FRAME_SECS));
    }
}

#[cfg(test)]
mod tests;
