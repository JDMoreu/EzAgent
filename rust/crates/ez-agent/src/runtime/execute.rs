//! `AgentRuntime.execute`: la única puerta de la política (F4-09).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use ez_core::agent_config::AgentConfig;
use ez_core::paths::resolve_safe;
use ez_core::pyvalue::{python_str, truthy};
use serde_json::{Map, Value};

use crate::policy::{decide_with_threshold, with_self_confirmed, Action};
use crate::registry::{ToolInfo, ToolResult, ToolSpec};

use super::state::lock;
use super::{AgentRuntime, TAINT_FREE_TOOLS, WRITE_TOOLS};

/// Ventana de `_level1_times` para el tope de escrituras por minuto.
const WRITE_WINDOW: Duration = Duration::from_secs(60);
const WRITE_LIMIT: usize = 20;

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
        })
        .unwrap_or_default()
}

/// `_FolderBulk.__gt__`: con `bulk_forced`, cualquier umbral distinto de cero
/// se supera; con umbral 0 se compara la cantidad real.
pub(crate) fn effective_bulk(info: &ToolInfo, threshold: i64) -> i64 {
    if !info.bulk_forced {
        return info.bulk;
    }
    if threshold == 0 {
        info.bulk
    } else {
        info.bulk.max(threshold.saturating_add(1))
    }
}

/// `json.loads(raw_arguments or "{}")` y el objeto resultante.
fn parse_arguments(raw: &Value) -> Result<Map<String, Value>, String> {
    match raw {
        Value::Object(map) => Ok(map.clone()),
        // `raw_arguments or "{}"`: cualquier valor falsy llega como objeto vacío.
        value if !truthy(value) => Ok(Map::new()),
        Value::String(text) => match ez_core::pyloads::loads(text) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err("Los argumentos deben ser un objeto JSON.".into()),
            Err(_) => Err("Argumentos JSON inválidos.".into()),
        },
        _ => Err("Argumentos JSON inválidos.".into()),
    }
}

fn describe(spec: &ToolSpec, args: &Map<String, Value>) -> Result<ToolInfo, String> {
    catch_unwind(AssertUnwindSafe(|| spec.describe(args))).map_err(panic_text)
}

impl AgentRuntime {
    /// `execute(name, raw_arguments)`: presupuesto, validación, política,
    /// confirmación, límites y auditoría; el manejador corre con timeout.
    pub fn execute(&self, name: &str, raw_arguments: &Value) -> ToolResult {
        let config = self.config();
        let calls = lock(&self.inner.session).calls_this_turn;
        if calls >= config.general.max_tool_calls_per_turn {
            return ToolResult::err("He alcanzado el límite de herramientas de este turno.");
        }
        let registry = self.registry();
        let Some(spec) = registry.get(name) else {
            return ToolResult::err(format!("Herramienta desconocida: {name}"));
        };
        let args = match parse_arguments(raw_arguments) {
            Ok(args) => args,
            Err(error) => return ToolResult::err(error),
        };
        let payload = Value::Object(args.clone());
        if let Some(error) = registry.validate(name, &payload) {
            self.inner
                .audit_call(name, spec.level, &payload, "deny", false, 0.0, None);
            return ToolResult::err(error);
        }
        if let Some(error) = self.validate_delegate_cwd(name, &args, &config) {
            self.inner
                .audit_call(name, 1, &payload, "deny", false, 0.0, None);
            return ToolResult::err(error);
        }
        let info = match describe(&spec, &args) {
            Ok(info) => info,
            Err(detail) => return ToolResult::err(format!("Argumentos inválidos: {detail}")),
        };
        // DECISION (del Python): leer la salida de un agente marca el turno como
        // no confiable; controlar agentes no toca el sistema.
        let tainted = self.tainted() && !TAINT_FREE_TOOLS.contains(&name);
        let decision = with_self_confirmed(
            decide_with_threshold(
                info.level,
                tainted,
                effective_bulk(&info, config.general.bulk_threshold),
                info.always_confirm,
                self.confirm_nondestructive(),
                config.general.bulk_threshold,
            ),
            spec.self_confirmed,
        );
        lock(&self.inner.session).calls_this_turn += 1;
        if decision.action == Action::Deny {
            self.inner
                .audit_call(name, info.level, &payload, "deny", false, 0.0, None);
            return ToolResult::err(format!("Denegado: {}", decision.reason));
        }
        let needs_confirmation = matches!(decision.action, Action::Confirm | Action::ConfirmStrong);
        if needs_confirmation && !self.confirm_operation(name, &info, decision.action) {
            self.inner
                .audit_call(name, info.level, &payload, "confirm_no", false, 0.0, None);
            return ToolResult::err("El usuario canceló la operación.");
        }
        if info.level == 1 && WRITE_TOOLS.contains(&name) {
            let blocked = self.register_write();
            if let Some(error) = blocked {
                self.inner
                    .audit_call(name, info.level, &payload, "deny", false, 0.0, None);
                return ToolResult::err(error);
            }
        }
        let started = Instant::now();
        let result = registry.run(name, &payload);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if result.ok && spec.untrusted_output {
            let marks = name != "delegate_to_agent"
                || args
                    .get("action")
                    .map(python_str)
                    .unwrap_or_default()
                    .to_lowercase()
                    == "read";
            if marks {
                lock(&self.inner.session).tainted = true;
            }
        }
        if name == "load_skill" {
            self.remember_loaded_skill(&args);
        }
        let audit_decision = if matches!(decision.action, Action::Confirm | Action::ConfirmStrong) {
            "confirm_yes"
        } else {
            "allow"
        };
        self.inner.audit_call(
            name,
            info.level,
            &payload,
            audit_decision,
            result.ok,
            elapsed,
            result.error_kind.as_deref(),
        );
        result
    }

    /// Pregunta a la UI y espera; `on_confirm_done` corre siempre, también si
    /// la puerta falla (el `finally` del Python).
    fn confirm_operation(&self, name: &str, info: &ToolInfo, action: Action) -> bool {
        let summary = if info.summary.is_empty() {
            format!("Voy a ejecutar {name}. ¿Confirmas?")
        } else {
            info.summary.clone()
        };
        let strong = action == Action::ConfirmStrong;
        let Some(gate) = self.confirm_gate() else {
            return false;
        };
        let answer = catch_unwind(AssertUnwindSafe(|| {
            gate.ask(
                &summary,
                strong,
                if strong { Some(info.bulk) } else { None },
            )
        }))
        .unwrap_or(false);
        if let Some(done) = &self.inner.callbacks.on_confirm_done {
            let _ = catch_unwind(AssertUnwindSafe(|| done()));
        }
        answer
    }

    /// `_level1_times`: bloquea por encima de 20 escrituras de nivel 1 por minuto.
    fn register_write(&self) -> Option<&'static str> {
        let now = Instant::now();
        let warn = {
            let mut session = lock(&self.inner.session);
            session
                .level1_times
                .retain(|moment| now.duration_since(*moment) < WRITE_WINDOW);
            if session.level1_times.len() >= WRITE_LIMIT {
                return Some("Demasiadas operaciones de escritura por minuto.");
            }
            session.level1_times.push(now);
            session.level1_writes += 1;
            session.level1_writes == 4
        };
        if warn {
            self.inner
                .audit_warn("Más de 3 escrituras de nivel 1 en un turno.");
        }
        None
    }

    /// `_validate_delegate_cwd`: la carpeta de trabajo de Herdr también pasa por
    /// la política; sin `cwd` se valida el `default_cwd` de la configuración.
    fn validate_delegate_cwd(
        &self,
        name: &str,
        args: &Map<String, Value>,
        config: &AgentConfig,
    ) -> Option<String> {
        if name != "delegate_to_agent" {
            return None;
        }
        let action = args
            .get("action")
            .map(python_str)
            .unwrap_or_default()
            .to_lowercase();
        let mut raw = args.get("cwd").cloned().unwrap_or(Value::Null);
        let empty = raw.is_null() || matches!(&raw, Value::String(text) if text.is_empty());
        if empty {
            if action != "run" && action != "open_terminal" {
                return None;
            }
            raw = self
                .voice_config()
                .get("default_cwd")
                .filter(|value| truthy(value))
                .cloned()?;
        }
        let Value::String(raw) = raw else {
            return Some("La carpeta de trabajo debe ser texto.".into());
        };
        match resolve_safe(
            &raw,
            &config.workspace,
            config.paths.read_roots.as_slice(),
            config.paths.deny.as_slice(),
            config.paths.deny_globs.as_slice(),
        ) {
            Ok(_) => None,
            Err(error) => Some(format!("Carpeta de trabajo no permitida: {error}")),
        }
    }

    /// `render_result`: JSON recortado a 2000 caracteres para el modelo.
    pub fn render_result(&self, result: &ToolResult) -> String {
        crate::registry::ToolRegistry::render_for_llm(result, 2000)
    }
}
