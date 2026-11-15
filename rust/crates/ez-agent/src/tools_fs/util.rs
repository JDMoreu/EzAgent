//! Utilidades internas de `tools_fs.py`: rutas, copias de seguridad y escritura atómica.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use ez_core::agent_config::AgentConfig;
use ez_core::paths::{expand, resolve, resolve_safe};
use ez_core::pyvalue::py_strip;
use serde_json::{Map, Value};

use crate::journal::Journal;

/// Tope para contar elementos de una carpeta sin recorrer árboles enormes.
pub const COUNT_LIMIT: usize = 5000;

/// `Result` con el mensaje que Python pondría en `str(exc)`.
pub type FsResult<T> = Result<T, String>;

/// Abre una ruta o URL con la aplicación predeterminada (`os.startfile`).
pub type Opener = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Envía una ruta a la Papelera; inyectable para las pruebas de paridad.
pub trait Trash: Send + Sync {
    fn delete(&self, path: &Path) -> FsResult<()>;
}

/// La Papelera del sistema (`send2trash`).
pub struct SystemTrash;

impl Trash for SystemTrash {
    fn delete(&self, path: &Path) -> FsResult<()> {
        trash::delete(path).map_err(|error| error.to_string())
    }
}

pub type TrashHandle = Arc<dyn Trash>;

/// Marca de tiempo local de las copias `.bak` (`%Y%m%d-%H%M%S`).
pub type Clock = Arc<dyn Fn() -> String + Send + Sync>;

pub fn local_stamp() -> Clock {
    Arc::new(|| Local::now().format("%Y%m%d-%H%M%S").to_string())
}

/// Todo lo que necesitan los manejadores.
pub struct FsContext {
    pub config: AgentConfig,
    pub journal: Arc<Journal>,
    pub opener: Opener,
    pub trash: TrashHandle,
    pub clock: Clock,
}

impl FsContext {
    pub fn send_to_trash(&self, path: &Path) -> FsResult<()> {
        self.trash.delete(path)
    }

    pub fn resolve_read(&self, raw: &str) -> FsResult<PathBuf> {
        let paths = &self.config.paths;
        resolve_safe(
            raw,
            &self.config.workspace,
            &paths.read_roots,
            &paths.deny,
            &paths.deny_globs,
        )
        .map_err(|error| error.to_string())
    }

    pub fn resolve_write(&self, raw: &str) -> FsResult<PathBuf> {
        let paths = &self.config.paths;
        resolve_safe(
            raw,
            &self.config.workspace,
            &paths.write_roots,
            &paths.deny,
            &paths.deny_globs,
        )
        .map_err(|error| error.to_string())
    }

    fn is_write_root(&self, path: &Path) -> bool {
        self.config
            .paths
            .write_roots
            .iter()
            .any(|root| resolve(&expand(root)) == path)
    }

    pub fn guard_not_root(&self, path: &Path, action: &str) -> FsResult<()> {
        if self.is_write_root(path) {
            return Err(format!("No se puede {action} una carpeta raíz permitida."));
        }
        Ok(())
    }
}

/// Lleva un temporal a la Papelera sin enmascarar el error original.
fn discard_temp(path: &Path, trash: &TrashHandle) {
    // DECISION (del Python): si la Papelera no está disponible, el temporal se
    // queda; nunca se borra de forma definitiva.
    if path.exists() {
        let _ = trash.delete(path);
    }
}

/// Cuenta elementos contenidos en `path` (sin incluir la propia carpeta).
pub fn count_items(path: &Path) -> usize {
    if !path.is_dir() {
        return usize::from(path.exists());
    }
    let mut count = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(listing) = std::fs::read_dir(&folder) else {
            return count;
        };
        for entry in listing.filter_map(Result::ok) {
            count += 1;
            if count >= COUNT_LIMIT {
                return COUNT_LIMIT;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    count
}

pub fn count_phrase(count: usize) -> String {
    if count >= COUNT_LIMIT {
        format!("{COUNT_LIMIT} elementos o más")
    } else if count == 1 {
        "1 elemento".into()
    } else {
        format!("{count} elementos")
    }
}

fn with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    target.with_file_name(name)
}

/// Una ruta `.bak` libre, sin sobrescribir una existente.
pub fn backup_path(target: &Path, stamp: &str) -> PathBuf {
    let plain = with_suffix(target, ".bak");
    if !plain.exists() {
        return plain;
    }
    let mut candidate = with_suffix(target, &format!(".{stamp}.bak"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = with_suffix(target, &format!(".{stamp}-{counter}.bak"));
        counter += 1;
    }
    candidate
}

pub fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
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

pub fn make_backup(target: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    let backup = backup_path(target, stamp);
    copy_tree(target, &backup)?;
    Ok(backup)
}

fn temp_beside(target: &Path) -> PathBuf {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    parent.join(format!(".ezagent-{}.tmp", uuid::Uuid::new_v4().simple()))
}

/// Escribe un texto en un temporal de la misma carpeta y lo sustituye de forma atómica.
pub fn atomic_write(target: &Path, content: &str, trash: &TrashHandle) -> std::io::Result<()> {
    let tmp = temp_beside(target);
    let outcome = (|| {
        let mut handle = std::fs::File::create(&tmp)?;
        handle.write_all(content.as_bytes())?;
        handle.flush()?;
        handle.sync_all()?;
        std::fs::rename(&tmp, target)
    })();
    if outcome.is_err() {
        discard_temp(&tmp, trash);
    }
    outcome
}

/// Copia un archivo a un temporal y sustituye `dst` de forma atómica.
pub fn atomic_copy(src: &Path, dst: &Path, trash: &TrashHandle) -> std::io::Result<()> {
    let tmp = temp_beside(dst);
    let outcome = std::fs::copy(src, &tmp).and_then(|_| std::fs::rename(&tmp, dst));
    if outcome.is_err() {
        discard_temp(&tmp, trash);
    }
    outcome
}

pub fn text_arg(args: &Map<String, Value>, name: &str, allow_empty: bool) -> FsResult<String> {
    let Some(Value::String(value)) = args.get(name) else {
        return Err(format!("El parámetro «{name}» debe ser texto."));
    };
    if !allow_empty && py_strip(value).is_empty() {
        return Err(format!("El parámetro «{name}» no puede estar vacío."));
    }
    Ok(value.clone())
}

pub fn int_arg(args: &Map<String, Value>, name: &str, default: i64) -> FsResult<i64> {
    let bad = || format!("El parámetro «{name}» debe ser un número entero.");
    match args.get(name) {
        None => Ok(default),
        Some(Value::Number(number)) if number.is_i64() => number.as_i64().ok_or_else(bad),
        // DECISION (del Python): el registro acepta 2.0 como «integer»; aquí se normaliza.
        Some(Value::Number(number)) => match number.as_f64() {
            Some(float) if float.fract() == 0.0 => Ok(float as i64),
            _ => Err(bad()),
        },
        Some(_) => Err(bad()),
    }
}

pub fn bool_arg(args: &Map<String, Value>, name: &str, default: bool) -> FsResult<bool> {
    match args.get(name) {
        None => Ok(default),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(format!("El parámetro «{name}» debe ser booleano.")),
    }
}

pub fn error_summary(raw: &str, action: &str, error: &str) -> String {
    format!("No se puede {action} «{raw}»: {error}")
}

pub fn is_http_url(raw: &str) -> bool {
    let lowered = raw.to_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// Opener por defecto: el Explorador de Windows abre archivos y enlaces con su
/// aplicación predeterminada (sin shell y con argumentos en lista).
pub fn default_opener() -> Opener {
    Arc::new(|target: &str| {
        std::process::Command::new("explorer.exe")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}
