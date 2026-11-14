//! Port of `src/ezagent/agent/tools_fs.py` (F3-07).
//!
//! Herramientas nativas de archivos con política, diario y Papelera. Todas las
//! rutas pasan por `resolve_safe`; las escrituras se limitan a `write_roots` y
//! las lecturas a `read_roots`. El borrado usa siempre la Papelera. Los
//! resúmenes que ve el usuario los genera el código, no el LLM.

mod describe;
mod handlers;
mod util;

use std::sync::Arc;

use ez_core::agent_config::AgentConfig;
use serde_json::{json, Map, Value};

use crate::journal::Journal;
use crate::registry::ToolSpec;
pub use util::{default_opener, FsContext, Opener};

type Handle = fn(&FsContext, &Map<String, Value>) -> crate::registry::ToolResult;
type Describe = fn(&FsContext, &Map<String, Value>) -> crate::registry::ToolInfo;

fn spec(
    context: &Arc<FsContext>,
    (name, description, level): (&str, &str, i64),
    parameters: Value,
    (handler, describe): (Handle, Describe),
) -> ToolSpec {
    let run = Arc::clone(context);
    let mut tool = ToolSpec::new(
        name,
        description,
        parameters,
        level,
        Arc::new(move |args| Ok(handler(&run, args).into())),
    );
    let explain = Arc::clone(context);
    tool.describe_fn = Some(Arc::new(move |args| describe(&explain, args)));
    tool
}

fn strict(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// Las siete herramientas de archivos, de la más simple a la más sensible.
pub fn build_fs_tools(
    config: &AgentConfig,
    journal: Arc<Journal>,
    opener: Opener,
) -> Vec<ToolSpec> {
    let context = Arc::new(FsContext {
        config: config.clone(),
        journal,
        opener,
    });
    let path = json!({"type": "string", "description": "Ruta de la carpeta o del archivo."});
    let pair = json!({
        "src": {"type": "string", "description": "Ruta de origen."},
        "dst": {"type": "string", "description": "Ruta de destino."},
    });
    vec![
        spec(
            &context,
            ("create_dir", "Crea una carpeta dentro de las carpetas de escritura permitidas. La carpeta padre debe existir.", 1),
            strict(json!({"path": path}), &["path"]),
            (FsContext::create_dir, FsContext::describe_create_dir),
        ),
        spec(
            &context,
            ("write_file", "Crea un archivo de texto. Si el archivo existe, solo lo sobrescribe con overwrite=true y guarda una copia .bak.", 1),
            strict(
                json!({
                    "path": path,
                    "content": {"type": "string", "description": "Contenido de texto del archivo."},
                    "overwrite": {"type": "boolean", "description": "Sobrescribir si ya existe (nivel 2)."},
                }),
                &["path", "content"],
            ),
            (FsContext::write_file, FsContext::describe_write_file),
        ),
        spec(
            &context,
            ("copy_path", "Copia un archivo o una carpeta. No sobrescribe sin confirmación; si el destino existe, guarda una copia de seguridad.", 1),
            strict(pair.clone(), &["src", "dst"]),
            (FsContext::copy_path, FsContext::describe_copy_path),
        ),
        spec(
            &context,
            ("move_path", "Mueve o renombra un archivo o una carpeta dentro de las carpetas de escritura permitidas. Si el destino existe, pide confirmación.", 1),
            strict(pair, &["src", "dst"]),
            (FsContext::move_path, FsContext::describe_move_path),
        ),
        spec(
            &context,
            ("delete_path", "Envía un archivo o una carpeta a la Papelera de reciclaje. Nunca borra definitivamente y siempre pide confirmación.", 2),
            strict(json!({"path": path}), &["path"]),
            (FsContext::delete_path, FsContext::describe_delete_path),
        ),
        spec(
            &context,
            ("open_path", "Abre un archivo permitido o una URL http(s) con la aplicación predeterminada del sistema.", 1),
            strict(
                json!({"path_or_url": {"type": "string", "description": "Ruta de archivo o URL http(s)."}}),
                &["path_or_url"],
            ),
            (FsContext::open_path, FsContext::describe_open_path),
        ),
        spec(
            &context,
            ("undo_last", "Revierte las últimas n operaciones de archivos registradas en el diario (crear, copiar, mover o sobrescribir).", 1),
            strict(
                json!({"n": {"type": "integer", "minimum": 1, "description": "Número de operaciones a deshacer."}}),
                &[],
            ),
            (FsContext::undo_last, FsContext::describe_undo_last),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ToolRegistry;

    fn setup(name: &str) -> (std::path::PathBuf, ToolRegistry) {
        let dir =
            std::env::temp_dir().join(format!("ez-fs-{}-{name}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::create_dir_all(&dir);
        let root = dir.display().to_string().replace('\\', "/");
        let file = dir.join("permissions.toml");
        let _ = std::fs::write(
            &file,
            format!("[paths]\nread_roots = ['{root}']\nwrite_roots = ['{root}/work']\ndeny = []\ndeny_globs = []\n"),
        );
        let (mut config, _) = AgentConfig::load(Some(&file));
        config.workspace = dir.join("work");
        let _ = std::fs::create_dir_all(&config.workspace);
        let journal = Arc::new(Journal::new(Some(&dir.join("journal"))).expect("diario"));
        let opener: Opener = Arc::new(|_| Ok(()));
        let registry = ToolRegistry::new();
        registry.register_many(build_fs_tools(&config, journal, opener));
        (dir, registry)
    }

    #[test]
    fn writes_copies_moves_and_undoes() {
        let (dir, registry) = setup("flow");
        let run = |name: &str, args: Value| registry.run(name, &args);
        let made = run("write_file", json!({"path": "a.txt", "content": "uno"}));
        assert!(made.ok, "{made:?}");
        assert_eq!(made.data["bytes"], 3);
        let again = run("write_file", json!({"path": "a.txt", "content": "dos"}));
        assert!(again
            .error
            .unwrap_or_default()
            .contains("Usa overwrite=true"));
        let over = run(
            "write_file",
            json!({"path": "a.txt", "content": "dos", "overwrite": true}),
        );
        assert_eq!(over.data["overwritten"], true);
        assert!(over.data["backup"]
            .as_str()
            .unwrap_or_default()
            .ends_with("a.txt.bak"));
        let copied = run("copy_path", json!({"src": "a.txt", "dst": "b.txt"}));
        assert!(copied.ok, "{copied:?}");
        let moved = run("move_path", json!({"src": "b.txt", "dst": "c.txt"}));
        assert!(moved.ok, "{moved:?}");
        let dir_made = run("create_dir", json!({"path": "sub"}));
        assert_eq!(dir_made.data["existed"], false);
        let undone = run("undo_last", json!({"n": 3}));
        assert!(undone.ok, "{undone:?}");
        assert_eq!(undone.data["undone"], 3);
        assert!(!dir.join("work").join("sub").exists());
        assert!(
            dir.join("work").join("b.txt").exists() || !dir.join("work").join("c.txt").exists()
        );
        let outside = run(
            "write_file",
            json!({"path": "../fuera.txt", "content": "x"}),
        );
        assert!(!outside.ok);
        assert_eq!(
            run("undo_last", json!({"n": 0})).error.as_deref(),
            Some("El parámetro 'n' debe ser mayor o igual que 1.")
        );
    }

    #[test]
    fn describes_operations_for_the_policy() {
        let (dir, registry) = setup("describe");
        let work = dir.join("work");
        let _ = std::fs::create_dir_all(work.join("carpeta"));
        let _ = std::fs::write(work.join("carpeta").join("x.txt"), "x");
        let _ = std::fs::write(work.join("f.txt"), "x");
        let describe = |name: &str, args: Value| {
            let tool = registry.get(name).expect("herramienta");
            tool.describe(args.as_object().expect("objeto"))
        };
        let folder = describe("delete_path", json!({"path": "carpeta"}));
        assert!(folder.bulk_forced && folder.always_confirm);
        assert_eq!(folder.bulk, 1);
        assert!(folder
            .summary
            .starts_with("Voy a enviar a la Papelera la carpeta «"));
        assert!(folder.summary.ends_with("con 1 elemento dentro."));
        let file = describe("delete_path", json!({"path": "f.txt"}));
        assert!(!file.bulk_forced);
        assert_eq!(file.bulk, 1);
        let over = describe(
            "write_file",
            json!({"path": "f.txt", "content": "z", "overwrite": true}),
        );
        assert_eq!((over.level, over.always_confirm), (2, true));
        let copy = describe("copy_path", json!({"src": "carpeta", "dst": "dest"}));
        assert_eq!((copy.level, copy.bulk), (1, 1));
        assert!(copy.summary.contains("la carpeta «") && copy.summary.contains("(1 elemento)"));
        assert_eq!(
            describe("undo_last", json!({"n": 3})).summary,
            "Voy a deshacer las últimas 3 operaciones."
        );
        assert_eq!(
            describe("undo_last", json!({})).summary,
            "Voy a deshacer la última operación."
        );
    }
}
