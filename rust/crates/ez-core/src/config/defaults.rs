//! Tablas de fábrica de `voice/config.py`: agentes y perfiles de lanzamiento.

use serde_json::{json, Map, Value};

use super::AGENT_PROFILES_VERSION;

/// `DEFAULT_AGENTS`: agente → argv de fábrica.
pub fn default_agents() -> Map<String, Value> {
    as_object(json!({
        "claude": ["claude"],
        "opencode": ["opencode"],
        "mcode": ["mcode"],
        "minimax": ["mcode"],
        "codex": ["codex"],
        "pi": ["pi"],
        "qwen": ["qwen"]
    }))
}

/// `DEFAULT_AGENT_PROFILES`: perfiles de lanzamiento de fábrica.
pub fn default_profiles() -> Map<String, Value> {
    as_object(json!({
        "claude": {
            "label": "Claude Code",
            "aliases": ["claude code", "cloud code", "clod code", "clode"],
            "models": ["fable", "opus", "sonnet", "haiku"],
            "efforts": ["low", "medium", "high", "xhigh", "max"],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": ["--effort", "{effort}"],
            "swarm_args": ["--allowedTools", "Bash(ezs *)"],
            "session_commands": {
                "model": [{"send": "/model {model}", "soft": true}],
                "effort": [{"send": "/effort {effort}", "soft": true}]
            },
            "notes": "Modelos por alias (fable, opus, sonnet, haiku). Admite plan mode y subagentes propios; bueno para planificar y auditar."
        },
        "opencode": {
            "label": "OpenCode",
            "aliases": ["open code", "open-code", "opencoder", "opencod"],
            "models": ["deepseek-v4.1-flash", "deepseek-v4-pro", "glm-5.3", "kimi-k3", "minimax-m3.1-flash", "claude-haiku-5-5"],
            "efforts": [],
            "model_efforts": {
                "deepseek-v4.1-flash": ["low", "high", "max"],
                "deepseek-v4-pro": ["high", "max"],
                "glm-5.3": ["low", "high", "max"],
                "kimi-k3": ["max"],
                "minimax-m3.1-flash": ["low", "medium", "high", "xhigh", "max"],
                "claude-haiku-5-5": ["none", "low", "medium", "high", "xhigh", "max"]
            },
            "prelaunch": "opencode_state",
            "default_model": "",
            "default_effort": "",
            "model_args": [],
            "effort_args": [],
            "model_prefix": "opencode-go/",
            "env": {"OPENCODE_CONFIG_CONTENT": "{\"model\":\"{model_ref}{variant}\"}"},
            "env_args": ["--standalone"],
            "session_commands": {
                "model": [
                    {"send": "/models", "expect": "Select model", "message": "no se abrió el selector de modelos"},
                    {"send": "{model_search}", "expect": "Switched model to", "message": "OpenCode no encontró el modelo {model}"}
                ],
                "effort": [
                    {"send": "/effort", "expect": "Select variant", "fail": "No variants available|does not support any variants", "message": "el modelo actual de OpenCode no tiene niveles de esfuerzo", "optional": true},
                    {"keys": ["esc"]},
                    {"cycle": {"keys": ["ctrl+t"], "until": "Switched variant to {effort}", "max": 8}, "message": "OpenCode no tiene el nivel {effort} para este modelo", "optional": true}
                ]
            },
            "notes": "Modelo y esfuerzo se fijan al lanzar (proveedor/modelo#nivel, p. ej. opencode-go/deepseek-v4.1-flash#high); el nivel debe existir en ese modelo. action=guide trae el manual."
        },
        "mcode": {
            "label": "MiniMax Code",
            "aliases": ["m code", "minimax code", "mini max code", "emcode"],
            "models": [],
            "efforts": ["default", "low", "medium", "high", "xhigh", "max"],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": [],
            "prelaunch": "mcode_effort",
            "discover": "mcode",
            "session_commands": {},
            "notes": "Modelo como proveedor/modelo (se resuelve contra su config); el esfuerzo se escribe en su config.yaml antes de abrir y queda como su valor por defecto. action=guide trae el manual."
        },
        "minimax": {
            "label": "MiniMax (mcode)",
            "aliases": ["mini max"],
            "models": [],
            "efforts": ["default", "low", "medium", "high", "xhigh", "max"],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": [],
            "prelaunch": "mcode_effort",
            "discover": "mcode",
            "session_commands": {},
            "notes": "Alias de mcode."
        },
        "codex": {
            "label": "Codex",
            "aliases": ["codex cli", "kodex", "codecs", "códex"],
            "models": [],
            "efforts": ["low", "medium", "high", "xhigh", "max", "ultra"],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": ["-c", "model_reasoning_effort={effort}"],
            "swarm_args": ["--yolo"],
            "discover": "codex",
            "session_commands": {},
            "notes": "Modelo y esfuerzo se fijan al lanzar; dentro de la sesión /model abre un selector que no se puede dictar."
        },
        "qwen": {
            "label": "Qwen Code",
            "aliases": ["qwen code", "quen code", "qwen coder", "cuen"],
            "models": ["qwen3-coder-plus"],
            "efforts": [],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": [],
            "session_commands": {},
            "notes": "El modelo se fija al lanzar con --model; no tiene flag de esfuerzo. action=guide trae el manual."
        },
        "pi": {
            "label": "Pi",
            "aliases": ["pi agent", "pi coding agent", "pie"],
            "models": [],
            "efforts": ["off", "minimal", "low", "medium", "high", "xhigh", "max"],
            "default_model": "",
            "default_effort": "",
            "model_args": ["--model", "{model}"],
            "effort_args": ["--thinking", "{effort}"],
            "session_commands": {},
            "notes": "Modelo como proveedor/id (admite sufijo :nivel de razonamiento). El esfuerzo es el nivel de thinking."
        }
    }))
}

/// Valores de fábrica del dataclass, en su orden de declaración.
///
/// Va como texto JSON para que el orden y los flotantes sean los del Python; lo
/// comprueba `tests/config_parity.rs`.
const DEFAULT_VALUES_JSON: &str = r##"{
  "hotkey": "ctrl+alt+space",
  "cancel_hotkey": "ctrl+alt+e",
  "repeat_hotkey": "ctrl+alt+r",
  "mute_hotkey": "ctrl+alt+m",
  "menu_hotkey": "ctrl+alt+o",
  "chat_hotkey": "ctrl+alt+t",
  "chat_voice": false,
  "whisper_url": "http://127.0.0.1:8088/inference",
  "auto_start_whisper": true,
  "stt_model": "ggml-large-v3-turbo-q8_0",
  "stt_engine": "auto",
  "whisper_home": "",
  "stt_prompt": "",
  "stt_dictionary": "",
  "stt_vocabulary_hints": true,
  "stt_beam_size": 5,
  "stt_timeout_secs": 10.0,
  "voice": "em_alex",
  "speed": 0.95,
  "tts_model_variant": "int8",
  "kokoro_model_path": "",
  "kokoro_voices_path": "",
  "tts_sentence_pause": 0.3,
  "tts_clause_pause": 0.12,
  "input_device_index": null,
  "output_device_index": null,
  "silence_secs": 1.3,
  "min_rms": 0.0006,
  "pre_roll_secs": 0.6,
  "min_speech_secs": 0.2,
  "max_utterance_secs": 30.0,
  "noise_multiplier": 1.3,
  "barge_in": true,
  "barge_in_rms": 0.008,
  "barge_in_secs": 0.25,
  "smart_turn": true,
  "turn_quick_secs": 0.35,
  "turn_threshold": 0.5,
  "vad_defaults_version": 2,
  "defaults_version": 2,
  "llm_provider": "opencode",
  "llm_profile_name": "",
  "llm_saved_profile": "",
  "llm_model": "deepseek-v4.1-flash",
  "llm_base_url": "https://opencode.ai/inference/go/openai/v1",
  "api_key": "",
  "llm_api_style": "",
  "llm_temperature": 0.3,
  "llm_max_tokens": 320,
  "llm_context_tokens": 32000,
  "llm_reasoning_effort": "none",
  "llm_timeout_secs": 30.0,
  "llm_turn_timeout_secs": 120.0,
  "llm_max_tool_rounds": 8,
  "llm_pinned": false,
  "confirm_actions": false,
  "hc_path": "hc",
  "herdr_app_path": "herdr-app",
  "default_cwd": "",
  "agents": {
    "claude": ["claude"],
    "opencode": ["opencode"],
    "mcode": ["mcode"],
    "minimax": ["mcode"],
    "codex": ["codex"],
    "pi": ["pi"],
    "qwen": ["qwen"]
  },
  "agent_profiles": {},
  "agent_profiles_version": 8,
  "announce_all": false,
  "max_read_lines": 40,
  "hud_hidden": false,
  "drop_x": null,
  "drop_y": null,
  "drop_layout": "",
  "drop_dock": "",
  "chat_width": 0.0,
  "chat_height": 0.0,
  "ui_layout": "pill",
  "ui_motion": "system",
  "ui_scale": 1.0,
  "show_advanced": false,
  "settings_x": null,
  "settings_y": null,
  "settings_w": null,
  "settings_h": null,
  "close_action": "pause",
  "voice_wake": true,
  "wake_phrase": "EzAgent, escúchame",
  "theme": "auto",
  "theme_base": "dark",
  "theme_primary": "#1FAE9F",
  "theme_secondary": "#8FE3C0",
  "theme_accent": "#FF8A65",
  "theme_background": "#0E6F7C"
}"##;

pub(super) fn default_values() -> Map<String, Value> {
    let mut values = match serde_json::from_str(DEFAULT_VALUES_JSON) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    // El perfil de fábrica no cabe en el texto: se comparte con `default_profiles`.
    values.insert(
        "agent_profiles".to_string(),
        Value::Object(default_profiles()),
    );
    values.insert(
        "agent_profiles_version".to_string(),
        json!(AGENT_PROFILES_VERSION),
    );
    values
}

fn as_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => {
            debug_assert!(false, "se esperaba un objeto: {other}");
            Map::new()
        }
    }
}
