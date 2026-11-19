//! Validación estricta de tipo, modelo y esfuerzo antes de delegar (§5).
//!
//! No crea terminales ni escribe configuraciones: solo comprueba el catálogo y
//! devuelve un [`Choice`] listo para `ez delegate`. Los avisos viajan dentro de
//! `Choice` para que la persona los vea sin que la delegación falle.

use ez_core::pyvalue::truthy;

use super::*;

/// Tipo, modelo y esfuerzo ya validados para delegar en un subagente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// Clave del catálogo (`opencode`, `qwen`…).
    pub key: String,
    /// Etiqueta para personas (`OpenCode`, `Qwen Code`…).
    pub label: String,
    /// Modelo pedido (recortado), o None para el de por omisión.
    pub model: Option<String>,
    /// Esfuerzo pedido (recortado), o None.
    pub effort: Option<String>,
    /// Avisos (modelo sin verificar…).
    pub warnings: Vec<String>,
}

impl<H: SessionHost> AgentDriver<H> {
    /// Valida sin escribir nada (§5). Err = texto para la persona.
    pub fn validate_choice(
        &self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<Choice, String> {
        let cfg = self.session.config();
        let key = self.resolve_type(cfg, agent)?;
        let profile = profile_for(cfg, &key);
        let label = label_of(&profile, &key);
        self.check_program(cfg, &key, &profile)?;

        let model = model.map(py_strip).filter(|value| !value.is_empty());
        let effort = effort.map(py_strip).filter(|value| !value.is_empty());
        if let Some(requested) = model {
            let problem = check_value(cfg, &key, "modelo", requested);
            if !problem.is_empty() {
                return Err(problem);
            }
        }
        if let Some(requested) = effort {
            let problem = check_value(cfg, &key, "esfuerzo", requested);
            if !problem.is_empty() {
                return Err(problem);
            }
        }

        let mut warnings = Vec::new();
        if let Some(requested) = model {
            self.check_model(&profile, &label, requested, &mut warnings)?;
        }
        let codex = if discover(&profile, "codex") {
            self.check_codex(&label, model, &mut warnings)?
        } else {
            None
        };
        if let Some(requested) = effort {
            self.check_effort(&profile, &label, model, requested, codex.as_deref())?;
        }
        Ok(Choice {
            key,
            label,
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            warnings,
        })
    }

    /// Tipo con los alias del catálogo; desconocido → error con las claves.
    fn resolve_type(&self, cfg: &VoiceConfig, agent: &str) -> Result<String, String> {
        if let Some(key) = self.resolve_agent(agent) {
            return Ok(key);
        }
        let mut available: Vec<String> = agents(cfg)
            .map(|agents| agents.keys().cloned().collect())
            .unwrap_or_default();
        available.sort();
        Err(format!(
            "No conozco el agente «{agent}». Agentes del catálogo: {}.",
            available.join(", ")
        ))
    }

    /// El primer elemento del comando debe estar en el PATH (o ser una ruta).
    fn check_program(
        &self,
        cfg: &VoiceConfig,
        key: &str,
        profile: &Map<String, Value>,
    ) -> Result<(), String> {
        let command = text_list(profile.get("command"));
        let command = if command.is_empty() {
            text_list(agents(cfg).and_then(|agents| agents.get(key)))
        } else {
            command
        };
        let program = command.first().map(String::as_str).unwrap_or("");
        if program.is_empty() || !(self.hooks.which)(program) {
            return Err(format!(
                "«{key}» está en el catálogo pero no encuentro su programa ({program})."
            ));
        }
        Ok(())
    }

    /// Modelo contra `models` o contra el catálogo descubierto de mcode.
    fn check_model(
        &self,
        profile: &Map<String, Value>,
        label: &str,
        requested: &str,
        warnings: &mut Vec<String>,
    ) -> Result<(), String> {
        if discover(profile, "codex") {
            return Ok(());
        }
        let models = text_list(profile.get("models"));
        if !models.is_empty() {
            if models.iter().any(|known| model_matches(known, requested)) {
                return Ok(());
            }
            return Err(format!(
                "{label} no tiene el modelo «{requested}». Modelos: {}.",
                models.join(", ")
            ));
        }
        if discover(profile, "mcode") {
            let known = (self.hooks.discover_mcode_models)();
            if known.is_empty() {
                warnings.push(unverified(label, requested));
            } else if !known
                .iter()
                .any(|reference| mcode_matches(reference, requested))
            {
                return Err(format!(
                    "{label} no tiene el modelo «{requested}». Modelos: {}.",
                    known.join(", ")
                ));
            }
            return Ok(());
        }
        warnings.push(format!(
            "El catálogo de {label} no lista modelos; uso «{requested}» sin verificar."
        ));
        Ok(())
    }

    /// Modelo y niveles de Codex contra su catálogo descubierto.
    fn check_codex(
        &self,
        label: &str,
        model: Option<&str>,
        warnings: &mut Vec<String>,
    ) -> Result<Option<Vec<String>>, String> {
        let table = (self.hooks.discover_codex_models)();
        if table.is_empty() {
            if let Some(requested) = model {
                warnings.push(unverified(label, requested));
            }
            return Ok(None);
        }
        let Some(requested) = model else {
            return Ok(Some(union_levels(&table)));
        };
        let slug = table
            .keys()
            .find(|name| name.eq_ignore_ascii_case(requested))
            .cloned();
        let Some(slug) = slug else {
            let known: Vec<String> = table.keys().cloned().collect();
            return Err(format!(
                "Codex no tiene el modelo {}. Modelos: {}.",
                repr_str(requested),
                known.join(", ")
            ));
        };
        Ok(Some(text_list(table.get(&slug))))
    }

    /// Esfuerzo contra `efforts`, `model_efforts` y los niveles de Codex.
    fn check_effort(
        &self,
        profile: &Map<String, Value>,
        label: &str,
        model: Option<&str>,
        requested: &str,
        codex: Option<&[String]>,
    ) -> Result<(), String> {
        if let Some(name) = model {
            if let Some(by_model) = profile.get("model_efforts").and_then(Value::as_object) {
                let name = name.rsplit('/').next().unwrap_or(name);
                let known = text_list(by_model.get(name));
                if !known.is_empty() && !has_ignore_case(&known, requested) {
                    return Err(format!(
                        "El modelo {name} no tiene el esfuerzo {}. Valores válidos: {}.",
                        repr_str(requested),
                        known.join(", ")
                    ));
                }
            }
        }
        if let Some(levels) = codex {
            if !levels.is_empty() && !has_ignore_case(levels, requested) {
                let suffix = model.map_or(String::new(), |name| format!(" para {name}"));
                return Err(format!(
                    "El esfuerzo {} no existe en Codex{suffix}. Valores válidos: {}.",
                    repr_str(requested),
                    levels.join(", ")
                ));
            }
            return Ok(());
        }
        if has_effort_support(profile) {
            return Ok(());
        }
        Err(format!("{label} no tiene niveles de esfuerzo."))
    }
}

/// Etiqueta del perfil, o la clave si no la declara.
fn label_of(profile: &Map<String, Value>, key: &str) -> String {
    profile
        .get("label")
        .filter(|value| truthy(value))
        .map(python_str)
        .unwrap_or_else(|| key.to_string())
}

// DECISION: la validación no reutiliza `match_mcode_model`/`resolve_codex` de
// `launch.rs` porque ese archivo queda fuera del alcance de HD-3 y esas rutas
// escriben configuración; aquí solo se lee el catálogo inyectado por hooks.

/// Modelo con o sin prefijo (`opencode-go/m`) o `proveedor/m`.
fn model_matches(known: &str, requested: &str) -> bool {
    let known = known.to_lowercase();
    let requested = requested.to_lowercase();
    let tail = requested.rsplit('/').next().unwrap_or(&requested);
    known == requested || known == tail
}

/// Ref completo de mcode o lo que va tras su primera barra.
fn mcode_matches(reference: &str, requested: &str) -> bool {
    let reference = reference.to_lowercase();
    let requested = requested.to_lowercase();
    let tail = reference.split_once('/').map_or("", |(_, rest)| rest);
    requested == reference || requested == tail
}

/// Niveles de todos los modelos de Codex, sin repetir y en orden.
fn union_levels(table: &Map<String, Value>) -> Vec<String> {
    let mut levels: Vec<String> = Vec::new();
    for value in table.values() {
        for level in text_list(Some(value)) {
            if !levels.contains(&level) {
                levels.push(level);
            }
        }
    }
    levels
}

/// Modelo que no se pudo verificar en el catálogo del programa.
fn unverified(label: &str, requested: &str) -> String {
    format!("No pude leer los modelos de {label}; uso «{requested}» sin verificar.")
}

/// ¿El perfil tiene alguna forma de fijar el esfuerzo?
fn has_effort_support(profile: &Map<String, Value>) -> bool {
    !text_list(profile.get("efforts")).is_empty()
        || profile
            .get("model_efforts")
            .and_then(Value::as_object)
            .is_some_and(|by_model| !by_model.is_empty())
        || !text_list(profile.get("effort_args")).is_empty()
        || profile
            .get("env")
            .and_then(Value::as_object)
            .is_some_and(|env| !env.is_empty())
        || is_prelaunch(profile, "mcode_effort")
}

#[cfg(test)]
mod tests;
