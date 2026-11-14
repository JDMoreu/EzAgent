//! Port of `src/ezagent/agent/memory.py 563-1102` (F3-09b): history recall.

use std::collections::HashSet;

use ez_core::pyvalue::{char_len, head_chars};
use rusqlite::{params, ToSql};
use serde_json::{json, Value};

use super::schema::{
    fold_text, normalize_entry, tokens, DEFAULT_RECALL_LIMIT, RECALL_FRAGMENT_CHARS,
};
use super::store::MemoryStore;

const STOPWORDS: &str = "a al algo ante así ayer con como cuando de del desde donde dónde el él ella ellas ellos en entre era es esa ese eso esta está este esto estos estas fue ha hablamos han has hasta hay hoy la las le les lo los más me mi mis mucho muy no nos o para pero por que qué se ser si sí sin sobre son su sus también te ti tu tus un una unas uno unos y ya yo";

fn is_stopword(token: &str) -> bool {
    STOPWORDS.split_whitespace().any(|word| word == token)
}

/// Términos útiles de la consulta, sin acentos ni palabras vacías.
fn query_terms(query: &str) -> Vec<String> {
    let raw: Vec<String> = tokens(query).iter().map(|token| fold_text(token)).collect();
    let mut meaningful: Vec<String> = raw
        .iter()
        .filter(|token| !is_stopword(token) && char_len(token) >= 2)
        .cloned()
        .collect();
    if meaningful.is_empty() {
        meaningful = raw
            .iter()
            .filter(|token| !is_stopword(token))
            .cloned()
            .collect();
    }
    if meaningful.is_empty() {
        raw
    } else {
        meaningful
    }
}

fn like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn char_slice(text: &str, start: usize, end: usize) -> &str {
    match text.char_indices().nth(start) {
        Some((begin, _)) => match text.char_indices().nth(end) {
            Some((finish, _)) => &text[begin..finish],
            None => &text[begin..],
        },
        None => "",
    }
}

fn char_position(text: &str, byte_index: usize) -> usize {
    text[..byte_index].chars().count()
}

/// Fragmento de como máximo `max_chars` alrededor del primer término.
pub fn make_snippet(text: &str, terms: &[String], max_chars: usize) -> String {
    let content = text;
    if char_len(content) <= max_chars {
        return content.to_string();
    }
    let folded = fold_text(content);
    let index = terms
        .iter()
        .filter(|term| !term.is_empty())
        .filter_map(|term| folded.find(term).map(|byte| char_position(&folded, byte)))
        .min()
        .unwrap_or(0);
    let length = char_len(content);
    let mut start = index.saturating_sub(80);
    let end = (start + max_chars).min(length);
    if end - start < max_chars {
        start = end.saturating_sub(max_chars);
    }
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < length { "…" } else { "" };
    let available = max_chars.saturating_sub(char_len(prefix) + char_len(suffix));
    let segment = char_slice(content, start, end);
    if available == 0 {
        return head_chars(segment, max_chars).to_string();
    }
    let segment = if char_len(segment) > available {
        head_chars(segment, available)
    } else {
        segment
    };
    head_chars(&format!("{prefix}{segment}{suffix}"), max_chars).to_string()
}

fn result_from_row(
    id: i64,
    ts: &str,
    session: &str,
    role: &str,
    content: &str,
    terms: &[String],
    score: f64,
) -> Value {
    json!({
        "id": id,
        "ts": ts,
        "session": session,
        "role": role,
        "snippet": make_snippet(content, terms, RECALL_FRAGMENT_CHARS),
        "score": score,
    })
}

impl MemoryStore {
    fn recall_fts(&self, query: &str, limit: i64) -> Result<Vec<Value>, String> {
        let terms = query_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let expression = terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let connection = self.connect()?;
        let statement = "SELECT m.id, m.ts, m.session, m.role, m.content, \
             bm25(messages_fts) AS score \
             FROM messages_fts JOIN messages AS m ON m.id = messages_fts.rowid \
             WHERE messages_fts MATCH ?1 \
             ORDER BY score ASC, m.id DESC LIMIT ?2";
        let mut prepared = connection
            .prepare(statement)
            .map_err(|error| error.to_string())?;
        let rows = prepared
            .query_map(params![expression, limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut results = Vec::new();
        for row in rows {
            let (id, ts, session, role, content, score) = row.map_err(|error| error.to_string())?;
            results.push(result_from_row(
                id, &ts, &session, &role, &content, &terms, score,
            ));
        }
        Ok(results)
    }

    fn recall_like(&self, query: &str, limit: i64) -> Result<Vec<Value>, String> {
        let mut terms = query_terms(query);
        if terms.is_empty() {
            terms = vec![fold_text(query)];
        }
        // DECISION (del Python): LIKE de SQLite no ignora acentos; se prueban
        // también los tokens originales para no perder coincidencias exactas.
        let mut seen = HashSet::new();
        let mut candidates: Vec<String> = Vec::new();
        let raw_terms = tokens(query).into_iter().map(|token| token.to_lowercase());
        for candidate in terms.iter().cloned().chain(raw_terms) {
            if seen.insert(candidate.clone()) {
                candidates.push(candidate);
            }
        }
        let patterns: Vec<String> = candidates.iter().map(|term| like_pattern(term)).collect();
        let where_clause = vec!["content LIKE ? ESCAPE '\\'"; patterns.len()].join(" OR ");
        if patterns.is_empty() {
            return Ok(Vec::new());
        }
        let statement = format!(
            "SELECT id, ts, session, role, content FROM messages \
             WHERE {where_clause} ORDER BY ts DESC, id DESC LIMIT ?"
        );
        let mut values: Vec<&dyn ToSql> = patterns
            .iter()
            .map(|pattern| pattern as &dyn ToSql)
            .collect();
        values.push(&limit);
        let connection = self.connect()?;
        let mut prepared = connection
            .prepare(&statement)
            .map_err(|error| error.to_string())?;
        let rows = prepared
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        let mut results = Vec::new();
        for row in rows {
            let (id, ts, session, role, content) = row.map_err(|error| error.to_string())?;
            results.push(result_from_row(
                id, &ts, &session, &role, &content, &terms, 0.0,
            ));
        }
        Ok(results)
    }

    /// Busca conversaciones anteriores; fragmentos de 300 caracteres como máximo.
    pub fn recall_limited(&self, query: &str, limit: i64) -> Result<Vec<Value>, String> {
        let text = normalize_entry(query);
        if text.is_empty() {
            self.set_last_backend("none");
            return Ok(Vec::new());
        }
        let safe_limit = limit.clamp(1, 20);
        if self.fts_enabled {
            // DECISION (del Python): si FTS5 no devuelve nada, se prueba LIKE
            // para no perder coincidencias por la tokenización.
            match self.recall_fts(&text, safe_limit) {
                Ok(results) if !results.is_empty() => {
                    self.set_last_backend("fts5");
                    return Ok(results);
                }
                Ok(_) => {}
                Err(error) => tracing::warn!("Falló la consulta FTS5; se usa LIKE. {error}"),
            }
        }
        self.set_last_backend("like");
        self.recall_like(&text, safe_limit)
    }

    /// `recall(query)` con el límite por defecto.
    pub fn recall(&self, query: &str) -> Result<Vec<Value>, String> {
        self.recall_limited(query, DEFAULT_RECALL_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use chrono::DateTime;

    use super::super::store::{MemoryOptions, MemoryStore};
    use super::super::MemoryTimestamp;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("ez-memory-{}-{name}-{nanos}", std::process::id()))
    }

    fn store(name: &str, prefer_fts: bool) -> MemoryStore {
        let moment = DateTime::parse_from_rfc3339("2026-11-14T05:07:23+00:00").expect("fecha");
        let dir = scratch(name);
        let options = MemoryOptions {
            prefer_fts,
            clock: Some(Arc::new(move || moment)),
            ..MemoryOptions::default()
        };
        MemoryStore::from_paths(&dir, dir.join("history.db"), options).expect("almacén válido")
    }

    fn seed(store: &MemoryStore) {
        store
            .record(
                "user",
                "La memoria del agente guarda la conversación.",
                None,
                &MemoryTimestamp::Text("2026-11-01T10:00:00+00:00".into()),
            )
            .expect("guardado");
        store
            .record(
                "assistant",
                "Prefiero el café de la oficina.",
                None,
                &MemoryTimestamp::Text("2026-11-02T10:00:00+00:00".into()),
            )
            .expect("guardado");
    }

    #[test]
    fn keeps_short_text_and_centers_the_snippet() {
        assert_eq!(make_snippet("corto", &[], 300), "corto");
        let long = format!("{} objetivo {}", "x".repeat(400), "y".repeat(400));
        let snippet = make_snippet(&long, &["objetivo".to_string()], 300);
        assert_eq!(snippet.chars().count(), 300);
        assert!(snippet.contains("objetivo"));
        assert!(snippet.starts_with('…') && snippet.ends_with('…'));
    }

    #[test]
    fn extracts_terms_without_stopwords() {
        assert_eq!(query_terms("¿Qué es la memoria?"), vec!["memoria"]);
        assert_eq!(query_terms("de la"), vec!["de", "la"]);
    }

    #[test]
    fn recalls_with_fts_and_falls_back_to_like() {
        let store = store("recall", true);
        seed(&store);
        let results = store.recall("memoria agente").expect("búsqueda");
        assert_eq!(results.len(), 1);
        assert_eq!(store.last_backend(), "fts5");
        assert!(results[0]["snippet"]
            .as_str()
            .expect("texto")
            .contains("memoria del agente"));
        let misses = store.recall("zzz").expect("búsqueda");
        assert!(misses.is_empty());
        assert_eq!(store.last_backend(), "like");
    }

    #[test]
    fn like_backend_is_accent_insensitive_via_raw_terms() {
        let store = store("like", false);
        seed(&store);
        let results = store.recall_limited("café", 5).expect("búsqueda");
        assert_eq!(results.len(), 1);
        assert_eq!(store.last_backend(), "like");
        assert!(results[0]["snippet"]
            .as_str()
            .expect("texto")
            .contains("café"));
    }

    #[test]
    fn empty_query_returns_nothing() {
        let store = store("empty", true);
        seed(&store);
        assert!(store.recall("   ").expect("búsqueda").is_empty());
        assert_eq!(store.last_backend(), "none");
    }
}
