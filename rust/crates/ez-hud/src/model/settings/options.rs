//! Rangos de los sliders, opciones fijas y paletas del tema.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use super::value::{SelectOption, SliderSpec};

// Escala del medidor de entrada (dBFS): la pista entera es el rango del umbral.
pub const METER_DB_FLOOR: f64 = -80.0;
pub const METER_DB_CEIL: f64 = -20.0;
pub const METER_DB_STEP: f64 = 0.5;

/// Amplitud lineal de un nivel en dBFS.
pub fn db_to_rms(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

// Tamaño de la barra: de 60 % a 200 % del base, en pasos de 5 %.
pub const UI_SCALE_MIN: f64 = 0.6;
pub const UI_SCALE_MAX: f64 = 2.0;
pub const UI_SCALE_STEP: f64 = 0.05;

pub static SLIDERS: LazyLock<BTreeMap<&'static str, SliderSpec>> = LazyLock::new(|| {
    BTreeMap::from([
        (
            "ui_scale",
            SliderSpec::new(UI_SCALE_MIN, UI_SCALE_MAX, UI_SCALE_STEP, 2, " %", false),
        ),
        (
            "min_rms",
            SliderSpec::new(
                db_to_rms(METER_DB_FLOOR),
                db_to_rms(METER_DB_CEIL),
                METER_DB_STEP,
                6,
                "",
                true,
            ),
        ),
        (
            "noise_multiplier",
            SliderSpec::new(1.0, 2.5, 0.1, 1, "×", false),
        ),
        (
            "silence_secs",
            SliderSpec::new(0.8, 3.5, 0.1, 1, " s", false),
        ),
        (
            "turn_threshold",
            SliderSpec::new(0.3, 0.95, 0.05, 2, "", false),
        ),
        (
            "pre_roll_secs",
            SliderSpec::new(0.2, 1.5, 0.1, 1, " s", false),
        ),
        (
            "min_speech_secs",
            SliderSpec::new(0.1, 1.0, 0.05, 2, " s", false),
        ),
        (
            "max_utterance_secs",
            SliderSpec::new(5.0, 120.0, 5.0, 0, " s", false),
        ),
        (
            "barge_in_rms",
            SliderSpec::new(0.001, 0.05, 0.001, 3, "", false),
        ),
        ("speed", SliderSpec::new(0.75, 1.25, 0.05, 2, "×", false)),
        (
            "tts_sentence_pause",
            SliderSpec::new(0.0, 1.0, 0.05, 2, " s", false),
        ),
        (
            "tts_clause_pause",
            SliderSpec::new(0.0, 0.6, 0.02, 2, " s", false),
        ),
        (
            "stt_timeout_secs",
            SliderSpec::new(5.0, 180.0, 5.0, 0, " s", false),
        ),
        (
            "llm_temperature",
            SliderSpec::new(0.0, 1.5, 0.05, 2, "", false),
        ),
        (
            "llm_max_tokens",
            SliderSpec::new(80.0, 1200.0, 20.0, 0, "", false),
        ),
        (
            "llm_context_tokens",
            SliderSpec::new(8000.0, 200_000.0, 4000.0, 0, " tok", false),
        ),
    ])
});

pub static TTS_VARIANTS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    vec![
        SelectOption::text("int8", "int8 · rápida"),
        SelectOption::text("fp32", "fp32 · precisión"),
    ]
});

pub static STT_ENGINE_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    vec![
        SelectOption::text("auto", "Automático"),
        SelectOption::text("vulkan", "GPU · Vulkan"),
        SelectOption::text("cpu", "CPU"),
    ]
});

pub static LLM_REASONING_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    vec![
        SelectOption::text("none", "Sin razonamiento"),
        SelectOption::text("low", "Bajo"),
        SelectOption::text("medium", "Medio"),
        SelectOption::text("high", "Alto"),
    ]
});

pub static FIXED_OPTIONS: LazyLock<BTreeMap<&'static str, Vec<SelectOption>>> =
    LazyLock::new(|| {
        BTreeMap::from([
            (
                "ui_layout",
                vec![
                    SelectOption::text("pill", "Píldora"),
                    SelectOption::text("circle", "Esfera"),
                ],
            ),
            (
                "theme",
                vec![
                    SelectOption::text("auto", "Automático"),
                    SelectOption::text("light", "Claro"),
                    SelectOption::text("dark", "Oscuro"),
                    SelectOption::text("custom", "Personalizado"),
                ],
            ),
            (
                "ui_motion",
                vec![
                    SelectOption::text("system", "Según el sistema"),
                    SelectOption::text("full", "Completo"),
                    SelectOption::text("reduced", "Reducido"),
                ],
            ),
            (
                "close_action",
                vec![
                    SelectOption::text("pause", "Pausa"),
                    SelectOption::text("hangup", "Cuelga"),
                ],
            ),
            (
                "theme_base",
                vec![
                    SelectOption::text("light", "Clara"),
                    SelectOption::text("dark", "Oscura"),
                ],
            ),
            (
                "llm_api_style",
                vec![
                    SelectOption::text("openai", "OpenAI"),
                    SelectOption::text("anthropic", "Anthropic"),
                ],
            ),
        ])
    });

/// Paletas base del tema personalizado: `(nombre, fondo, principal, secundario,
/// acento, etiqueta)`, en el mismo orden que `flow.PRESETS`.
pub(crate) const PRESETS: [(&str, &str, &str, &str, &str, &str); 5] = [
    ("marea", "#0E6F7C", "#1FAE9F", "#8FE3C0", "#FF8A65", "Marea"),
    (
        "atardecer",
        "#5B1A3A",
        "#E0564F",
        "#FFB36B",
        "#FFE08A",
        "Atardecer",
    ),
    (
        "bosque", "#13361F", "#2E7D4F", "#A8D672", "#F2C94C", "Bosque",
    ),
    (
        "lavanda", "#2E2A6B", "#7A6FF0", "#C9B8FF", "#FF9EC4", "Lavanda",
    ),
    (
        "medianoche",
        "#0B1026",
        "#2D3A8C",
        "#5FA8FF",
        "#9BF2FF",
        "Medianoche",
    ),
];

pub(crate) fn preset(
    name: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    PRESETS
        .iter()
        .find(|(key, _, _, _, _, _)| *key == name)
        .map(|(_, background, primary, secondary, accent, _)| {
            (*background, *primary, *secondary, *accent)
        })
}

pub(crate) fn preset_label(name: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|(key, _, _, _, _, _)| *key == name)
        .map(|(_, _, _, _, _, label)| *label)
}
