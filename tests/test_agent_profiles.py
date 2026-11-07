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
    assert herdr.agent_argv("opencode", "deepseek", None) == [
        "opencode",
        "--model",
        "deepseek",
    ]


def test_run_agent_pasa_modelo_y_effort_al_comando_visible():
    cfg = VoiceConfig()
    herdr = Herdr(cfg)
    captured: list[list[str]] = []

    def fake_app(args, timeout=20):
        captured.append(args)
        return {"terminal": "t1", "tab": "x", "space": "s"}

    herdr._json_app = fake_app  # type: ignore[method-assign]
    result = herdr.run_agent(
        "claude", "haz el plan", model="sonnet", effort="low"
    )
    assert result["ok"] is True
    assert result["model"] == "sonnet"
    assert result["effort"] == "low"
    argv_json = captured[0][captured[0].index("--argv") + 1]
    argv = json.loads(argv_json)
    assert argv == ["claude", "--model", "sonnet", "--effort", "low"]
