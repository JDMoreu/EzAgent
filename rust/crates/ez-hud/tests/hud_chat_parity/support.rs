//! Utilidades compartidas por las pruebas de `hud_chat`.

use std::path::{Path, PathBuf};

use ez_core::pyjson::float_repr;
use ez_gfx::Image;
use ez_hud::paint::chat::{Block, ChatHost, FileInfo};
use ez_parity::fixtures::FixtureCase;

pub(super) fn s(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn o(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{}: {value}", s(key)))
        .collect();
    format!("{{{}}}", body.join(", "))
}

pub(super) fn a(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

pub(super) fn b(flag: bool) -> String {
    flag.to_string()
}

pub(super) fn i(number: i64) -> String {
    number.to_string()
}

pub(super) fn f(number: f64) -> String {
    float_repr(number)
}

fn normalize_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c.is_ascii_digit()
            || (c == '-' && index + 1 < chars.len() && chars[index + 1].is_ascii_digit())
        {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'e' | 'E' | '+' | '-'))
            {
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            if let Ok(value) = token.parse::<f64>() {
                out.push_str(&format!("{value:.12e}"));
                continue;
            }
            out.push_str(&token);
        } else {
            out.push(c);
            index += 1;
        }
    }
    out
}

pub(super) fn compare(case: &FixtureCase, value: String) {
    compare_with(case, value, None);
}

/// Igual que [`compare`], sustituyendo `<tmp>` del oráculo por `root` (ya
/// escapado como cadena JSON sin comillas).
pub(super) fn compare_with(case: &FixtureCase, value: String, root: Option<&str>) {
    let mut options = ez_core::pyjson::DumpOptions::utf8();
    options.sort_keys = true;
    let expected = ez_core::pyjson::dumps_with(&case.output, &options);
    same(&case.case, &expected, &value, root);
}

/// Compara dos documentos JSON con números normalizados.
pub(super) fn same(label: &str, expected: &str, produced: &str, root: Option<&str>) {
    let mut options = ez_core::pyjson::DumpOptions::utf8();
    options.sort_keys = true;
    let expected = match root {
        Some(root) => {
            let escaped = s(root);
            expected.replace("<tmp>", &escaped[1..escaped.len() - 1])
        }
        None => expected.to_string(),
    };
    let expected = ez_core::pyloads::loads(&expected).expect("JSON esperado válido");
    let expected = ez_core::pyjson::dumps_with(&expected, &options);
    let produced = ez_core::pyloads::loads(produced).expect("JSON producido válido");
    let produced = ez_core::pyjson::dumps_with(&produced, &options);
    let expected = normalize_numbers(&expected);
    let actual = normalize_numbers(&produced);
    assert!(
        expected == actual,
        "{label}:\n  esperado: {expected}\n  producido: {actual}"
    );
}

/// Sonda de `ChatHost` que no parsea ni encuentra archivos.
pub(super) struct EmptyHost;

impl ChatHost for EmptyHost {
    fn parse(&self, _: &str) -> Vec<Block> {
        Vec::new()
    }

    fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

pub(super) fn num(case: &FixtureCase, key: &str) -> f64 {
    case.input.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

pub(super) fn int(case: &FixtureCase, key: &str) -> i64 {
    case.input.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

pub(super) fn flag(case: &FixtureCase, key: &str) -> bool {
    case.input
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub(super) fn line(case: &FixtureCase, key: &str) -> String {
    case.input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Instante que `time.localtime` devuelve con esa hora de pared (zona local).
pub(super) fn epoch_from_wall(wall: &[i64]) -> f64 {
    if wall.len() < 6 {
        return 0.0;
    }
    let (year, month, day, hour, minute, second) =
        (wall[0], wall[1], wall[2], wall[3], wall[4], wall[5]);
    let mut guess =
        (days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second) as f64;
    for _ in 0..5 {
        let iso = ez_core::pytime::local_iso_seconds(guess.trunc()).unwrap_or_default();
        let parts: Vec<i64> = iso
            .split(['-', 'T', ':'])
            .filter_map(|part| part.parse().ok())
            .collect();
        if parts.len() < 6 {
            break;
        }
        let delta = (days_from_civil(year, month, day)
            - days_from_civil(parts[0], parts[1], parts[2]))
            * 86_400
            + (hour - parts[3]) * 3_600
            + (minute - parts[4]) * 60
            + (second - parts[5]);
        if delta == 0 {
            break;
        }
        guess += delta as f64;
    }
    guess
}

/// Días desde 1970-01-01 de una fecha civil (Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
