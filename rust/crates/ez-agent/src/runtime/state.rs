//! Estado interno y ciclo de vida del [`AgentRuntime`] (F4-09).

use std::collections::HashSet;
use std::path::Path;
use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak,
};
use std::time::{Instant, UNIX_EPOCH};

use ez_core::agent_config::AgentConfig;
use ez_core::config::VoiceConfig;
use ez_core::pyvalue::{head_chars, python_str, truthy};
use serde_json::{json, Value};

use crate::audit::{AuditEntry, AuditLog};
use crate::confirm::ConfirmationManager;
use crate::journal::Journal;
use crate::memory::MemoryStore;
use crate::registry::{ToolRegistry, ToolResult};
use crate::skills::{ensure_default_skills, SkillLibrary, MAX_CATALOG};
use crate::system_prompt::{Callbacks, Section, SystemPromptBuilder};

use super::{AgentRuntime, ConfirmationGate, HerdrApi, RuntimeCallbacks};

type AskCallback = Box<dyn Fn(&str, bool) + Send + Sync>;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

/// Lo que cambia en cada turno.
#[derive(Default)]
pub(crate) struct SessionState {
    pub turn_id: String,
    pub tainted: bool,
    pub confirm_nondestructive: bool,
    pub last_agent_address: Option<String>,
    pub loaded_skills: HashSet<String>,
    pub carry_tools: Vec<String>,
    pub next_carry: Vec<String>,
    pub calls_this_turn: i64,
    pub level1_writes: i64,
    pub level1_times: Vec<Instant>,
    pub last_user_text: String,
}

/// Todo lo que comparten las copias de [`AgentRuntime`].
pub struct Inner {
    pub(crate) voice_config: RwLock<VoiceConfig>,
    pub(crate) herdr: Option<Arc<dyn HerdrApi>>,
    pub(crate) callbacks: RuntimeCallbacks,
    pub(crate) config: RwLock<AgentConfig>,
    pub(crate) audit: Arc<AuditLog>,
    pub(crate) session_id: String,
    pub(crate) memory: Arc<MemoryStore>,
    pub(crate) skills: Arc<SkillLibrary>,
    pub(crate) journal: Arc<Journal>,
    pub(crate) prompt: Mutex<Option<SystemPromptBuilder>>,
    pub(crate) confirm: RwLock<Option<Arc<dyn ConfirmationGate>>>,
    pub(crate) registry: RwLock<Arc<ToolRegistry>>,
    pub(crate) tool_groups: RwLock<Vec<super::ToolGroupBuilder>>,
    pub(crate) session: Arc<Mutex<SessionState>>,
    pub(crate) permissions_mtime: Mutex<f64>,
}

fn float_mtime(path: &Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

/// `_audit_warn`: aviso de sistema en la auditoría, sin turno si aún no hay.
fn audit_warning(audit: &AuditLog, session: &Mutex<SessionState>, message: &str) {
    let turn_id = {
        let session = lock(session);
        if session.turn_id.is_empty() {
            "startup".to_string()
        } else {
            session.turn_id.clone()
        }
    };
    let args = json!({"warning": head_chars(message, 500)});
    audit.log(&AuditEntry {
        turn_id: &turn_id,
        tool: "system",
        level: 0,
        args: &args,
        decision: "allow",
        taint: false,
        result_ok: true,
        ms: 0.0,
        error_kind: None,
    });
}

impl Inner {
    pub(crate) fn audit_warn(&self, message: &str) {
        audit_warning(&self.audit, &self.session, message);
    }

    pub(crate) fn warn(&self, message: &str) {
        if let Some(callback) = &self.callbacks.on_warn {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callback(message);
            }));
        }
    }

    pub(crate) fn confirm_gate(&self) -> Option<Arc<dyn ConfirmationGate>> {
        read(&self.confirm).clone()
    }

    pub(crate) fn confirm_ask(&self, summary: &str, strong: bool) {
        let Some(callback) = self.callbacks.on_confirm_ask.clone() else {
            return;
        };
        let token = self.confirm_gate().and_then(|gate| gate.pending_token());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(summary, strong, token.as_deref());
        }));
    }

    /// `_audit`: una línea por llamada a herramienta.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn audit_call(
        &self,
        tool: &str,
        level: i64,
        args: &Value,
        decision: &str,
        ok: bool,
        ms: f64,
        error_kind: Option<&str>,
    ) {
        let (turn_id, taint) = {
            let session = lock(&self.session);
            (session.turn_id.clone(), session.tainted)
        };
        let turn_id = if turn_id.is_empty() {
            "no-turn".to_string()
        } else {
            turn_id
        };
        self.audit.log(&AuditEntry {
            turn_id: &turn_id,
            tool,
            level,
            args,
            decision,
            taint,
            result_ok: ok,
            ms,
            error_kind,
        });
    }

    /// Memoria persistente + tareas pendientes, dentro del prompt.
    pub(crate) fn memory_and_tasks(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let memory = self.memory.memory_block();
        if !memory.is_empty() {
            parts.push(memory);
        }
        let tasks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.callbacks.pending_tasks)()
        }))
        .unwrap_or_default();
        if !tasks.is_empty() {
            parts.push(tasks);
        }
        parts.join("\n\n")
    }

    pub(crate) fn agents_catalog(&self) -> String {
        self.herdr
            .as_ref()
            .map_or_else(String::new, |herdr| herdr.catalog_text())
    }

    pub(crate) fn agent_windows(&self) -> String {
        self.herdr
            .as_ref()
            .map_or_else(String::new, |herdr| herdr.context_text())
    }

    pub(crate) fn swarm_active(&self) -> bool {
        self.callbacks.swarm.as_ref().is_some_and(|swarm| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| swarm.active()))
                .unwrap_or(false)
        })
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn prompt_callbacks(&self, weak: Weak<Inner>) -> Callbacks {
        let text_section = |method: fn(&Inner) -> String| {
            let weak = weak.clone();
            Section::Func(Box::new(move || {
                weak.upgrade()
                    .map_or_else(String::new, |inner| method(&inner))
            }))
        };
        let skills = Arc::clone(&self.skills);
        let audit = weak.clone();
        let say = weak.clone();
        Callbacks {
            memory_block: Some(text_section(Inner::memory_and_tasks)),
            skills_catalog: Some(Section::Func(Box::new(move || skills.catalog_text()))),
            agents_catalog: Some(text_section(Inner::agents_catalog)),
            agent_windows: Some(text_section(Inner::agent_windows)),
            audit_warn: Some(Box::new(move |message| {
                if let Some(inner) = audit.upgrade() {
                    inner.audit_warn(message);
                }
            })),
            say_warn: Some(Box::new(move |message| {
                if let Some(inner) = say.upgrade() {
                    inner.warn(message);
                }
            })),
        }
    }
}

/// Reconstruye un `ToolResult` desde el valor que devuelve `execute`.
pub(crate) fn tool_result_from_value(value: &Value) -> ToolResult {
    let Some(map) = value.as_object() else {
        return ToolResult::ok(value.clone());
    };
    let text = |key: &str| match map.get(key) {
        None | Some(Value::Null) => None,
        Some(value) => Some(python_str(value)),
    };
    ToolResult {
        ok: map.get("ok").map_or(!map.contains_key("error"), truthy),
        data: map.get("data").cloned().unwrap_or(Value::Null),
        error: text("error"),
        outcome: map
            .get("outcome")
            .filter(|outcome| truthy(outcome))
            .map_or_else(|| "completed".to_string(), python_str),
        error_kind: text("error_kind"),
    }
}

/// `AgentRuntime.__init__` completo.
pub(crate) fn build(
    voice_config: VoiceConfig,
    herdr: Option<Arc<dyn HerdrApi>>,
    callbacks: RuntimeCallbacks,
    config_path: Option<&Path>,
) -> Result<AgentRuntime, String> {
    let (config, _warnings) = AgentConfig::load(config_path);
    config.ensure_dirs().map_err(|error| error.to_string())?;
    let audit = Arc::new(AuditLog::new(&config.audit_path));
    let session = Arc::new(Mutex::new(SessionState {
        confirm_nondestructive: voice_config.get("confirm_actions").is_some_and(truthy),
        ..SessionState::default()
    }));
    let audit_for_skills = Arc::clone(&audit);
    let session_for_skills = Arc::clone(&session);
    let skills = Arc::new(SkillLibrary::new(
        &config.skills_dir,
        &config.global_skills_dir,
        Some(Box::new(move |message: &str| {
            audit_warning(&audit_for_skills, &session_for_skills, message);
        })),
        MAX_CATALOG,
    ));
    skills.refresh(false);
    ensure_default_skills(&config.skills_dir, None);
    skills.refresh(false);
    let memory = Arc::new(MemoryStore::new(&config)?);
    let journal = Arc::new(Journal::new(Some(&config.journal_dir))?);
    let inner = Arc::new(Inner {
        voice_config: RwLock::new(voice_config),
        herdr,
        callbacks,
        config: RwLock::new(config),
        audit,
        session_id: super::uuid_hex(),
        memory,
        skills,
        journal,
        prompt: Mutex::new(None),
        confirm: RwLock::new(None),
        registry: RwLock::new(Arc::new(ToolRegistry::new())),
        tool_groups: RwLock::new(Vec::new()),
        session,
        permissions_mtime: Mutex::new(0.0),
    });
    let runtime = AgentRuntime { inner };
    runtime.rebuild_prompt();
    runtime.rebuild_confirm();
    runtime.rebuild_tools();
    let permissions_path = read(&runtime.inner.config).permissions_path.clone();
    let mtime = float_mtime(&permissions_path);
    *lock(&runtime.inner.permissions_mtime) = mtime;
    Ok(runtime)
}

impl AgentRuntime {
    /// Relee permisos y skills solo cuando cambiaron en disco (F26/F27).
    pub fn refresh(&self) {
        let mtime = float_mtime(&read(&self.inner.config).permissions_path);
        let previous = *lock(&self.inner.permissions_mtime);
        let permissions_changed = mtime != previous;
        if permissions_changed {
            *lock(&self.inner.permissions_mtime) = mtime;
            if let Err(error) = self.reload_permissions() {
                self.inner
                    .audit_warn(&format!("No pude recargar permissions.toml: {error}"));
            }
        }
        let skills_changed = self.inner.skills.refresh(false);
        if permissions_changed || skills_changed {
            self.rebuild_tools();
        }
    }

    fn reload_permissions(&self) -> Result<(), String> {
        let path = read(&self.inner.config).permissions_path.clone();
        let (config, _warnings) = AgentConfig::load(Some(&path));
        config.ensure_dirs().map_err(|error| error.to_string())?;
        let config = self.preserve_session_paths(config);
        *write(&self.inner.config) = config;
        self.rebuild_prompt();
        self.rebuild_confirm();
        Ok(())
    }

    /// Conserva las rutas que ya usan los objetos de larga vida (F27).
    fn preserve_session_paths(&self, mut config: AgentConfig) -> AgentConfig {
        let current = self.config();
        let mut changed: Vec<&'static str> = Vec::new();
        if config.workspace != current.workspace {
            config.workspace = current.workspace.clone();
            changed.push("workspace");
        }
        if config.memory_dir != current.memory_dir {
            config.memory_dir = current.memory_dir.clone();
            changed.push("memory_dir");
        }
        if config.history_path != current.history_path {
            config.history_path = current.history_path.clone();
            changed.push("history_path");
        }
        if config.journal_dir != current.journal_dir {
            config.journal_dir = current.journal_dir.clone();
            changed.push("journal_dir");
        }
        if config.audit_path != current.audit_path {
            config.audit_path = current.audit_path.clone();
            changed.push("audit_path");
        }
        if config.skills_dir != current.skills_dir {
            config.skills_dir = current.skills_dir.clone();
            changed.push("skills_dir");
        }
        if config.global_skills_dir != current.global_skills_dir {
            config.global_skills_dir = current.global_skills_dir.clone();
            changed.push("global_skills_dir");
        }
        if config.system_md_path != current.system_md_path {
            config.system_md_path = current.system_md_path.clone();
            changed.push("system_md_path");
        }
        if config.tools_d_dir != current.tools_d_dir {
            config.tools_d_dir = current.tools_d_dir.clone();
            changed.push("tools_d_dir");
        }
        if config.paths.read_roots != current.paths.read_roots {
            config.paths.read_roots = current.paths.read_roots.clone();
            changed.push("paths.read_roots");
        }
        if changed.is_empty() {
            return config;
        }
        let message = format!(
            "Cambiaron rutas de permissions.toml ({}), pero esta sesión sigue usando las \
             anteriores; reinicia EzAgent para aplicarlas.",
            changed.join(", ")
        );
        self.inner.audit_warn(&message);
        self.inner.warn(&message);
        config
    }

    /// `_make_confirm` / recarga: gestor con el callback de la UI.
    pub(crate) fn rebuild_confirm(&self) {
        let config = self.config();
        let on_ask: Option<AskCallback> = if self.inner.callbacks.on_confirm_ask.is_some() {
            let weak = Arc::downgrade(&self.inner);
            Some(Box::new(move |summary: &str, strong: bool| {
                if let Some(inner) = weak.upgrade() {
                    inner.confirm_ask(summary, strong);
                }
            }))
        } else {
            None
        };
        let manager = ConfirmationManager::new(
            &config.confirm.accept,
            &config.confirm.reject,
            config.general.confirm_timeout_s,
            on_ask,
        );
        *write(&self.inner.confirm) = Some(Arc::new(manager));
    }

    /// Rehace el `SystemPromptBuilder` con los callbacks del runtime.
    pub(crate) fn rebuild_prompt(&self) {
        let weak = Arc::downgrade(&self.inner);
        let callbacks = self.inner.prompt_callbacks(weak);
        let path = read(&self.inner.config).system_md_path.clone();
        let builder = SystemPromptBuilder::new(&path, callbacks);
        *lock(&self.inner.prompt) = Some(builder);
    }
}
