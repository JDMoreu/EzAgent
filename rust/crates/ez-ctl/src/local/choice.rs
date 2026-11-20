//! La elección del agente en `ez delegate` (IR-3): recomendación, guarda de
//! recursos y relleno de modelo/esfuerzo con las reglas de §6.
//!
//! Todo se recolecta una vez ([`gather`]) para que `ez recommend` y
//! `ez delegate` vean el mismo uso, catálogo y presión de memoria. Las pruebas
//! fabrican [`Inputs`] a mano: nada de esto abre un agente.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_herdr::driver::text_list;
use ez_herdr::subagents::Registry;
use ez_scan::capacity::Thresholds;
use ez_scan::recommend::{self, Catalog, Pressure, Recommendation, Request, Size};
use ez_scan::usage::{self, UsageDirs, UsageReport};
use serde_json::Value;

use super::{LocalDelegate, LocalError};

/// Lo que necesita una recomendación, ya leído.
pub(crate) struct Inputs {
    pub(crate) cfg: VoiceConfig,
    pub(crate) report: UsageReport,
    pub(crate) catalog: Catalog,
    pub(crate) pressure: Pressure,
    pub(crate) last_developer_model: Option<String>,
}

/// Lee uso, catálogo, presión y el último `developer` del padre.
pub(crate) fn gather(
    env: &dyn Fn(&str) -> Option<String>,
    cfg: &VoiceConfig,
    parent: &str,
    cwd: &Path,
) -> Inputs {
    let dirs = UsageDirs::from_lookup(env);
    let planes = usage::load_planes(&dirs);
    let report = usage::report_with(cfg, &dirs, &planes, usage::now_ms());
    Inputs {
        cfg: cfg.clone(),
        report,
        catalog: recommend::load(&dirs),
        pressure: pressure(env, parent, cwd),
        last_developer_model: last_developer(env, parent),
    }
}

/// La recomendación completa para un papel (`ez recommend`).
pub(crate) fn recommend_for(
    inputs: &Inputs,
    role: &str,
    size: Size,
) -> Result<Recommendation, String> {
    let request = Request {
        role,
        size,
        last_developer_model: inputs.last_developer_model.as_deref(),
        ..Request::default()
    };
    recommend::choose(
        &inputs.cfg,
        &inputs.report,
        &inputs.catalog,
        &request,
        &inputs.pressure,
    )
}

/// El resultado de completar lo que falta en un `ez delegate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Filled {
    pub(crate) agent: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    /// «Elegí …» o «Completé …», para la salida.
    pub(crate) chosen: Option<String>,
    pub(crate) warnings: Vec<String>,
}

/// Usa la recomendación cuando falta el tipo y completa lo que falte.
///
/// Con `--agent` y `--model` explícitos, el modelo se respeta y solo se
/// completa el esfuerzo; el modelo nunca se toca ni se avisa de que faltaba.
pub(crate) fn fill(request: &LocalDelegate, inputs: &Inputs) -> Result<Filled, LocalError> {
    let automatic = request.agent.trim().is_empty();
    if !automatic && request.model.is_some() {
        // La persona ya eligió el modelo: solo falta el esfuerzo, y nunca se
        // avisa de un modelo que no faltaba (fallo de IR-4).
        let (effort, warnings) = complete_effort(request, inputs);
        return Ok(Filled {
            agent: request.agent.clone(),
            model: request.model.clone(),
            effort,
            chosen: None,
            warnings,
        });
    }
    let query = Request {
        role: &request.role,
        size: Size::M,
        agent: (!automatic).then_some(request.agent.as_str()),
        model: request.model.as_deref(),
        effort: request.effort.as_deref(),
        last_developer_model: inputs.last_developer_model.as_deref(),
    };
    let recommendation = recommend::choose(
        &inputs.cfg,
        &inputs.report,
        &inputs.catalog,
        &query,
        &inputs.pressure,
    );
    let recommendation = match recommendation {
        Ok(recommendation) => recommendation,
        Err(error) if automatic => return Err(LocalError::Rejected(error)),
        Err(error) => {
            return Ok(Filled {
                agent: request.agent.clone(),
                model: request.model.clone(),
                effort: request.effort.clone(),
                chosen: None,
                warnings: vec![format!(
                    "No pude completar el modelo o el esfuerzo: {error}"
                )],
            })
        }
    };
    Ok(Filled {
        agent: recommendation.agent.clone(),
        model: Some(recommendation.model.clone()),
        effort: recommendation.effort.clone(),
        chosen: Some(chosen_line(automatic, &recommendation)),
        warnings: recommendation.warnings,
    })
}

/// El esfuerzo de un modelo elegido a mano y los avisos si no hay ninguno.
///
/// El modelo ya lo puso la persona: aquí solo se completa el esfuerzo con los
/// niveles del agente o, si no los declara, se avisa de solo eso.
fn complete_effort(request: &LocalDelegate, inputs: &Inputs) -> (Option<String>, Vec<String>) {
    if let Some(effort) = request.effort.clone() {
        return (Some(effort), Vec::new());
    }
    let model = request.model.as_deref().unwrap_or_default();
    match effort_for(&inputs.cfg, &request.agent, model, &request.role) {
        Some(effort) => (Some(effort), Vec::new()),
        None => (
            None,
            vec![format!(
                "No pude completar el esfuerzo: {} no declara niveles para {model}.",
                request.agent
            )],
        ),
    }
}

/// El esfuerzo válido del agente para un modelo: el del modelo, el del perfil
/// o el más cercano al de fábrica del papel.
fn effort_for(cfg: &VoiceConfig, agent: &str, model: &str, role: &str) -> Option<String> {
    let profiles = cfg.get("agent_profiles")?.as_object()?;
    let key = cfg
        .get("agents")
        .and_then(Value::as_object)
        .and_then(|agents| ez_core::config::resolve_agent_key(agent, agents, Some(profiles)))
        .unwrap_or_else(|| agent.trim().to_string());
    let profile = profiles.get(&key)?.as_object()?;
    let tail = model.split('#').next().unwrap_or(model);
    let tail = tail.rsplit('/').next().unwrap_or(tail);
    let mut levels = Vec::new();
    if let Some(by_model) = profile.get("model_efforts").and_then(Value::as_object) {
        if let Some(found) = by_model
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(tail))
        {
            levels = text_list(Some(found.1));
        }
    }
    if levels.is_empty() {
        levels = text_list(profile.get("efforts"));
    }
    let wanted = recommend::desired_effort(role);
    if let Some(exact) = levels
        .iter()
        .find(|level| level.eq_ignore_ascii_case(wanted))
    {
        return Some(exact.clone());
    }
    let wanted_rank = effort_rank(wanted)?;
    // Mismo orden que la recomendación: el más cercano y, en empate, el más
    // alto para no bajar la calidad sin decirlo.
    levels
        .iter()
        .filter_map(|level| {
            let rank = effort_rank(level)?;
            Some((rank.abs_diff(wanted_rank), std::cmp::Reverse(rank), level))
        })
        .min_by_key(|(distance, reversed, _)| (*distance, *reversed))
        .map(|(_, _, level)| level.clone())
}

fn effort_rank(level: &str) -> Option<u8> {
    // DECISION: el orden y el desempate repiten los de `ez_scan::recommend`
    // (fuera del alcance de IR-5); si cambian allí, cambian aquí.
    match level.trim().to_lowercase().as_str() {
        "off" | "none" => Some(0),
        "minimal" => Some(1),
        "low" => Some(2),
        "default" | "medium" => Some(3),
        "high" => Some(4),
        "xhigh" => Some(5),
        "max" | "ultra" => Some(6),
        _ => None,
    }
}

/// «Elegí OpenCode · opencode-go/deepseek-v4.1-flash · high porque …».
fn chosen_line(automatic: bool, recommendation: &Recommendation) -> String {
    let verb = if automatic { "Elegí" } else { "Completé" };
    format!(
        "{verb} {} · {} · {} porque {}",
        recommendation.label,
        recommendation.model,
        recommendation.effort.as_deref().unwrap_or("sin esfuerzo"),
        recommendation.reasons.join(" ")
    )
}

/// La guarda de §4: `Err` con código 4 salvo `--force`.
///
/// Si la medición falla (no hay `hc`, por ejemplo), no bloquea: solo se avisa.
pub(crate) fn resource_guard(
    force: bool,
    check: impl FnOnce() -> Result<(), String>,
) -> Result<Option<String>, LocalError> {
    // DECISION: si la medición falla (no hay `hc`), no se bloquea: la falta de
    // dato no es «recursos justos»; se avisa y el lanzamiento sigue.
    match check() {
        Ok(()) => Ok(None),
        Err(text) if !text.starts_with("Recursos justos:") => Ok(Some(format!(
            "No pude medir los recursos: {text}. Sigo adelante."
        ))),
        Err(text) if force => Ok(Some(format!(
            "--force: me salté la guarda de recursos. {text}"
        ))),
        Err(text) => Err(LocalError::Resources(text)),
    }
}

/// Los umbrales de `voice.json` según el entorno inyectado.
pub(crate) fn thresholds(env: &dyn Fn(&str) -> Option<String>) -> Thresholds {
    let path = voice_path(env);
    Thresholds::load(path.as_deref())
}

/// La presión de memoria de §4, ya resumida para la recomendación.
pub(crate) fn pressure(env: &dyn Fn(&str) -> Option<String>, parent: &str, cwd: &Path) -> Pressure {
    let thresholds = thresholds(env);
    let Ok(resources) = ez_scan::resources::collect(&ez_scan::RealProbe, parent, cwd, &thresholds)
    else {
        return Pressure::default();
    };
    let Some(reason) = ez_scan::capacity::reason(&resources, &thresholds) else {
        return Pressure::default();
    };
    let mut memory_mb: Vec<(String, u64)> = Vec::new();
    for child in &resources.subagents {
        if child.agent.trim().is_empty() {
            continue;
        }
        match memory_mb
            .iter_mut()
            .find(|(agent, _)| agent.eq_ignore_ascii_case(&child.agent))
        {
            Some((_, memory)) => *memory = (*memory).max(child.memory_mb),
            None => memory_mb.push((child.agent.clone(), child.memory_mb)),
        }
    }
    Pressure {
        tight: true,
        reason: Some(reason),
        memory_mb,
    }
}

/// El modelo del `developer` más reciente del padre (registro de subagentes).
///
/// DECISION: «más reciente» es el de `since_ms` mayor (el último cambio de
/// estado); el registro no guarda una fecha de creación aparte.
fn last_developer(env: &dyn Fn(&str) -> Option<String>, parent: &str) -> Option<String> {
    if parent.trim().is_empty() {
        return None;
    }
    let dirs = UsageDirs::from_lookup(env);
    let records = Registry::at(&dirs.subagents)
        .list_lenient(parent)
        .ok()?
        .records;
    records
        .into_iter()
        .filter(|record| record.role.eq_ignore_ascii_case("developer"))
        .max_by_key(|record| record.since_ms)
        .and_then(|record| record.model)
}

/// `%APPDATA%\EzAgent\voice.json` (o el `XDG_CONFIG_HOME` de las pruebas).
fn voice_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let dirs = UsageDirs::from_lookup(env);
    let path = dirs.appdata.join("voice.json");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(agent: &str, model: Option<&str>, effort: Option<&str>) -> LocalDelegate {
        LocalDelegate {
            role: "developer".to_string(),
            agent: agent.to_string(),
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            ..LocalDelegate::default()
        }
    }

    fn inputs(report: UsageReport) -> Inputs {
        Inputs {
            cfg: VoiceConfig::new(),
            report,
            catalog: Catalog::factory(),
            pressure: Pressure::default(),
            last_developer_model: None,
        }
    }

    fn installed(key: &str, label: &str) -> ez_scan::usage::AgentUsage {
        ez_scan::usage::AgentUsage {
            key: key.to_string(),
            label: label.to_string(),
            installed: true,
            window_hours: 5.0,
            used_percent: None,
            observed_window_hours: None,
            tokens: None,
            messages: None,
            cost_usd: None,
            models: Vec::new(),
            limit: None,
            resets_ms: None,
            reset_text: None,
            plan: None,
            priority: None,
            state: ez_scan::usage::UsageState::Libre,
            observed_ms: None,
            sources: ez_scan::usage::Sources::default(),
            warnings: Vec::new(),
        }
    }

    fn report() -> UsageReport {
        UsageReport {
            generated_ms: 0,
            agents: vec![
                installed("opencode", "OpenCode"),
                installed("claude", "Claude Code"),
            ],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn sin_agente_elige_y_lo_dice() {
        let filled = fill(&request("", None, None), &inputs(report())).expect("elección");
        assert_eq!(filled.agent, "opencode");
        assert_eq!(
            filled.model.as_deref(),
            Some("opencode-go/deepseek-v4.1-flash")
        );
        assert_eq!(filled.effort.as_deref(), Some("high"));
        let line = filled.chosen.expect("línea");
        assert!(line.starts_with("Elegí OpenCode · opencode-go/deepseek-v4.1-flash · high porque"));
    }

    #[test]
    fn con_agente_explicito_solo_completa_lo_que_falta() {
        let filled = fill(&request("claude", None, None), &inputs(report())).expect("relleno");
        assert_eq!(filled.agent, "claude");
        assert_eq!(filled.model.as_deref(), Some("sonnet"));
        assert!(filled
            .chosen
            .expect("línea")
            .starts_with("Completé Claude Code"));

        let filled = fill(
            &request("claude", Some("sonnet"), Some("low")),
            &inputs(report()),
        )
        .expect("sin cambios");
        assert_eq!(filled.model.as_deref(), Some("sonnet"));
        assert_eq!(filled.effort.as_deref(), Some("low"));
        assert!(filled.chosen.is_none());
    }

    #[test]
    fn con_modelo_explicito_sin_esfuerzo_solo_completa_el_esfuerzo() {
        let filled = fill(
            &request("opencode", Some("deepseek-v4.1-flash"), None),
            &inputs(report()),
        )
        .expect("relleno");
        assert_eq!(filled.agent, "opencode");
        assert_eq!(filled.model.as_deref(), Some("deepseek-v4.1-flash"));
        assert_eq!(filled.effort.as_deref(), Some("high"));
        assert!(filled.chosen.is_none());
        assert!(filled.warnings.is_empty(), "{:?}", filled.warnings);
    }

    #[test]
    fn sin_niveles_de_esfuerzo_el_aviso_habla_solo_del_esfuerzo() {
        let filled = fill(
            &request("qwen", Some("qwen3-coder-plus"), None),
            &inputs(report()),
        )
        .expect("relleno");
        assert_eq!(filled.model.as_deref(), Some("qwen3-coder-plus"));
        assert!(filled.effort.is_none());
        assert!(
            filled
                .warnings
                .iter()
                .all(|warning| !warning.contains("modelo")),
            "{:?}",
            filled.warnings
        );
        assert!(
            filled
                .warnings
                .iter()
                .any(|warning| warning.contains("esfuerzo")),
            "{:?}",
            filled.warnings
        );
    }

    #[test]
    fn sin_catalogo_para_el_agente_explicito_lo_deja_como_vino() {
        let filled = fill(&request("codex", None, None), &inputs(report())).expect("elección");
        assert_eq!(filled.agent, "codex");
        assert!(filled.model.is_none());
        assert!(
            filled
                .warnings
                .iter()
                .any(|warning| warning.contains("No pude completar")),
            "{:?}",
            filled.warnings
        );
    }

    #[test]
    fn la_guarda_es_codigo_4_y_force_la_salta() {
        let error = resource_guard(false, || {
            Err("Recursos justos: RAM libre 1000 MB (mínimo 2500). Espera a que termine alguno o usa --force.".to_string())
        })
        .expect_err("debe rechazar");
        assert_eq!(error.code(), 4);
        assert!(matches!(error, LocalError::Resources(_)));

        let warning = resource_guard(true, || Err("Recursos justos: poca memoria.".to_string()))
            .expect("con --force pasa");
        let warning = warning.expect("aviso");
        assert!(warning.contains("--force"), "{warning:?}");

        assert!(resource_guard(false, || Ok(())).expect("bien").is_none());
    }

    #[test]
    fn un_fallo_de_medicion_no_bloquea() {
        let warning = resource_guard(false, || Err("no hay hc".to_string())).expect("no bloquea");
        assert!(warning.expect("aviso").contains("no hay hc"));
    }
}
