//! Manejadores de `tools_fs.py`: cada uno devuelve un `ToolResult` con el texto de Python.

use std::path::Path;

use ez_core::llm_providers::urlsplit;
use ez_core::pyvalue::{format_g, py_strip};
use serde_json::{json, Map, Value};

use super::util::{
    atomic_copy, atomic_write, bool_arg, copy_tree, count_items, int_arg, is_http_url, make_backup,
    text_arg, FsContext, FsResult, TrashHandle,
};
use crate::registry::ToolResult;

fn wrap(result: FsResult<ToolResult>) -> ToolResult {
    result.unwrap_or_else(ToolResult::err)
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn kind_of(path: &Path) -> &'static str {
    if path.is_dir() {
        "folder"
    } else {
        "file"
    }
}

impl FsContext {
    pub fn create_dir(&self, args: &Map<String, Value>) -> ToolResult {
        wrap((|| {
            let target = self.resolve_write(&text_arg(args, "path", false)?)?;
            self.guard_not_root(&target, "crear")?;
            if target.exists() {
                if target.is_dir() {
                    return Ok(ToolResult::ok(
                        json!({"path": target.display().to_string(), "existed": true}),
                    ));
                }
                return Ok(ToolResult::err(format!(
                    "Ya existe un archivo en «{}».",
                    target.display()
                )));
            }
            // DECISION (del Python): create_dir no crea padres ausentes.
            let parent = target.parent().unwrap_or(Path::new(""));
            if !parent.exists() {
                return Ok(ToolResult::err(format!(
                    "No existe la carpeta padre «{}».",
                    parent.display()
                )));
            }
            std::fs::create_dir(&target).map_err(io)?;
            let summary = format!("Crear la carpeta «{}»", target.display());
            self.journal
                .record_create(&target, "create_dir", &summary)?;
            Ok(ToolResult::ok(
                json!({"path": target.display().to_string(), "existed": false}),
            ))
        })())
    }

    pub fn write_file(&self, args: &Map<String, Value>) -> ToolResult {
        wrap((|| {
            let raw = text_arg(args, "path", false)?;
            let content = text_arg(args, "content", true)?;
            let overwrite = bool_arg(args, "overwrite", false)?;
            let target = self.resolve_write(&raw)?;
            self.guard_not_root(&target, "escribir en")?;
            if target.is_dir() {
                return Ok(ToolResult::err(format!(
                    "«{}» es una carpeta, no un archivo.",
                    target.display()
                )));
            }
            let limit_mb = self.config.general.max_file_mb;
            if content.len() as f64 > (limit_mb * 1024.0 * 1024.0).trunc() {
                return Ok(ToolResult::err(format!(
                    "El contenido supera el máximo permitido de {} MB.",
                    format_g(limit_mb)
                )));
            }
            if target.exists() && !overwrite {
                return Ok(ToolResult::err(format!(
                    "El archivo «{}» ya existe. Usa overwrite=true para sobrescribirlo.",
                    target.display()
                )));
            }
            let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
            let created_parent = !parent.exists();
            if created_parent {
                std::fs::create_dir_all(&parent).map_err(io)?;
            }
            let backup = if target.exists() {
                Some(make_backup(&target, &(self.clock)()).map_err(io)?)
            } else {
                None
            };
            atomic_write(&target, &content, &self.trash).map_err(io)?;
            match &backup {
                Some(backup) => {
                    let summary = format!("Sobrescribir el archivo «{}»", target.display());
                    self.journal.record_overwrite(&target, backup, &summary)?;
                }
                None => {
                    let summary = format!("Crear el archivo «{}»", target.display());
                    self.journal
                        .record_create(&target, "write_file", &summary)?;
                }
            }
            let mut data = Map::new();
            data.insert("path".into(), json!(target.display().to_string()));
            data.insert("bytes".into(), json!(content.len()));
            data.insert("overwritten".into(), json!(backup.is_some()));
            if let Some(backup) = &backup {
                data.insert("backup".into(), json!(backup.display().to_string()));
            }
            if created_parent {
                data.insert("created_parent".into(), json!(parent.display().to_string()));
            }
            Ok(ToolResult::ok(Value::Object(data)))
        })())
    }

    pub fn copy_path(&self, args: &Map<String, Value>) -> ToolResult {
        self.transfer(args, false)
    }

    pub fn move_path(&self, args: &Map<String, Value>) -> ToolResult {
        self.transfer(args, true)
    }

    /// `copy_path` y `move_path` comparten validaciones y la forma del resultado.
    fn transfer(&self, args: &Map<String, Value>, moving: bool) -> ToolResult {
        wrap((|| {
            let (src_raw, dst_raw) = (text_arg(args, "src", false)?, text_arg(args, "dst", false)?);
            let (src, dst) = if moving {
                (self.resolve_write(&src_raw)?, self.resolve_write(&dst_raw)?)
            } else {
                (self.resolve_read(&src_raw)?, self.resolve_write(&dst_raw)?)
            };
            if moving {
                self.guard_not_root(&src, "mover")?;
                self.guard_not_root(&dst, "mover a")?;
            } else {
                self.guard_not_root(&dst, "copiar en")?;
            }
            if !src.exists() {
                return Ok(ToolResult::err(format!("No existe «{}».", src.display())));
            }
            if src == dst {
                return Ok(ToolResult::err("El origen y el destino son iguales."));
            }
            if src.is_dir() && dst.starts_with(&src) {
                let verb = if moving { "mover" } else { "copiar" };
                return Ok(ToolResult::err(format!(
                    "No se puede {verb} una carpeta dentro de sí misma."
                )));
            }
            let overwritten = dst.exists();
            let mut backup = None;
            if overwritten {
                if src.is_dir() != dst.is_dir() {
                    return Ok(ToolResult::err(
                        "El origen y el destino deben ser del mismo tipo.",
                    ));
                }
                if dst.is_dir() && src.starts_with(&dst) {
                    return Ok(ToolResult::err(
                        "El destino contiene al origen; elige otra carpeta.",
                    ));
                }
                backup = Some(make_backup(&dst, &(self.clock)()).map_err(io)?);
            } else if !dst.parent().is_some_and(Path::exists) {
                // DECISION (del Python): no crea padres ausentes; solo write_file lo hace.
                let parent = dst.parent().unwrap_or(Path::new(""));
                return Ok(ToolResult::err(format!(
                    "No existe la carpeta destino «{}».",
                    parent.display()
                )));
            }
            if moving {
                self.move_into_place(&src, &dst, overwritten)?;
                self.journal
                    .record_move(&src, &dst, overwritten, backup.as_deref())?;
            } else {
                if !overwritten || src.is_dir() {
                    copy_tree(&src, &dst).map_err(io)?;
                } else {
                    atomic_copy(&src, &dst, &self.trash).map_err(io)?;
                }
                self.journal
                    .record_copy(&src, &dst, overwritten, backup.as_deref())?;
            }
            // `kind` se evalúa después del movimiento: al mover una carpeta el
            // origen ya no existe y Python informa «file» (quirk reproducido).
            let kind = kind_of(&src);
            let mut data = Map::new();
            data.insert("src".into(), json!(src.display().to_string()));
            data.insert("dst".into(), json!(dst.display().to_string()));
            data.insert("overwritten".into(), json!(overwritten));
            data.insert("kind".into(), json!(kind));
            if let Some(backup) = &backup {
                data.insert("backup".into(), json!(backup.display().to_string()));
            }
            Ok(ToolResult::ok(Value::Object(data)))
        })())
    }

    fn move_into_place(&self, src: &Path, dst: &Path, overwritten: bool) -> FsResult<()> {
        if !overwritten {
            return move_path_raw(src, dst, &self.trash);
        }
        if src.is_dir() {
            // Para carpetas no hay sustitución atómica: el destino va a la
            // Papelera y el origen se mueve a su sitio.
            self.send_to_trash(dst)?;
            return move_path_raw(src, dst, &self.trash);
        }
        // Los archivos se sustituyen con un rename atómico.
        if std::fs::rename(src, dst).is_err() {
            self.send_to_trash(dst)?;
            return move_path_raw(src, dst, &self.trash);
        }
        Ok(())
    }

    pub fn delete_path(&self, args: &Map<String, Value>) -> ToolResult {
        wrap((|| {
            let target = self.resolve_write(&text_arg(args, "path", false)?)?;
            self.guard_not_root(&target, "borrar")?;
            if !target.exists() && target.symlink_metadata().is_err() {
                return Ok(ToolResult::err(format!(
                    "No existe «{}».",
                    target.display()
                )));
            }
            let kind = kind_of(&target);
            let items = count_items(&target);
            // Nunca hay borrado definitivo: siempre Papelera.
            self.send_to_trash(&target)?;
            Ok(ToolResult::ok(
                json!({"path": target.display().to_string(), "kind": kind, "items": items}),
            ))
        })())
    }

    pub fn open_path(&self, args: &Map<String, Value>) -> ToolResult {
        wrap((|| {
            let raw = py_strip(&text_arg(args, "path_or_url", false)?).to_string();
            if is_http_url(&raw) {
                let parts = urlsplit(&raw);
                let valid = parts.as_ref().is_ok_and(|p| {
                    matches!(p.scheme.as_str(), "http" | "https") && !p.netloc.is_empty()
                });
                if !valid {
                    return Ok(ToolResult::err("La URL no es válida."));
                }
                (self.opener)(&raw)?;
                return Ok(ToolResult::ok(json!({"opened": raw, "kind": "url"})));
            }
            if raw.contains("://") {
                return Ok(ToolResult::err(
                    "Solo se permiten rutas de archivo o URL http(s).",
                ));
            }
            let target = self.resolve_read(&raw)?;
            if !target.exists() {
                return Ok(ToolResult::err(format!(
                    "No existe «{}».",
                    target.display()
                )));
            }
            if !target.is_file() {
                return Ok(ToolResult::err(format!(
                    "Solo se pueden abrir archivos, no carpetas: «{}».",
                    target.display()
                )));
            }
            let shown = target.display().to_string();
            (self.opener)(&shown)?;
            Ok(ToolResult::ok(json!({"opened": shown, "kind": "file"})))
        })())
    }

    pub fn undo_last(&self, args: &Map<String, Value>) -> ToolResult {
        wrap((|| {
            let n = int_arg(args, "n", 1)?;
            if n < 1 {
                return Ok(ToolResult::err("n debe ser mayor o igual que 1."));
            }
            let validate = |path: &Path| -> Result<std::path::PathBuf, String> {
                let resolved = self.resolve_write(&path.display().to_string())?;
                self.guard_not_root(&resolved, "deshacer sobre")?;
                Ok(resolved)
            };
            let outcomes = self.journal.undo_last(n, Some(&validate))?;
            if outcomes.is_empty() {
                return Ok(ToolResult::ok(json!({
                    "undone": 0, "failed": 0, "details": [],
                    "message": "No hay operaciones que deshacer.",
                })));
            }
            let failed: Vec<_> = outcomes.iter().filter(|outcome| !outcome.ok).collect();
            let error = (!failed.is_empty()).then(|| {
                let reasons: Vec<String> = failed
                    .iter()
                    .map(|o| format!("{}: {}", o.summary, o.error.as_deref().unwrap_or("None")))
                    .collect();
                format!(
                    "No se pudieron deshacer todas las operaciones. {}",
                    reasons.join("; ")
                )
            });
            Ok(ToolResult {
                ok: failed.is_empty(),
                data: json!({
                    "undone": outcomes.len() - failed.len(),
                    "failed": failed.len(),
                    "details": outcomes.iter().map(|o| o.to_dict()).collect::<Vec<_>>(),
                }),
                error,
                outcome: "completed".into(),
                error_kind: None,
            })
        })())
    }
}

/// `shutil.move`: renombra y, entre volúmenes, copia y envía el origen a la Papelera.
fn move_path_raw(src: &Path, dst: &Path, trash: &TrashHandle) -> FsResult<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    // DECISION: `shutil.move` borra el origen tras copiar; aquí va a la Papelera.
    copy_tree(src, dst).map_err(io)?;
    trash.delete(src)
}
