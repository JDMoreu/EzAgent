//! Lanzamiento de agentes: comando, flags o entorno, y lo que queda pendiente.

use ez_core::config::VoiceConfig;
use ez_core::pyvalue::{py_strip, python_str, repr_str, text_or_empty};
use serde_json::{Map, Value};

use super::*;
use crate::session::session_steps;

impl<H: SessionHost> AgentDriver<H> {
    /// Comando del agente y lo que se fijará dentro de la sesión.
    ///
    /// Si el CLI tiene flag para el modelo o el esfuerzo, va en el comando; si
    /// no, pero tiene comando de sesión, queda en `pending`; si tampoco, se
    /// anota un aviso (el lanzamiento no falla).
    pub fn agent_launch(
        &self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> AgentLaunch {
        self.build_launch(agent, model, effort)
    }

    /// Atajo de `agent_launch` que devuelve solo el argv (o None).
    pub fn agent_argv(
        &self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Option<Vec<String>> {
        self.agent_launch(agent, model, effort).argv
    }

    pub(super) fn build_launch(
        &self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> AgentLaunch {
        let cfg = self.session.config();
        let Some(key) = resolve_agent_in(cfg, agent) else {
            return unknown_agent(cfg, agent);
        };
        let profile = profile_for(cfg, &key);
        let asked = model.is_some_and(|value| !value.is_empty())
            || effort.is_some_and(|value| !value.is_empty());
        if asked && profile.is_empty() {
            return missing_profile(&key);
        }
        let mut wanted = Wanted {
            model: wanted_value(model, text_or_empty(profile.get("default_model"))),
            effort: wanted_value(effort, text_or_empty(profile.get("default_effort"))),
        };
        if is_prelaunch(&profile, "opencode_state")
            && !wanted.effort.is_empty()
            && wanted.model.is_empty()
        {
            // Un esfuerzo sin modelo no se puede fijar en el arranque: se usa
            // el modelo por defecto de OpenCode (evita ciclar variantes dentro).
            wanted.model = (self.hooks.opencode_default_model)();
        }
        for (kind, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
            let value = wanted.get(kind);
            if !value.is_empty() {
                let problem = check_value(cfg, &key, label, value);
                if !problem.is_empty() {
                    return AgentLaunch {
                        error: problem,
                        ..AgentLaunch::default()
                    };
                }
            }
        }
        if let Some(problem) = self.discover_problem(&profile, &mut wanted) {
            return AgentLaunch {
                error: problem,
                ..AgentLaunch::default()
            };
        }
        let mut launch = AgentLaunch {
            argv: Some(agent_command(cfg, &key, &profile)),
            ..AgentLaunch::default()
        };
        let consumed = self.fill_args(&profile, &wanted, &mut launch);
        self.finish_args(cfg, &key, &profile, &wanted, consumed, &mut launch);
        launch
    }

    /// mcode y Codex resuelven el modelo contra su catálogo local.
    fn discover_problem(
        &self,
        profile: &Map<String, Value>,
        wanted: &mut Wanted,
    ) -> Option<String> {
        if discover(profile, "mcode") && !wanted.model.is_empty() {
            let known = (self.hooks.discover_mcode_models)();
            if !known.is_empty() {
                match match_mcode_model(&known, &wanted.model) {
                    Some(reference) => wanted.model = reference,
                    None => {
                        return Some(format!(
                            "mcode no tiene el modelo {}. Modelos: {}.",
                            repr_str(&wanted.model),
                            known.join(", ")
                        ))
                    }
                }
            }
        }
        if discover(profile, "codex") {
            let problem = resolve_codex(&(self.hooks.discover_codex_models)(), wanted);
            if !problem.is_empty() {
                return Some(problem);
            }
        }
        if !wanted.model.is_empty() && !wanted.effort.is_empty() {
            if let Some(model_efforts) = profile.get("model_efforts").and_then(Value::as_object) {
                let name = wanted.model.rsplit('/').next().unwrap_or(&wanted.model);
                let known = text_list(model_efforts.get(name));
                if !known.is_empty() && !has_ignore_case(&known, &wanted.effort) {
                    return Some(format!(
                        "El modelo {name} no tiene el esfuerzo {}. Valores válidos: {}.",
                        repr_str(&wanted.effort),
                        known.join(", ")
                    ));
                }
            }
        }
        None
    }

    /// Modelo y esfuerzo por variable de entorno o por flag; qué quedó fijado.
    fn fill_args(
        &self,
        profile: &Map<String, Value>,
        wanted: &Wanted,
        launch: &mut AgentLaunch,
    ) -> (bool, bool) {
        let model_args = flag_args(profile, "model_args", ["--model", "{model}"]);
        let prefix = text_or_empty(profile.get("model_prefix"));
        let model_ref = if wanted.model.contains('/') || wanted.model.is_empty() {
            wanted.model.clone()
        } else {
            format!("{prefix}{}", wanted.model)
        };
        let mut consumed = (false, false);
        let env = profile.get("env").and_then(Value::as_object);
        if !wanted.model.is_empty() {
            if let Some(env) = env.filter(|env| !env.is_empty()) {
                // Modelo y esfuerzo por variable de entorno (OpenCode lee
                // «proveedor/modelo#variante» de OPENCODE_CONFIG_CONTENT).
                let variant = if wanted.effort.is_empty() {
                    String::new()
                } else {
                    format!("#{}", wanted.effort)
                };
                for (name, template) in env {
                    let value = python_str(template)
                        .replace("{model_ref}", &model_ref)
                        .replace("{variant}", &variant);
                    launch.env.insert(name.clone(), Value::String(value));
                }
                append_args(launch, text_list(profile.get("env_args")));
                if is_prelaunch(profile, "opencode_state") {
                    launch.env.extend((self.hooks.prepare_opencode_state)(
                        &model_ref,
                        &wanted.effort,
                    ));
                }
                consumed = (true, !wanted.effort.is_empty());
            } else if !model_args.is_empty() {
                // «{variant}» deja pegar el esfuerzo al modelo (mcode: modelo#var).
                let has_variant = model_args.iter().any(|part| part.contains("{variant}"));
                let variant = if !wanted.effort.is_empty() && has_variant {
                    format!("#{}", wanted.effort)
                } else {
                    String::new()
                };
                let args = model_args
                    .iter()
                    .map(|part| {
                        part.replace("{model}", &model_ref)
                            .replace("{variant}", &variant)
                    })
                    .collect();
                append_args(launch, args);
                consumed = (true, has_variant);
            }
        }
        if is_prelaunch(profile, "mcode_effort")
            && !wanted.effort.is_empty()
            && (self.hooks.set_mcode_effort)(&wanted.effort)
        {
            // mcode lee el esfuerzo de su config.yaml, no de un flag del TUI.
            consumed.1 = true;
        }
        consumed
    }

    /// Lo que no cupo en flags/env: comando de sesión o aviso.
    fn finish_args(
        &self,
        cfg: &VoiceConfig,
        key: &str,
        profile: &Map<String, Value>,
        wanted: &Wanted,
        consumed: (bool, bool),
        launch: &mut AgentLaunch,
    ) {
        let effort_args = flag_args(profile, "effort_args", ["--effort", "{effort}"]);
        for (kind, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
            let value = wanted.get(kind);
            let done = match kind {
                "model" => consumed.0,
                _ => consumed.1,
            };
            if value.is_empty() || done {
                continue;
            }
            if kind == "effort" && !effort_args.is_empty() {
                let args = effort_args
                    .iter()
                    .map(|part| part.replace("{effort}", value))
                    .collect();
                append_args(launch, args);
            } else if session_steps(cfg, key, kind).is_empty() {
                launch.warnings.push(format!(
                    "{key} no permite fijar el {label} desde aquí; se abrió con su \
                     {label} por defecto."
                ));
            } else {
                launch
                    .pending
                    .insert(kind.to_string(), Value::String(value.to_string()));
            }
        }
    }
}

/// `str(model or default or "").strip()`, sin recortar el resultado del
/// catálogo (Python tampoco lo hace).
fn wanted_value(asked: Option<&str>, default: String) -> String {
    let value = match asked {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => default,
    };
    py_strip(&value).to_string()
}

/// `Agente desconocido: …` con las claves ordenadas.
fn unknown_agent(cfg: &VoiceConfig, agent: &str) -> AgentLaunch {
    let mut available: Vec<String> = agents(cfg)
        .map(|agents| agents.keys().cloned().collect())
        .unwrap_or_default();
    available.sort();
    AgentLaunch {
        error: format!(
            "Agente desconocido: {}. Disponibles: {}.",
            repr_str(agent),
            available.join(", ")
        ),
        ..AgentLaunch::default()
    }
}

/// Un agente sin perfil no puede fijar modelo ni esfuerzo.
fn missing_profile(key: &str) -> AgentLaunch {
    AgentLaunch {
        error: format!(
            "El agente {} no tiene perfil de modelo/esfuerzo. Configura \
             agent_profiles en voice.json.",
            repr_str(key)
        ),
        ..AgentLaunch::default()
    }
}

fn agent_command(cfg: &VoiceConfig, key: &str, profile: &Map<String, Value>) -> Vec<String> {
    let command = text_list(profile.get("command"));
    if command.is_empty() {
        text_list(agents(cfg).and_then(|agents| agents.get(key)))
    } else {
        command
    }
}

/// Resuelve el modelo de mcode contra los refs de su config.yaml.
///
/// mcode distingue mayúsculas (`minimax/MiniMax-M3.1-Flash`); se acepta el ref
/// completo o lo que va tras la primera barra.
fn match_mcode_model(known: &[String], model: &str) -> Option<String> {
    let low = model.to_lowercase();
    known
        .iter()
        .find(|reference| {
            let reference = reference.to_lowercase();
            let tail = reference.split_once('/').map_or("", |(_, rest)| rest);
            low == reference || low == tail
        })
        .cloned()
}

/// Valida modelo/esfuerzo contra el catálogo de Codex; "" si está bien.
fn resolve_codex(table: &Map<String, Value>, wanted: &mut Wanted) -> String {
    if table.is_empty() {
        return String::new();
    }
    let asked = wanted.model.clone();
    let levels = if asked.is_empty() {
        let mut levels: Vec<String> = Vec::new();
        for value in table.values() {
            for level in text_list(Some(value)) {
                if !levels.contains(&level) {
                    levels.push(level);
                }
            }
        }
        levels
    } else {
        let slug = table
            .keys()
            .find(|slug| slug.to_lowercase() == asked.to_lowercase())
            .cloned();
        let Some(slug) = slug else {
            let known: Vec<String> = table.keys().cloned().collect();
            return format!(
                "Codex no tiene el modelo {}. Modelos: {}.",
                repr_str(&asked),
                known.join(", ")
            );
        };
        wanted.model = slug.clone();
        text_list(table.get(&slug))
    };
    if !wanted.effort.is_empty() && !has_ignore_case(&levels, &wanted.effort) {
        let suffix = if asked.is_empty() {
            String::new()
        } else {
            format!(" para {}", wanted.model)
        };
        return format!(
            "El esfuerzo {} no existe en Codex{suffix}. Valores válidos: {}.",
            repr_str(&wanted.effort),
            levels.join(", ")
        );
    }
    String::new()
}

/// `AgentDriver._resolve_codex` con la tabla del catálogo en disco y el
/// `wanted` como diccionario (`{"model": …, "effort": …}`); devuelve el error y
/// el modelo resuelto.
#[doc(hidden)]
pub fn resolve_codex_value(wanted: &Value) -> (String, String) {
    let mut wanted = Wanted {
        model: text_or_empty(wanted.get("model")),
        effort: text_or_empty(wanted.get("effort")),
    };
    let error = resolve_codex(&crate::prelaunch::discover_codex_models(), &mut wanted);
    (error, wanted.model)
}
