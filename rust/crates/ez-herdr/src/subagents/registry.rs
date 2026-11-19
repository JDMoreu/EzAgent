//! Registro persistente de subagentes con escritura atómica y candado por nombre.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{parent_folder, validate_name, Change, Record, RegistryError, TaskState};

const LOCK_RETRY_MS: u64 = 50;
const LOCK_TIMEOUT_S: u64 = 3;
const LOCK_STALE_MS: u64 = 30_000;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Registro de subagentes, guardado bajo una carpeta raíz.
pub struct Registry {
    root: PathBuf,
}

impl Registry {
    /// `EZ_SUBAGENTS_DIR` o `%LOCALAPPDATA%\EzAgent\subagentes`.
    pub fn from_env() -> Registry {
        if let Some(dir) = std::env::var_os("EZ_SUBAGENTS_DIR") {
            if !dir.is_empty() {
                return Self::at(PathBuf::from(dir));
            }
        }
        // DECISION: fuera de Windows `LOCALAPPDATA` no existe; se usa `HOME` y,
        // como último recurso, la carpeta temporal para no dejar la raíz vacía.
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        Self::at(base.join("EzAgent").join("subagentes"))
    }

    /// Registro guardado bajo `root`.
    pub fn at(root: impl Into<PathBuf>) -> Registry {
        Self { root: root.into() }
    }

    /// Carpeta raíz del registro.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Crea el registro (error si ya existe ese nombre bajo ese padre).
    pub fn create(&self, record: &Record) -> Result<(), RegistryError> {
        if !matches!(record.state, TaskState::InProgress | TaskState::Failed) {
            return Err(RegistryError::Invalid(format!(
                "{} no puede crearse en estado «{}»; solo «en_proceso» o «fallido».",
                record.name,
                record.state.as_str()
            )));
        }
        let (path, lock_path) = self.paths(&record.parent, &record.name)?;
        let _lock = acquire_lock(&lock_path)?;
        if path.exists() {
            return Err(RegistryError::Exists(format!(
                "ya existe el subagente {} bajo {}",
                record.name, record.parent
            )));
        }
        write_record(&path, record)
    }

    /// Lee un registro; `None` si no existe.
    pub fn get(&self, parent: &str, name: &str) -> Result<Option<Record>, RegistryError> {
        let (path, _) = self.paths(parent, name)?;
        read_record(&path)
    }

    /// Todos los registros de un padre.
    pub fn list(&self, parent: &str) -> Result<Vec<Record>, RegistryError> {
        read_dir_records(&self.folder(parent)?)
    }

    /// Busca en todos los padres el registro de esa terminal.
    pub fn find_by_terminal(&self, terminal: &str) -> Result<Option<Record>, RegistryError> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error, &self.root)),
        };
        for entry in entries {
            let entry = entry.map_err(|error| io_error(error, &self.root))?;
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            for record in read_dir_records(&folder)? {
                if record.terminal == terminal {
                    return Ok(Some(record));
                }
            }
        }
        Ok(None)
    }

    /// Cambia el estado si la transición está permitida (§13), añade a `history`,
    /// aplica `edit` y guarda; todo bajo el candado.
    pub fn transition(
        &self,
        parent: &str,
        name: &str,
        to: TaskState,
        by: &str,
        detail: &str,
        edit: impl FnOnce(&mut Record),
    ) -> Result<Record, RegistryError> {
        let (path, lock_path) = self.paths(parent, name)?;
        let _lock = acquire_lock(&lock_path)?;
        let mut record = read_record(&path)?.ok_or_else(|| {
            RegistryError::NotFound(format!("no hay registro de {name} bajo {parent}"))
        })?;
        if !super::transition_allowed(record.state, to) {
            return Err(RegistryError::Invalid(format!(
                "{} está «{}»; no puede pasar a «{}».",
                name,
                record.state.as_str(),
                to.as_str()
            )));
        }
        edit(&mut record);
        let at_ms = now_ms();
        record.state = to;
        record.since_ms = at_ms;
        record.history.push(Change {
            state: to,
            at_ms,
            by: by.to_string(),
            detail: detail.to_string(),
        });
        write_record(&path, &record)?;
        Ok(record)
    }

    /// Guarda el último estado avisado al padre.
    pub fn mark_notified(
        &self,
        parent: &str,
        name: &str,
        state: TaskState,
    ) -> Result<(), RegistryError> {
        let (path, lock_path) = self.paths(parent, name)?;
        let _lock = acquire_lock(&lock_path)?;
        let mut record = read_record(&path)?.ok_or_else(|| {
            RegistryError::NotFound(format!("no hay registro de {name} bajo {parent}"))
        })?;
        record.notified = Some(state);
        write_record(&path, &record)
    }

    fn folder(&self, parent: &str) -> Result<PathBuf, RegistryError> {
        Ok(self.root.join(parent_folder(parent)?))
    }

    fn paths(&self, parent: &str, name: &str) -> Result<(PathBuf, PathBuf), RegistryError> {
        validate_name(name)?;
        let folder = self.folder(parent)?;
        Ok((
            folder.join(format!("{name}.json")),
            folder.join(format!("{name}.lock")),
        ))
    }
}

fn read_dir_records(folder: &Path) -> Result<Vec<Record>, RegistryError> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(error, folder)),
    };
    let mut records = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| io_error(error, folder))?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(record) = read_record(&path)? {
            records.push(record);
        }
    }
    Ok(records)
}

fn read_record(path: &Path) -> Result<Option<Record>, RegistryError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error, path)),
    };
    let record = serde_json::from_str(&text).map_err(|error| {
        RegistryError::Invalid(format!("registro inválido {}: {error}", path.display()))
    })?;
    Ok(Some(record))
}

fn write_record(path: &Path, record: &Record) -> Result<(), RegistryError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?;
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(record).map_err(|error| {
            RegistryError::Invalid(format!("no pude serializar {}: {error}", path.display()))
        })?
    );
    // El temporal lleva un número único para no colisionar con otro escritor.
    let tmp = path.with_extension(format!("{}.tmp", SEQ.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&tmp, text.as_bytes()).map_err(|error| io_error(error, &tmp))?;
    std::fs::rename(&tmp, path).map_err(|error| io_error(error, path))
}

/// Candado de creación exclusiva. Se borra al caer, también si falla la operación.
struct Lock {
    path: PathBuf,
    token: String,
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Ok(content) = std::fs::read_to_string(&self.path) {
            if content.trim() == self.token {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

fn acquire_lock(path: &Path) -> Result<Lock, RegistryError> {
    // La carpeta del padre puede no existir todavía en el primer `create`.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io_error(error, parent))?;
    }
    let deadline = Instant::now() + Duration::from_secs(LOCK_TIMEOUT_S);
    loop {
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut file) => {
                let token = format!("{}:{}", now_ms(), SEQ.fetch_add(1, Ordering::Relaxed));
                if let Err(error) = file.write_all(token.as_bytes()).and_then(|()| file.flush()) {
                    let _ = std::fs::remove_file(path);
                    return Err(io_error(error, path));
                }
                return Ok(Lock {
                    path: path.to_path_buf(),
                    token,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale(path) {
                    let _ = std::fs::remove_file(path);
                    continue;
                }
                if Instant::now() >= deadline {
                    return Err(RegistryError::Busy(format!(
                        "el registro está ocupado: {}",
                        path.display()
                    )));
                }
                std::thread::sleep(Duration::from_millis(LOCK_RETRY_MS));
            }
            Err(error) => return Err(io_error(error, path)),
        }
    }
}

fn is_stale(path: &Path) -> bool {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if let Some(ts) = content
        .trim()
        .split(':')
        .next()
        .and_then(|text| text.parse::<u64>().ok())
    {
        return now_ms().saturating_sub(ts) > LOCK_STALE_MS;
    }
    let Ok(modified) = std::fs::metadata(path).and_then(|meta| meta.modified()) else {
        return false;
    };
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return false;
    };
    elapsed.as_millis() as u64 > LOCK_STALE_MS
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn io_error(error: std::io::Error, path: &Path) -> RegistryError {
    RegistryError::Io(format!("{}: {}", path.display(), error))
}
