"""Modelo y esfuerzo por agente delegado."""

from __future__ import annotations

import json

from ezagent.voice.config import VoiceConfig
from ezagent.voice.herdr import Herdr


def test_perfil_claude_modelo_y_esfuerzo():
    herdr = Herdr(VoiceConfig())
    assert herdr.agent_argv("claude") == ["claude"]
    assert herdr.agent_argv("claude", "sonnet", "low") == [
        "claude",
        "--model",
        "sonnet",
        "--effort",
        "low",
    ]


def test_perfil_codex_usa_config_para_effort():
    herdr = Herdr(VoiceConfig())
    assert herdr.agent_argv("codex", "gpt-5", "high") == [
        "codex",
        "--model",
        "gpt-5",
        "-c",
        "model_reasoning_effort=high",
    ]


def test_perfil_personalizado_en_voice_json(tmp_path):
    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps(
            {
                "agent_profiles": {
                    "claude": {
                        "command": ["claude", "--dangerously-skip-permissions"],
                        "model_args": ["--model", "{model}"],
                        "effort_args": ["--effort", "{effort}"],
                    }
                }
            }
        ),
        encoding="utf-8",
    )
    cfg = VoiceConfig.load(path)
    herdr = Herdr(cfg)
    assert herdr.agent_argv("claude", "opus", "high") == [
        "claude",
        "--dangerously-skip-permissions",
        "--model",
        "opus",
        "--effort",
        "high",
    ]
    # Los perfiles no editados siguen disponibles.
    # opencode no acepta --model: el modelo va por OPENCODE_CONFIG_CONTENT.
    launch = herdr.agent_launch("opencode", "deepseek", None)
    assert launch.argv == ["opencode", "--standalone"]
    assert (
        launch.env["OPENCODE_CONFIG_CONTENT"]
        == '{"model":"opencode-go/deepseek"}'
    )
    assert "XDG_STATE_HOME" in launch.env
    assert launch.pending == {}


def test_run_agent_escribe_el_comando_con_modelo_y_effort():
    herdr = Herdr(VoiceConfig())
    sent: list[str] = []
    herdr._json_app = lambda args, timeout=20: {"terminal": "t1"}  # type: ignore[method-assign]
    herdr.get_agent = lambda a: {"agent": "claude-code", "state": "idle"}  # type: ignore[method-assign]
    herdr.send_agent = lambda a, t, confirmed=False: (  # type: ignore[method-assign]
        sent.append(t) or {"ok": True}
    )
    result = herdr.run_agent(
        "claude", "haz el plan", model="sonnet", effort="low"
    )
    assert result["ok"] is True
    assert result["model"] == "sonnet"
    assert result["effort"] == "low"
    assert sent == ["claude --model sonnet --effort low", "haz el plan"]
