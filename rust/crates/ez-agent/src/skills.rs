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
use sha2::{Digest, Sha256};

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
            .map_or_else(|_| relative.clone(), |rest| posix(rest));
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
            Err(_) => {
                // DECISION: serde_yaml no distingue la clase del error de PyYAML.
                let message = format!(
                    "Skill «{folder_name}»: frontmatter YAML ilegible (ScannerError); se descarta."
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn posix(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn truncate(text: &str) -> (String, bool) {
    if char_len(text) > MAX_SKILL_CHARS {
        (head_chars(text, MAX_SKILL_CHARS).to_string(), true)
    } else {
        (text.to_string(), false)
    }
}

fn mtime_ns(meta: &std::fs::Metadata) -> i128 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_nanos() as i128)
}

fn path_signature(path: &Path) -> (String, i128, i64) {
    match std::fs::metadata(path) {
        Ok(meta) => (
            path.display().to_string(),
            mtime_ns(&meta),
            meta.len() as i64,
        ),
        Err(_) => (path.display().to_string(), -1, -1),
    }
}

/// Todas las rutas bajo `root` (sin seguir enlaces a carpetas), ordenadas por ruta posix.
fn walk(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(listing) = std::fs::read_dir(root) else {
        return;
    };
    for entry in listing.filter_map(Result::ok) {
        let path = entry.path();
        out.push(path.clone());
        let is_real_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_real_dir {
            walk(&path, out);
        }
    }
}

fn sorted_tree(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    walk(root, &mut paths);
    paths.sort_by_key(|path| posix(path));
    paths
}

/// mtime y tamaño de la carpeta y de todo su contenido, sin leerlo.
fn tree_signature(root: &Path) -> Fingerprint {
    let mut signature = vec![path_signature(root)];
    signature.extend(sorted_tree(root).iter().map(|path| path_signature(path)));
    signature
}

fn is_regular_file(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|meta| meta.is_file())
}

fn relative_posix(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(|_| posix(path), posix)
}

/// SHA-256 de todos los archivos de una carpeta, con rutas relativas.
pub fn hash_folder(folder: &Path) -> String {
    let mut files: Vec<PathBuf> = sorted_tree(folder)
        .into_iter()
        .filter(|path| is_regular_file(path))
        .collect();
    files.sort_by_key(|path| relative_posix(folder, path));
    let mut digest = Sha256::new();
    for path in files {
        digest.update(relative_posix(folder, &path).as_bytes());
        digest.update([0_u8]);
        match std::fs::read(&path) {
            Ok(bytes) => digest.update(&bytes),
            Err(_) => digest.update(b"<unreadable>"),
        }
        digest.update([0_u8]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn list_files(folder: &Path) -> Vec<String> {
    let mut files: Vec<String> = sorted_tree(folder)
        .into_iter()
        .filter(|path| is_regular_file(path) && file_name(path) != "SKILL.md")
        .map(|path| relative_posix(folder, &path))
        .collect();
    files.sort();
    files
}

/// Copia un árbol de carpetas (el `shutil.copytree` de `ensure_default_skills`).
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ())
}

/// Carpeta de las skills empaquetadas junto al ejecutable (o, en desarrollo, en el repo).
pub fn default_skills_dir() -> PathBuf {
    // DECISION: el Python usa `agent/default_skills` junto al módulo; aquí se
    // busca junto al ejecutable y, si no, en el árbol de código fuente.
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("default_skills")));
    match beside.filter(|dir| dir.is_dir()) {
        Some(dir) => dir,
        None => {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/ezagent/agent/default_skills")
        }
    }
}

/// Copia las skills empaquetadas a `skills_dir` si faltan; nunca sobrescribe.
/// Devuelve los nombres copiados.
pub fn ensure_default_skills(skills_dir: &Path, source_dir: Option<&Path>) -> Vec<String> {
    let source = source_dir.map_or_else(default_skills_dir, Path::to_path_buf);
    if !source.is_dir() {
        tracing::warn!(
            "No existe la carpeta de skills empaquetadas: {}",
            source.display()
        );
        return Vec::new();
    }
    if let Err(error) = std::fs::create_dir_all(skills_dir) {
        tracing::warn!(
            "No se pudo preparar la carpeta de skills {}: {error}",
            skills_dir.display()
        );
        return Vec::new();
    }
    let Ok(listing) = std::fs::read_dir(&source) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path.join("SKILL.md").is_file()
                && !file_name(path).starts_with(['.', '_'])
        })
        .collect();
    folders.sort_by_key(|path| file_name(path).to_lowercase());
    let mut copied = Vec::new();
    for folder in folders {
        let destination = skills_dir.join(file_name(&folder));
        if destination.exists() {
            continue;
        }
        // No se borra una copia parcial: se registra para que el usuario la revise.
        match copy_tree(&folder, &destination) {
            Ok(()) => copied.push(file_name(&folder)),
            Err(error) => tracing::warn!(
                "No se pudo copiar la skill «{}» a {}: {error}",
                file_name(&folder),
                destination.display()
            ),
        }
    }
    copied
}

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
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ez-skills-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn write_skill(root: &Path, folder: &str, front: &str, body: &str) {
        let dir = root.join(folder);
        let _ = std::fs::create_dir_all(dir.join("references"));
        let _ = std::fs::write(dir.join("SKILL.md"), format!("---\n{front}\n---\n{body}\n"));
        let _ = std::fs::write(dir.join("references").join("guia.md"), "guía\r\nlarga");
    }

    #[test]
    fn discovers_validates_and_loads() {
        let root = scratch("load");
        let (local, global) = (root.join("local"), root.join("global"));
        write_skill(
            &local,
            "hojas-excel",
            "name: hojas-excel\ndescription: Crea hojas   de cálculo",
            "# Pasos\n1. a",
        );
        write_skill(&global, "hojas-excel", "description: duplicada", "x");
        write_skill(
            &global,
            "Raro",
            "name: otro\ndescription: Una skill rara",
            "y",
        );
        write_skill(&local, "_proposed", "description: oculta", "z");
        write_skill(&local, "sin-desc", "name: sin-desc", "z");
        let library = SkillLibrary::new(&local, &global, None, MAX_CATALOG);
        assert_eq!(
            library.names(),
            vec!["hojas-excel".to_string(), "Raro".to_string()]
        );
        assert_eq!(library.collisions().len(), 1);
        let warnings = library.warnings().join("\n");
        assert!(warnings.contains("falta «description»"));
        assert!(warnings.contains("el name «otro» no coincide con la carpeta «Raro»"));
        assert!(library
            .catalog_text()
            .starts_with("<available_skills>\n- hojas-excel: Crea hojas de cálculo\n"));
        let content = library.load_skill("HOJAS-EXCEL", None).expect("cuerpo");
        assert_eq!(content.text, "# Pasos\n1. a");
        assert_eq!(content.files, vec!["references/guia.md".to_string()]);
        let file = library
            .load_skill("hojas-excel", Some("references\\guia.md"))
            .expect("archivo");
        assert_eq!(file.text, "guía\nlarga");
        assert_eq!(file.file.as_deref(), Some("references/guia.md"));
        assert_eq!(
            library
                .load_skill("hojas-excel", Some("../x"))
                .unwrap_err()
                .message,
            "La ruta del archivo no puede contener «..»."
        );
        assert_eq!(
            library.load_skill("nada", None).unwrap_err().kind,
            SkillErrorKind::NotFound
        );
        assert!(library.keywords().contains("hoja"));
        assert!(!library.refresh(false));
        assert_eq!(library.folder_hash("hojas-excel").expect("hash").len(), 64);
        for dir in [&local, &global] {
            let _ =
                std::fs::remove_file(dir.join("hojas-excel").join("references").join("guia.md"));
        }
    }

    #[test]
    fn tokens_are_folded_and_singular() {
        let tokens = skill_tokens("Páginas PDF, página");
        assert!(tokens.contains("pagina") && tokens.contains("pdf"));
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn splits_frontmatter() {
        assert_eq!(
            split_frontmatter("---\na: 1\n---\ncuerpo"),
            Some(("a: 1\n".into(), "cuerpo".into()))
        );
        assert_eq!(split_frontmatter("hola"), None);
        assert_eq!(split_frontmatter("---\na: 1\n"), None);
    }
}
