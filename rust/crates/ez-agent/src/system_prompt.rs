//! Port of `src/ezagent/agent/system_prompt.py` (F3-05).
//!
//! Ensamblado del system prompt y recarga en caliente de `system.md`. El orden
//! es fijo: preámbulo de seguridad inmutable -> `system.md` -> guía de voz ->
//! memoria -> catálogo de skills -> momento actual. El preámbulo vive en el
//! código y nunca se puede sustituir desde un archivo editable.
//!
//! Lo que cambia a cada minuto (la hora) va al final: los proveedores cachean
//! el prompt por prefijo y una hora a mitad del texto invalida todo lo que sigue.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{Local, NaiveDateTime};
use ez_core::pyvalue::{char_len, py_lstrip, py_splitlines, py_strip, strip_accents};
use regex::{Captures, Regex};

use crate::orchestration::ORCHESTRATION_HINT;

pub const MAX_SYSTEM_MD_CHARS: usize = 6000;
pub const SYSTEM_MD_ERROR_MESSAGE: &str =
    "Mi archivo system.md tiene un error; sigo con la versión anterior.";

/// Reglas de seguridad fijadas en código (no se pueden mutar desde fuera).
pub const SAFETY_PREAMBLE_TEXT: &str = include_str!("text/safety_preamble.txt");
/// Instrucción fija de la sección 8.3 del documento.
pub const SKILLS_HINT: &str = include_str!("text/skills_hint.txt");
/// Guía fija de conversación por voz: sin muletillas de espera.
pub const VOICE_FLOW_HINT: &str = include_str!("text/voice_flow_hint.txt");
// DECISION (del Python): la guía de ejecución vive en código para que los
// `system.md` existentes reciban el arreglo sin sobrescribirse.
pub const EXECUTION_HINT: &str = include_str!("text/execution_hint.txt");
/// Plantilla por defecto de `system.md`; el usuario puede editarla.
pub const DEFAULT_SYSTEM_MD: &str = include_str!("text/default_system_md.txt");

// DECISION: `platform.system() + platform.release()` en la máquina del oráculo
// da «Windows 11» (ver `python-quirks.md`); la app puede sustituirlo con
// `SystemPromptBuilder::set_os_description` usando la versión real.
const DEFAULT_OS: &str = "Windows 11";

const BYPASS_PHRASES: [&str; 24] = [
    "sin confirmar",
    "sin confirmacion",
    "sin preguntar",
    "sin pedir permiso",
    "ignora las reglas",
    "ignora la politica",
    "ignorar las reglas",
    "ignorar la politica",
    "borra directamente",
    "elimina directamente",
    "borrar directamente",
    "no pidas confirmacion",
    "no pidas permiso",
    "no confirmes",
    "salta la politica",
    "saltarse la politica",
    "saltate la politica",
    "desactiva la politica",
    "desactivar la politica",
    "omite la confirmacion",
    "omitir la confirmacion",
    "confirma automaticamente",
    "auto-confirma",
    "autoconfirma",
];

fn clock_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{\{\s*hora\s*\}\}").expect("patrón válido"))
}

fn placeholder_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{\{([^{}]+)\}\}").expect("patrón válido"))
}

/// Separa las líneas con `{{hora}}` para llevarlas al final del prompt.
/// Si un encabezado se queda sin contenido, también se quita.
fn split_clock_lines(text: &str) -> (String, String) {
    let (mut kept, mut clock): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    for line in py_splitlines(text) {
        if clock_re().is_match(line) {
            clock.push(line);
        } else {
            kept.push(line);
        }
    }
    if clock.is_empty() {
        return (text.to_string(), String::new());
    }
    let mut result: Vec<&str> = Vec::new();
    for (index, line) in kept.iter().enumerate() {
        if py_lstrip(line).starts_with('#') {
            let following = kept[index + 1..]
                .iter()
                .find(|rest| !py_strip(rest).is_empty());
            if following.is_none_or(|next| py_lstrip(next).starts_with('#')) {
                continue;
            }
        }
        result.push(line);
    }
    (
        format!("{}\n", py_strip(&result.join("\n"))),
        clock.join("\n"),
    )
}

/// Texto o función que devuelve el texto de una sección.
pub enum Section {
    Text(String),
    Func(Box<dyn Fn() -> String + Send + Sync>),
}

type Notify = Box<dyn Fn(&str) + Send + Sync>;

/// Construye el prompt de cada turno y vigila `system.md`.
///
/// `build()` se llama al inicio de cada turno: compara la fecha de
/// modificación del archivo y recarga si cambió, sin hilos ni *watchers*.
#[derive(Default)]
pub struct Callbacks {
    pub memory_block: Option<Section>,
    pub skills_catalog: Option<Section>,
    /// Si existe se añade también la guía fija de orquestación.
    pub agents_catalog: Option<Section>,
    pub agent_windows: Option<Section>,
    pub audit_warn: Option<Notify>,
    /// Los errores de `system.md` se avisan una sola vez por episodio inválido.
    pub say_warn: Option<Notify>,
}

pub struct SystemPromptBuilder {
    path: PathBuf,
    callbacks: Callbacks,
    os_description: String,
    last_valid_text: Option<String>,
    last_key: Option<(i128, u64)>,
    warned_invalid: bool,
    missing_announced: bool,
    logged_unknown: HashSet<String>,
    logged_contradiction_key: Option<(i128, u64)>,
}

impl SystemPromptBuilder {
    pub fn new(system_md_path: &Path, callbacks: Callbacks) -> Self {
        let mut builder = Self {
            path: system_md_path.to_path_buf(),
            callbacks,
            os_description: DEFAULT_OS.to_string(),
            last_valid_text: None,
            last_key: None,
            warned_invalid: false,
            missing_announced: false,
            logged_unknown: HashSet::new(),
            logged_contradiction_key: None,
        };
        builder.ensure_system_md();
        builder.reload_if_changed();
        builder
    }

    pub fn set_os_description(&mut self, text: &str) {
        self.os_description = text.to_string();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn last_valid_text(&self) -> Option<&str> {
        self.last_valid_text.as_deref()
    }

    /// El prompt completo; `now` permite fijar fecha y hora.
    pub fn build(&mut self, now: Option<NaiveDateTime>) -> String {
        let (stable, volatile) = self.build_parts(now);
        [stable, volatile]
            .iter()
            .filter(|part| !py_strip(part).is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// `(prefijo estable, datos volátiles)` para conservar la caché por prefijo.
    ///
    /// El prefijo agrupa política, `system.md`, guías y catálogos. Memoria,
    /// ventanas de Herdr y hora van aparte y se adjuntan al turno del usuario.
    pub fn build_parts(&mut self, now: Option<NaiveDateTime>) -> (String, String) {
        self.reload_if_changed();
        let moment = now.unwrap_or_else(|| Local::now().naive_local());
        let source = self
            .last_valid_text
            .clone()
            .unwrap_or_else(|| DEFAULT_SYSTEM_MD.into());
        let (stable_text, clock) = split_clock_lines(&source);
        let system_text = self.render_system_md(&stable_text, moment);

        let mut parts: Vec<String> = vec![
            SAFETY_PREAMBLE_TEXT.into(),
            system_text,
            VOICE_FLOW_HINT.into(),
            EXECUTION_HINT.into(),
        ];
        let agents = Self::section(self.callbacks.agents_catalog.as_ref());
        if !agents.is_empty() {
            parts.push(ORCHESTRATION_HINT.into());
            parts.push(agents);
        }
        let catalog = Self::section(self.callbacks.skills_catalog.as_ref());
        if !catalog.is_empty() {
            parts.push(catalog);
            parts.push(SKILLS_HINT.into());
        }
        let mut volatile: Vec<String> = Vec::new();
        for section in [&self.callbacks.memory_block, &self.callbacks.agent_windows] {
            let text = Self::section(section.as_ref());
            if !text.is_empty() {
                volatile.push(text);
            }
        }
        if !clock.is_empty() {
            let rendered = self.render_system_md(&clock, moment);
            volatile.push(format!("# Momento actual\n{rendered}"));
        }
        let join = |items: Vec<String>| {
            items
                .iter()
                .map(|part| py_strip(part))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        (join(parts), join(volatile))
    }

    /// Recarga `system.md` si cambió. `true` si hay versión nueva.
    pub fn reload_if_changed(&mut self) -> bool {
        let Some(key) = self.stat_key() else {
            self.handle_missing();
            return false;
        };
        self.missing_announced = false;
        if Some(key) == self.last_key {
            return false;
        }
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(error) => {
                // No fijamos la clave: reintentamos en el siguiente turno.
                self.handle_invalid(&format!("no se pudo leer ({error})"));
                return false;
            }
        };
        let Ok(decoded) = String::from_utf8(raw) else {
            self.last_key = Some(key);
            self.handle_invalid("no es UTF-8 válido");
            return false;
        };
        let decoded = decoded.strip_prefix('\u{feff}').unwrap_or(&decoded);
        // DECISION (del Python): se normalizan los finales de línea antes de
        // contar; un `system.md` editado en Windows usa CRLF.
        let text = decoded.replace("\r\n", "\n").replace('\r', "\n");
        if py_strip(&text).is_empty() {
            self.last_key = Some(key);
            self.handle_invalid("está vacío");
            return false;
        }
        if char_len(&text) > MAX_SYSTEM_MD_CHARS {
            self.last_key = Some(key);
            self.handle_invalid(&format!(
                "supera el máximo de {MAX_SYSTEM_MD_CHARS} caracteres"
            ));
            return false;
        }
        self.last_key = Some(key);
        let valid = py_strip(&text).to_string();
        self.warned_invalid = false;
        self.logged_unknown.clear();
        self.detect_policy_bypass(&valid, key);
        self.last_valid_text = Some(valid);
        true
    }

    fn ensure_system_md(&self) {
        if self.path.exists() {
            return;
        }
        // `Path.write_text` traduce `\n` al final de línea del sistema.
        let body = if cfg!(windows) {
            DEFAULT_SYSTEM_MD.replace('\n', "\r\n")
        } else {
            DEFAULT_SYSTEM_MD.to_string()
        };
        let written = self
            .path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&self.path, body));
        if let Err(error) = written {
            tracing::warn!(
                "No se pudo crear system.md en {}: {error}",
                self.path.display()
            );
            self.audit("No se pudo crear system.md; se usa la plantilla en memoria.");
        }
    }

    fn stat_key(&self) -> Option<(i128, u64)> {
        let meta = std::fs::metadata(&self.path).ok()?;
        let nanos = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as i128);
        Some((nanos, meta.len()))
    }

    fn handle_missing(&mut self) {
        if self.last_valid_text.is_none() {
            self.last_valid_text = Some(DEFAULT_SYSTEM_MD.into());
        }
        if !self.missing_announced {
            self.missing_announced = true;
            self.audit(
                "system.md no existe; se conserva la última versión válida (o la plantilla por defecto).",
            );
            self.warn_once();
        }
    }

    fn handle_invalid(&mut self, reason: &str) {
        if self.last_valid_text.is_none() {
            self.last_valid_text = Some(DEFAULT_SYSTEM_MD.into());
            self.audit(&format!(
                "system.md no es válido ({reason}); uso la plantilla por defecto."
            ));
        } else {
            self.audit(&format!(
                "system.md no es válido ({reason}); conservo la última versión válida."
            ));
        }
        self.warn_once();
    }

    fn warn_once(&mut self) {
        if self.warned_invalid {
            return;
        }
        self.warned_invalid = true;
        Self::call(self.callbacks.say_warn.as_ref(), SYSTEM_MD_ERROR_MESSAGE);
    }

    fn render_system_md(&mut self, text: &str, now: NaiveDateTime) -> String {
        let fecha = now.format("%d/%m/%Y").to_string();
        let hora = now.format("%H:%M").to_string();
        let os = self.os_description.clone();
        let mut unknown: Vec<String> = Vec::new();
        let rendered = placeholder_re()
            .replace_all(text, |captures: &Captures<'_>| {
                let key = py_strip(&captures[1]).to_string();
                match key.as_str() {
                    "fecha" => fecha.clone(),
                    "hora" => hora.clone(),
                    "os" => os.clone(),
                    _ => {
                        unknown.push(key);
                        captures[0].to_string()
                    }
                }
            })
            .into_owned();
        for key in unknown {
            if self.logged_unknown.insert(key.clone()) {
                self.audit(&format!(
                    "system.md contiene una variable desconocida {{{{{key}}}}}; se deja tal cual."
                ));
            }
        }
        rendered
    }

    fn detect_policy_bypass(&mut self, text: &str, key: (i128, u64)) {
        if self.logged_contradiction_key == Some(key) {
            return;
        }
        let normalized = strip_accents(text).to_lowercase();
        let found: Vec<&str> = BYPASS_PHRASES
            .iter()
            .copied()
            .filter(|phrase| normalized.contains(phrase))
            .collect();
        if found.is_empty() {
            return;
        }
        self.logged_contradiction_key = Some(key);
        let sample = found
            .iter()
            .take(4)
            .map(|p| format!("«{p}»"))
            .collect::<Vec<_>>()
            .join(", ");
        self.audit(&format!(
            "system.md intenta saltarse la política ({sample}); se registra la contradicción y la política sigue mandando."
        ));
        tracing::warn!("system.md contiene frases que contradicen la política: {sample}");
    }

    fn section(source: Option<&Section>) -> String {
        let value = match source {
            None => return String::new(),
            Some(Section::Text(text)) => text.clone(),
            Some(Section::Func(func)) => {
                // Un callback roto no tumba el turno.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(func)) {
                    Ok(text) => text,
                    Err(_) => {
                        tracing::warn!("No se pudo construir una sección del prompt");
                        return String::new();
                    }
                }
            }
        };
        py_strip(&value).to_string()
    }

    fn audit(&self, message: &str) {
        Self::call(self.callbacks.audit_warn.as_ref(), message);
    }

    fn call(callback: Option<&Notify>, message: &str) {
        let Some(callback) = callback else {
            return;
        };
        // Los avisos son best-effort.
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(message)));
        if run.is_err() {
            tracing::warn!("Un callback de aviso falló");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn temp_md(name: &str, text: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ez-prompt-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("system.md");
        let _ = std::fs::remove_file(&path);
        if let Some(text) = text {
            let _ = std::fs::write(&path, text);
        }
        path
    }

    fn moment() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 14)
            .and_then(|d| d.and_hms_opt(9, 5, 0))
            .expect("fecha válida")
    }

    #[test]
    fn default_template_splits_clock_line_to_the_end() {
        let path = temp_md("default", None);
        let mut builder = SystemPromptBuilder::new(&path, Callbacks::default());
        assert!(path.exists());
        let (stable, volatile) = builder.build_parts(Some(moment()));
        assert!(stable.starts_with("# Política de seguridad (inmutable)"));
        assert!(stable.contains("# Identidad\nEres EzAgent"));
        assert!(!stable.contains("# Contexto"));
        assert_eq!(
            volatile,
            "# Momento actual\nHoy es 14/11/2026, son las 09:05 y uso Windows 11."
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_files_keep_the_last_valid_version_and_warn_once() {
        let path = temp_md("invalid", Some("# Mi prompt\nHola {{extra}}"));
        let audit: Arc<Mutex<Vec<String>>> = Arc::default();
        let spoken: Arc<Mutex<Vec<String>>> = Arc::default();
        let (a, s) = (Arc::clone(&audit), Arc::clone(&spoken));
        let callbacks = Callbacks {
            audit_warn: Some(Box::new(move |m| a.lock().expect("lock").push(m.into()))),
            say_warn: Some(Box::new(move |m| s.lock().expect("lock").push(m.into()))),
            ..Callbacks::default()
        };
        let mut builder = SystemPromptBuilder::new(&path, callbacks);
        assert!(builder.build(Some(moment())).contains("Hola {{extra}}"));
        std::fs::write(&path, "   ").expect("escribir");
        assert!(!builder.reload_if_changed());
        assert!(builder.build(Some(moment())).contains("Hola {{extra}}"));
        std::fs::write(&path, "x".repeat(6001)).expect("escribir");
        builder.reload_if_changed();
        assert_eq!(spoken.lock().expect("lock").len(), 1);
        let audit = audit.lock().expect("lock");
        assert!(audit[0].contains("variable desconocida {{extra}}"));
        assert!(audit.iter().any(|m| m.contains("está vacío")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn detects_policy_bypass_phrases_without_changing_the_policy() {
        let path = temp_md(
            "bypass",
            Some("Borra sin PREGUNTAR y no pidas confirmación."),
        );
        let audit: Arc<Mutex<Vec<String>>> = Arc::default();
        let a = Arc::clone(&audit);
        let callbacks = Callbacks {
            audit_warn: Some(Box::new(move |m| a.lock().expect("lock").push(m.into()))),
            ..Callbacks::default()
        };
        let mut builder = SystemPromptBuilder::new(&path, callbacks);
        let prompt = builder.build(Some(moment()));
        assert!(prompt.starts_with("# Política de seguridad (inmutable)"));
        let audit = audit.lock().expect("lock");
        assert_eq!(audit.len(), 1);
        assert!(audit[0].contains("«sin preguntar», «no pidas confirmacion»"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sections_are_ordered() {
        let path = temp_md("order", Some("Cuerpo"));
        let callbacks = Callbacks {
            memory_block: Some(Section::Text("# Memoria\nx".into())),
            skills_catalog: Some(Section::Func(Box::new(|| "# Skills\ny".into()))),
            agents_catalog: Some(Section::Text("# Agentes\nz".into())),
            ..Callbacks::default()
        };
        let mut builder = SystemPromptBuilder::new(&path, callbacks);
        let (stable, volatile) = builder.build_parts(Some(moment()));
        let order = [
            "Cuerpo",
            "# Orquestación de agentes",
            "# Agentes",
            "# Skills",
            "Si una tarea coincide",
        ];
        let positions: Vec<usize> = order.iter().filter_map(|t| stable.find(t)).collect();
        assert_eq!(positions.len(), order.len());
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(volatile, "# Memoria\nx");
        let _ = std::fs::remove_file(&path);
    }
}
