//! Selección de herramientas por intención (F4-09).

use std::collections::HashSet;
use std::sync::OnceLock;

use ez_core::pyvalue::casefold;
use regex::Regex;

use super::BASE_TOOLS;

fn time_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(hora|fecha|d[ií]a|hoy|calendario)\b").expect("patrón válido")
    })
}

fn write_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(escribe|escribir|escribeme|escríbeme|crea|crear|creame|créame|genera|generar|generame|genérame|guarda|guardar|redacta|redactar|añade|anade|añadir|agrega|agregar)\b",
        )
        .expect("patrón válido")
    })
}

fn copy_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)\b(copia|duplica)\b").expect("patrón válido"))
}

fn open_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"(?i)\b(abre|abrir|abreme|ábreme)\b").expect("patrón válido"))
}

fn delete_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(borra|borrar|elimina|eliminar|quita|quitar)\b").expect("patrón válido")
    })
}

fn move_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(mueve|mover|mu[eé]veme|renombra|renombrar|ren[oó]mbrame|pasa(?:lo|la)? a la carpeta)\b")
            .expect("patrón válido")
    })
}

/// Alternativas de `_SWARM_RE` sin la referencia inversa (Rust `regex` no la
/// admite); la tercera se resuelve en [`swarm_matches`].
fn swarm_words_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:enjambre|swarm|cooperativ[oa]s?|colabor\w+|en equipo|equipo de agentes|entre (?:ellos|s[ií])|se coordinen|trabajen juntos|el rol de|roles)\b",
        )
        .expect("patrón válido")
    })
}

fn swarm_plan_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?is)\bplan\w*\b.*\b(?:valid\w+|audit\w+|revis\w+)\b").expect("patrón válido")
    })
}

fn role_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:planificador\w*|desarrollador\w*|programador\w*|qa|tester|validador\w*|auditor\w*|arquitect[oa]s?|orquestador\w*|coordinador\w*|backend|frontend|devops|revisor\w*)\b",
        )
        .expect("patrón válido")
    })
}

fn undo_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(deshaz|deshacer|deshazlo|revierte|revertir|vuelve atr[aá]s)\b")
            .expect("patrón válido")
    })
}

fn list_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(lista|listar|listado|enumera|enumerar)\b|\b(qu[eé] hay|qu[eé] archivos|mu[eé]strame la carpeta)\b")
            .expect("patrón válido")
    })
}

fn doc_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(pdf|excel|xlsx|word|docx|powerpoint|pptx|hoja de c[aá]lculo|presentaci[oó]n)\b")
            .expect("patrón válido")
    })
}

fn skill_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(skill|skills|habilidad|habilidades|script|scripts)\b")
            .expect("patrón válido")
    })
}

fn memory_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)\b(memoria|recuerda|recordar|recuerdas|olvida|olvidar|compacta|compactar)\b",
        )
        .expect("patrón válido")
    })
}

/// `_SWARM_RE`: la referencia inversa del Python se reproduce buscando dos
/// roles distintos en el texto (`.*` con DOTALL permite cualquier distancia).
fn swarm_matches(text: &str) -> bool {
    if swarm_words_re().is_match(text) || swarm_plan_re().is_match(text) {
        return true;
    }
    let mut seen: Option<String> = None;
    for found in role_re().find_iter(text) {
        let word = casefold(found.as_str());
        match &seen {
            None => seen = Some(word),
            // DECISION: el `\3` del Python es exacto; casefold reproduce el
            // IGNORECASE del patrón.
            Some(first) if *first != word => return true,
            Some(_) => {}
        }
    }
    false
}

/// ¿El texto comparte una palabra clave con el catálogo de skills?
fn skill_matches(text: &str, keywords: Option<&HashSet<String>>) -> bool {
    let Some(keywords) = keywords.filter(|keywords| !keywords.is_empty()) else {
        return false;
    };
    !crate::skills::skill_tokens(text).is_disjoint(keywords)
}

/// Valores por defecto de los argumentos nombrados de `select_tool_names`.
#[derive(Debug, Clone)]
pub struct SelectOptions<'a> {
    pub scripts_loaded: bool,
    pub has_memory: bool,
    pub skill_keywords: Option<&'a HashSet<String>>,
    pub swarm_active: bool,
}

impl Default for SelectOptions<'_> {
    fn default() -> Self {
        Self {
            scripts_loaded: false,
            has_memory: true,
            skill_keywords: None,
            swarm_active: false,
        }
    }
}

/// Elige hasta 14 nombres para el prompt de este turno.
///
/// DECISION (del Python): se parte de 5 herramientas base y cada intención
/// *suma* capacidades; una intención nunca quita otra.
pub fn select_tool_names(text: &str, options: &SelectOptions<'_>) -> Vec<String> {
    let mut names: Vec<String> = BASE_TOOLS.iter().map(|name| (*name).to_string()).collect();
    if options.swarm_active || swarm_matches(text) {
        names.push("swarm".into());
    }
    if options.scripts_loaded {
        names.push("run_skill_script".into());
    }
    if time_re().is_match(text) {
        names.push("get_datetime".into());
    }
    if write_re().is_match(text) {
        names.push("write_file".into());
        names.push("create_dir".into());
    }
    if copy_re().is_match(text) {
        names.push("copy_path".into());
    }
    if open_re().is_match(text) {
        names.push("open_path".into());
    }
    if move_re().is_match(text) {
        names.push("move_path".into());
    }
    if undo_re().is_match(text) {
        names.push("undo_last".into());
    }
    if delete_re().is_match(text) {
        names.push("delete_path".into());
    }
    if list_re().is_match(text) {
        names.push("list_dir".into());
    }
    if doc_re().is_match(text)
        || skill_re().is_match(text)
        || skill_matches(text, options.skill_keywords)
    {
        names.push("load_skill".into());
    }
    if skill_re().is_match(text) {
        names.push("run_skill_script".into());
    }
    if memory_re().is_match(text) && options.has_memory {
        names.push("remember".into());
        names.push("recall".into());
        names.push("forget".into());
        names.push("compact_memory".into());
    }
    let mut unique: Vec<String> = Vec::with_capacity(names.len());
    for name in names {
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    if !options.has_memory {
        unique.retain(|name| name != "recall");
    }
    unique.truncate(14);
    unique
}
