//! Port of `src/ezagent/agent/tools_memory.py` (F3-09b).
//!
//! Herramientas de memoria: `remember`, `recall`, `forget` y `compact_memory`.
//! Los textos de confirmación los genera este módulo a partir de los argumentos.
//! `remember` nunca escribe sin un sí explícito: si no se inyecta un callback de
//! confirmación, la operación se cancela.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{int_value, py_splitlines, py_strip, python_str, text_or_empty};
use serde_json::{json, Map, Value};

use crate::confirm::ConfirmationManager;
use crate::memory::schema::{
    summarize_text, DEFAULT_RECALL_LIMIT, MAX_ENTRY_CHARS, MEMORY_LIMIT, USER_LIMIT,
};
use crate::memory::MemoryStore;
use crate::registry::{ToolInfo, ToolResult, ToolReturn, ToolSpec};

pub type ConfirmCallback = Arc<dyn Fn(&str, bool) -> bool + Send + Sync>;

const REJECT_STATUSES: [&str; 5] = ["invalid_kind", "empty", "too_long", "secret", "web"];

fn string_arg(args: &Map<String, Value>, key: &str) -> String {
    args.get(key).map_or_else(String::new, python_str)
}

fn deny_confirmation(summary: &str, _strong: bool) -> bool {
    tracing::info!("Sin callback de confirmación; se cancela: {summary}");
    false
}

// DECISION: el callback inyectado manda; si no, se adapta un ConfirmationManager.
fn resolve_confirmation(
    confirm: Option<ConfirmCallback>,
    confirmation: Option<Arc<ConfirmationManager>>,
) -> ConfirmCallback {
    if let Some(confirm) = confirm {
        return confirm;
    }
    if let Some(manager) = confirmation {
        return Arc::new(move |summary, strong| manager.ask(summary, strong, None));
    }
    Arc::new(deny_confirmation)
}

fn safe_ask(ask: &ConfirmCallback, summary: &str, strong: bool) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ask(summary, strong))) {
        Ok(answer) => answer,
        Err(_) => {
            tracing::error!("Falló la confirmación de memoria; se cancela.");
            false
        }
    }
}

// DECISION: las herramientas que esperan confirmación reciben más timeout que
// el resto para no cortar mientras el usuario responde.
fn tool_timeout(config: &AgentConfig) -> f64 {
    (config.general.confirm_timeout_s + 10.0).max(30.0)
}

fn count_lines(text: Option<&Value>) -> usize {
    py_splitlines(&text_or_empty(text))
        .into_iter()
        .filter(|line| !py_strip(line).is_empty())
        .count()
}

fn ok(data: Value) -> Result<ToolReturn, String> {
    Ok(ToolReturn::from(ToolResult::ok(data)))
}

fn err(message: impl Into<String>) -> Result<ToolReturn, String> {
    Ok(ToolReturn::from(ToolResult::err(message)))
}

struct MemoryTools {
    store: Arc<MemoryStore>,
    ask: ConfirmCallback,
    timeout: f64,
}

impl MemoryTools {
    // DECISION: el registro puede abandonar un manejador por timeout mientras
    // sigue vivo en su hilo; se comprueba el plazo antes de escribir.
    fn deadline(&self) -> Instant {
        Instant::now() + Duration::from_secs_f64((self.timeout - 1.0).max(0.0))
    }

    fn remember(&self, args: &Map<String, Value>) -> Result<ToolReturn, String> {
        let deadline = self.deadline();
        let kind = string_arg(args, "kind");
        let text = string_arg(args, "text");
        let validation = self.store.validate_entry(&kind, &text);
        if REJECT_STATUSES.contains(&validation.status.as_str()) {
            return err(validation.message);
        }
        if validation.status == "duplicate" || validation.status == "full" {
            return ok(validation.to_dict());
        }
        // DECISION (del Python): la confirmación corta no menciona el archivo;
        // el tipo se ve en el resumen de la herramienta.
        let question = format!("¿Guardo que «{}»?", summarize_text(&text, 80));
        if !safe_ask(&self.ask, &question, false) {
            return ok(json!({
                "status": "cancelled",
                "message": "No se guardó: falta la confirmación del usuario.",
            }));
        }
        if Instant::now() > deadline {
            return ok(json!({
                "status": "cancelled",
                "message": "No se guardó: se agotó el tiempo de confirmación.",
            }));
        }
        let result = self.store.remember(&kind, &text)?;
        if REJECT_STATUSES.contains(&result.status.as_str()) {
            return err(result.message);
        }
        ok(result.to_dict())
    }

    fn recall(&self, args: &Map<String, Value>) -> Result<ToolReturn, String> {
        let query = py_strip(&string_arg(args, "query")).to_string();
        if query.is_empty() {
            return err("La consulta no puede estar vacía.");
        }
        let limit = args.get("limit").map_or(DEFAULT_RECALL_LIMIT, |value| {
            int_value(value, DEFAULT_RECALL_LIMIT)
        });
        let results = self.store.recall_limited(&query, limit)?;
        let count = results.len();
        let empty = results.is_empty();
        let mut payload = Map::new();
        payload.insert("query".into(), json!(query));
        payload.insert("results".into(), json!(results));
        payload.insert("count".into(), json!(count));
        payload.insert("backend".into(), json!(self.store.last_backend()));
        if empty {
            payload.insert("message".into(), json!("No encontré nada en el historial."));
        }
        ok(Value::Object(payload))
    }

    fn forget(&self, args: &Map<String, Value>) -> Result<ToolReturn, String> {
        let deadline = self.deadline();
        let query = py_strip(&string_arg(args, "query")).to_string();
        if query.is_empty() {
            return err("La consulta no puede estar vacía.");
        }
        let matches = self.store.find_matches(&query);
        let total: usize = matches.values().map(Vec::len).sum();
        if total == 0 {
            return ok(json!({
                "status": "not_found",
                "removed": 0,
                "message": format!(
                    "No encontré líneas que coincidan con «{}».",
                    summarize_text(&query, 80)
                ),
            }));
        }
        let detail = matches
            .iter()
            .map(|(kind, lines)| format!("{} ({})", self.store.filename(kind), lines.len()))
            .collect::<Vec<_>>()
            .join(", ");
        let question = format!(
            "¿Borro {total} línea(s) de {detail} que coinciden con «{}»? Esta acción no se puede deshacer.",
            summarize_text(&query, 80)
        );
        if !safe_ask(&self.ask, &question, true) {
            return ok(json!({
                "status": "cancelled",
                "message": "No se borró nada: falta la confirmación del usuario.",
            }));
        }
        if Instant::now() > deadline {
            return ok(json!({
                "status": "cancelled",
                "message": "No se borró nada: se agotó el tiempo de confirmación.",
            }));
        }
        let result = self.store.forget(&query)?;
        ok(result.to_dict())
    }

    fn compact(&self, args: &Map<String, Value>) -> Result<ToolReturn, String> {
        let deadline = self.deadline();
        let kind = string_arg(args, "kind");
        let text = string_arg(args, "text");
        let prepared = self.store.prepare_compaction(&kind, &text);
        if prepared.status != "ready" {
            return err(prepared.message);
        }
        let question = format!(
            "¿Reescribo {} con {} línea(s)? Se guardará una copia .bak de la versión anterior.",
            self.store.filename(&kind),
            prepared.line_count
        );
        if !safe_ask(&self.ask, &question, true) {
            return ok(json!({
                "status": "cancelled",
                "message": "No se compactó: falta la confirmación del usuario.",
            }));
        }
        if Instant::now() > deadline {
            return ok(json!({
                "status": "cancelled",
                "message": "No se compactó: se agotó el tiempo de confirmación.",
            }));
        }
        let result = self.store.compact_memory(&kind, &text)?;
        if result.status != "compacted" {
            return err(result.message);
        }
        ok(result.to_dict())
    }
}

fn remember_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["user", "memory"],
                "description": "user = quién es el usuario y sus preferencias; memory = hechos y flujo de trabajo.",
            },
            "text": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_ENTRY_CHARS,
                "description": "Dato estable, en una sola línea.",
            },
        },
        "required": ["kind", "text"],
        "additionalProperties": false,
    })
}

fn recall_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "minLength": 1,
                "description": "Texto o palabras a buscar.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 10,
                "default": DEFAULT_RECALL_LIMIT,
                "description": "Número máximo de fragmentos.",
            },
        },
        "required": ["query"],
        "additionalProperties": false,
    })
}

fn forget_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "minLength": 1,
                "description": "Texto de las líneas que se deben olvidar.",
            }
        },
        "required": ["query"],
        "additionalProperties": false,
    })
}

fn compact_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["user", "memory"],
                "description": "Archivo que se va a compactar.",
            },
            "text": {
                "type": "string",
                "minLength": 1,
                "maxLength": USER_LIMIT.max(MEMORY_LIMIT),
                "description": "Contenido completo de la versión fusionada, en viñetas «- [AAAA-MM-DD] texto».",
            },
        },
        "required": ["kind", "text"],
        "additionalProperties": false,
    })
}

/// Construye las cuatro herramientas de memoria.
pub fn build_memory_tools(
    config: &AgentConfig,
    store: Arc<MemoryStore>,
    confirm: Option<ConfirmCallback>,
    confirmation: Option<Arc<ConfirmationManager>>,
) -> Vec<ToolSpec> {
    let timeout = tool_timeout(config);
    let context = Arc::new(MemoryTools {
        store,
        ask: resolve_confirmation(confirm, confirmation),
        timeout,
    });
    let handler =
        |run: &Arc<MemoryTools>,
         method: fn(&MemoryTools, &Map<String, Value>) -> Result<ToolReturn, String>| {
            let run = Arc::clone(run);
            Arc::new(move |args: &Map<String, Value>| method(&run, args))
        };

    let mut remember = ToolSpec::new(
        "remember",
        "Propone guardar un dato estable en la memoria personal (USER.md o MEMORY.md). El código valida el texto y pide confirmación; nunca guarda sin un sí.",
        remember_schema(),
        1,
        handler(&context, MemoryTools::remember),
    );
    remember.timeout_s = timeout;
    remember.describe_fn = Some(Arc::new(|args| ToolInfo {
        level: 1,
        summary: format!(
            "Guardar en memoria ({}): «{}»",
            string_arg(args, "kind"),
            summarize_text(&string_arg(args, "text"), 80)
        ),
        ..ToolInfo::default()
    }));

    let mut recall = ToolSpec::new(
        "recall",
        "Busca conversaciones anteriores en el historial local. Devuelve hasta `limit` fragmentos de 300 caracteres.",
        recall_schema(),
        0,
        handler(&context, MemoryTools::recall),
    );
    recall.timeout_s = 10.0;
    recall.describe_fn = Some(Arc::new(|args| ToolInfo {
        level: 0,
        summary: format!(
            "Buscar en el historial: «{}» (límite {})",
            summarize_text(&string_arg(args, "query"), 80),
            args.get("limit")
                .map_or(DEFAULT_RECALL_LIMIT.to_string(), python_str)
        ),
        ..ToolInfo::default()
    }));

    let mut forget = ToolSpec::new(
        "forget",
        "Borra de USER.md y MEMORY.md las líneas que coinciden con la consulta. Requiere confirmación fuerte.",
        forget_schema(),
        2,
        handler(&context, MemoryTools::forget),
    );
    forget.timeout_s = timeout;
    forget.describe_fn = Some(Arc::new(|args| ToolInfo {
        level: 2,
        summary: format!(
            "Olvidar líneas que coincidan con «{}»",
            summarize_text(&string_arg(args, "query"), 80)
        ),
        ..ToolInfo::default()
    }));

    let mut compact = ToolSpec::new(
        "compact_memory",
        "Reescribe USER.md o MEMORY.md con una versión compactada acordada con el usuario. Requiere confirmación y deja una copia .bak.",
        compact_schema(),
        2,
        handler(&context, MemoryTools::compact),
    );
    compact.timeout_s = timeout;
    compact.describe_fn = Some(Arc::new(|args| ToolInfo {
        level: 2,
        summary: format!(
            "Compactar {} con {} línea(s)",
            string_arg(args, "kind"),
            count_lines(args.get("text"))
        ),
        ..ToolInfo::default()
    }));

    vec![remember, recall, forget, compact]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::store::MemoryOptions;
    use crate::memory::MemoryTimestamp;
    use crate::registry::ToolReturn;
    use chrono::DateTime;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("ez-memory-{}-{name}-{nanos}", std::process::id()))
    }

    fn config(name: &str) -> AgentConfig {
        let dir = scratch(name);
        let _ = std::fs::create_dir_all(&dir);
        let (config, _) = AgentConfig::load(Some(&dir.join("permissions.toml")));
        config
    }

    fn store(name: &str) -> Arc<MemoryStore> {
        let moment = DateTime::parse_from_rfc3339("2026-11-14T05:07:23+00:00").expect("fecha");
        let dir = scratch(name);
        let options = MemoryOptions::default().clock(Arc::new(move || moment));
        Arc::new(MemoryStore::from_paths(&dir, dir.join("history.db"), options).expect("almacén"))
    }

    type AskLog = Arc<Mutex<Vec<(String, bool)>>>;

    fn ask(accept: bool) -> (ConfirmCallback, AskLog) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&log);
        let callback: ConfirmCallback = Arc::new(move |summary, strong| {
            recorded
                .lock()
                .expect("registro")
                .push((summary.to_string(), strong));
            accept
        });
        (callback, log)
    }

    fn run(tools: &[ToolSpec], name: &str, args: Value) -> ToolResult {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .expect("herramienta");
        let map = args.as_object().expect("objeto");
        match (tool.handler)(map).expect("ejecuta") {
            ToolReturn::Result(result) => result,
            ToolReturn::Value(value) => ToolResult::ok(value),
        }
    }

    fn seed(store: &MemoryStore) {
        store
            .record(
                "user",
                "La memoria del agente guarda la conversación.",
                None,
                &MemoryTimestamp::Text("2026-11-01T10:00:00+00:00".into()),
            )
            .expect("guardado");
    }

    fn tools(name: &str, store: &Arc<MemoryStore>, ask: Option<ConfirmCallback>) -> Vec<ToolSpec> {
        build_memory_tools(&config(name), Arc::clone(store), ask, None)
    }

    #[test]
    fn remember_cancels_without_confirmation() {
        let store = store("tools-cancel");
        let result = run(
            &tools("tools-cancel", &store, None),
            "remember",
            json!({"kind": "user", "text": "Prefiere café"}),
        );
        assert!(result.ok);
        assert_eq!(result.data["status"], "cancelled");
        assert_eq!(store.read_markdown("user"), "");
    }

    #[test]
    fn remember_saves_after_a_weak_confirmation() {
        let store = store("tools-save");
        let (callback, log) = ask(true);
        let result = run(
            &tools("tools-save", &store, Some(callback)),
            "remember",
            json!({"kind": "user", "text": "Prefiere café"}),
        );
        assert!(result.ok);
        assert_eq!(result.data["status"], "saved");
        assert_eq!(
            log.lock().expect("registro").as_slice(),
            [("¿Guardo que «Prefiere café»?".to_string(), false)]
        );
        assert_eq!(
            store.read_markdown("user"),
            "- [2026-11-14] Prefiere café\n"
        );
    }

    #[test]
    fn remember_rejects_secrets_without_asking() {
        let store = store("tools-secret");
        let (callback, log) = ask(true);
        let result = run(
            &tools("tools-secret", &store, Some(callback)),
            "remember",
            json!({"kind": "user", "text": "mi contraseña es 1234"}),
        );
        assert!(!result.ok);
        assert_eq!(
            result.error.as_deref(),
            Some("No guardo contraseñas en la memoria.")
        );
        assert!(log.lock().expect("registro").is_empty());
    }

    #[test]
    fn recall_payload_and_empty_query() {
        let store = store("tools-recall");
        seed(&store);
        let tools = tools("tools-recall", &store, None);
        let result = run(&tools, "recall", json!({"query": "memoria"}));
        assert!(result.ok);
        assert_eq!(result.data["count"], 1);
        assert_eq!(result.data["backend"], "fts5");
        let empty = run(&tools, "recall", json!({"query": "   "}));
        assert!(!empty.ok);
        assert_eq!(
            empty.error.as_deref(),
            Some("La consulta no puede estar vacía.")
        );
    }

    #[test]
    fn forget_and_compact_require_strong_confirmation() {
        let store = store("tools-forget");
        store.remember("user", "Prefiere café").expect("guardado");
        let (callback, log) = ask(true);
        let tools = tools("tools-forget", &store, Some(callback));
        let forgotten = run(&tools, "forget", json!({"query": "café"}));
        assert!(forgotten.ok, "{:?}", forgotten.error);
        assert_eq!(forgotten.data["status"], "forgotten");
        assert_eq!(forgotten.data["removed"], 1);
        let args = json!({"kind": "user", "text": "# Perfil\nnueva línea"});
        let compacted = run(&tools, "compact_memory", args);
        assert!(compacted.ok, "{:?}", compacted.error);
        assert_eq!(compacted.data["status"], "compacted");
        let log = log.lock().expect("registro");
        assert!(log.iter().all(|(_, strong)| *strong));
    }

    #[test]
    fn timeout_has_a_floor_of_thirty_seconds() {
        let mut config = config("tools-timeout");
        assert_eq!(tool_timeout(&config), 30.0);
        config.general.confirm_timeout_s = 25.0;
        assert_eq!(tool_timeout(&config), 35.0);
    }

    #[test]
    fn describes_summarize_the_arguments() {
        let tools = tools("tools-describe", &store("tools-describe"), None);
        let remember = tools
            .iter()
            .find(|tool| tool.name == "remember")
            .expect("remember");
        let args = json!({"kind": "user", "text": "Prefiere café"});
        let info = remember.describe(args.as_object().expect("objeto"));
        assert_eq!(info.level, 1);
        assert_eq!(info.summary, "Guardar en memoria (user): «Prefiere café»");
    }
}
