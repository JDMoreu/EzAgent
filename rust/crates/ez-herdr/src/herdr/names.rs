//! Resolvedor de nombres dictados por voz (mejora deliberada sobre el Python).
//!
//! Whisper transcribe mal los nombres en inglés dichos en español: el espacio
//! «TEST» llega como «TES», «TESS» o «TETS», y «OpenCode» como «OpenCobe». El
//! Python pasaba el texto tal cual a Herdr y este respondía `no space "TES"`;
//! aquí se compara con los candidatos **vivos** (espacios, terminales y
//! perfiles de agente) y se decide entre usar, usar diciéndolo o preguntar.
//!
//! Los umbrales están fijados por las pruebas de este módulo.

use ez_core::pyvalue::{
    collapse_spaces, fold_accents, py_strip, python_str, text_or_empty, truthy,
};
use serde_json::{Map, Value};

use super::{visible_name, Herdr};

/// Qué se está nombrando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Space,
    Terminal,
    Agent,
}

impl NameKind {
    /// Cómo se nombra en la pregunta de confirmación.
    pub fn article(self) -> &'static str {
        match self {
            Self::Space => "al espacio",
            Self::Terminal => "a la terminal",
            Self::Agent => "al agente",
        }
    }

    /// Cómo se dice lo usado en la nota del resultado.
    pub fn used(self) -> &'static str {
        match self {
            Self::Space => "usé el espacio",
            Self::Terminal => "usé la terminal",
            Self::Agent => "usé el agente",
        }
    }

    /// Encabezado del error cuando no hay parecido suficiente.
    pub fn not_found(self) -> &'static str {
        match self {
            Self::Space => "No encontré el espacio",
            Self::Terminal => "No encontré la terminal",
            Self::Agent => "No encontré el agente",
        }
    }

    /// Cómo se titula la lista de candidatos vivos.
    pub fn available(self) -> &'static str {
        match self {
            Self::Space => "Espacios abiertos",
            Self::Terminal => "Terminales abiertas",
            Self::Agent => "Agentes disponibles",
        }
    }

    /// «¿Te refieres al espacio «TEST»?», generado por código.
    pub fn question(self, label: &str) -> String {
        format!("¿Te refieres {} «{label}»?", self.article())
    }

    /// «usé el espacio «TEST»» para el resultado de la herramienta.
    pub fn note(self, label: &str) -> String {
        format!("{} «{label}»", self.used())
    }

    /// Error que lista los nombres disponibles para que el asistente pregunte.
    pub fn unknown(self, spoken: &str, available: &[String]) -> String {
        let names: Vec<String> = available.iter().map(|label| format!("«{label}»")).collect();
        format!(
            "{} «{spoken}». {}: {}.",
            self.not_found(),
            self.available(),
            names.join(", ")
        )
    }
}

/// Un candidato vivo: lo que se pasa a Herdr y los nombres con que se le llama.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameCandidate {
    pub value: String,
    pub label: String,
    pub names: Vec<String>,
}

impl NameCandidate {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        let value = value.into();
        let label = label.into();
        let mut names = vec![value.clone()];
        if label != value {
            names.push(label.clone());
        }
        Self {
            value,
            label,
            names,
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        let alias = alias.into();
        if !py_strip(&alias).is_empty() && !self.names.contains(&alias) {
            self.names.push(alias);
        }
        self
    }

    fn matches_exact(&self, wanted: &str) -> bool {
        self.names.iter().any(|name| {
            let normalized = normalize(name);
            !normalized.is_empty() && compact(&normalized) == wanted
        })
    }
}

/// Resultado de comparar un nombre dicho con los candidatos vivos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameMatch {
    /// Coincidencia normalizada: se usa sin decir nada.
    Exact(NameHit),
    /// Un único candidato muy probable: se usa y se dice en el resultado.
    Confident(NameHit),
    /// Puntuación media o dos candidatos parecidos: hay que preguntar.
    Doubtful(NameHit),
    /// Sin parecido suficiente; la lista sirve para que el asistente pregunte.
    Unknown { available: Vec<String> },
}

/// Candidato elegido por el resolvedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameHit {
    pub value: String,
    pub label: String,
}

fn hit(candidate: &NameCandidate) -> NameHit {
    NameHit {
        value: candidate.value.clone(),
        label: candidate.label.clone(),
    }
}

/// Nombres visibles de los candidatos, sin repetir y en orden alfabético.
pub fn candidate_labels(candidates: &[NameCandidate]) -> Vec<String> {
    let mut available: Vec<String> = Vec::new();
    for candidate in candidates {
        if !available.contains(&candidate.label) {
            available.push(candidate.label.clone());
        }
    }
    available.sort_by_key(|label| label.to_lowercase());
    available
}

/// Minúsculas, sin tildes, sin signos y con un solo espacio entre palabras.
pub fn normalize(text: &str) -> String {
    let folded = fold_accents(text);
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for character in folded.chars() {
        if character.is_alphanumeric() {
            word.push(character);
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words.join(" ")
}

/// `normalize` sin espacios: «Open Code» y «opencode» son la misma palabra.
fn compact(normalized: &str) -> String {
    normalized.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Clave fonética sencilla para nombres en inglés dichos en español.
///
/// Colapsa dobles letras y junta los pares que suenan igual: b/v, c/k/q,
/// s/z, y d final → t (la «t» y la «d» finales se confunden al dictar).
pub fn phonetic(text: &str) -> String {
    normalize(text)
        .split(' ')
        .map(phonetic_word)
        .collect::<Vec<String>>()
        .concat()
}

fn phonetic_word(word: &str) -> String {
    let mut prepared = word
        .replace("ck", "k")
        .replace("qu", "k")
        .replace("ph", "f");
    if prepared.contains('x') {
        prepared = prepared.replace('x', "ks");
    }
    let characters: Vec<char> = prepared.chars().collect();
    let mut out = String::new();
    for (index, character) in characters.iter().enumerate() {
        let mapped = match character {
            'c' | 'q' => 'k',
            'z' => 's',
            'v' => 'b',
            'y' => 'i',
            'h' => continue,
            'd' if index + 1 == characters.len() => 't',
            other => *other,
        };
        if !out.ends_with(mapped) {
            out.push(mapped);
        }
    }
    out
}

/// Distancia de Damerau-Levenshtein (alineación óptima de cadenas).
fn distance(left: &[char], right: &[char]) -> usize {
    let mut table = vec![vec![0_usize; right.len() + 1]; left.len() + 1];
    for (index, row) in table.iter_mut().enumerate() {
        row[0] = index;
    }
    for (index, cell) in table[0].iter_mut().enumerate() {
        *cell = index;
    }
    for i in 1..=left.len() {
        for j in 1..=right.len() {
            let cost = usize::from(left[i - 1] != right[j - 1]);
            let mut best = (table[i - 1][j] + 1)
                .min(table[i][j - 1] + 1)
                .min(table[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                best = best.min(table[i - 2][j - 2] + 1);
            }
            table[i][j] = best;
        }
    }
    table[left.len()][right.len()]
}

/// Similitud de 0 a 1 entre dos textos ya normalizados.
pub fn similarity(left: &str, right: &str) -> f64 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 1.0;
    }
    1.0 - distance(&left, &right) as f64 / longest as f64
}

/// Por debajo de esto no hay parecido suficiente para preguntar.
const DOUBTFUL_SCORE: f64 = 0.60;
/// A partir de aquí el candidato es muy probable.
const CONFIDENT_SCORE: f64 = 0.86;
/// Ventaja mínima sobre el segundo candidato para no preguntar.
const CONFIDENT_GAP: f64 = 0.10;
/// Parecido que se asume por prefijo o contenido («plan» → «planner»).
const CONTAINS_SCORE: f64 = 0.82;
/// Clave fonética idéntica: cuenta como coincidencia muy alta.
const PHONETIC_SCORE: f64 = 0.90;
/// Con menos de tres letras solo vale la coincidencia exacta.
const MIN_LEN: usize = 3;

fn candidate_score(spoken: &str, spoken_phonetic: &str, candidate: &NameCandidate) -> f64 {
    let mut best: f64 = 0.0;
    for name in &candidate.names {
        let normalized = normalize(name);
        if normalized.is_empty() {
            continue;
        }
        let compact_name = compact(&normalized);
        let mut score = similarity(spoken, &compact_name);
        let candidate_phonetic = phonetic(name);
        if spoken_phonetic == candidate_phonetic {
            score = score.max(PHONETIC_SCORE);
        } else {
            score = score.max(similarity(spoken_phonetic, &candidate_phonetic));
        }
        let shortest = spoken.chars().count().min(compact_name.chars().count());
        if shortest >= MIN_LEN
            && (compact_name.starts_with(spoken) || spoken.starts_with(&compact_name))
        {
            score = score.max(CONTAINS_SCORE);
        }
        best = best.max(score);
    }
    best
}

/// Compara `spoken` con los candidatos vivos.
pub fn resolve_name(spoken: &str, candidates: &[NameCandidate]) -> NameMatch {
    let normalized = normalize(spoken);
    let spoken_compact = compact(&normalized);
    if spoken_compact.is_empty() || candidates.is_empty() {
        return NameMatch::Unknown {
            available: candidate_labels(candidates),
        };
    }
    let exact: Vec<&NameCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.matches_exact(&spoken_compact))
        .collect();
    match exact.as_slice() {
        [only] => return NameMatch::Exact(hit(only)),
        [first, ..] => return NameMatch::Doubtful(hit(first)),
        [] => {}
    }
    if spoken_compact.chars().count() < MIN_LEN {
        return NameMatch::Unknown {
            available: candidate_labels(candidates),
        };
    }
    let spoken_phonetic = phonetic(spoken);
    let mut scored: Vec<(f64, &NameCandidate)> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate_score(&spoken_compact, &spoken_phonetic, candidate),
                candidate,
            )
        })
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    let (best, winner) = scored[0];
    let second = scored.get(1).map_or(0.0, |(score, _)| *score);
    if best < DOUBTFUL_SCORE {
        return NameMatch::Unknown {
            available: candidate_labels(candidates),
        };
    }
    let confident =
        best >= CONFIDENT_SCORE && (second < DOUBTFUL_SCORE || best - second >= CONFIDENT_GAP);
    if confident {
        NameMatch::Confident(hit(winner))
    } else {
        NameMatch::Doubtful(hit(winner))
    }
}

impl Herdr {
    /// Candidatos vivos de un tipo de nombre.
    ///
    /// Con `fresh` se consulta `hc ls`; sin él se reutiliza el último listado
    /// cacheado (barato) y, si no hay, la lista sale vacía.
    pub fn name_candidates(&self, kind: NameKind, fresh: bool) -> Vec<NameCandidate> {
        match kind {
            NameKind::Agent => self.agent_candidates(),
            _ => {
                let agents = if fresh {
                    self.live_agents()
                } else {
                    // La misma antigüedad que el refresco del contexto del
                    // prompt: la caché casi siempre sirve para el atajo exacto.
                    self.cached_agents(3.0).unwrap_or_default()
                };
                match kind {
                    NameKind::Space => space_candidates(&agents),
                    _ => terminal_candidates(&agents),
                }
            }
        }
    }

    /// Resolución que no pregunta: `Some` solo con coincidencia exacta o muy
    /// probable, o con el texto tal cual si no hay candidatos que comparar.
    pub fn resolve_safely(&self, kind: NameKind, spoken: &str) -> Option<String> {
        let candidates = self.name_candidates(kind, true);
        if candidates.is_empty() {
            return Some(py_strip(spoken).to_string());
        }
        match resolve_name(spoken, &candidates) {
            NameMatch::Exact(found) | NameMatch::Confident(found) => Some(found.value),
            NameMatch::Doubtful(_) | NameMatch::Unknown { .. } => None,
        }
    }

    fn live_agents(&self) -> Vec<Map<String, Value>> {
        match self.list_agents(20.0) {
            Ok(agents) => agents,
            Err(error) => {
                tracing::debug!("Herdr sin lista para resolver nombres: {}", error);
                Vec::new()
            }
        }
    }

    fn agent_candidates(&self) -> Vec<NameCandidate> {
        let cfg = self.read_cfg();
        let Some(Value::Object(agents)) = cfg.get("agents") else {
            return Vec::new();
        };
        let profiles = cfg.get("agent_profiles").and_then(Value::as_object);
        agents
            .keys()
            .map(|key| {
                let profile = profiles
                    .and_then(|profiles| profiles.get(key))
                    .and_then(Value::as_object);
                let label = match profile.and_then(|profile| profile.get("label")) {
                    Some(value) if truthy(value) => python_str(value),
                    _ => key.clone(),
                };
                let mut candidate = NameCandidate::new(key.clone(), label);
                if let Some(Value::Array(aliases)) =
                    profile.and_then(|profile| profile.get("aliases"))
                {
                    for alias in aliases {
                        candidate = candidate.with_alias(python_str(alias));
                    }
                }
                candidate
            })
            .collect()
    }
}

fn space_candidates(agents: &[Map<String, Value>]) -> Vec<NameCandidate> {
    let mut spaces: Vec<String> = Vec::new();
    for item in agents {
        if let Some(space) = space_of(item) {
            if !spaces.contains(&space) {
                spaces.push(space);
            }
        }
    }
    spaces
        .into_iter()
        .map(|space| NameCandidate::new(space.clone(), space))
        .collect()
}

fn terminal_candidates(agents: &[Map<String, Value>]) -> Vec<NameCandidate> {
    let mut terminals = Vec::new();
    for item in agents {
        let address = py_strip(&text_or_empty(item.get("address"))).to_string();
        if address.is_empty() {
            continue;
        }
        let label = match item.get("label") {
            Some(value) if truthy(value) => python_str(value),
            _ => visible_name(item),
        };
        let mut candidate = NameCandidate::new(address, label);
        let name = py_strip(&text_or_empty(item.get("name"))).to_string();
        if !name.is_empty() {
            candidate = candidate.with_alias(name);
        }
        terminals.push(candidate);
    }
    terminals
}

fn space_of(item: &Map<String, Value>) -> Option<String> {
    let where_text = collapse_spaces(&text_or_empty(item.get("where")));
    if let Some((left, _)) = where_text.split_once(" / ") {
        let space = py_strip(left);
        if !space.is_empty() {
            return Some(space.to_string());
        }
    }
    let space = collapse_spaces(&text_or_empty(item.get("space")));
    (!space.is_empty()).then_some(space)
}

#[cfg(test)]
mod tests;
