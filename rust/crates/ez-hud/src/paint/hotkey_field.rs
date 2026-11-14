//! Port of `src/ezagent/voice/hud/hotkey_field.py` (F5-12).
//!
//! Atajos de teclado como campo de captura: pulsas la combinación y se ve en
//! vivo. Lógica pura (sin dibujo ni ventanas): traducir teclas de Windows (VK)
//! a los nombres que entiende `hotkeys.parse_hotkey`, dar el texto que se
//! muestra en cada tecla dibujada, decidir cuándo una combinación está completa
//! y detectar duplicados con los otros atajos.
//!
//! La consulta de los modificadores pulsados (`os_modifiers`) vive en
//! [`crate::win::settings_window`] porque necesita `GetAsyncKeyState`; este
//! módulo no toca Win32.

use ez_core::pyvalue::py_strip;

/// Modificadores en el orden fijo de `voice.json`.
pub const MODIFIERS: [&str; 4] = ["ctrl", "alt", "shift", "win"];

/// Filas de ajustes que son atajos (claves de `voice.json`).
pub const HOTKEY_KEYS: [&str; 6] = [
    "hotkey",
    "cancel_hotkey",
    "repeat_hotkey",
    "mute_hotkey",
    "menu_hotkey",
    "chat_hotkey",
];

/// `MOD_LABELS`: rótulo dibujado de cada modificador.
pub const MOD_LABELS: [(&str, &str); 4] = [
    ("ctrl", "Ctrl"),
    ("alt", "Alt"),
    ("shift", "Mayús"),
    ("win", "Win"),
];

const ALIASES: [(&str, &str); 5] = [
    ("control", "ctrl"),
    ("option", "alt"),
    ("windows", "win"),
    ("super", "win"),
    ("escape", "esc"),
];

const KEY_LABELS: [(&str, &str); 15] = [
    ("space", "Espacio"),
    ("enter", "Intro"),
    ("esc", "Esc"),
    ("tab", "Tab"),
    ("backspace", "Retroceso"),
    ("delete", "Supr"),
    ("insert", "Ins"),
    ("home", "Inicio"),
    ("end", "Fin"),
    ("pageup", "Re Pág"),
    ("pagedown", "Av Pág"),
    ("up", "Arriba"),
    ("down", "Abajo"),
    ("left", "Izquierda"),
    ("right", "Derecha"),
];

const VK_NAMES: [(i32, &str); 14] = [
    (32, "space"),
    (13, "enter"),
    (9, "tab"),
    (8, "backspace"),
    (46, "delete"),
    (45, "insert"),
    (36, "home"),
    (35, "end"),
    (33, "pageup"),
    (34, "pagedown"),
    (38, "up"),
    (40, "down"),
    (37, "left"),
    (39, "right"),
];

/// Shift, Ctrl, Alt (Menu), Win izquierda y derecha, y sus variantes por lado.
const MODIFIER_VKS: [i32; 11] = [16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165];

/// `VK_ESCAPE`.
pub const VK_ESCAPE: i32 = 27;

fn bit(name: &str) -> Option<u8> {
    MODIFIERS
        .iter()
        .position(|other| *other == name)
        .map(|index| 1 << index)
}

/// `frozenset[str]` de modificadores, como máscara en el orden de [`MODIFIERS`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    /// Conjunto vacío.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Conjunto con los nombres dados (los desconocidos se ignoran).
    pub fn from_names(names: &[&str]) -> Self {
        let mut mods = Self::empty();
        for name in names {
            mods.insert(name);
        }
        mods
    }

    /// `mods.add(part)`.
    pub fn insert(&mut self, name: &str) {
        if let Some(flag) = bit(name) {
            self.0 |= flag;
        }
    }

    /// `part in mods`.
    pub fn contains(self, name: &str) -> bool {
        bit(name).is_some_and(|flag| self.0 & flag != 0)
    }

    /// `bool(mods)`.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// `mods - {"shift"}` (el Shift solo no basta para cerrar un atajo).
    pub fn without_shift(self) -> Self {
        Self(self.0 & !bit("shift").unwrap_or(0))
    }

    /// Nombres en el orden fijo de [`MODIFIERS`].
    pub fn iter(self) -> impl Iterator<Item = &'static str> {
        MODIFIERS
            .into_iter()
            .filter(move |name| self.contains(name))
    }
}

/// Nombre de la tecla final para un código VK, o `None` si no vale.
pub fn vk_to_key(vk: i32) -> Option<String> {
    if (65..=90).contains(&vk) {
        return char::from_u32(vk as u32).map(|key| key.to_lowercase().to_string());
    }
    if (48..=57).contains(&vk) {
        return char::from_u32(vk as u32).map(|key| key.to_string());
    }
    if (112..=135).contains(&vk) {
        return Some(format!("f{}", vk - 111));
    }
    VK_NAMES
        .iter()
        .find(|(code, _)| *code == vk)
        .map(|(_, name)| (*name).to_string())
}

/// `is_modifier_vk`: ¿es una tecla modificadora (sola no cierra el atajo)?
pub fn is_modifier_vk(vk: i32) -> bool {
    MODIFIER_VKS.contains(&vk)
}

/// `(modificadores, tecla)` de un atajo como `ctrl+alt+space`; `None` si no vale.
pub fn canonical(spec: &str) -> Option<(Mods, String)> {
    let mut mods = Mods::empty();
    let mut key = String::new();
    for raw in spec.split('+') {
        let lowered = py_strip(raw).to_lowercase();
        let part = ALIASES
            .iter()
            .find(|(alias, _)| *alias == lowered)
            .map_or(lowered.as_str(), |(_, canonical)| *canonical);
        if part.is_empty() {
            continue;
        }
        if MODIFIERS.contains(&part) {
            mods.insert(part);
        } else if !key.is_empty() {
            return None;
        } else {
            key = part.to_string();
        }
    }
    (!key.is_empty()).then_some((mods, key))
}

/// Texto de `voice.json`: modificadores en orden fijo y la tecla, en minúsculas.
pub fn format_spec(mods: Mods, key: &str) -> String {
    let mut parts: Vec<&str> = mods.iter().collect();
    parts.push(key);
    parts.join("+")
}

/// Rótulo de una tecla final: `Espacio`, `F5`, `R`…
pub fn key_label(key: &str) -> String {
    KEY_LABELS
        .iter()
        .find(|(name, _)| *name == key)
        .map_or_else(|| key.to_uppercase(), |(_, label)| (*label).to_string())
}

/// Rótulos de las teclas de un atajo, en el orden en que se dibujan.
pub fn parts_for(spec: &str) -> Vec<String> {
    let Some((mods, key)) = canonical(spec) else {
        return if spec.is_empty() {
            Vec::new()
        } else {
            vec![spec.to_string()]
        };
    };
    let mut parts = mod_labels(mods);
    parts.push(key_label(&key));
    parts
}

/// `mod_labels`: rótulo de cada modificador presente, en orden fijo.
pub fn mod_labels(mods: Mods) -> Vec<String> {
    mods.iter()
        .filter_map(|name| {
            MOD_LABELS
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, label)| (*label).to_string())
        })
        .collect()
}

/// Tipo de [`CaptureEvent`] (`kind` del Python).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    Pending,
    Done,
    Cancel,
    Reject,
}

/// Resultado de pulsar una tecla durante la captura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureEvent {
    pub kind: CaptureKind,
    pub spec: String,
    pub reason: String,
}

impl CaptureEvent {
    /// `CaptureEvent(kind)`.
    pub fn new(kind: CaptureKind) -> Self {
        Self {
            kind,
            spec: String::new(),
            reason: String::new(),
        }
    }

    /// `CaptureEvent("done", spec=...)`.
    pub fn done(spec: String) -> Self {
        Self {
            kind: CaptureKind::Done,
            spec,
            reason: String::new(),
        }
    }

    /// `CaptureEvent("reject", reason=...)`.
    pub fn reject(reason: &str) -> Self {
        Self {
            kind: CaptureKind::Reject,
            spec: String::new(),
            reason: reason.to_string(),
        }
    }
}

/// Decide qué hace una tecla mientras se captura un atajo.
///
/// Esc sin modificadores cancela; los modificadores solos esperan a la tecla
/// final; una tecla final cierra la combinación si es válida (con Ctrl, Alt o
/// Win, salvo las teclas F, que valen solas).
pub fn press(vk: i32, mods: Mods) -> CaptureEvent {
    if vk == VK_ESCAPE && mods.is_empty() {
        return CaptureEvent::new(CaptureKind::Cancel);
    }
    if is_modifier_vk(vk) {
        return CaptureEvent::new(CaptureKind::Pending);
    }
    let Some(key) = vk_to_key(vk) else {
        return CaptureEvent::reject("Esa tecla no se puede usar");
    };
    let is_function = key.strip_prefix('f').is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
    });
    if !is_function && mods.without_shift().is_empty() {
        return CaptureEvent::reject("Combínala con Ctrl, Alt o Win");
    }
    CaptureEvent::done(format_spec(mods, &key))
}

/// Etiqueta del otro atajo que ya usa `spec` (o `None` si está libre).
pub fn conflict(
    spec: &str,
    current: &[(&str, &str)],
    skip: &str,
    labels: &[(&str, &str)],
) -> Option<String> {
    let wanted = canonical(spec)?;
    current
        .iter()
        .find(|(key, other)| *key != skip && canonical(other).as_ref() == Some(&wanted))
        .map(|(key, _)| {
            labels
                .iter()
                .find(|(name, _)| name == key)
                .map_or_else(|| (*key).to_string(), |(_, label)| (*label).to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VK_E: i32 = 69;
    const VK_K: i32 = 75;
    const VK_F8: i32 = 119;
    const VK_SPACE: i32 = 32;
    const VK_CTRL: i32 = 17;
    const VK_SEMICOLON: i32 = 186;

    fn mods(names: &[&str]) -> Mods {
        Mods::from_names(names)
    }

    #[test]
    fn a_modifier_alone_waits_for_the_final_key() {
        let event = press(VK_CTRL, mods(&["ctrl"]));
        assert_eq!(event.kind, CaptureKind::Pending);
        assert_eq!(event.spec, "");
        assert_eq!(event.reason, "");
        assert!(is_modifier_vk(16));
        assert!(is_modifier_vk(165));
        assert!(!is_modifier_vk(VK_E));
    }

    #[test]
    fn esc_cancels_and_cannot_be_recorded() {
        assert_eq!(press(VK_ESCAPE, Mods::empty()).kind, CaptureKind::Cancel);
        let rejected = press(VK_ESCAPE, mods(&["ctrl"]));
        assert_eq!(rejected.kind, CaptureKind::Reject);
        assert_eq!(rejected.reason, "Esa tecla no se puede usar");
    }

    #[test]
    fn a_complete_combination_is_closed_in_a_fixed_order() {
        for (vk, held, spec) in [
            (VK_K, vec!["alt", "ctrl"], "ctrl+alt+k"),
            (VK_SPACE, vec!["alt", "ctrl"], "ctrl+alt+space"),
            (VK_K, vec!["win", "shift", "ctrl"], "ctrl+shift+win+k"),
            (VK_F8, Vec::new(), "f8"),
        ] {
            let event = press(vk, mods(&held));
            assert_eq!((event.kind, event.spec.as_str()), (CaptureKind::Done, spec));
        }
    }

    #[test]
    fn combinations_that_would_break_typing_or_have_no_name_are_rejected() {
        for (vk, held) in [
            (VK_K, Vec::new()),
            (VK_K, vec!["shift"]),
            (VK_SEMICOLON, vec!["ctrl"]),
        ] {
            let event = press(vk, mods(&held));
            assert_eq!(event.kind, CaptureKind::Reject);
            assert!(!event.reason.is_empty());
        }
        assert_eq!(
            press(VK_K, mods(&["shift"])).reason,
            "Combínala con Ctrl, Alt o Win"
        );
    }

    #[test]
    fn default_hotkeys_read_back_as_keycaps() {
        assert_eq!(parts_for("ctrl+alt+space"), ["Ctrl", "Alt", "Espacio"]);
        assert_eq!(parts_for("control+shift+F5"), ["Ctrl", "Mayús", "F5"]);
        assert_eq!(parts_for("a+b"), ["a+b"]);
        assert!(parts_for("").is_empty());
        assert_eq!(key_label("pageup"), "Re Pág");
        assert_eq!(key_label("f5"), "F5");
        assert_eq!(key_label("k"), "K");
        assert_eq!(mod_labels(mods(&["win", "ctrl"])), ["Ctrl", "Win"]);
        assert_eq!(HOTKEY_KEYS.len(), 6);
    }

    #[test]
    fn conflicts_ignore_order_case_and_aliases() {
        let current = [
            ("hotkey", "ctrl+alt+space"),
            ("cancel_hotkey", "ctrl+alt+e"),
        ];
        let labels = [("hotkey", "Llamada"), ("cancel_hotkey", "Colgar")];
        assert_eq!(
            conflict("Alt+Control+E", &current, "hotkey", &labels).as_deref(),
            Some("Colgar")
        );
        assert_eq!(
            conflict("ctrl+alt+e", &current, "cancel_hotkey", &labels),
            None
        );
        assert_eq!(conflict("ctrl+alt+q", &current, "hotkey", &labels), None);
        assert_eq!(conflict("a+b", &current, "hotkey", &labels), None);
        assert_eq!(
            conflict("ctrl+alt+space", &current, "hotkey", &[]).as_deref(),
            None
        );
        assert_eq!(
            conflict("alt+x", &[("mute", "Alt+X")], "", &[]).as_deref(),
            Some("mute")
        );
    }

    #[test]
    fn keys_translate_from_windows_virtual_codes() {
        assert_eq!(vk_to_key(65).as_deref(), Some("a"));
        assert_eq!(vk_to_key(90).as_deref(), Some("z"));
        assert_eq!(vk_to_key(48).as_deref(), Some("0"));
        assert_eq!(vk_to_key(57).as_deref(), Some("9"));
        assert_eq!(vk_to_key(112).as_deref(), Some("f1"));
        assert_eq!(vk_to_key(135).as_deref(), Some("f24"));
        assert_eq!(vk_to_key(VK_SPACE).as_deref(), Some("space"));
        assert_eq!(vk_to_key(39).as_deref(), Some("right"));
        assert_eq!(vk_to_key(VK_ESCAPE), None);
        assert_eq!(vk_to_key(VK_SEMICOLON), None);
    }

    #[test]
    fn canonical_splits_aliases_and_rejects_two_keys() {
        assert_eq!(
            canonical(" CONTROL + Shift + A "),
            Some((mods(&["ctrl", "shift"]), "a".to_string()))
        );
        assert_eq!(
            canonical("option+escape"),
            Some((mods(&["alt"]), "esc".to_string()))
        );
        assert_eq!(canonical("ctrl+a+b"), None);
        assert_eq!(canonical(""), None);
        assert_eq!(canonical("control"), None);
        assert_eq!(canonical("+"), None);
        assert_eq!(format_spec(mods(&["win", "alt"]), "tab"), "alt+win+tab");
    }

    #[test]
    fn every_recorded_key_parses_back_to_itself() {
        let mut sample: Vec<i32> = (65..=90).chain(48..=57).chain(112..136).collect();
        sample.extend([32, 13, 9, 8, 46, 45, 36, 35, 33, 34, 38, 40, 37, 39]);
        for vk in sample {
            let event = press(vk, mods(&["ctrl", "alt"]));
            assert_eq!(event.kind, CaptureKind::Done, "vk {vk}");
            let (held, key) = canonical(&event.spec).expect("se relee");
            assert_eq!(key, vk_to_key(vk).expect("tiene nombre"), "vk {vk}");
            assert!(held.contains("ctrl") && held.contains("alt"));
        }
    }
}
