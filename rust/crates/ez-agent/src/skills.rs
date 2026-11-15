//! Port of `src/ezagent/agent/skills.py` (F3-06).
//!
//! Descubrimiento, validación y carga de skills locales. Una skill es una
//! carpeta con un `SKILL.md` (frontmatter YAML con `name` y `description`) y,
//! opcionalmente, `scripts/`, `references/` y `assets/`.
//!
//! Orden de búsqueda (gana la primera coincidencia y las colisiones se
//! registran): `%APPDATA%/EzAgent/skills/` y luego `~/.agents/skills/`.
//! Nunca se descargan ni se instalan skills desde internet.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use ez_core::paths::{resolve, resolve_safe};
use ez_core::pyvalue::{char_len, collapse_spaces, fold_accents, head_chars, py_strip};
use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value;

pub const MAX_CATALOG: usize = 20;
pub const MAX_DESCRIPTION_CHARS: usize = 1024;
pub const MAX_CATALOG_DESCRIPTION_CHARS: usize = 110;
pub const MAX_SKILL_CHARS: usize = 20_000;
pub const MAX_NAME_CHARS: usize = 64;

/// Palabras de enlace que no identifican una skill (ya sin tildes).
const SKILL_STOPWORDS: [&str; 38] = [
    "para", "por", "con", "sin", "que", "cual", "cuales", "como", "cuando", "donde", "desde",
    "hasta", "sobre", "entre", "este", "esta", "estos", "estas", "todo", "toda", "todos", "todas",
    "otro", "otra", "otros", "otras", "mas", "pero", "sus", "ser", "son", "fue", "han", "hay",
    "muy", "hola", "tal", "hacer",
];
const MORE_STOPWORDS: [&str; 4] = ["hace", "puede", "debe", "usar"];

fn name_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$").expect("patrón válido")
    })
}

fn token_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[a-z0-9]+").expect("patrón válido"))
}

/// Palabras normalizadas (sin tildes, en singular simple) para comparar skills.
pub fn skill_tokens(text: &str) -> HashSet<String> {
    let folded = fold_accents(text);
    token_re()
        .find_iter(&folded)
        .map(|found| found.as_str())
        .map(|token| {
            // Singular simple: «páginas» y «página» deben coincidir.
            if token.len() > 4 && token.ends_with('s') {
                token[..token.len() - 1].to_string()
            } else {
                token.to_string()
            }
        })
        .collect()
}

fn is_stopword(token: &str) -> bool {
    SKILL_STOPWORDS.contains(&token) || MORE_STOPWORDS.contains(&token) || token == "usa"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillErrorKind {
    Other,
    NotFound,
    File,
}

/// Error de skills con mensaje apto para el usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillError {
    pub kind: SkillErrorKind,
    pub message: String,
}

impl SkillError {
    pub fn new(kind: SkillErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn file(message: impl Into<String>) -> Self {
        Self::new(SkillErrorKind::File, message)
    }

    fn not_found(name: &str) -> Self {
        Self::new(
            SkillErrorKind::NotFound,
            format!("No existe la skill «{name}»."),
        )
    }
}

impl fmt::Display for SkillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SkillError {}

/// Metadatos de una skill descubierta (divulgación progresiva).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub body: String,
    pub files: Vec<String>,
    pub scripts: Vec<String>,
    pub warnings: Vec<String>,
    pub declared_name: String,
    pub source: String,
}

/// Contenido devuelto por `load_skill`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillContent {
    pub name: String,
    pub text: String,
    pub files: Vec<String>,
    pub path: String,
    pub file: Option<String>,
    pub truncated: bool,
}

type Fingerprint = Vec<(String, i128, i64)>;

#[derive(Default)]
struct Inner {
    skills: IndexMap<String, Skill>,
    warnings: Vec<String>,
    collisions: Vec<String>,
    fingerprint: Fingerprint,
    catalog_version: u64,
    keywords_version: Option<u64>,
    keywords: Arc<HashSet<String>>,
}

type AuditWarn = Box<dyn Fn(&str) + Send + Sync>;

/// Catálogo de skills descubiertas y accesibles.
pub struct SkillLibrary {
    skills_dir: PathBuf,
    global_skills_dir: PathBuf,
    audit_warn: Option<AuditWarn>,
    max_catalog: usize,
    inner: Mutex<Inner>,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SkillLibrary {
    pub fn new(
        skills_dir: &Path,
        global_skills_dir: &Path,
        audit_warn: Option<AuditWarn>,
        max_catalog: usize,
    ) -> Self {
        let library = Self {
            skills_dir: skills_dir.to_path_buf(),
            global_skills_dir: global_skills_dir.to_path_buf(),
            audit_warn,
            max_catalog,
            inner: Mutex::new(Inner::default()),
        };
        library.refresh(false);
        library
    }

    /// Vuelve a descubrir ambas carpetas si cambiaron o si `force`.
    ///
    /// Devuelve `true` si releyó el catálogo. Compara mtime y tamaño sin volver
    /// a leer los `SKILL.md` ni parsear YAML cuando nada cambió.
    pub fn refresh(&self, force: bool) -> bool {
        let mut inner = lock(&self.inner);
        let fingerprint = self.fingerprint_tree();
        if !force && fingerprint == inner.fingerprint {
            return false;
        }
        inner.fingerprint = fingerprint;
        inner.skills.clear();
        inner.warnings.clear();
        inner.collisions.clear();
        self.discover(&mut inner, &self.skills_dir, "local");
        self.discover(&mut inner, &self.global_skills_dir, "global");
        // Recalcular tras leer evita perder un cambio ocurrido durante el barrido.
        inner.fingerprint = self.fingerprint_tree();
        inner.catalog_version += 1;
        true
    }

    fn fingerprint_tree(&self) -> Fingerprint {
        let mut entries = Vec::new();
        for root in [&self.skills_dir, &self.global_skills_dir] {
            entries.extend(tree_signature(root));
        }
        entries
    }

    pub fn warnings(&self) -> Vec<String> {
        lock(&self.inner).warnings.clone()
    }

    pub fn collisions(&self) -> Vec<String> {
        lock(&self.inner).collisions.clone()
    }

    pub fn get(&self, name: &str) -> Option<Skill> {
        let cleaned = py_strip(name);
        let inner = lock(&self.inner);
        if let Some(skill) = inner.skills.get(cleaned) {
            return Some(skill.clone());
        }
        let lowered = cleaned.to_lowercase();
        inner
            .skills
            .iter()
            .find(|(known, _)| known.to_lowercase() == lowered)
            .map(|(_, skill)| skill.clone())
    }

    pub fn names(&self) -> Vec<String> {
        lock(&self.inner).skills.keys().cloned().collect()
    }

    pub fn all(&self) -> Vec<Skill> {
        lock(&self.inner).skills.values().cloned().collect()
    }

    /// Catálogo compacto para el LLM: máximo 20 skills.
    pub fn catalog_text(&self) -> String {
        let skills: Vec<Skill> = lock(&self.inner)
            .skills
            .values()
            .take(self.max_catalog)
            .cloned()
            .collect();
        if skills.is_empty() {
            return String::new();
        }
        let mut lines = vec!["<available_skills>".to_string()];
        for skill in &skills {
            let description = clip(&skill.description, MAX_CATALOG_DESCRIPTION_CHARS);
            lines.push(format!("- {}: {description}", skill.name));
        }
        lines.push("</available_skills>".into());
        lines.join("\n")
    }

    /// Palabras clave de nombres y descripciones, cacheadas por versión del catálogo.
    pub fn keywords(&self) -> Arc<HashSet<String>> {
        let mut inner = lock(&self.inner);
        if inner.keywords_version == Some(inner.catalog_version) {
            return Arc::clone(&inner.keywords);
        }
        let mut tokens: HashSet<String> = HashSet::new();
        for skill in inner.skills.values() {
            tokens.extend(skill_tokens(&skill.name));
            tokens.extend(skill_tokens(&skill.description));
        }
        let keywords: HashSet<String> = tokens
            .into_iter()
            .filter(|token| char_len(token) >= 3 && !is_stopword(token))
            .collect();
        inner.keywords = Arc::new(keywords);
        inner.keywords_version = Some(inner.catalog_version);
        Arc::clone(&inner.keywords)
    }

    /// Devuelve el cuerpo sin frontmatter o un archivo auxiliar de la skill.
    pub fn load_skill(&self, name: &str, file: Option<&str>) -> Result<SkillContent, SkillError> {
        let skill = self.get(name).ok_or_else(|| SkillError::not_found(name))?;
        let files = skill.files.clone();
        let wanted = file.filter(|file| !py_strip(file).is_empty());
        let Some(file) = wanted else {
            let (text, truncated) = truncate(&skill.body);
            return Ok(SkillContent {
                name: skill.name.clone(),
                text,
                files,
                path: skill.path.display().to_string(),
                file: None,
                truncated,
            });
        };
        // DECISION (del Python): cualquier archivo de texto dentro de la skill,
        // siempre que la ruta resuelta no salga de la carpeta ni lleve «..».
        let relative = safe_relative_file(file)?;
        let candidate = relative
            .split('/')
            .fold(skill.path.clone(), |path, part| path.join(part));
        let skill_root = resolve(&skill.path);
        let resolved = resolve_safe(
            &candidate.display().to_string(),
            &skill.path,
            &[skill.path.display().to_string()],
            &Vec::<String>::new(),
            &Vec::<String>::new(),
        )
        .map_err(|error| {
            SkillError::file(format!("No se puede leer ese archivo de la skill: {error}"))
        })?;
        if resolved != skill_root && !resolved.starts_with(&skill_root) {
            return Err(SkillError::file(
                "El archivo pedido está fuera de la carpeta de la skill.",
            ));
        }
        if !resolved.is_file() {
            return Err(SkillError::file(format!(
                "No existe el archivo «{relative}» en la skill «{}».",
                skill.name
            )));
        }
        let raw = std::fs::read(&resolved)
            .map_err(|error| SkillError::file(format!("No se pudo leer «{relative}»: {error}")))?;
        let decoded = String::from_utf8(raw)
            .map_err(|_| SkillError::file(format!("El archivo «{relative}» no es texto UTF-8.")))?;
        let decoded = decoded.strip_prefix('\u{feff}').unwrap_or(&decoded);
        // DECISION (del Python): el LLM ve siempre LF, aunque el archivo use CRLF.
        let text = decoded.replace("\r\n", "\n").replace('\r', "\n");
        let (text, truncated) = truncate(&text);
        let shown = resolved
            .strip_prefix(&skill_root)
            .map_or_else(|_| relative.clone(), posix);
        Ok(SkillContent {
            name: skill.name.clone(),
            text,
            files,
            path: skill.path.display().to_string(),
            file: Some(shown),
            truncated,
        })
    }

    /// SHA-256 determinista del contenido de la carpeta de la skill.
    pub fn folder_hash(&self, name: &str) -> Result<String, SkillError> {
        let skill = self.get(name).ok_or_else(|| SkillError::not_found(name))?;
        Ok(hash_folder(&skill.path))
    }

    fn warn(&self, inner: &mut Inner, message: &str) {
        inner.warnings.push(message.to_string());
        tracing::warn!("{message}");
        if let Some(audit) = &self.audit_warn {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| audit(message)));
            if run.is_err() {
                tracing::warn!("El callback de auditoría de skills falló");
            }
        }
    }

    fn discover(&self, inner: &mut Inner, root: &Path, source: &str) {
        if !root.is_dir() {
            return;
        }
        // DECISION (del Python): se ignoran carpetas ocultas y las que empiezan
        // por «_» (p. ej. `skills/_proposed/`): propuestas sin revisar.
        let mut folders: Vec<PathBuf> = match std::fs::read_dir(root) {
            Ok(listing) => listing
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() && !file_name(path).starts_with(['.', '_']))
                .collect(),
            Err(error) => {
                let message = format!(
                    "No se pudo leer la carpeta de skills {}: {error}",
                    root.display()
                );
                self.warn(inner, &message);
                return;
            }
        };
        folders.sort_by_key(|path| file_name(path).to_lowercase());
        for folder in folders {
            if !folder.join("SKILL.md").is_file() {
                continue;
            }
            let Some(skill) = self.read_skill(inner, &folder, source) else {
                continue;
            };
            if let Some(existing) = inner.skills.get(&skill.name) {
                let message = format!(
                    "La skill «{}» de {} se ignora: ya existe en {}.",
                    skill.name,
                    folder.display(),
                    existing.path.display()
                );
                inner.collisions.push(message.clone());
                self.warn(inner, &message);
                continue;
            }
            for warning in skill.warnings.clone() {
                self.warn(inner, &format!("Skill «{}»: {warning}", skill.name));
            }
            inner.skills.insert(skill.name.clone(), skill);
        }
        if inner.skills.len() > self.max_catalog {
            let message = format!(
                "Hay {} skills; el catálogo solo mostrará las primeras {}.",
                inner.skills.len(),
                self.max_catalog
            );
            self.warn(inner, &message);
        }
    }

    fn read_skill(&self, inner: &mut Inner, folder: &Path, source: &str) -> Option<Skill> {
        let folder_name = file_name(folder);
        let raw = match std::fs::read(folder.join("SKILL.md")) {
            Ok(raw) => raw,
            Err(error) => {
                let message = format!("Skill «{folder_name}»: no se pudo leer SKILL.md: {error}");
                self.warn(inner, &message);
                return None;
            }
        };
        let Ok(text) = String::from_utf8(raw) else {
            let message =
                format!("Skill «{folder_name}»: SKILL.md no es UTF-8 válido; se descarta.");
            self.warn(inner, &message);
            return None;
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        // DECISION (del Python): se normalizan CRLF/CR a LF antes del frontmatter.
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let Some((meta_text, body)) = split_frontmatter(&text) else {
            let message = format!("Skill «{folder_name}»: frontmatter YAML ilegible; se descarta.");
            self.warn(inner, &message);
            return None;
        };
        let metadata = match serde_yaml::from_str::<Value>(&meta_text) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                let message = format!(
                    "Skill «{folder_name}»: el frontmatter no es un mapa YAML; se descarta."
                );
                self.warn(inner, &message);
                return None;
            }
            Err(error) => {
                // DECISION: serde_yaml no expone la clase de PyYAML; se aproxima
                // por el mensaje (una secuencia de flujo sin cerrar es ParserError).
                let detail = error.to_string();
                let kind = if detail.contains("expected ',' or ']'")
                    || detail.contains("expected ',' or '}'")
                {
                    "ParserError"
                } else {
                    "ScannerError"
                };
                let message = format!(
                    "Skill «{folder_name}»: frontmatter YAML ilegible ({kind}); se descarta."
                );
                self.warn(inner, &message);
                return None;
            }
        };
        let description = metadata
            .get("description")
            .and_then(Value::as_str)
            .filter(|text| !py_strip(text).is_empty());
        let Some(description) = description else {
            let message = format!("Skill «{folder_name}»: falta «description»; se descarta.");
            self.warn(inner, &message);
            return None;
        };
        let mut description = collapse_spaces(description);
        let mut warnings: Vec<String> = Vec::new();
        if char_len(&description) > MAX_DESCRIPTION_CHARS {
            warnings.push(format!(
                "la descripción supera {MAX_DESCRIPTION_CHARS} caracteres; se recorta."
            ));
            description =
                ez_core::pyvalue::py_rstrip(head_chars(&description, MAX_DESCRIPTION_CHARS))
                    .to_string();
        }
        let declared_name = metadata
            .get("name")
            .and_then(Value::as_str)
            .map_or_else(String::new, |name| py_strip(name).to_string());
        if declared_name.is_empty() {
            warnings.push("no declara «name»; se usa el nombre de la carpeta.".into());
        } else {
            if declared_name != folder_name {
                warnings.push(format!(
                    "el name «{declared_name}» no coincide con la carpeta «{folder_name}»."
                ));
            }
            if !valid_name(&declared_name) {
                warnings.push(
                    "el name no cumple 1-64 caracteres en minúsculas, números y guiones.".into(),
                );
            }
        }
        if !valid_name(&folder_name) {
            warnings.push(
                "el nombre de la carpeta no cumple 1-64 caracteres en minúsculas, números y guiones."
                    .into(),
            );
        }
        let files = list_files(folder);
        let scripts = files
            .iter()
            .filter(|item| item.starts_with("scripts/"))
            .cloned()
            .collect();
        Some(Skill {
            name: folder_name,
            description,
            path: folder.to_path_buf(),
            body: py_strip(&body).to_string(),
            files,
            scripts,
            warnings,
            declared_name,
            source: source.to_string(),
        })
    }
}

mod files;
pub use files::{default_skills_dir, ensure_default_skills, hash_folder};
use files::{file_name, list_files, posix, tree_signature, truncate};

/// `(frontmatter, cuerpo)` si el texto abre y cierra con `---`.
fn split_frontmatter(text: &str) -> Option<(String, String)> {
    if !text.starts_with("---") {
        return None;
    }
    // DECISION: el texto ya viene con LF; se parte por `\n` (el Python también
    // parte en otros separadores raros que no aparecen en un frontmatter).
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if py_strip(lines.first()?) != "---" {
        return None;
    }
    let close = (1..lines.len()).find(|index| py_strip(lines[*index]) == "---")?;
    Some((lines[1..close].concat(), lines[close + 1..].concat()))
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_CHARS).contains(&char_len(name)) && name_re().is_match(name)
}

fn safe_relative_file(raw: &str) -> Result<String, SkillError> {
    let normalized = py_strip(raw).replace('\\', "/");
    if normalized.is_empty() {
        return Err(SkillError::file(
            "Falta la ruta del archivo dentro de la skill.",
        ));
    }
    let mut chars = normalized.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':');
    if normalized.starts_with('/') || drive {
        return Err(SkillError::file(
            "La ruta del archivo debe ser relativa a la carpeta de la skill.",
        ));
    }
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() {
        return Err(SkillError::file("La ruta del archivo está vacía."));
    }
    if parts.contains(&"..") {
        return Err(SkillError::file(
            "La ruta del archivo no puede contener «..».",
        ));
    }
    Ok(parts.join("/"))
}

fn clip(text: &str, limit: usize) -> String {
    if char_len(text) <= limit {
        return text.to_string();
    }
    if limit <= 1 {
        return head_chars(text, limit).to_string();
    }
    format!("{}…", head_chars(text, limit - 1))
}

#[cfg(test)]
mod tests;
