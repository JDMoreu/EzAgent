//! Programa de terminal guionizado que imprime una pantalla de agente (F1-11).
//!
//! Las pantallas imitan a los TUI reales de OpenCode, Claude Code, Codex y
//! mcode (ver `tests/fixtures/screens`): varias líneas, sin el prompt de
//! PowerShell al final, para que `AgentSession::wait_ready` acepte el arranque.
//! El escenario decide qué se ve: trabajando, terminado, bloqueado pidiendo
//! permiso, límite de uso o contexto lleno.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Escenas que entiende el guion.
pub const SCENES: [&str; 5] = ["working", "done", "blocked", "usage_limit", "context_full"];

/// Conclusión por defecto de una escena terminada.
pub const DEFAULT_CONCLUSION: &str = "Listo. La tarea terminó correctamente.";

fn default_agent() -> String {
    "opencode".to_string()
}

fn default_script() -> Vec<String> {
    vec!["working".to_string(), "done".to_string()]
}

/// Escenario de `fake-agent`: agente, textos y secuencia de escenas.
#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    #[serde(default = "default_agent")]
    pub agent: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub conclusion: String,
    #[serde(default = "default_script")]
    pub script: Vec<String>,
    #[serde(default)]
    pub delay_ms: u64,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            task: String::new(),
            conclusion: String::new(),
            script: vec!["working".to_string(), "done".to_string()],
            delay_ms: 300,
        }
    }
}

impl Scenario {
    /// Lee el escenario JSON; los campos ausentes toman el valor por defecto.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("no pude leer el escenario {}", path.display()))?;
        Self::from_json(&text)
    }

    /// Escenario desde texto JSON (pruebas y `--scenario`).
    pub fn from_json(text: &str) -> Result<Self> {
        let mut scenario: Scenario =
            serde_json::from_str(text).context("el escenario de fake-agent no es JSON válido")?;
        if scenario.script.is_empty() {
            scenario.script = vec!["done".to_string()];
        }
        if scenario.agent.trim().is_empty() {
            scenario.agent = default_agent();
        }
        Ok(scenario)
    }

    /// Pantallas que hay que imprimir, en orden.
    pub fn frames(&self) -> Vec<String> {
        self.script
            .iter()
            .map(|scene| screen(&self.agent, scene, &self.task, &self.conclusion))
            .collect()
    }
}

fn agent_header(agent: &str, task: &str) -> Vec<String> {
    let key = agent.trim().to_lowercase();
    let key = key.strip_suffix(".exe").unwrap_or(&key);
    let task_line = if task.trim().is_empty() {
        "Ask the agent to do anything".to_string()
    } else {
        task.to_string()
    };
    match key {
        "claude" | "claude-code" => vec![
            "▐▛███▛█ Claude Code v2.1.319".to_string(),
            "▝▜██████▀ Sonnet 5.5 with low effort · Claude Pro".to_string(),
            "▝▝ ▝▝ ~\\Documents\\EzAgent".to_string(),
            format!("❯ {task_line}"),
        ],
        "codex" => vec![
            ">_ OpenAI Codex (v0.167.0)".to_string(),
            "~\\Documents\\EzAgent".to_string(),
            format!("› {task_line}"),
        ],
        "mcode" | "minimax" => vec![
            "███ ███ ██ ███ ██ ██ ███ ███ █████ ██ ██ ██████ ██████ ██████ ███████".to_string(),
            "v0.7.6".to_string(),
            format!("› {task_line}"),
        ],
        _ => vec!["• Sesión de OpenCode".to_string(), task_line],
    }
}

fn agent_footer(agent: &str) -> Vec<String> {
    let key = agent.trim().to_lowercase();
    let key = key.strip_suffix(".exe").unwrap_or(&key);
    match key {
        "claude" | "claude-code" => vec![
            "✻ Worked for 2s · done 1:19 p.m.".to_string(),
            "❯".to_string(),
            "⏵⏵ auto mode on (shift+tab to cycle)".to_string(),
        ],
        "codex" => vec![
            "Worked for 2s • 1:19 PM".to_string(),
            "› Ask Codex to do anything".to_string(),
            "← for agents · ? for shortcuts ⚠ 1 warning · f2 to view".to_string(),
        ],
        "mcode" | "minimax" => vec![
            "Completed in 1s · ⚡ 367 tok/s".to_string(),
            "Message · Enter send · Shift+Enter newline".to_string(),
            "EzAgent ◇ ⎇ main Full access ✦ M3.1-Flash · Effort low".to_string(),
        ],
        _ => vec![
            "Build · DeepSeek V4.1 Flash · 2.4s · 4.8 tok/s".to_string(),
            "Build · DeepSeek V4.1 Flash OpenCode Go · low".to_string(),
            "╹▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀".to_string(),
            "~/Documents/EzAgent:main 27.6K (3%) ctrl+p commands".to_string(),
        ],
    }
}

fn scene_body(agent: &str, scene: &str, conclusion: &str) -> Vec<String> {
    let conclusion = if conclusion.trim().is_empty() {
        DEFAULT_CONCLUSION
    } else {
        conclusion.trim()
    };
    match scene {
        "working" => vec!["+ Thought · 0.8s".to_string(), "Trabajando…".to_string()],
        "done" => vec![format!("● {conclusion}")],
        "blocked" => vec![
            "Do you trust the files in this folder?".to_string(),
            "❯ 1. Yes, I trust".to_string(),
            "  2. No, exit".to_string(),
            "Enter to confirm · Esc to cancel".to_string(),
        ],
        "usage_limit" => vec![
            "Usage limit reached. Your limit will reset at 5:00 PM.".to_string(),
            "Try again later or switch to another provider.".to_string(),
        ],
        "context_full" => vec![
            "Context low · 1% remaining".to_string(),
            "Compact the conversation or start a new session.".to_string(),
        ],
        other => vec![
            format!("● Escena «{other}» sin guion."),
            format!("● {conclusion}"),
        ],
    }
    .into_iter()
    .map(|line| match agent.trim().to_lowercase().as_str() {
        "claude" | "claude-code" if scene == "working" => line.replace("+ Thought", "✻ Thought"),
        _ => line,
    })
    .collect()
}

/// Pantalla completa de una escena para un agente.
///
/// Una escena desconocida no rompe el programa: se muestra con la conclusión.
pub fn screen(agent: &str, scene: &str, task: &str, conclusion: &str) -> String {
    let mut lines = agent_header(agent, task);
    lines.extend(scene_body(agent, scene, conclusion));
    lines.extend(agent_footer(agent));
    lines.join("\n")
}

/// Pantalla inicial cuando el terminal lanza un agente con una tarea.
pub fn launch_screen(agent: &str, task: &str) -> String {
    screen(agent, "working", task, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_has_a_tui_screen() {
        for scene in SCENES {
            let text = screen("opencode", scene, "Nombra tres colores", "");
            let lines: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            assert!(lines.len() >= 4, "escena {scene} demasiado corta: {text}");
            assert!(
                !lines.last().expect("última línea").starts_with("PS "),
                "la pantalla de {scene} parece un prompt de shell"
            );
        }
    }

    #[test]
    fn done_and_blocked_scenes_show_their_markers() {
        let done = screen("claude", "done", "Resume el plan", "Terminé el plan.");
        assert!(done.contains("Terminé el plan."));
        assert!(done.contains("Claude Code"));
        let blocked = screen("codex", "blocked", "Trabaja", "");
        assert!(blocked.contains("Do you trust"));
        assert!(blocked.contains("Yes, I trust"));
        assert!(blocked.contains("No, exit"));
    }

    #[test]
    fn limits_and_context_scenes_are_visible() {
        assert!(screen("opencode", "usage_limit", "", "").contains("Usage limit"));
        assert!(screen("opencode", "context_full", "", "").contains("Context low"));
    }

    #[test]
    fn scenario_defaults_and_unknown_scene() {
        let scenario = Scenario::from_json(r#"{"task": "hola"}"#).expect("escenario");
        assert_eq!(scenario.agent, "opencode");
        assert_eq!(scenario.script, vec!["working", "done"]);
        assert!(scenario.frames()[0].contains("hola"));

        let weird = screen("opencode", "sin-guion", "", "");
        assert!(weird.contains("sin guion"));
        assert!(weird.contains(DEFAULT_CONCLUSION));
    }

    #[test]
    fn empty_script_falls_back_to_done() {
        let scenario = Scenario::from_json(r#"{"script": []}"#).expect("escenario");
        assert_eq!(scenario.script, vec!["done"]);
    }
}
