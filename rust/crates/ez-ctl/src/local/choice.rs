//! La elección del agente en `ez delegate` (IR-3): recomendación, guarda de
//! recursos y relleno de modelo/esfuerzo con las reglas de §6.
//!
//! Todo se recolecta una vez ([`gather`]) para que `ez recommend` y
//! `ez delegate` vean el mismo uso, catálogo y presión de memoria. Las pruebas
//! fabrican [`Inputs`] a mano: nada de esto abre un agente.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_herdr::subagents::Registry;
use ez_scan::capacity::Thresholds;
use ez_scan::recommend::{self, Catalog, Pressure, Recommendation, Request, Size};
use ez_scan::usage::{self, UsageDirs, UsageReport};

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

/// Usa la recomendación cuando falta el tipo y completa modelo/esfuerzo.
///
/// Con `--agent` explícito, lo que no esté en `modelos.toml` se queda como
/// vino y se avisa: la persona manda.
pub(crate) fn fill(request: &LocalDelegate, inputs: &Inputs) -> Result<Filled, LocalError> {
    let automatic = request.agent.trim().is_empty();
    if !automatic && request.model.is_some() && request.effort.is_some() {
        return Ok(Filled {
            agent: request.agent.clone(),
            model: request.model.clone(),
            effort: request.effort.clone(),
            chosen: None,
            warnings: Vec::new(),
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
    match check() {
        Ok(()) => Ok(None),
        Err(text) if !text.starts_with("Recursos justos:") => Ok(Some(format!(
            "No pude medir los recursos: {text}. Sigo adelante."
        ))),
        Err(_) if force => Ok(Some(
            "--force: me salté la guarda de recursos baja.".to_string(),
        )),
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
