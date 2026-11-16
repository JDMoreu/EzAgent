//! Port of `src/ezagent/voice/hud/menu_layout.py` (F2-12).
//!
//! Layout vertical del menú del HUD (tabla 7.3). Todas las medidas están en
//! píxeles lógicos a 96 DPI. El layout devuelve rectángulos en coordenadas de
//! ventana: `y = 0` es el borde superior de la isla.
//!
//! Las claves de fila son las claves reales de `voice.json` (RG-3). El menú
//! cubre audio, VAD, voz/TTS, Whisper/STT, LLM/API, conversación y atajos; el
//! contenido que no cabe en el alto visible se desplaza con la rueda.
//! `test_voice`, `llm_profile` y `llm_model_pick` son controles del menú, no
//! claves persistidas.

use std::collections::BTreeMap;

use crate::tokens::{H_ISLAND, H_MENU};

pub const ISLAND_H: i32 = H_ISLAND;
pub const PAD_TOP: i32 = 10;
pub const HEADER: i32 = 32;
pub const GAP_HEADER: i32 = 10;
pub const LABEL: i32 = 22;
pub const GROUP_GAP: i32 = 14;
pub const FOOTER: i32 = 48;
pub const PAD_BOTTOM: i32 = 10;

/// Rejilla de filas: 44 (una línea) y 52 (dos líneas: etiqueta y control).
pub const ROW_SELECT: i32 = 44;
pub const ROW_SLIDER: i32 = 52;
pub const ROW_TOGGLE: i32 = 44;
pub const ROW_TEXT: i32 = 52;
pub const ROW_METER: i32 = 52;
pub const ROW_ACTION: i32 = 44;
/// Vista de las paletas (Claro, Oscuro, Personalizado).
pub const ROW_PALETTE: i32 = 68;

/// `ROW`: alto base de cada tipo de fila.
pub const ROW: [(&str, i32); 7] = [
    ("select", ROW_SELECT),
    ("slider", ROW_SLIDER),
    ("toggle", ROW_TOGGLE),
    ("text", ROW_TEXT),
    ("meter", ROW_METER),
    ("action", ROW_ACTION),
    ("palette", ROW_PALETTE),
];

/// Alto base de un tipo de fila (`KeyError` convertido en `None`).
pub fn row_height(kind: &str) -> Option<i32> {
    ROW.iter()
        .find(|(key, _)| *key == kind)
        .map(|(_, height)| *height)
}

/// Una fila: `(clave de voice.json, tipo)`.
pub type Row = (&'static str, &'static str);
/// Un grupo: `(id, filas)`.
pub type Group = (&'static str, &'static [Row]);
/// Una sección de la barra lateral: `(id, título, grupos)`.
pub type Section = (&'static str, &'static str, &'static [&'static str]);

// Grupos de ajustes. Cada grupo es una tarjeta con su etiqueta; varias tarjetas
// forman una sección de la barra lateral (ver `SECTIONS`). Las filas de
// `ADVANCED_ROWS` solo se ven con «Opciones avanzadas» y las de
// `CUSTOM_THEME_ROWS` solo con el tema «Personalizado».
pub const GROUPS: &[Group] = &[
    (
        "look",
        &[
            ("ui_layout", "select"),
            ("ui_scale", "slider"),
            ("ui_motion", "select"),
        ],
    ),
    (
        "theme",
        &[
            ("theme", "select"),
            ("theme_preview", "palette"),
            // Solo con el tema «Personalizado» (el menú las oculta si no):
            ("theme_base", "select"),
            ("theme_preset", "select"),
            ("theme_primary", "text"),
            ("theme_secondary", "text"),
            ("theme_accent", "text"),
            ("theme_background", "text"),
        ],
    ),
    (
        "mic",
        &[
            ("input_device_index", "select"),
            ("input_meter", "meter"),
            ("noise_multiplier", "slider"),
        ],
    ),
    (
        // Cuándo se da por terminada tu frase.
        "turn",
        &[("silence_secs", "slider"), ("smart_turn", "toggle")],
    ),
    (
        "wake",
        &[
            ("voice_wake", "toggle"),
            ("wake_phrase", "text"),
            ("close_action", "select"),
        ],
    ),
    ("speech", &[("voice", "select"), ("speed", "slider")]),
    (
        "dialog",
        &[
            ("barge_in", "toggle"),
            ("confirm_actions", "toggle"),
            ("chat_voice", "toggle"),
        ],
    ),
    (
        "actions",
        &[
            ("test_voice", "action"),
            ("repeat_last", "action"),
            ("new_conversation", "action"),
        ],
    ),
    (
        "stt",
        &[
            ("stt_model", "select"),
            ("auto_start_whisper", "toggle"),
            ("stt_dictionary", "text"),
        ],
    ),
    (
        "llm",
        &[
            // Configuración guardada -> proveedor -> modelo -> conexión -> guardar.
            ("llm_saved_profile", "select"),
            ("llm_profile", "select"),
            ("llm_model_pick", "select"),
            ("llm_model", "text"),
            ("api_key", "text"),
            ("llm_api_style", "select"),
            ("llm_base_url", "text"),
            ("llm_profile_name", "text"),
        ],
    ),
    (
        "hotkeys",
        &[
            ("hotkey", "text"),
            ("cancel_hotkey", "text"),
            ("repeat_hotkey", "text"),
            ("mute_hotkey", "text"),
            ("menu_hotkey", "text"),
            ("chat_hotkey", "text"),
        ],
    ),
    ("advanced", &[("show_advanced", "toggle")]),
    (
        // Ajustes finos: viven aquí, justo debajo del interruptor, para que al
        // activarlo se vea lo que aparece.
        "adv_listen",
        &[
            ("turn_threshold", "slider"),
            ("pre_roll_secs", "slider"),
            ("min_speech_secs", "slider"),
            ("max_utterance_secs", "slider"),
        ],
    ),
    (
        "adv_voice",
        &[
            ("tts_model_variant", "select"),
            ("tts_sentence_pause", "slider"),
            ("tts_clause_pause", "slider"),
            ("barge_in_rms", "slider"),
        ],
    ),
    (
        "adv_stt",
        &[
            ("stt_engine", "select"),
            ("stt_prompt", "text"),
            ("stt_timeout_secs", "slider"),
        ],
    ),
    (
        // Conexión con el proveedor y muestreo del modelo.
        "adv_llm",
        &[
            ("llm_provider", "text"),
            ("llm_temperature", "slider"),
            ("llm_max_tokens", "slider"),
            ("llm_context_tokens", "slider"),
            ("llm_reasoning_effort", "select"),
        ],
    ),
    (
        // Los archivos que la app lee de verdad, en %APPDATA%\EzAgent.
        "files",
        &[
            ("open_config_dir", "action"),
            ("open_system_md", "action"),
            ("open_permissions", "action"),
            ("open_voice_json", "action"),
        ],
    ),
];

/// Filas que solo se ven con el tema personalizado.
pub const CUSTOM_THEME_ROWS: &[&str] = &[
    "theme_base",
    "theme_preset",
    "theme_primary",
    "theme_secondary",
    "theme_accent",
    "theme_background",
];

/// Filas poco habituales: se ocultan hasta que el usuario pide verlas
/// (`show_advanced`); sus valores siguen aplicándose igual.
pub const ADVANCED_ROWS: &[&str] = &[
    "turn_threshold",
    "pre_roll_secs",
    "min_speech_secs",
    "max_utterance_secs",
    "tts_model_variant",
    "tts_sentence_pause",
    "tts_clause_pause",
    "barge_in_rms",
    "stt_engine",
    "stt_prompt",
    "stt_timeout_secs",
    "llm_provider",
    "llm_temperature",
    "llm_max_tokens",
    "llm_context_tokens",
    "llm_reasoning_effort",
];

/// Secciones de la barra lateral: `(id, título, grupos)`.
pub const SECTIONS: &[Section] = &[
    ("appearance", "Aspecto", &["look", "theme"]),
    ("listening", "Escucha", &["mic", "turn", "wake"]),
    ("voice", "Voz", &["speech", "dialog", "actions"]),
    ("models", "Modelos", &["stt", "llm"]),
    (
        "system",
        "Sistema",
        &[
            "hotkeys",
            "advanced",
            "adv_listen",
            "adv_voice",
            "adv_stt",
            "adv_llm",
            "files",
        ],
    ),
];

/// Orden de la barra lateral de la ventana de ajustes.
pub const SIDEBAR_ORDER: [&str; 5] = ["appearance", "listening", "voice", "models", "system"];

/// `SECTION_TITLES[name]`.
pub fn section_title(name: &str) -> Option<&'static str> {
    SECTIONS
        .iter()
        .find(|(id, _, _)| *id == name)
        .map(|(_, title, _)| *title)
}

/// `SECTION_GROUPS[name]`.
pub fn section_groups(name: &str) -> Option<&'static [&'static str]> {
    SECTIONS
        .iter()
        .find(|(id, _, _)| *id == name)
        .map(|(_, _, groups)| *groups)
}

/// `GROUP_LABELS[group]`.
pub const GROUP_LABELS: [(&str, &str); 17] = [
    ("look", "FORMA Y TAMAÑO"),
    ("theme", "TEMA"),
    ("mic", "MICRÓFONO"),
    ("turn", "FIN DE TURNO"),
    ("wake", "REACTIVACIÓN"),
    ("speech", "VOZ · KOKORO"),
    ("dialog", "CONVERSACIÓN"),
    ("actions", "ACCIONES"),
    ("stt", "TRANSCRIPCIÓN · WHISPER"),
    ("llm", "MODELO DE IA"),
    ("hotkeys", "ATAJOS"),
    ("advanced", "AVANZADO"),
    ("adv_listen", "ESCUCHA · AFINADO"),
    ("adv_voice", "VOZ · AFINADO"),
    ("adv_stt", "TRANSCRIPCIÓN · AFINADO"),
    ("adv_llm", "MODELO DE IA · CONEXIÓN"),
    ("files", "ARCHIVOS"),
];

/// `GROUP_LABELS[group]`, con `KeyError` convertido en `None`.
pub fn group_label(group: &str) -> Option<&'static str> {
    GROUP_LABELS
        .iter()
        .find(|(key, _)| *key == group)
        .map(|(_, label)| *label)
}

pub const MENU_HEIGHT: i32 = H_MENU;

/// Rectángulo del layout del menú.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center_y(&self) -> f64 {
        (self.y0 + self.y1) / 2.0
    }
}

fn insert(out: &mut BTreeMap<String, Rect>, key: &str, y0: f64, y1: f64) {
    out.insert(key.to_string(), Rect::new(0.0, y0, 0.0, y1));
}

/// Devuelve el layout de la tabla 7.3.
///
/// `extra` añade altura a una fila (por ejemplo, un selector desplegado:
/// `n * 32`). `viewport_h` es el alto visible del menú; si es menor que
/// `MENU_HEIGHT`, el pie se sube y el contenido entre la cabecera y el pie se
/// convierte en zona con scroll (C-11). Sin `viewport_h`, el resultado coincide
/// exactamente con la tabla 7.3.
pub fn layout_menu(
    extra: Option<&BTreeMap<String, f64>>,
    viewport_h: Option<f64>,
    groups: Option<&[Group]>,
) -> BTreeMap<String, Rect> {
    // `groups or GROUPS` y `extra or {}` del Python: un grupo vacío es falsy.
    let groups = groups.filter(|groups| !groups.is_empty()).unwrap_or(GROUPS);
    let mut out = BTreeMap::new();
    let mut y = ISLAND_H as f64;
    y += PAD_TOP as f64;
    insert(&mut out, "header", y, y + HEADER as f64);
    y += HEADER as f64 + GAP_HEADER as f64;
    for &(group_name, rows) in groups {
        insert(
            &mut out,
            &format!("label_{group_name}"),
            y,
            y + LABEL as f64,
        );
        y += LABEL as f64;
        let group_top = y;
        for &(key, kind) in rows.iter() {
            let base =
                row_height(kind).unwrap_or_else(|| panic!("tipo de fila desconocido: {kind}"));
            let extra_height = extra
                .and_then(|values| values.get(key).copied())
                .unwrap_or(0.0);
            let height = base as f64 + extra_height;
            out.insert(key.to_string(), Rect::new(0.0, y, 0.0, y + height));
            y += height;
        }
        insert(&mut out, &format!("group_{group_name}"), group_top, y);
        y += GROUP_GAP as f64;
    }
    let mut footer_top = y;
    let mut footer_bottom = y + FOOTER as f64;
    if let Some(viewport) = viewport_h {
        // El pie va siempre al fondo de lo visible: sube si el contenido no
        // cabe y baja si sobra sitio (la ventana de ajustes tiene alto fijo).
        footer_bottom = viewport - PAD_BOTTOM as f64;
        footer_top = footer_bottom - FOOTER as f64;
    }
    insert(&mut out, "footer", footer_top, footer_bottom);
    y = footer_bottom + PAD_BOTTOM as f64;
    insert(&mut out, "total", 0.0, y);
    out
}

/// Alto visible del menú: `min(560, 0,7 · alto de pantalla − 36)`.
pub fn visible_menu_height(screen_h: f64) -> f64 {
    (MENU_HEIGHT as f64).min((0.7 * screen_h - ISLAND_H as f64).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_seven_three_anchors_are_exact() {
        let layout = layout_menu(None, None, None);
        assert_eq!(layout.len(), 102);
        assert_eq!(layout["header"], Rect::new(0.0, 46.0, 0.0, 78.0));
        assert_eq!(layout["label_look"], Rect::new(0.0, 88.0, 0.0, 110.0));
        assert_eq!(layout["ui_layout"], Rect::new(0.0, 110.0, 0.0, 154.0));
        assert_eq!(layout["group_look"], Rect::new(0.0, 110.0, 0.0, 250.0));
        assert_eq!(layout["theme_preview"], Rect::new(0.0, 330.0, 0.0, 398.0));
        assert_eq!(layout["group_files"], Rect::new(0.0, 3666.0, 0.0, 3842.0));
        assert_eq!(layout["footer"], Rect::new(0.0, 3856.0, 0.0, 3904.0));
        assert_eq!(layout["total"], Rect::new(0.0, 0.0, 0.0, 3914.0));
        assert_eq!(layout["footer"].height(), FOOTER as f64);
        assert_eq!(layout["group_look"].center_y(), 180.0);
    }

    #[test]
    fn extra_height_stretches_a_row() {
        let extra = BTreeMap::from([("ui_layout".to_string(), 64.0)]);
        let layout = layout_menu(Some(&extra), None, None);
        assert_eq!(layout["ui_layout"], Rect::new(0.0, 110.0, 0.0, 218.0));
        assert_eq!(layout["total"].y1, 3978.0);
    }

    #[test]
    fn a_small_viewport_pins_the_footer_to_the_bottom() {
        let layout = layout_menu(None, Some(600.0), None);
        assert_eq!(layout["footer"], Rect::new(0.0, 542.0, 0.0, 590.0));
        assert_eq!(layout["total"].y1, 600.0);
        let tall = layout_menu(None, Some(4000.0), None);
        assert_eq!(tall["footer"], Rect::new(0.0, 3942.0, 0.0, 3990.0));
        assert_eq!(tall["total"].y1, 4000.0);
    }

    #[test]
    fn custom_groups_replace_the_default_table() {
        let rows: &[Row] = &[("theme", "select")];
        let groups: &[Group] = &[("theme", rows)];
        let layout = layout_menu(None, None, Some(groups));
        assert_eq!(layout["theme"], Rect::new(0.0, 110.0, 0.0, 154.0));
        assert_eq!(layout["group_theme"].y1, 154.0);
        assert!(!layout.contains_key("header_x"));
        assert_eq!(layout["footer"].y0, 168.0);
        // `groups or GROUPS`: una lista vacía usa la tabla completa.
        assert_eq!(layout_menu(None, None, Some(&[])).len(), 102);
    }

    #[test]
    fn visible_height_never_exceeds_the_menu() {
        assert_eq!(visible_menu_height(1080.0), 560.0);
        assert_eq!(visible_menu_height(600.0), 384.0);
        assert_eq!(visible_menu_height(0.0), 0.0);
        assert_eq!(visible_menu_height(-100.0), 0.0);
    }

    #[test]
    fn sections_labels_and_rows_are_the_python_tables() {
        assert_eq!(SIDEBAR_ORDER.len(), 5);
        assert_eq!(section_title("appearance"), Some("Aspecto"));
        assert_eq!(section_title("nope"), None);
        assert_eq!(
            section_groups("voice"),
            Some(&["speech", "dialog", "actions"][..])
        );
        assert_eq!(group_label("llm"), Some("MODELO DE IA"));
        assert_eq!(group_label("nope"), None);
        assert_eq!(
            section_groups("system"),
            Some(
                &[
                    "hotkeys",
                    "advanced",
                    "adv_listen",
                    "adv_voice",
                    "adv_stt",
                    "adv_llm",
                    "files"
                ][..]
            )
        );
        let files = GROUPS
            .iter()
            .find(|(name, _)| *name == "files")
            .map(|(_, rows)| *rows)
            .expect("grupo files");
        let actions: Vec<&str> = files.iter().map(|(key, _kind)| *key).collect();
        assert_eq!(
            actions,
            [
                "open_config_dir",
                "open_system_md",
                "open_permissions",
                "open_voice_json"
            ]
        );
        for action in actions {
            assert!(crate::model::menu::geometry::ACTION_KEYS.contains(&action));
            assert!(crate::model::settings::LABELS.get(action).is_some());
        }
        assert!(group_label("files").is_some());
        assert_eq!(GROUPS.len(), 17);
        assert_eq!(row_height("palette"), Some(68));
        assert_eq!(row_height("nope"), None);
        assert_eq!(CUSTOM_THEME_ROWS.len(), 6);
        assert_eq!(ADVANCED_ROWS.len(), 16);
        assert_eq!(MENU_HEIGHT, 560);
    }
}
