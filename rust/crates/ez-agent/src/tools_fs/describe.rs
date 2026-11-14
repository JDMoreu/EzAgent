//! Descripciones para la política (`describe_*` de `tools_fs.py`).
//!
//! El resumen que ve el usuario lo genera este módulo a partir de las rutas y
//! cantidades reales; el LLM no lo redacta.

use std::path::Path;

use ez_core::pyvalue::{py_strip, python_str};
use serde_json::{Map, Value};

use super::util::{
    backup_path, count_items, count_phrase, error_summary, int_arg, is_http_url, FsContext,
    FsResult,
};
use crate::registry::ToolInfo;

fn arg_text(args: &Map<String, Value>, key: &str) -> String {
    args.get(key).map_or_else(String::new, python_str)
}

fn info(level: i64, summary: impl Into<String>) -> ToolInfo {
    ToolInfo {
        level,
        summary: summary.into(),
        ..ToolInfo::default()
    }
}

/// Origen de una copia/movimiento: texto para el resumen y cantidad.
fn origin(src: &Path) -> (String, usize) {
    if src.is_dir() {
        let count = count_items(src);
        (
            format!("la carpeta «{}» ({})", src.display(), count_phrase(count)),
            count,
        )
    } else {
        (format!("el archivo «{}»", src.display()), 1)
    }
}

impl FsContext {
    pub fn describe_create_dir(&self, args: &Map<String, Value>) -> ToolInfo {
        let raw = arg_text(args, "path");
        let outcome = (|| -> FsResult<String> {
            let target = self.resolve_write(&raw)?;
            self.guard_not_root(&target, "crear")?;
            Ok(if target.is_dir() {
                format!(
                    "La carpeta «{}» ya existe; no hay que crearla.",
                    target.display()
                )
            } else if target.exists() {
                format!(
                    "Ya existe un archivo en «{}»; no se puede crear la carpeta.",
                    target.display()
                )
            } else {
                format!("Voy a crear la carpeta «{}».", target.display())
            })
        })();
        info(
            1,
            outcome.unwrap_or_else(|e| error_summary(&raw, "crear la carpeta", &e)),
        )
    }

    pub fn describe_write_file(&self, args: &Map<String, Value>) -> ToolInfo {
        let raw = arg_text(args, "path");
        let overwrite = args.get("overwrite").is_some_and(ez_core::pyvalue::truthy);
        let chars = args
            .get("content")
            .and_then(Value::as_str)
            .map_or(0, |text| text.chars().count());
        let resolved = self.resolve_write(&raw).and_then(|target| {
            self.guard_not_root(&target, "escribir en")?;
            Ok(target)
        });
        let target = match resolved {
            Ok(target) => target,
            Err(error) => return info(1, error_summary(&raw, "escribir el archivo", &error)),
        };
        if target.is_dir() {
            return info(
                1,
                format!(
                    "«{}» es una carpeta; no se puede escribir como archivo.",
                    target.display()
                ),
            );
        }
        if target.exists() && overwrite {
            let backup = backup_path(&target);
            return ToolInfo {
                level: 2,
                always_confirm: true,
                summary: format!(
                    "Voy a sobrescribir el archivo «{}» y guardaré una copia de seguridad en «{}».",
                    target.display(),
                    backup.display()
                ),
                ..ToolInfo::default()
            };
        }
        if target.exists() {
            return info(
                1,
                format!(
                    "El archivo «{}» ya existe; no se sobrescribirá sin overwrite=true.",
                    target.display()
                ),
            );
        }
        let parent = target.parent().unwrap_or(Path::new(""));
        if parent.exists() {
            info(
                1,
                format!(
                    "Voy a crear el archivo «{}» ({chars} caracteres).",
                    target.display()
                ),
            )
        } else {
            info(
                1,
                format!(
                    "Voy a crear el archivo «{}» ({chars} caracteres) y la carpeta «{}».",
                    target.display(),
                    parent.display()
                ),
            )
        }
    }

    pub fn describe_copy_path(&self, args: &Map<String, Value>) -> ToolInfo {
        self.describe_transfer(args, false)
    }

    pub fn describe_move_path(&self, args: &Map<String, Value>) -> ToolInfo {
        self.describe_transfer(args, true)
    }

    fn describe_transfer(&self, args: &Map<String, Value>, moving: bool) -> ToolInfo {
        let (raw_src, raw_dst) = (arg_text(args, "src"), arg_text(args, "dst"));
        let (verb_inf, verb_fut, guard_src) = if moving {
            ("mover", "mover", true)
        } else {
            ("copiar", "copiar", false)
        };
        let resolved = (|| -> FsResult<_> {
            let src = if moving {
                self.resolve_write(&raw_src)?
            } else {
                self.resolve_read(&raw_src)?
            };
            let dst = self.resolve_write(&raw_dst)?;
            if guard_src {
                self.guard_not_root(&src, "mover")?;
                self.guard_not_root(&dst, "mover a")?;
            } else {
                self.guard_not_root(&dst, "copiar en")?;
            }
            Ok((src, dst))
        })();
        let (src, dst) = match resolved {
            Ok(pair) => pair,
            Err(error) => {
                return info(
                    1,
                    error_summary(&format!("{raw_src} → {raw_dst}"), verb_inf, &error),
                );
            }
        };
        if !src.exists() {
            return info(
                1,
                format!("No se puede {verb_inf} «{}»: no existe.", src.display()),
            );
        }
        if src == dst {
            return info(1, "El origen y el destino son iguales.");
        }
        if src.is_dir() && dst.starts_with(&src) {
            return info(
                1,
                format!("No se puede {verb_inf} una carpeta dentro de sí misma."),
            );
        }
        if dst.is_dir() && src.starts_with(&dst) {
            return info(1, "El destino contiene al origen; elige otra carpeta.");
        }
        let (origin, bulk) = origin(&src);
        let bulk = bulk as i64;
        if dst.exists() {
            let backup = backup_path(&dst);
            let (on, to) = if moving {
                ("mover", "sobre")
            } else {
                ("copiar", "sobre")
            };
            return ToolInfo {
                level: 2,
                bulk,
                always_confirm: true,
                summary: format!(
                    "Voy a {on} {origin} {to} «{}» y guardaré una copia de seguridad en «{}».",
                    dst.display(),
                    backup.display()
                ),
                ..ToolInfo::default()
            };
        }
        let connector = if moving { "a" } else { "en" };
        ToolInfo {
            level: 1,
            bulk,
            summary: format!("Voy a {verb_fut} {origin} {connector} «{}».", dst.display()),
            ..ToolInfo::default()
        }
    }

    pub fn describe_delete_path(&self, args: &Map<String, Value>) -> ToolInfo {
        let raw = arg_text(args, "path");
        let target = match self.resolve_write(&raw).and_then(|t| {
            self.guard_not_root(&t, "borrar")?;
            Ok(t)
        }) {
            Ok(target) => target,
            Err(error) => {
                return ToolInfo {
                    level: 2,
                    always_confirm: true,
                    summary: error_summary(&raw, "borrar", &error),
                    ..ToolInfo::default()
                };
            }
        };
        let confirm = |summary: String, bulk: i64, forced: bool| ToolInfo {
            level: 2,
            bulk,
            always_confirm: true,
            summary,
            bulk_forced: forced,
        };
        if !target.exists() && target.symlink_metadata().is_err() {
            return confirm(
                format!("No se puede borrar «{}»: no existe.", target.display()),
                0,
                false,
            );
        }
        if target.is_dir() {
            let count = count_items(&target);
            let summary = if count == 0 {
                format!(
                    "Voy a enviar a la Papelera la carpeta «{}», que está vacía.",
                    target.display()
                )
            } else {
                format!(
                    "Voy a enviar a la Papelera la carpeta «{}», con {} dentro.",
                    target.display(),
                    count_phrase(count)
                )
            };
            // `_FolderBulk`: conserva la cantidad real y fuerza el umbral.
            return confirm(summary, count as i64, true);
        }
        confirm(
            format!(
                "Voy a enviar a la Papelera el archivo «{}».",
                target.display()
            ),
            1,
            false,
        )
    }

    pub fn describe_open_path(&self, args: &Map<String, Value>) -> ToolInfo {
        let raw = py_strip(&arg_text(args, "path_or_url")).to_string();
        if is_http_url(&raw) {
            return info(1, format!("Voy a abrir el enlace «{raw}» en el navegador."));
        }
        if raw.contains("://") {
            return info(1, "Solo se pueden abrir rutas de archivo o URL http(s).");
        }
        match self.resolve_read(&raw) {
            Err(error) => info(1, error_summary(&raw, "abrir", &error)),
            Ok(target) if !target.exists() => info(
                1,
                format!("No se puede abrir «{}»: no existe.", target.display()),
            ),
            Ok(target) if !target.is_file() => info(
                1,
                format!(
                    "Solo se pueden abrir archivos, no carpetas: «{}».",
                    target.display()
                ),
            ),
            Ok(target) => info(1, format!("Voy a abrir el archivo «{}».", target.display())),
        }
    }

    pub fn describe_undo_last(&self, args: &Map<String, Value>) -> ToolInfo {
        let n = int_arg(args, "n", 1).unwrap_or(1);
        if n <= 1 {
            info(1, "Voy a deshacer la última operación.")
        } else {
            info(1, format!("Voy a deshacer las últimas {n} operaciones."))
        }
    }
}
