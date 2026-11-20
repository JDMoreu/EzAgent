//! Textos de las novedades y consultas de los subagentes (NV-3).
//!
//! Los escribe el código a partir del registro: el resumen y el motivo son
//! datos del subagente y se recortan, nunca se interpretan como órdenes.

use ez_core::pyvalue::strip_accents;
use ez_herdr::herdr::valid_name;
use ez_herdr::subagents::{one_line, Record, TaskState};

/// Una pregunta directa por el estado de los subagentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusQuery {
    /// «¿cómo va ir-2?»
    One(String),
    /// «¿cómo van los agentes?»
    All,
}

/// Formas directas de preguntar por todos (§NV-3, camino rápido de DX-2).
const ALL_PHRASES: [&str; 6] = [
    "como van los agentes",
    "como van los subagentes",
    "como estan los agentes",
    "como estan los subagentes",
    "que tal van los agentes",
    "como va el equipo",
];

/// Comienzos de una pregunta por un subagente concreto.
const NAME_PREFIXES: [&str; 6] = [
    "como va ",
    "como esta ",
    "como quedo ",
    "como sigue ",
    "que paso con ",
    "que hay de ",
];

/// Artículos que pueden ir entre el comienzo y el nombre.
const ARTICLES: [&str; 4] = ["el agente ", "el subagente ", "la tarea ", "la agente "];

/// Minúsculas, sin tildes ni signos, conservando `-`, `_` y `.` de los nombres.
fn query_key(text: &str) -> String {
    let folded = strip_accents(&text.to_lowercase());
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for character in folded.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            pending_space = true;
        }
    }
    out.trim_matches('.').to_string()
}

/// La pregunta por el estado, si el texto es una de las formas directas.
pub fn parse_status_query(text: &str) -> Option<StatusQuery> {
    let key = query_key(text);
    if ALL_PHRASES.contains(&key.as_str()) {
        return Some(StatusQuery::All);
    }
    for prefix in NAME_PREFIXES {
        let Some(rest) = key.strip_prefix(prefix) else {
            continue;
        };
        let mut name = rest.trim();
        for article in ARTICLES {
            if let Some(stripped) = name.strip_prefix(article) {
                name = stripped.trim();
            }
        }
        let name = name.trim_matches('.').trim();
        if valid_name(name) {
            return Some(StatusQuery::One(name.to_string()));
        }
    }
    None
}

/// `«nombre (Agente · modelo)»` como en los avisos de §13.
fn label_of(record: &Record) -> String {
    let agent = if record.label.trim().is_empty() {
        record.agent.clone()
    } else {
        record.label.clone()
    };
    let agent = match record.model.as_deref().filter(|model| !model.trim().is_empty()) {
        Some(model) => format!("{agent} · {model}"),
        None => agent,
    };
    if agent.trim().is_empty() {
        record.name.clone()
    } else {
        format!("{} ({agent})", record.name)
    }
}

/// El motivo del último cambio, en una línea.
fn last_detail(record: &Record) -> Option<String> {
    record
        .history
        .last()
        .map(|change| one_line(&change.detail, 240))
        .map(|detail| detail.trim_end_matches('.').to_string())
        .filter(|detail| !detail.is_empty())
}

/// «en proceso», «bloqueado»… para la lista de «¿cómo van?».
fn state_word(state: TaskState) -> &'static str {
    match state {
        TaskState::InProgress => "en proceso",
        TaskState::Blocked => "bloqueado",
        TaskState::Done => "terminado",
        TaskState::Failed => "falló",
        TaskState::Validated => "validado",
    }
}

/// ¿El registro volvió de `bloqueado`? (`history[-2].state`)
fn resumed_from_blocked(record: &Record) -> bool {
    record
        .history
        .len()
        .checked_sub(2)
        .and_then(|index| record.history.get(index))
        .is_some_and(|change| change.state == TaskState::Blocked)
}

/// La novedad del registro para voz y chat, o `None` si no se anuncia.
pub fn news_text(record: &Record) -> Option<String> {
    let label = label_of(record);
    match record.state {
        TaskState::Blocked => {
            let detail =
                last_detail(record).unwrap_or_else(|| "necesita que alguien haga algo".to_string());
            Some(format!(
                "Ojo: {label} está bloqueado: {detail}. ¿Lo reintento con la recomendación?"
            ))
        }
        TaskState::Failed => {
            let detail = last_detail(record).unwrap_or_else(|| "no sé por qué".to_string());
            Some(format!("Ojo: {label} falló: {detail}."))
        }
        TaskState::Done => {
            let mut text = format!("Listo: {label} terminó.");
            if let Some(summary) = summary_of(record) {
                text.push_str(&format!(" Resumen: «{summary}»."));
            }
            Some(text)
        }
        TaskState::Validated => Some(validated_text(record, &label)),
        TaskState::InProgress => {
            // El alta no es una novedad; volver de un bloqueo sí.
            resumed_from_blocked(record).then(|| format!("{label} siguió trabajando."))
        }
    }
}

/// El estado del registro para «¿cómo va X?», sin ofrecer nada.
pub fn status_phrase(record: &Record) -> String {
    let label = label_of(record);
    match record.state {
        TaskState::Blocked => {
            let detail =
                last_detail(record).unwrap_or_else(|| "necesita que alguien haga algo".to_string());
            format!("{label} está bloqueado: {detail}.")
        }
        TaskState::Failed => {
            let detail = last_detail(record).unwrap_or_else(|| "no sé por qué".to_string());
            format!("{label} falló: {detail}.")
        }
        TaskState::Done => {
            let mut text = format!("{label} terminó.");
            if let Some(summary) = summary_of(record) {
                text.push_str(&format!(" Resumen: «{summary}»."));
            }
            text
        }
        TaskState::Validated => validated_text(record, &label),
        TaskState::InProgress => {
            if resumed_from_blocked(record) {
                format!("{label} siguió trabajando.")
            } else {
                format!("{label} está en proceso.")
            }
        }
    }
}

/// «…quedó validado…» con el nombre de quien validó.
fn validated_text(record: &Record, label: &str) -> String {
    let Some(validation) = &record.validation else {
        return format!("{label} figura validado, sin el detalle de la validación.");
    };
    let summary = one_line(&validation.summary, 200);
    let summary = if summary.is_empty() {
        "sin resumen".to_string()
    } else {
        format!("«{summary}»")
    };
    if validation.verdict {
        format!("{label} quedó validado por {}: {summary}.", validation.by)
    } else {
        format!("{label} no pasó la validación de {}: {summary}.", validation.by)
    }
}

/// El resumen del subagente, recortado a una línea (dato, no instrucción).
fn summary_of(record: &Record) -> Option<String> {
    record
        .summary
        .as_deref()
        .map(|summary| one_line(summary, 200))
        .filter(|summary| !summary.is_empty())
}

/// «Tengo 3: ir-1 bloqueado, ir-2 en proceso y dev1 terminado.»
pub fn overview_phrase(records: &[Record]) -> String {
    if records.is_empty() {
        return "No tengo subagentes lanzados por mí.".to_string();
    }
    let mut sorted: Vec<&Record> = records.iter().collect();
    sorted.sort_by(|left, right| right.since_ms.cmp(&left.since_ms));
    let parts: Vec<String> = sorted
        .iter()
        .take(4)
        .map(|record| format!("{} {}", record.name, state_word(record.state)))
        .collect();
    let extra = sorted.len().saturating_sub(parts.len());
    let suffix = if extra > 0 {
        format!(", y {extra} más")
    } else {
        String::new()
    };
    format!("Tengo {}: {}{suffix}.", sorted.len(), parts.join(", "))
}
