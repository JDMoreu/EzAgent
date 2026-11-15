//! Port of `src/ezagent/agent/journal.py` (F3-04).
//!
//! Diario de operaciones deshacibles. Cada operación que modifica el disco
//! guarda en `%APPDATA%/EzAgent/journal/` un archivo JSON con los pasos para
//! revertirla. `undo_last` revierte las últimas `n` operaciones, de la más
//! reciente a la más antigua, y las marca como deshechas.
//!
//! Las operaciones de borrado no se registran: van a la Papelera del sistema.
//!
//! DECISION (del Python): un archivo JSON por operación (escritura atómica con
//! `rename`) en lugar de una lista única. Así el diario no se corrompe completo
//! si falla una escritura y el orden sale del nombre, que empieza por la marca
//! de tiempo en nanosegundos.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::tools_fs::{Clock, SystemTrash, Trash, TrashHandle};
use ez_core::dirs;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyloads;
use ez_core::pytime::now_local_iso_seconds;
use ez_core::pyvalue::{python_str, truthy};
use serde_json::{json, Map, Value};

/// Validador de rutas del deshacer: devuelve la ruta resuelta y permitida.
pub type Validate<'a> = &'a dyn Fn(&Path) -> Result<PathBuf, String>;

/// Carpeta por defecto del diario: `%APPDATA%/EzAgent/journal` (o XDG).
pub fn default_journal_dir() -> PathBuf {
    let base = ["APPDATA", "XDG_CONFIG_HOME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    match base {
        Some(base) => PathBuf::from(base).join("EzAgent").join("journal"),
        None => dirs::home().join(".config").join("EzAgent").join("journal"),
    }
}

/// Resultado de revertir una entrada del diario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOutcome {
    pub entry_id: String,
    pub kind: String,
    pub summary: String,
    pub ok: bool,
    pub error: Option<String>,
}

impl UndoOutcome {
    pub fn to_dict(&self) -> Value {
        json!({
            "entry_id": self.entry_id,
            "kind": self.kind,
            "summary": self.summary,
            "ok": self.ok,
            "error": self.error,
        })
    }
}

/// Diario en disco con la operación inversa de cada cambio.
pub struct Journal {
    dir: PathBuf,
    state: Mutex<u64>,
    trash: TrashHandle,
    clock: Clock,
}

fn step(pairs: &[(&str, &Path)], action: &str) -> Value {
    let mut map = Map::new();
    map.insert("action".into(), json!(action));
    for (key, path) in pairs {
        map.insert((*key).into(), json!(path.display().to_string()));
    }
    Value::Object(map)
}

impl Journal {
    pub fn new(directory: Option<&Path>) -> Result<Self, String> {
        Self::with_hooks(
            directory,
            Arc::new(SystemTrash),
            Arc::new(now_local_iso_seconds),
        )
    }

    /// Igual que [`Journal::new`], con Papelera y reloj inyectables.
    pub fn with_hooks(
        directory: Option<&Path>,
        trash: TrashHandle,
        clock: Clock,
    ) -> Result<Self, String> {
        let dir = directory.map_or_else(default_journal_dir, Path::to_path_buf);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(Self {
            dir,
            state: Mutex::new(0),
            trash,
            clock,
        })
    }

    fn now_iso(&self) -> String {
        (self.clock)()
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }

    /// Guarda una entrada y devuelve su identificador.
    pub fn record(&self, kind: &str, summary: &str, steps: Vec<Value>) -> Result<String, String> {
        if steps.is_empty() {
            return Err("La operación inversa no puede estar vacía.".into());
        }
        let mut sequence = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *sequence += 1;
        // El nombre ordena por tiempo y, dentro del mismo instante, por secuencia.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let random = format!("{:032x}", uuid::Uuid::new_v4().as_u128());
        let entry_id = format!("{nanos:020}-{:08}-{}", *sequence, &random[..12]);
        let mut entry = Map::new();
        entry.insert("version".into(), json!(1));
        entry.insert("id".into(), json!(entry_id));
        entry.insert("ts".into(), json!(self.now_iso()));
        entry.insert("kind".into(), json!(kind));
        entry.insert("summary".into(), json!(summary));
        entry.insert("steps".into(), Value::Array(steps));
        entry.insert("undone".into(), json!(false));
        entry.insert("undone_at".into(), Value::Null);
        entry.insert("undo_error".into(), Value::Null);
        self.write_entry(&entry)?;
        Ok(entry_id)
    }

    pub fn record_create(&self, path: &Path, kind: &str, summary: &str) -> Result<String, String> {
        let kind = if kind.is_empty() { "create" } else { kind };
        let summary = if summary.is_empty() {
            format!("Crear «{}»", path.display())
        } else {
            summary.to_string()
        };
        self.record(kind, &summary, vec![step(&[("path", path)], "trash")])
    }

    pub fn record_copy(
        &self,
        src: &Path,
        dst: &Path,
        overwrite: bool,
        backup: Option<&Path>,
    ) -> Result<String, String> {
        if !overwrite {
            let summary = format!("Copiar «{}» en «{}»", src.display(), dst.display());
            return self.record("copy", &summary, vec![step(&[("path", dst)], "trash")]);
        }
        let backup = backup.ok_or_else(|| {
            "La copia con sobrescritura necesita una copia de seguridad.".to_string()
        })?;
        let steps = vec![
            step(&[("path", dst)], "trash"),
            step(&[("src", backup), ("dst", dst)], "move"),
        ];
        let summary = format!("Copiar «{}» sobre «{}»", src.display(), dst.display());
        self.record("copy_overwrite", &summary, steps)
    }

    pub fn record_move(
        &self,
        src: &Path,
        dst: &Path,
        overwrite: bool,
        backup: Option<&Path>,
    ) -> Result<String, String> {
        let mut steps = vec![step(&[("src", dst), ("dst", src)], "move")];
        if !overwrite {
            let summary = format!("Mover «{}» a «{}»", src.display(), dst.display());
            return self.record("move", &summary, steps);
        }
        let backup = backup.ok_or_else(|| {
            "El movimiento con sobrescritura necesita una copia de seguridad.".to_string()
        })?;
        steps.push(step(&[("src", backup), ("dst", dst)], "move"));
        let summary = format!("Mover «{}» sobre «{}»", src.display(), dst.display());
        self.record("move_overwrite", &summary, steps)
    }

    pub fn record_overwrite(
        &self,
        target: &Path,
        backup: &Path,
        summary: &str,
    ) -> Result<String, String> {
        let summary = if summary.is_empty() {
            format!("Sobrescribir «{}»", target.display())
        } else {
            summary.to_string()
        };
        let steps = vec![step(&[("src", backup), ("dst", target)], "copy_back")];
        self.record("overwrite", &summary, steps)
    }

    /// Revierte las últimas `n` operaciones no deshechas, de la más reciente a
    /// la más antigua. Devuelve un resultado por entrada.
    pub fn undo_last(
        &self,
        n: i64,
        validate: Option<Validate<'_>>,
    ) -> Result<Vec<UndoOutcome>, String> {
        if n < 1 {
            return Err("n debe ser mayor o igual que 1.".into());
        }
        let _guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entries: Vec<Map<String, Value>> = self
            .load_entries()
            .into_iter()
            .filter(|entry| !entry.get("undone").is_some_and(truthy))
            .take(n as usize)
            .collect();
        let mut outcomes = Vec::new();
        for mut entry in entries {
            let outcome = undo_entry(&entry, validate, &*self.trash);
            self.mark_undone(&mut entry, outcome.error.as_deref());
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    fn load_entries(&self) -> Vec<Map<String, Value>> {
        let Ok(listing) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = listing
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        files.reverse();
        let mut entries = Vec::new();
        for file in files {
            // DECISION (del Python): una entrada corrupta se ignora.
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if let Ok(Value::Object(data)) = pyloads::loads(&text.replace("\r\n", "\n")) {
                if data.get("steps").is_some_and(Value::is_array) {
                    entries.push(data);
                }
            }
        }
        entries
    }

    fn write_entry(&self, entry: &Map<String, Value>) -> Result<(), String> {
        let id = entry.get("id").map(python_str).unwrap_or_default();
        let safe = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !safe {
            return Err("Identificador de diario inválido.".into());
        }
        let mut options = DumpOptions::default().with_indent(2);
        options.ensure_ascii = false;
        let data = dumps_with(&Value::Object(entry.clone()), &options);
        let tmp = self
            .dir
            .join(format!(".entry-{}.tmp", uuid::Uuid::new_v4().simple()));
        let write = || -> std::io::Result<()> {
            let mut handle = File::create(&tmp)?;
            handle.write_all(data.as_bytes())?;
            handle.flush()?;
            handle.sync_all()?;
            std::fs::rename(&tmp, self.dir.join(format!("{id}.json")))
        };
        write().map_err(|error| {
            if tmp.exists() {
                let _ = self.trash.delete(&tmp);
            }
            error.to_string()
        })
    }

    fn mark_undone(&self, entry: &mut Map<String, Value>, error: Option<&str>) {
        entry.insert("undone".into(), json!(true));
        entry.insert("undone_at".into(), json!(self.now_iso()));
        entry.insert("undo_error".into(), json!(error));
        // DECISION (del Python): si no se puede reescribir la entrada, el
        // resultado de la reversión ya se devolvió; no se oculta con un error.
        let _ = self.write_entry(entry);
    }
}

fn undo_entry(
    entry: &Map<String, Value>,
    validate: Option<Validate<'_>>,
    trash: &dyn Trash,
) -> UndoOutcome {
    let text = |key: &str| entry.get(key).map(python_str).unwrap_or_default();
    let (entry_id, kind, summary) = (text("id"), text("kind"), text("summary"));
    let steps = match entry.get("steps") {
        Some(Value::Array(steps)) if !steps.is_empty() => steps,
        _ => {
            let error = Some("La entrada no tiene pasos inversos.".to_string());
            return UndoOutcome {
                entry_id,
                kind,
                summary,
                ok: false,
                error,
            };
        }
    };
    let result = steps
        .iter()
        .try_for_each(|step| run_step(step, validate, trash));
    let (ok, error) = match result {
        Ok(()) => (true, None),
        Err(message) => (false, Some(message)),
    };
    UndoOutcome {
        entry_id,
        kind,
        summary,
        ok,
        error,
    }
}

fn validated(
    step: &Map<String, Value>,
    key: &str,
    validate: Option<Validate<'_>>,
) -> Result<PathBuf, String> {
    let raw = step.get(key).filter(|raw| truthy(raw));
    let Some(raw) = raw else {
        return Err(format!("Falta la ruta «{key}» en el paso inverso."));
    };
    let path = PathBuf::from(python_str(raw));
    match validate {
        Some(check) => check(&path),
        None => Ok(path),
    }
}

fn exists_or_link(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn run_step(step: &Value, validate: Option<Validate<'_>>, trash: &dyn Trash) -> Result<(), String> {
    let Value::Object(step) = step else {
        return Err("Paso inverso inválido.".into());
    };
    let action = step.get("action").map(python_str).unwrap_or_default();
    match action.as_str() {
        "trash" => {
            let path = validated(step, "path", validate)?;
            if !exists_or_link(&path) {
                return Ok(());
            }
            trash.delete(&path)
        }
        "move" => {
            let src = validated(step, "src", validate)?;
            let dst = validated(step, "dst", validate)?;
            if !exists_or_link(&src) {
                return Err(format!(
                    "No existe «{}» para devolverlo a su sitio.",
                    src.display()
                ));
            }
            if exists_or_link(&dst) {
                return Err(format!(
                    "Ya existe «{}»; no se sobrescribe al deshacer.",
                    dst.display()
                ));
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            move_path(&src, &dst, trash)
        }
        "copy_back" => {
            let src = validated(step, "src", validate)?;
            let dst = validated(step, "dst", validate)?;
            if !src.exists() {
                return Err(format!(
                    "No existe la copia de seguridad «{}».",
                    src.display()
                ));
            }
            restore_copy(&src, &dst, trash)
        }
        other => Err(format!("Acción inversa desconocida: {other}")),
    }
}

/// `shutil.move`: renombra y, entre volúmenes, copia y envía el origen a la Papelera.
fn move_path(src: &Path, dst: &Path, trash: &dyn Trash) -> Result<(), String> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    // DECISION: `shutil.move` borra el origen tras copiar; aquí va a la
    // Papelera, porque el contrato prohíbe el borrado recursivo.
    copy_tree(src, dst).map_err(|error| error.to_string())?;
    trash.delete(src)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for item in std::fs::read_dir(src)? {
            let item = item?;
            copy_tree(&item.path(), &dst.join(item.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ())
}

fn restore_copy(src: &Path, dst: &Path, trash: &dyn Trash) -> Result<(), String> {
    let parent = dst
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let tmp = parent.join(format!(
        ".ezagent-undo-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let outcome = std::fs::copy(src, &tmp).and_then(|_| std::fs::rename(&tmp, dst));
    outcome.map_err(|error| {
        if tmp.exists() {
            let _ = trash.delete(&tmp);
        }
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ez-journal-{}-{name}",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn records_entries_with_sortable_ids() {
        let dir = scratch("record");
        let journal = Journal::new(Some(&dir.join("j"))).expect("diario");
        let first = journal
            .record_create(Path::new("a.txt"), "", "")
            .expect("entrada");
        let second = journal
            .record_create(Path::new("b.txt"), "create", "Crear b")
            .expect("entrada");
        assert!(first < second);
        assert_eq!(first.len(), 20 + 1 + 8 + 1 + 12);
        let text = std::fs::read_to_string(journal.directory().join(format!("{first}.json")))
            .expect("archivo");
        assert!(text.starts_with("{\n  \"version\": 1,\n  \"id\": "));
        assert!(text.contains("\"summary\": \"Crear «a.txt»\""));
        assert!(journal.record("x", "y", Vec::new()).is_err());
    }

    #[test]
    fn undoes_moves_and_reports_errors() {
        let dir = scratch("undo");
        let journal = Journal::new(Some(&dir.join("j"))).expect("diario");
        let (src, dst) = (dir.join("origen.txt"), dir.join("destino.txt"));
        std::fs::write(&dst, "x").expect("archivo");
        journal
            .record_move(&src, &dst, false, None)
            .expect("entrada");
        journal
            .record_create(&dir.join("no-existe.txt"), "", "")
            .expect("entrada");
        let outcomes = journal.undo_last(2, None).expect("deshacer");
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].ok, "{:?}", outcomes[0]);
        assert!(outcomes[1].ok, "{:?}", outcomes[1]);
        assert!(src.exists() && !dst.exists());
        assert!(journal.undo_last(1, None).expect("deshacer").is_empty());
        assert!(journal.undo_last(0, None).is_err());
        journal
            .record_move(&src, &dst, false, None)
            .expect("entrada");
        let outcome = journal.undo_last(1, None).expect("deshacer").remove(0);
        assert!(!outcome.ok);
        assert_eq!(
            outcome.error,
            Some(format!(
                "No existe «{}» para devolverlo a su sitio.",
                dst.display()
            ))
        );
        let _ = std::fs::remove_file(&src);
    }
}
