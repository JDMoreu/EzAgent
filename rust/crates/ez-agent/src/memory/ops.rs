//! Port of `src/ezagent/agent/memory.py 563-1102` (F3-09b): Markdown layers.

use std::io::Write;
use std::path::Path;

use ez_core::pyvalue::{char_len, head_chars, py_lstrip, py_splitlines, py_strip, tail_chars};
use indexmap::IndexMap;
use uuid::Uuid;

use super::schema::{
    bullet_capture, find_sensitive_text, fold_text, kind_file, kind_limit, normalize_entry,
    normalize_for_compare, normalize_kind, summarize_text, MemoryResult, MAX_ENTRY_CHARS,
};
use super::store::{read_text, MemoryStore};

impl MemoryStore {
    fn today(&self) -> String {
        self.clock_now().format("%Y-%m-%d").to_string()
    }

    fn path(&self, kind: &str) -> &Path {
        if kind == "user" {
            &self.user_path
        } else {
            &self.memory_path
        }
    }

    pub fn filename(&self, kind: &str) -> String {
        normalize_kind(kind)
            .and_then(kind_file)
            .unwrap_or("memoria")
            .to_string()
    }

    pub fn limit(&self, kind: &str) -> usize {
        normalize_kind(kind).and_then(kind_limit).unwrap_or(0)
    }

    fn read(&self, kind: &str) -> String {
        read_text(self.path(kind))
    }

    // DECISION: escritura atómica con archivo temporal y rename; si falla, el
    // temporal se borra y el archivo original queda intacto.
    fn atomic_write(&self, path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
        let temporary = path.with_file_name(format!("{name}.{}.tmp", Uuid::new_v4().simple()));
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        Ok(())
    }

    /// Contenido completo del archivo, para edición manual o pruebas.
    pub fn read_markdown(&self, kind: &str) -> String {
        match normalize_kind(kind) {
            None => String::new(),
            Some(normalized) => self.read(normalized),
        }
    }

    /// Viñetas del archivo, sin la fecha ni el guion.
    pub fn entries(&self, kind: &str) -> Vec<String> {
        let Some(normalized) = normalize_kind(kind) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for line in py_splitlines(&self.read(normalized)) {
            if let Some((_, body)) = bullet_capture(line) {
                let body = py_strip(body);
                if !body.is_empty() {
                    result.push(body.to_string());
                }
                continue;
            }
            let stripped = py_lstrip(line);
            if let Some(body) = stripped.strip_prefix("- ") {
                let body = py_strip(body);
                if !body.is_empty() {
                    result.push(body.to_string());
                }
            }
        }
        result
    }

    fn find_duplicate(&self, kind: &str, value: &str) -> Option<String> {
        let target = normalize_for_compare(value);
        self.entries(kind)
            .into_iter()
            .find(|entry| normalize_for_compare(entry) == target)
    }

    // DECISION: la propuesta no repite el archivo completo (ya viaja en
    // <memory>); así cabe en el límite de 2.000 caracteres del bucle.
    fn compaction_proposal(&self, kind: &str, value: &str) -> String {
        let limit = kind_limit(kind).unwrap_or(0);
        let content = py_strip(&self.read(kind)).to_string();
        let mut recent: Vec<&str> = py_splitlines(&content)
            .into_iter()
            .filter(|line| !py_strip(line).is_empty())
            .collect();
        recent = recent.split_off(recent.len().saturating_sub(5));
        let mut suggestion = recent.join("\n");
        if char_len(&suggestion) > 500 {
            suggestion = tail_chars(&suggestion, 500).to_string();
        }
        format!(
            "{} está lleno ({}/{} caracteres). No he guardado nada. Redacta una versión fusionada del archivo (lo tienes completo en <memory>) que conserve lo importante, incluya el dato nuevo y no pase de {} caracteres; después pide confirmación para guardarla con compact_memory(kind='{}', text=...).\n\nDato nuevo: - [{}] {}\n\nÚltimas líneas como referencia:\n{}",
            self.filename(kind),
            char_len(&content),
            limit,
            limit,
            kind,
            self.today(),
            value,
            suggestion
        )
    }

    /// Valida un dato sin escribirlo. `status == "ready"` si se puede guardar.
    pub fn validate_entry(&self, kind: &str, text: &str) -> MemoryResult {
        let Some(normalized) = normalize_kind(kind) else {
            return MemoryResult::new(
                "invalid_kind",
                "Tipo de memoria no válido; usa 'user' o 'memory'.",
            );
        };
        let value = normalize_entry(text);
        if value.is_empty() {
            return MemoryResult {
                kind: Some(normalized.into()),
                ..MemoryResult::new("empty", "El texto está vacío.")
            };
        }
        if char_len(&value) > MAX_ENTRY_CHARS {
            return MemoryResult {
                kind: Some(normalized.into()),
                text: Some(value.clone()),
                ..MemoryResult::new(
                    "too_long",
                    &format!(
                        "El texto tiene {} caracteres; el máximo es {}.",
                        char_len(&value),
                        MAX_ENTRY_CHARS
                    ),
                )
            };
        }
        if let Some(category) = find_sensitive_text(&value) {
            return MemoryResult {
                kind: Some(normalized.into()),
                ..MemoryResult::new("secret", &format!("No guardo {category} en la memoria."))
            };
        }
        if super::schema::contains_web_content(&value) {
            return MemoryResult {
                kind: Some(normalized.into()),
                ..MemoryResult::new(
                    "web",
                    "No guardo contenido procedente de la web en la memoria.",
                )
            };
        }
        if let Some(duplicate) = self.find_duplicate(normalized, &value) {
            return MemoryResult {
                kind: Some(normalized.into()),
                text: Some(value),
                ..MemoryResult::new(
                    "duplicate",
                    &format!("Ya está guardado: «{}».", summarize_text(&duplicate, 80)),
                )
            };
        }
        let limit = kind_limit(normalized).unwrap_or(0);
        let content = self.read(normalized);
        let prefix = if content.is_empty() || content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let line = format!("- [{}] {value}", self.today());
        if char_len(&format!("{content}{prefix}{line}\n")) > limit {
            return MemoryResult {
                kind: Some(normalized.into()),
                text: Some(value.clone()),
                proposal: Some(self.compaction_proposal(normalized, &value)),
                ..MemoryResult::new(
                    "full",
                    &format!(
                        "{} está lleno ({}/{} caracteres); no se ha guardado nada.",
                        self.filename(normalized),
                        char_len(&content),
                        limit
                    ),
                )
            };
        }
        MemoryResult {
            kind: Some(normalized.into()),
            text: Some(value),
            ..MemoryResult::new("ready", "Listo para guardar.")
        }
    }

    /// Valida y guarda una viñeta. Nunca guarda secretos ni duplicados.
    pub fn remember(&self, kind: &str, text: &str) -> Result<MemoryResult, String> {
        let validation = self.validate_entry(kind, text);
        if validation.status != "ready" {
            return Ok(validation);
        }
        let (Some(normalized), Some(value)) =
            (validation.kind.as_deref(), validation.text.as_deref())
        else {
            return Ok(validation);
        };
        let limit = kind_limit(normalized).unwrap_or(0);
        let content = self.read(normalized);
        let prefix = if content.is_empty() || content.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let line = format!("- [{}] {value}", self.today());
        let new_content = format!("{content}{prefix}{line}\n");
        if char_len(&new_content) > limit {
            return Ok(MemoryResult {
                kind: Some(normalized.into()),
                text: Some(value.to_string()),
                proposal: Some(self.compaction_proposal(normalized, value)),
                ..MemoryResult::new(
                    "full",
                    &format!(
                        "{} está lleno ({}/{} caracteres); no se ha guardado nada.",
                        self.filename(normalized),
                        char_len(&content),
                        limit
                    ),
                )
            });
        }
        self.atomic_write(self.path(normalized), &new_content)?;
        Ok(MemoryResult {
            kind: Some(normalized.into()),
            text: Some(value.to_string()),
            ..MemoryResult::new(
                "saved",
                &format!("Guardado en {}.", self.filename(normalized)),
            )
        })
    }

    // DECISION: las líneas planas de una versión compactada se convierten en
    // viñetas con la fecha de hoy; las cabeceras Markdown se conservan.
    fn compaction_lines(&self, text: &str) -> Vec<String> {
        let today = self.today();
        let mut result = Vec::new();
        for raw_line in py_splitlines(text) {
            let line = py_strip(raw_line);
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                result.push(line.to_string());
                continue;
            }
            if let Some((date, body)) = bullet_capture(line) {
                let body = py_strip(body);
                if !body.is_empty() {
                    result.push(format!("- [{date}] {body}"));
                }
                continue;
            }
            if let Some(body) = line.strip_prefix("- ") {
                let body = py_strip(body);
                if !body.is_empty() {
                    result.push(format!("- [{today}] {body}"));
                }
                continue;
            }
            result.push(format!("- [{today}] {line}"));
        }
        result
    }

    /// Valida la versión compactada completa sin escribirla.
    pub fn prepare_compaction(&self, kind: &str, text: &str) -> MemoryResult {
        let Some(normalized) = normalize_kind(kind) else {
            return MemoryResult::new(
                "invalid_kind",
                "Tipo de memoria no válido; usa 'user' o 'memory'.",
            );
        };
        let lines = self.compaction_lines(text);
        if lines.is_empty() {
            return MemoryResult {
                kind: Some(normalized.into()),
                ..MemoryResult::new("empty", "La versión compactada está vacía.")
            };
        }
        let content = format!("{}\n", lines.join("\n"));
        let limit = kind_limit(normalized).unwrap_or(0);
        if char_len(&content) > limit {
            return MemoryResult {
                kind: Some(normalized.into()),
                text: Some(content),
                line_count: lines.len() as i64,
                ..MemoryResult::new(
                    "too_long",
                    &format!(
                        "La versión compactada tiene {} caracteres; el máximo es {}.",
                        char_len(&format!("{}\n", lines.join("\n"))),
                        limit
                    ),
                )
            };
        }
        for line in &lines {
            if let Some(category) = find_sensitive_text(line) {
                return MemoryResult {
                    kind: Some(normalized.into()),
                    ..MemoryResult::new("secret", &format!("No guardo {category} en la memoria."))
                };
            }
            if super::schema::contains_web_content(line) {
                return MemoryResult {
                    kind: Some(normalized.into()),
                    ..MemoryResult::new(
                        "web",
                        "No guardo contenido procedente de la web en la memoria.",
                    )
                };
            }
        }
        MemoryResult {
            kind: Some(normalized.into()),
            text: Some(content),
            line_count: lines.len() as i64,
            ..MemoryResult::new("ready", "Listo para compactar.")
        }
    }

    // DECISION: la copia .bak se crea siempre antes de reescribir, aunque el
    // contenido compactado sea idéntico al actual.
    pub fn compact_memory(&self, kind: &str, text: &str) -> Result<MemoryResult, String> {
        let prepared = self.prepare_compaction(kind, text);
        let (Some(normalized), Some(content)) =
            (prepared.kind.as_deref(), prepared.text.as_deref())
        else {
            return Ok(prepared);
        };
        if prepared.status != "ready" {
            return Ok(prepared);
        }
        let path = self.path(normalized);
        let backup = path.with_file_name(format!(
            "{}.bak",
            path.file_name()
                .map_or_else(String::new, |value| value.to_string_lossy().into_owned())
        ));
        if path.exists() {
            std::fs::copy(path, &backup).map_err(|error| error.to_string())?;
        }
        self.atomic_write(path, content)?;
        Ok(MemoryResult {
            kind: Some(normalized.into()),
            text: Some(content.to_string()),
            backup: Some(backup.display().to_string()),
            line_count: prepared.line_count,
            ..MemoryResult::new(
                "compacted",
                &format!(
                    "{} compactado; copia anterior en {}.",
                    self.filename(normalized),
                    backup
                        .file_name()
                        .map_or_else(String::new, |value| value.to_string_lossy().into_owned())
                ),
            )
        })
    }

    /// Borra las líneas de USER.md y MEMORY.md que contienen la consulta.
    pub fn forget(&self, query: &str) -> Result<MemoryResult, String> {
        let needle = fold_text(&normalize_entry(query));
        if needle.is_empty() {
            return Ok(MemoryResult::new("empty", "La consulta está vacía."));
        }
        let mut removed = 0i64;
        let mut files = Vec::new();
        for kind in ["user", "memory"] {
            let content = self.read(kind);
            let lines = py_splitlines(&content);
            let kept: Vec<&str> = lines
                .iter()
                .copied()
                .filter(|line| !(!py_strip(line).is_empty() && fold_text(line).contains(&needle)))
                .collect();
            if kept.len() == lines.len() {
                continue;
            }
            removed += (lines.len() - kept.len()) as i64;
            let new_content = if kept.is_empty() {
                String::new()
            } else {
                format!("{}\n", kept.join("\n"))
            };
            self.atomic_write(self.path(kind), &new_content)?;
            files.push(self.filename(kind));
        }
        if removed == 0 {
            return Ok(MemoryResult::new(
                "not_found",
                &format!(
                    "No encontré líneas que coincidan con «{}».",
                    summarize_text(query, 80)
                ),
            ));
        }
        Ok(MemoryResult {
            status: "forgotten".into(),
            removed,
            message: format!("Se borraron {removed} línea(s) de {}.", files.join(", ")),
            files,
            ..MemoryResult::default()
        })
    }

    /// Líneas que olvidaría `forget(query)`, por tipo de memoria.
    pub fn find_matches(&self, query: &str) -> IndexMap<String, Vec<String>> {
        let needle = fold_text(&normalize_entry(query));
        let mut result = IndexMap::new();
        if needle.is_empty() {
            return result;
        }
        for kind in ["user", "memory"] {
            let matches: Vec<String> = py_splitlines(&self.read(kind))
                .into_iter()
                .filter(|line| !py_strip(line).is_empty() && fold_text(line).contains(&needle))
                .map(str::to_string)
                .collect();
            if !matches.is_empty() {
                result.insert(kind.to_string(), matches);
            }
        }
        result
    }

    // DECISION: si el usuario editó a mano y superó el tope, el bloque del
    // prompt se recorta; si ambos archivos están vacíos se devuelve "".
    pub fn memory_block(&self) -> String {
        let mut sections = Vec::new();
        for kind in ["user", "memory"] {
            let mut text = py_strip(&self.read(kind)).to_string();
            if text.is_empty() {
                continue;
            }
            let limit = kind_limit(kind).unwrap_or(0);
            if char_len(&text) > limit {
                let kept = py_strip(head_chars(&text, limit.saturating_sub(1))).to_string();
                text = format!("{kept}…");
            }
            sections.push(format!("## {}\n{text}", self.filename(kind)));
        }
        if sections.is_empty() {
            return String::new();
        }
        format!("<memory>\n{}\n</memory>", sections.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::{MemoryOptions, MemoryStore};
    use chrono::DateTime;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("ez-memory-{}-{name}-{nanos}", std::process::id()))
    }

    fn store(name: &str) -> MemoryStore {
        let moment = DateTime::parse_from_rfc3339("2026-11-14T05:07:23+00:00").expect("fecha");
        let dir = scratch(name);
        let options = MemoryOptions::default().clock(Arc::new(move || moment));
        MemoryStore::from_paths(&dir, dir.join("history.db"), options).expect("almacén válido")
    }

    #[test]
    fn remembers_and_detects_duplicates() {
        let store = store("remember");
        let result = store
            .remember("usuario", "  Prefiere café  ")
            .expect("guardado");
        assert_eq!(result.status, "saved");
        assert_eq!(
            store.read_markdown("user"),
            "- [2026-11-14] Prefiere café\n"
        );
        let again = store.remember("user", "prefiere cafe.").expect("validado");
        assert_eq!(again.status, "duplicate");
        assert_eq!(again.message, "Ya está guardado: «Prefiere café».");
    }

    #[test]
    fn rejects_secrets_web_and_empty_entries() {
        let store = store("reject");
        assert_eq!(store.validate_entry("user", "").status, "empty");
        assert_eq!(store.validate_entry("otro", "x").status, "invalid_kind");
        let secret = store.validate_entry("user", "mi contraseña es 1");
        assert_eq!(secret.status, "secret");
        let web = store.validate_entry("user", "mira https://example.com");
        assert_eq!(web.status, "web");
    }

    #[test]
    fn reports_full_with_a_proposal() {
        let store = store("full");
        std::fs::write(
            &store.user_path,
            format!("- [2026-01-01] {}\n", "x".repeat(1300)),
        )
        .expect("escritura");
        let result = store.validate_entry("user", &"y".repeat(150));
        assert_eq!(result.status, "full");
        let proposal = result.proposal.expect("propuesta");
        assert!(proposal.contains("compact_memory"));
    }

    #[test]
    fn forgets_matching_lines_in_both_files() {
        let store = store("forget");
        store.remember("user", "Prefiere café").expect("guardado");
        store
            .remember("memory", "El café está en frascos")
            .expect("guardado");
        assert_eq!(store.find_matches("CAFE").len(), 2);
        let result = store.forget("café").expect("borrado");
        assert_eq!(result.status, "forgotten");
        assert_eq!(result.removed, 2);
        assert_eq!(result.files, vec!["USER.md", "MEMORY.md"]);
        assert_eq!(store.read_markdown("user"), "");
    }

    #[test]
    fn compacts_with_backup_and_keeps_headers() {
        let store = store("compact");
        store.remember("user", "dato viejo").expect("guardado");
        let text = "# Perfil\notra línea\n- [2020-01-01] antigua";
        assert_eq!(store.prepare_compaction("user", text).line_count, 3);
        let compacted = store
            .compact_memory("user", "# Perfil\notra línea")
            .expect("compactado");
        assert_eq!(compacted.status, "compacted");
        assert!(PathBuf::from(compacted.backup.expect("copia")).is_file());
        assert_eq!(
            store.read_markdown("user"),
            "# Perfil\n- [2026-11-14] otra línea\n"
        );
    }

    #[test]
    fn builds_memory_block_and_reads_crlf() {
        let store = store("block");
        std::fs::write(&store.user_path, "- [2026-01-01] hola\r\n- mundo\r\n").expect("escritura");
        std::fs::write(&store.memory_path, "- [2026-02-02] hecho\n").expect("escritura");
        let expected = "<memory>\n## USER.md\n- [2026-01-01] hola\n- mundo\n\n## MEMORY.md\n- [2026-02-02] hecho\n</memory>";
        assert_eq!(store.memory_block(), expected);
        assert_eq!(store.entries("user"), vec!["hola", "mundo"]);
    }
}
