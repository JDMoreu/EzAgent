//! El nombre del subagente (§6 del diseño).
//!
//! Un nombre de `hc` es `^[a-z0-9][a-z0-9_.-]{0,31}$` y no puede tener forma
//! de dirección (`^t[0-9][0-9a-z]*$`, que `hc` rechaza).

use super::super::child::{hc_valid_name, looks_like_address};

/// Máximo de caracteres de un nombre de `hc`.
const MAX_NAME: usize = 32;

/// Los papeles con abreviatura fija (§6).
const SHORT_ROLES: [(&str, &str); 5] = [
    ("planner", "plan"),
    ("plan_reviewer", "revplan"),
    ("developer", "dev"),
    ("qa", "qa"),
    ("qa_validator", "valqa"),
];

/// La abreviatura del papel: `developer` → `dev`; otro papel, sus caracteres
/// `[a-z0-9]` en minúsculas, hasta 8.
pub fn role_short(role: &str) -> String {
    let key = role.trim().to_lowercase();
    for (role, short) in SHORT_ROLES {
        if key == role {
            return short.to_string();
        }
    }
    let mut short: String = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(8)
        .collect();
    if short.is_empty() {
        // DECISION: un papel sin letras ni números («###») no puede formar un
        // nombre de `hc`; se usa la misma palabra que el resto de `hc_slug`.
        short = "agente".to_string();
    }
    short
}

/// El nombre por omisión: `<corto><n>-<tipo>`, recortado a 32.
pub fn default_name(role: &str, n: usize, agent: &str) -> String {
    let mut name = format!("{}{}-{}", role_short(role), n, clean(agent));
    if name.chars().count() > MAX_NAME {
        name = name.chars().take(MAX_NAME).collect();
    }
    while name.ends_with('-') {
        name.pop();
    }
    if looks_like_address(&name) {
        name = format!("a-{name}");
    }
    name
}

/// ¿El nombre sirve para `hc`? (`^[a-z0-9][a-z0-9_.-]{0,31}$`, sin forma de
/// dirección).
///
/// DECISION (R4-1): la regla vive una sola vez, en `child::hc_valid_name`.
pub fn valid_name(name: &str) -> bool {
    hc_valid_name(name)
}

/// El tipo del agente, en minúsculas y solo con lo que `hc` acepta.
fn clean(agent: &str) -> String {
    let mut out = String::with_capacity(agent.len());
    for character in agent.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' {
            out.push(character);
        } else {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_papeles_conocidos_tienen_abreviatura() {
        assert_eq!(role_short("planner"), "plan");
        assert_eq!(role_short("plan_reviewer"), "revplan");
        assert_eq!(role_short("developer"), "dev");
        assert_eq!(role_short("qa"), "qa");
        assert_eq!(role_short("qa_validator"), "valqa");
    }

    #[test]
    fn otro_papel_usa_sus_caracteres_en_minusculas_hasta_ocho() {
        assert_eq!(role_short("Arquitecto"), "arquitec");
        assert_eq!(role_short("qa lead"), "qalead");
        assert_eq!(role_short("###"), "agente");
    }

    #[test]
    fn el_nombre_por_omision_une_papel_numero_y_tipo() {
        assert_eq!(default_name("developer", 1, "opencode"), "dev1-opencode");
        assert_eq!(default_name("qa", 3, "codex"), "qa3-codex");
    }

    #[test]
    fn el_nombre_por_omision_se_recorta_a_32() {
        let name = default_name("developer", 12, "opencode-super-largo-que-no-cabe");
        assert_eq!(name.chars().count(), 32);
        assert!(!name.ends_with('-'));
    }

    #[test]
    fn el_nombre_por_omision_no_tiene_forma_de_direccion() {
        let name = default_name("t7", 1, "");
        assert!(!looks_like_address(&name), "{name}");
        assert!(valid_name(&name), "{name}");
    }

    #[test]
    fn un_nombre_valido_acepta_lo_que_hc_acepta() {
        assert!(valid_name("dev1-opencode"));
        assert!(valid_name("a.b_c-d1"));
        assert!(valid_name("9"));
        assert!(!valid_name(""));
        assert!(!valid_name("Dev"));
        assert!(!valid_name("-abc"));
        assert!(!valid_name("con espacio"));
        assert!(!valid_name("t7"));
        assert!(!valid_name("t9n"));
        assert!(!valid_name(&"x".repeat(33)));
    }
}
