//! Veredicto estructurado del pez verificador (10.11).
//!
//! El JSON lo escribe un agente en `verificacion-K/veredicto.json`: es dato no
//! confiable. Se valida el esquema completo antes de usarlo; un texto inválido
//! no cuenta como ronda limpia. Se acepta el JSON solo o dentro de una cerca
//! de Markdown, y las claves extra se ignoran.

use std::collections::HashMap;
use std::fmt;

use ez_core::pyvalue::{collapse_spaces, fold_accents, py_strip};
use serde_json::Value;

/// Umbral de churn: dos problemas con esta similitud son «el mismo problema».
pub const CHURN_SIMILARITY: f64 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Blocking,
    Major,
    Minor,
}

impl Severity {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "bloqueante" => Some(Self::Blocking),
            "mayor" => Some(Self::Major),
            "menor" => Some(Self::Minor),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocking => "bloqueante",
            Self::Major => "mayor",
            Self::Minor => "menor",
        }
    }

    /// Los que impiden una ronda limpia.
    pub fn blocks(self) -> bool {
        matches!(self, Self::Blocking | Self::Major)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub n: i64,
    pub cumple: bool,
    pub evidencia: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub criterion: Option<i64>,
    pub problem: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub cumple: bool,
    pub criteria: Vec<Criterion>,
    pub findings: Vec<Finding>,
}

/// Lo que el verificador debía devolver: su nonce y cuántos criterios cubrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerdictSpec<'a> {
    pub token: &'a str,
    pub criteria: usize,
}

/// Error de esquema con el texto que se le muestra al usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictError(pub String);

impl fmt::Display for VerdictError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn invalid(detail: &str) -> VerdictError {
    VerdictError(format!("El veredicto no es válido: {detail}"))
}

fn required<'a>(object: &'a Value, key: &str) -> Result<&'a Value, VerdictError> {
    object
        .get(key)
        .ok_or_else(|| invalid(&format!("falta «{key}»")))
}

fn text_of(object: &Value, key: &str) -> Result<String, VerdictError> {
    required(object, key)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| invalid(&format!("«{key}» debe ser texto")))
}

fn bool_of(object: &Value, key: &str) -> Result<bool, VerdictError> {
    required(object, key)?
        .as_bool()
        .ok_or_else(|| invalid(&format!("«{key}» debe ser booleano")))
}

fn int_of(object: &Value, key: &str) -> Result<i64, VerdictError> {
    required(object, key)?
        .as_i64()
        .ok_or_else(|| invalid(&format!("«{key}» debe ser entero")))
}

fn criterion_of(value: &Value) -> Result<Criterion, VerdictError> {
    Ok(Criterion {
        n: int_of(value, "n")?,
        cumple: bool_of(value, "cumple")?,
        evidencia: text_of(value, "evidencia")?,
    })
}

fn finding_of(value: &Value) -> Result<Finding, VerdictError> {
    let severity = text_of(value, "severidad")?;
    let severity = Severity::parse(&severity)
        .ok_or_else(|| invalid("«severidad» debe ser bloqueante, mayor o menor"))?;
    let criterion = match required(value, "criterio")? {
        Value::Null => None,
        other => Some(
            other
                .as_i64()
                .ok_or_else(|| invalid("«criterio» debe ser entero o nulo"))?,
        ),
    };
    Ok(Finding {
        severity,
        criterion,
        problem: text_of(value, "problema")?,
        evidence: text_of(value, "evidencia")?,
    })
}

fn list_of<T>(
    value: &Value,
    key: &str,
    parse: fn(&Value) -> Result<T, VerdictError>,
) -> Result<Vec<T>, VerdictError> {
    let items = required(value, key)?
        .as_array()
        .ok_or_else(|| invalid(&format!("«{key}» debe ser una lista")))?;
    items.iter().map(parse).collect()
}

/// El JSON solo, o dentro de una cerca de Markdown (10.11).
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(body) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let body = match body.find('\n') {
        Some(index) => &body[index + 1..],
        None => body,
    };
    body.trim_end().strip_suffix("```").unwrap_or(body).trim()
}

/// Una entrada por criterio, del 1 al total, sin repetir y con evidencia.
fn check_criteria(criteria: &[Criterion], total: usize) -> Result<(), VerdictError> {
    if criteria.len() != total {
        return Err(invalid(&format!(
            "«criterios» debe tener una entrada por criterio (1..{total})"
        )));
    }
    let mut seen: Vec<i64> = Vec::with_capacity(total);
    for criterion in criteria {
        if criterion.n < 1 || criterion.n > total as i64 {
            return Err(invalid(&format!(
                "«criterios» tiene un número fuera de rango: {}",
                criterion.n
            )));
        }
        if seen.contains(&criterion.n) {
            return Err(invalid(&format!(
                "«criterios» repite el criterio {}",
                criterion.n
            )));
        }
        if py_strip(&criterion.evidencia).is_empty() {
            return Err(invalid(&format!(
                "la evidencia del criterio {} está vacía",
                criterion.n
            )));
        }
        seen.push(criterion.n);
    }
    Ok(())
}

impl Verdict {
    /// Valida el esquema de 10.11; cualquier desviación es un veredicto inválido.
    pub fn parse(text: &str, spec: &VerdictSpec<'_>) -> Result<Self, VerdictError> {
        let value = ez_core::pyloads::loads(extract_json(text))
            .map_err(|_| VerdictError("El veredicto no es JSON válido.".to_string()))?;
        if !value.is_object() {
            return Err(invalid("debe ser un objeto JSON"));
        }
        let token = text_of(&value, "token")?;
        if token != spec.token {
            return Err(invalid("el token no coincide con el de esta verificación"));
        }
        let cumple = bool_of(&value, "cumple")?;
        let criteria = list_of(&value, "criterios", criterion_of)?;
        check_criteria(&criteria, spec.criteria)?;
        if cumple != criteria.iter().all(|criterion| criterion.cumple) {
            return Err(invalid("«cumple» no coincide con los criterios"));
        }
        Ok(Self {
            cumple,
            criteria,
            findings: list_of(&value, "hallazgos", finding_of)?,
        })
    }

    /// Ronda limpia: cumple y ningún hallazgo bloqueante o mayor (9.5).
    pub fn clean(&self) -> bool {
        self.cumple
            && !self
                .findings
                .iter()
                .any(|finding| finding.severity.blocks())
    }

    /// Hallazgos bloqueantes y mayores, en el orden del veredicto.
    pub fn blocking(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.severity.blocks())
    }

    /// Texto del `problema` de los hallazgos que se mandan a corregir.
    pub fn problems(&self) -> Vec<String> {
        self.blocking()
            .map(|finding| finding.problem.clone())
            .collect()
    }
}

/// Texto normalizado para comparar problemas: sin tildes, minúsculas, solo
/// letras y dígitos con un espacio entre palabras.
pub fn normalize(text: &str) -> String {
    let folded = fold_accents(&text.to_lowercase());
    let kept: String = folded
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect();
    collapse_spaces(&kept)
}

fn bigrams(text: &str) -> HashMap<(char, char), usize> {
    let characters: Vec<char> = text.chars().collect();
    let mut counts = HashMap::new();
    for pair in characters.windows(2) {
        *counts.entry((pair[0], pair[1])).or_insert(0) += 1;
    }
    counts
}

/// Similitud Sørensen–Dice sobre bigramas de caracteres (0..=1).
// DECISION: el diseño pide «≥ 0,85 de similitud» sin fijar la métrica; Dice
// sobre bigramas normalizados es estable, barato y funciona con español.
pub fn similarity(left: &str, right: &str) -> f64 {
    let (left, right) = (normalize(left), normalize(right));
    if left == right {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let (left_pairs, right_pairs) = (bigrams(&left), bigrams(&right));
    let common: usize = left_pairs
        .iter()
        .map(|(pair, count)| count.min(right_pairs.get(pair).unwrap_or(&0)))
        .sum();
    let total: usize = left_pairs.values().sum::<usize>() + right_pairs.values().sum::<usize>();
    if total == 0 {
        return 0.0;
    }
    (2.0 * common as f64) / total as f64
}

/// Churn: algún problema nuevo repite uno ya mandado a corregir en la tarea (10.14).
pub fn is_churn(problems: &[String], corrected: &[String]) -> bool {
    problems.iter().any(|problem| {
        corrected
            .iter()
            .any(|old| similarity(problem, old) >= CHURN_SIMILARITY)
    })
}
