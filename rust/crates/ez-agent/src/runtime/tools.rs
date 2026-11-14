//! Registro de herramientas y selección por turno (F4-09).

use std::sync::Arc;

use ez_core::pyvalue::truthy;
use serde_json::Value;

use crate::registry::ToolRegistry;
use crate::tools_memory::{build_memory_tools, ConfirmCallback};

use super::state::{lock, read, write};
use super::{select_tool_names, AgentRuntime, SelectOptions, ToolGroupBuilder, BASE_TOOLS};

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

impl AgentRuntime {
    /// `_rebuild_tools`: cada grupo se construye aparte; uno roto no tumba el
    /// arranque (se audita el aviso, como el `except` del Python).
    pub(crate) fn rebuild_tools(&self) {
        let registry = ToolRegistry::new();
        let config = self.config();
        let journal = Arc::clone(&self.inner.journal);
        let skills = Arc::clone(&self.inner.skills);
        let memory = Arc::clone(&self.inner.memory);
        let weak = Arc::downgrade(&self.inner);
        let confirm: ConfirmCallback = Arc::new(move |summary: &str, strong: bool| {
            let Some(inner) = weak.upgrade() else {
                return false;
            };
            let Some(gate) = inner.confirm_gate() else {
                return false;
            };
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gate.ask(summary, strong, None)
            }))
            .unwrap_or(false)
        });
        let groups: Vec<ToolGroupBuilder> = vec![
            {
                let config = config.clone();
                Arc::new(move |_: &AgentRuntime| crate::tools_core::build_core_tools(&config))
            },
            {
                let config = config.clone();
                let journal = Arc::clone(&journal);
                Arc::new(move |_: &AgentRuntime| {
                    crate::tools_fs::build_fs_tools(
                        &config,
                        Arc::clone(&journal),
                        crate::tools_fs::default_opener(),
                    )
                })
            },
            {
                let config = config.clone();
                Arc::new(move |_: &AgentRuntime| crate::tools_web::build_web_tools(&config, None))
            },
            {
                let config = config.clone();
                let skills = Arc::clone(&skills);
                Arc::new(move |_: &AgentRuntime| {
                    crate::tools_skills::build_skill_tools(&config, Arc::clone(&skills))
                })
            },
            {
                let config = config.clone();
                let memory = Arc::clone(&memory);
                let confirm = Arc::clone(&confirm);
                Arc::new(move |_: &AgentRuntime| {
                    build_memory_tools(
                        &config,
                        Arc::clone(&memory),
                        Some(Arc::clone(&confirm)),
                        None,
                    )
                })
            },
            {
                let config = config.clone();
                Arc::new(move |_: &AgentRuntime| crate::tools_d::load_tools_d(&config))
            },
        ];
        let mut all = groups;
        all.extend(read(&self.inner.tool_groups).iter().cloned());
        for group in all {
            let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| group(self)));
            match built {
                Ok(tools) => registry.register_many(tools),
                Err(payload) => self.inner.audit_warn(&format!(
                    "No pude cargar un grupo de herramientas: {}",
                    panic_text(payload)
                )),
            }
        }
        *write(&self.inner.registry) = Arc::new(registry);
    }

    /// `tools_for`: hasta 14 esquemas, conservando un turno las extra del anterior.
    pub fn tools_for(&self, text: &str) -> Vec<Value> {
        let loaded = lock(&self.inner.session).loaded_skills.clone();
        let scripts = loaded.iter().any(|name| self.skill_has_scripts(name));
        let keywords = self.inner.skills.keywords();
        let selected = select_tool_names(
            text,
            &SelectOptions {
                scripts_loaded: scripts,
                has_memory: true,
                skill_keywords: Some(keywords.as_ref()),
                swarm_active: self.inner.swarm_active(),
            },
        );
        let mut names = selected.clone();
        {
            let mut session = lock(&self.inner.session);
            for carried in &session.carry_tools {
                if !names.contains(carried) {
                    names.push(carried.clone());
                }
            }
            names.truncate(14);
            session.next_carry = selected
                .iter()
                .filter(|name| !BASE_TOOLS.contains(&name.as_str()))
                .cloned()
                .collect();
        }
        self.registry().schemas(Some(&names), 14)
    }

    fn skill_has_scripts(&self, name: &str) -> bool {
        self.inner
            .skills
            .get(name)
            .is_some_and(|skill| !skill.scripts.is_empty())
    }

    /// Entrada del `load_skill` que marca la skill para los turnos siguientes.
    pub(crate) fn remember_loaded_skill(&self, args: &serde_json::Map<String, Value>) {
        let name = args
            .get("name")
            .filter(|value| truthy(value))
            .map(ez_core::pyvalue::python_str)
            .unwrap_or_default();
        if !name.is_empty() {
            lock(&self.inner.session).loaded_skills.insert(name);
        }
    }
}
