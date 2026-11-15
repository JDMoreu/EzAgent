//! Contexto largo de Herdr: mensajes que no caben en la línea de comandos y
//! archivos de handoff de EzAgent.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ez_core::pyloads::loads;
use ez_core::pyvalue::{head_chars, py_strip, python_str, truthy};
use serde_json::{Map, Value};

use super::{error_row, Herdr, HerdrError, LONG_TEXT_CHARS};

impl Herdr {
    /// `hc send`: pega un mensaje en la terminal.
    pub fn send_agent(
        &self,
        address: &str,
        text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        let text = if text.chars().count() > LONG_TEXT_CHARS {
            match self.stage_long_text(address, text) {
                Some(staged) => staged,
                None => {
                    return Ok(error_row(
                        "El mensaje es demasiado largo y no pude guardarlo \
                         en un archivo para entregarlo.",
                    ));
                }
            }
        } else {
            text.to_string()
        };
        // DECISION: `hc send` marca el mensaje con <hc from="tN"> cuando se
        // invoca desde una terminal de Herdr (lo sabe por HC_TERMINAL_ID). Al
        // agente le llegaría ese texto literal; sin la variable llega limpio.
        let argv = vec![
            self.hc_path(),
            "send".to_string(),
            address.to_string(),
            text,
            "--json".to_string(),
        ];
        let result = self.run_command(&argv, 40.0, true)?;
        let output = py_strip(&result.stdout).to_string();
        if result.returncode != 0 {
            let message = if result.stderr.is_empty() {
                output
            } else {
                result.stderr
            };
            return Ok(error_row(head_chars(&message, 800).to_string()));
        }
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert("address".to_string(), Value::String(address.to_string()));
        row.insert(
            "message".to_string(),
            Value::String("Mensaje enviado.".to_string()),
        );
        Ok(row)
    }

    /// Guarda un mensaje largo, lo envía con `hc put` y devuelve la frase
    /// corta que se pega al agente (con la ruta donde quedó el archivo).
    fn stage_long_text(&self, address: &str, text: &str) -> Option<String> {
        self.stage_long_text_in(&handoff_dir(), address, text, &self.runtime.next_id())
    }

    fn stage_long_text_in(
        &self,
        folder: &Path,
        address: &str,
        text: &str,
        id: &str,
    ) -> Option<String> {
        let path = folder.join(format!("mensaje-{id}.md"));
        if std::fs::create_dir_all(folder).is_err() || write_text(&path, text).is_err() {
            return None;
        }
        let landed_path = path.to_string_lossy().into_owned();
        let result = self
            .run_command(
                &[
                    self.hc_path(),
                    "put".to_string(),
                    address.to_string(),
                    landed_path.clone(),
                    "--json".to_string(),
                ],
                40.0,
                true,
            )
            .ok()?;
        if result.returncode != 0 {
            return None;
        }
        let landed = match loads(py_strip(&result.stdout)) {
            Ok(Value::Object(map)) => match map.get("path").filter(|value| truthy(value)) {
                Some(value) => python_str(value),
                None => landed_path,
            },
            _ => landed_path,
        };
        Some(format!(
            "El mensaje completo es demasiado largo para pegarlo aquí. \
             Léelo entero en este archivo y haz lo que indica: {landed}"
        ))
    }

    /// Guarda un contexto largo como .md y devuelve su ruta (o None).
    ///
    /// Es la carpeta propia de EzAgent; los archivos de más de 7 días se
    /// retiran de ella al guardar uno nuevo.
    pub fn save_context(&self, text: &str) -> Option<String> {
        save_context_in(&handoff_dir(), text, &self.runtime.next_id())
    }
}

/// `%LOCALAPPDATA%/EzAgent/handoff` (`Path.home()` si no hay variable).
pub(super) fn handoff_dir() -> PathBuf {
    let base = match std::env::var_os("LOCALAPPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base),
        _ => ez_core::dirs::home(),
    };
    base.join("EzAgent").join("handoff")
}

pub(super) fn is_handoff_file(name: &str) -> bool {
    name.ends_with(".md") && (name.starts_with("contexto-") || name.starts_with("mensaje-"))
}

/// `save_context` con la carpeta y el id inyectados (pruebas).
pub(super) fn save_context_in(folder: &Path, text: &str, id: &str) -> Option<String> {
    let path = folder.join(format!("contexto-{id}.md"));
    if std::fs::create_dir_all(folder).is_err() || write_text(&path, text).is_err() {
        return None;
    }
    let cutoff = ez_core::pytime::timestamp() - 7.0 * 86_400.0;
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            if !is_handoff_file(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let seconds = modified
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs_f64())
                .unwrap_or(0.0);
            if seconds < cutoff {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Some(path.to_string_lossy().into_owned())
}

/// `Path.write_text`: en Windows traduce los finales de línea.
fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    let text = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    std::fs::write(path, text.as_bytes())
}

/// `uuid.uuid4().hex[:8]` sin dependencia de `uuid`.
///
/// DECISION: basta un sufijo único por proceso (nanosegundos + contador y una
/// mezcla de 64 bits); el nombre solo evita pisar archivos de handoff.
pub(super) fn hex_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let mut value = nanos
        ^ (u64::from(std::process::id()) << 32)
        ^ COUNTER.fetch_add(1, Ordering::Relaxed).rotate_left(17);
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    format!("{:08x}", (value >> 32) as u32)
}
