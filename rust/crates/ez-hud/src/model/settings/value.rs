//! Valores, errores y utilidades de cadena del modelo de ajustes.

use std::fmt;

use ez_core::pyjson::float_repr;
use ez_core::pyvalue::{py_strip, repr_str, tail_chars};

/// Un valor de ajuste: los mismos tipos que produce `settings.py`.
// DECISION: los ajustes solo guardan None/bool/int/float/str; este enum evita
// depender de `serde_json`, que `ez-hud` no declara.
#[derive(Clone, Debug)]
pub enum SettingValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl SettingValue {
    /// `bool(value)`.
    pub fn truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(flag) => *flag,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::Text(text) => !text.is_empty(),
        }
    }

    /// `str(value)`.
    pub fn python_str(&self) -> String {
        match self {
            Self::None => "None".into(),
            Self::Bool(true) => "True".into(),
            Self::Bool(false) => "False".into(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => float_repr(*value),
            Self::Text(text) => text.clone(),
        }
    }

    /// Texto ya normalizado, si el valor es una cadena.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// `a == b` de Python entre los tipos del modelo (`1 == 1.0 == True`).
impl PartialEq for SettingValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::Text(left), Self::Text(right)) => left == right,
            (Self::Int(left), Self::Float(right)) | (Self::Float(right), Self::Int(left)) => {
                *left as f64 == *right
            }
            (Self::Bool(left), Self::Int(right)) | (Self::Int(right), Self::Bool(left)) => {
                i64::from(*left) == *right
            }
            (Self::Bool(left), Self::Float(right)) | (Self::Float(right), Self::Bool(left)) => {
                f64::from(u8::from(*left)) == *right
            }
            _ => false,
        }
    }
}

impl From<&str> for SettingValue {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl From<String> for SettingValue {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<bool> for SettingValue {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<i64> for SettingValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for SettingValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// Constructor corto para tablas y pruebas.
pub fn setting_value<T: Into<SettingValue>>(value: T) -> SettingValue {
    value.into()
}

/// `dict` ordenado de ajustes (la clave conserva su posición al reemplazarse).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsValues {
    entries: Vec<(String, SettingValue)>,
}

impl SettingsValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: SettingValue) -> Option<SettingValue> {
        let key = key.into();
        if let Some(slot) = self.entries.iter_mut().find(|(name, _)| *name == key) {
            return Some(std::mem::replace(&mut slot.1, value));
        }
        self.entries.push((key, value));
        None
    }

    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &SettingValue)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl FromIterator<(String, SettingValue)> for SettingsValues {
    fn from_iter<T: IntoIterator<Item = (String, SettingValue)>>(iter: T) -> Self {
        let mut values = Self::new();
        for (key, value) in iter {
            values.insert(key, value);
        }
        values
    }
}

/// Errores con los mismos textos que los `KeyError`/`ValueError` del Python.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsError {
    UnknownKey(String),
    NotSelector(String),
    ProfileNameTooLong,
    Invalid(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey(key) => {
                write!(formatter, "ajuste desconocido: {}", repr_str(key))
            }
            Self::NotSelector(key) => write!(formatter, "{} no es un selector", repr_str(key)),
            Self::ProfileNameTooLong => {
                formatter.write_str("El nombre de la configuración admite hasta 80 caracteres.")
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Una opción de un selector en línea.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectOption {
    pub value: SettingValue,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: SettingValue, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }

    pub fn text(value: &str, label: &str) -> Self {
        Self::new(SettingValue::Text(value.to_string()), label)
    }
}

/// Rango cerrado, paso y formato de un slider.
///
/// Con `log` el valor es una amplitud lineal y `step` se mide en dB: el valor
/// se redondea en la escala de decibelios (`minimum`/`maximum` siguen siendo
/// amplitudes), que es la escala en la que se dibuja y se arrastra el medidor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderSpec {
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
    pub decimals: usize,
    pub suffix: &'static str,
    pub log: bool,
}

impl SliderSpec {
    pub const fn new(
        minimum: f64,
        maximum: f64,
        step: f64,
        decimals: usize,
        suffix: &'static str,
        log: bool,
    ) -> Self {
        Self {
            minimum,
            maximum,
            step,
            decimals,
            suffix,
            log,
        }
    }

    /// `SliderSpec(minimum, maximum, step, decimals)` con los dos valores por defecto.
    pub const fn plain(minimum: f64, maximum: f64, step: f64, decimals: usize) -> Self {
        Self::new(minimum, maximum, step, decimals, "", false)
    }
}

/// `float(value)` para los tipos del modelo.
pub(crate) fn float_of(value: &SettingValue) -> Option<f64> {
    match value {
        SettingValue::None => None,
        SettingValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        SettingValue::Int(number) => Some(*number as f64),
        SettingValue::Float(number) => Some(*number),
        SettingValue::Text(text) => parse_float(text),
    }
}

/// `int(value)` para los tipos del modelo.
pub(crate) fn int_of(value: &SettingValue) -> Option<i64> {
    match value {
        SettingValue::None => None,
        SettingValue::Bool(flag) => Some(i64::from(*flag)),
        SettingValue::Int(number) => Some(*number),
        SettingValue::Float(number) if number.is_finite() => Some(number.trunc() as i64),
        SettingValue::Float(_) => None,
        SettingValue::Text(text) => ez_core::config::python_int(text),
    }
}

/// `str(value or "")`.
pub(crate) fn text_or_empty(value: Option<&SettingValue>) -> String {
    match value {
        Some(value) if value.truthy() => value.python_str(),
        _ => String::new(),
    }
}

/// `value or fallback`.
pub(crate) fn or_default(value: Option<&SettingValue>, fallback: &SettingValue) -> SettingValue {
    match value {
        Some(value) if value.truthy() => value.clone(),
        _ => fallback.clone(),
    }
}

/// `round(value, decimals)`: el redondeo decimal de Python (a par) sobre el
/// valor binario, vía el formateador de Rust, que redondea igual.
pub(crate) fn round_decimals(value: f64, decimals: usize) -> f64 {
    format!("{value:.decimals$}")
        .parse::<f64>()
        .unwrap_or(value)
}

/// `f"{value:.{decimals}f}"` con coma decimal.
pub(crate) fn decimal(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}

/// `float(text)` de Python acepta `inf`, `infinity`, `nan`, signos, guiones
/// bajos entre dígitos y exponente; los espacios Unicode se recortan.
pub(crate) fn parse_float(text: &str) -> Option<f64> {
    let trimmed = py_strip(text);
    let (negative, body) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let lower = body.to_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if lower == "nan" {
        return Some(f64::NAN);
    }
    let cleaned = cleaned_float(body)?;
    let signed = if negative {
        format!("-{cleaned}")
    } else {
        cleaned
    };
    signed.parse::<f64>().ok()
}

/// Valida la gramática de `float()` y quita los guiones bajos.
fn cleaned_float(body: &str) -> Option<String> {
    let mut out = String::new();
    let mut mantissa_digits = 0usize;
    let mut exponent_digits = 0usize;
    let mut last_was_digit = false;
    let mut seen_dot = false;
    let mut seen_exponent = false;
    let mut chars = body.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '_' => {
                if !last_was_digit {
                    return None;
                }
                last_was_digit = false;
            }
            '0'..='9' => {
                out.push(character);
                if seen_exponent {
                    exponent_digits += 1;
                } else {
                    mantissa_digits += 1;
                }
                last_was_digit = true;
            }
            '.' => {
                if seen_dot || seen_exponent {
                    return None;
                }
                seen_dot = true;
                out.push('.');
                last_was_digit = false;
            }
            'e' | 'E' => {
                if seen_exponent || mantissa_digits == 0 {
                    return None;
                }
                seen_exponent = true;
                out.push('e');
                last_was_digit = false;
                if matches!(chars.peek(), Some('+') | Some('-')) {
                    out.push(chars.next()?);
                }
            }
            _ => return None,
        }
    }
    if mantissa_digits == 0 {
        return None;
    }
    if seen_exponent {
        if exponent_digits == 0 || !last_was_digit {
            return None;
        }
    } else if !last_was_digit && !(seen_dot && out.ends_with('.')) {
        return None;
    }
    Some(out)
}

/// Devuelve una clave enmascarada; nunca pinta el valor completo.
pub fn mask_secret(value: &SettingValue) -> String {
    let text = text_or_empty(Some(value));
    if text.is_empty() {
        return "Sin clave".into();
    }
    let tail = if text.chars().count() >= 8 {
        tail_chars(&text, 4)
    } else {
        ""
    };
    if tail.is_empty() {
        "Configurada".into()
    } else {
        format!("Configurada · ····{tail}")
    }
}
