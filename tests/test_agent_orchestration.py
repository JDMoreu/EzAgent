"""Orquestación: abrir agentes como una persona, alias, pipelines, sesión."""

from __future__ import annotations

import json
from types import SimpleNamespace
from typing import Any

import pytest

from ezagent.agent.config import AgentConfig
from ezagent.agent.orchestration import (
    ORCHESTRATION_HINT,
    chain_spec,
    parse_steps,
)
from ezagent.agent.system_prompt import SystemPromptBuilder
from ezagent.agent.tools_herdr import HerdrCallbacks, build_herdr_tools
from ezagent.voice.agent_driver import command_line
from ezagent.voice.config import VoiceConfig
from ezagent.voice.herdr import Herdr


class Rig:
    """Herdr con todo lo externo simulado: nada toca terminales reales.

    Simula la pantalla de OpenCode: /models abre «Select model», escribir el
    nombre avisa «Switched model to», /effort abre «Select variant» (o dice
    «No variants available») y Ctrl+T cicla las variantes.
    """

    def __init__(
        self,
        running: str = "opencode",
        variants: tuple[str, ...] = ("default", "low", "high", "max"),
        finds_model: bool = True,
    ) -> None:
        self.herdr = Herdr(VoiceConfig())
        self.app_calls: list[list[str]] = []
        self.sent: list[tuple[str, str]] = []
        self.keys: list[list[str]] = []
        self.running = running
        self.variants = list(variants)
        self.finds_model = finds_model
        self.index = 0
        self.screen = ""
        self.herdr._json_app = self._app  # type: ignore[method-assign]
        self.herdr.get_agent = self._get  # type: ignore[method-assign]
        self.herdr.send_agent = self._send  # type: ignore[method-assign]
        self.herdr.keys_agent = self._keys  # type: ignore[method-assign]
        self.herdr.read_agent = self._read  # type: ignore[method-assign]

    def _app(self, args: list[str], timeout: float = 20) -> dict[str, Any]:
        self.app_calls.append(args)
        return {"terminal": "t9", "tab": "x", "space": "s"}

    def _get(self, address: str) -> dict[str, Any]:
        return {"address": address, "agent": self.running, "state": "idle"}

    def _send(self, address: str, text: str, confirmed: bool = False):
        self.sent.append((address, text))
        if text == "/models":
            self.screen = "Select model\nSearch"
        elif self.screen.startswith("Select model"):
            self.screen = (
                f"Switched model to {text}"
                if self.finds_model
                else "Select model\nNo results"
            )
        elif text == "/effort":
            self.screen = (
                "Select variant"
                if self.variants
                else "No variants available\nThe current model does not "
                "support any variants."
            )
        return {"ok": True}

    def _keys(self, address: str, keys: list[str]):
        self.keys.append(list(keys))
        if keys == ["esc"]:
            self.screen = ""
        elif keys == ["ctrl+t"] and self.variants:
            self.index = (self.index + 1) % len(self.variants)
            self.screen = f"Switched variant to {self.variants[self.index]}"
        return {"ok": True}

    def _read(self, address: str, lines: int | None = None) -> str:
        return self.screen

    @property
    def texts(self) -> list[str]:
        return [text for _, text in self.sent]


def test_abrir_agente_abre_shell_y_escribe_el_comando():
    rig = Rig()
    result = rig.herdr.run_agent("opencode")
    assert result["ok"] is True
    args = rig.app_calls[0]
    assert json.loads(args[args.index("--argv") + 1]) == ["powershell.exe"]
    assert "--task" not in args
    assert rig.texts == ["opencode"]
    assert result["task_sent"] is False


def test_la_tarea_se_pega_despues_de_que_el_agente_arranca():
    rig = Rig()
    result = rig.herdr.run_agent("opencode", "hola")
    assert rig.texts == ["opencode", "hola"]
    assert result["task_sent"] is True
    assert "--task" not in rig.app_calls[0]


def test_no_envia_nada_si_el_agente_no_arranca():
    # Antes «hola» acababa tecleado en PowerShell cuando el agente fallaba.
    rig = Rig(running="powershell.exe")
    result = rig.herdr.run_agent("opencode", "hola")
    assert result["ok"] is False
    assert result["address"] == "t9"
    assert rig.texts == ["opencode"]


def test_opencode_modelo_y_esfuerzo_van_en_el_arranque_sin_menus(tmp_path):
    rig = Rig()
    result = rig.herdr.run_agent(
        "Open Code", "haz X", model="deepseek-v4.1-flash", effort="max"
    )
    assert result["ok"] is True
    line = rig.texts[0]
    assert line.startswith("$env:OPENCODE_CONFIG_CONTENT=")
    assert "opencode-go/deepseek-v4.1-flash#max" in line
    assert "$env:XDG_STATE_HOME=" in line
    assert line.endswith("opencode --standalone")
    assert rig.texts[1:] == ["haz X"]
    assert rig.keys == []  # ni /models ni /effort ni Ctrl+T
    assert result["task_sent"] is True


def test_opencode_escribe_la_variante_en_su_estado_propio(tmp_path):
    # Si OpenCode guardó «default» para el modelo, ganaba a la configuración:
    # por eso EzAgent le da un estado propio con la variante ya escrita.
    rig = Rig()
    rig.herdr.run_agent("opencode", "", model="deepseek-v4.1-flash", effort="low")
    state = tmp_path / "localappdata" / "EzAgent" / "opencode-state"
    saved = json.loads((state / "opencode" / "model.json").read_text("utf-8"))
    assert saved["variant"] == {"opencode-go/deepseek-v4.1-flash": "low"}
    assert saved["recent"] == [
        {"providerID": "opencode-go", "modelID": "deepseek-v4.1-flash"}
    ]
    # Sin esfuerzo no queda ninguna variante forzada.
    rig.herdr.run_agent("opencode", "", model="deepseek-v4.1-flash")
    saved = json.loads((state / "opencode" / "model.json").read_text("utf-8"))
    assert saved["variant"] == {}


def test_esfuerzo_que_el_modelo_no_tiene_se_rechaza_con_las_opciones():
    rig = Rig()
    result = rig.herdr.run_agent(
        "opencode", "x", model="deepseek-v4.1-flash", effort="medium"
    )
    assert result["ok"] is False
    assert "low, high, max" in result["error"]
    assert rig.sent == []  # no se abrió nada con un valor que se ignoraría


def test_opencode_solo_esfuerzo_usa_el_ciclo_de_variantes():
    rig = Rig()
    result = rig.herdr.run_agent("opencode", "haz X", effort="max")
    assert result["ok"] is True
    assert rig.texts == ["opencode", "/effort", "haz X"]
    assert rig.keys == [["esc"], ["ctrl+t"], ["ctrl+t"], ["ctrl+t"]]


def test_opencode_sin_variantes_avisa_y_no_teclea_el_nivel():
    # El caso que fallaba: /effort no abre nada y «max» acababa en el prompt.
    rig = Rig(variants=())
    result = rig.herdr.run_agent("opencode", "haz X", effort="max")
    assert result["ok"] is True
    assert any("esfuerzo" in w for w in result["warnings"])
    assert "max" not in rig.texts
    assert rig.texts == ["opencode", "/effort", "haz X"]


def test_opencode_nivel_inexistente_avisa_y_sigue():
    rig = Rig(variants=("default", "low"))
    result = rig.herdr.run_agent("opencode", "haz X", effort="max")
    assert result["ok"] is True
    assert any("max" in w for w in result["warnings"])
    assert rig.texts[-1] == "haz X"


def test_cambiar_modelo_en_caliente_no_encontrado_aborta():
    rig = Rig(finds_model=False)
    result = rig.herdr.configure_agent("t3", "zzz-modelo")
    assert result["ok"] is False
    assert "modelo" in result["error"]
    assert ["esc"] in rig.keys


MCODE_YAML = (
    "logLevel: info\n"
    "provider:\n"
    "  minimax:\n"
    "    models:\n"
    "      MiniMax-M3:\n"
    "        capabilities: &ref_0\n"
    "          a: 1\n"
    "      MiniMax-M3.1-Flash:\n"
    "        capabilities: *ref_0\n"
    "defaultModel: minimax/MiniMax-M3.1-Flash\n"
    "defaultModelThinking:\n"
    "  effort: default\n"
    "defaultModelVariant: thinking\n"
)


def _mcode_config(tmp_path, text: str = MCODE_YAML):
    path = tmp_path / "minimax" / "config.yaml"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="")
    return path


def test_mcode_fija_el_esfuerzo_en_su_config_sin_tocar_lo_demas(tmp_path):
    path = _mcode_config(tmp_path)
    launch = Rig().herdr.agent_launch("mcode", "minimax-m3.1-flash", "low")
    # Mayúsculas resueltas contra el config y esfuerzo fuera del argv.
    assert launch.argv == ["mcode", "--model", "minimax/MiniMax-M3.1-Flash"]
    assert launch.warnings == []
    assert path.read_text("utf-8") == MCODE_YAML.replace(
        "effort: default", "effort: low"
    )
    # Copia de seguridad la primera vez, con el contenido original.
    backup = path.with_name("config.yaml.ezagent-bak")
    assert backup.read_text("utf-8") == MCODE_YAML
    Rig().herdr.agent_launch("mcode", None, "max")
    assert backup.read_text("utf-8") == MCODE_YAML
    assert "effort: max" in path.read_text("utf-8")


def test_mcode_modelo_o_esfuerzo_invalidos_no_abren_nada(tmp_path):
    _mcode_config(tmp_path)
    herdr = Rig().herdr
    assert "MiniMax-M3" in herdr.agent_launch("mcode", "zzz").error
    assert "Valores válidos" in herdr.agent_launch("mcode", None, "turbo").error
    rig = Rig()
    assert rig.herdr.run_agent("mcode", "x", effort="turbo")["ok"] is False
    assert rig.sent == []


def test_mcode_agrega_el_ajuste_si_no_existe(tmp_path):
    path = _mcode_config(tmp_path, "logLevel: info\r\n")
    Rig().herdr.agent_launch("mcode", None, "high")
    assert path.read_bytes() == (
        b"logLevel: info\r\ndefaultModelThinking:\r\n  effort: high\r\n"
    )


def test_mcode_sin_config_avisa_que_no_pudo_fijar_el_esfuerzo(tmp_path):
    launch = Rig().herdr.agent_launch("mcode", None, "low")
    assert any("esfuerzo" in w for w in launch.warnings)


def test_pi_se_detecta_por_pantalla_aunque_herdr_no_lo_reconozca():
    rig = Rig(running="powershell.exe")
    rig.screen = "pi v1\nPackage updates\n~\\proj (main)\nsonnet • low"
    result = rig.herdr.run_agent("pi", "hola", model="sonnet", effort="low")
    assert result["ok"] is True
    assert rig.texts == ["pi --model sonnet --thinking low", "hola"]


def test_shell_con_prompt_al_final_no_cuenta_como_agente_arrancado():
    rig = Rig(running="powershell.exe")
    rig.screen = "Windows PowerShell\nCopyright\n\nPS C:\\x> pi --model sonnet"
    result = rig.herdr.run_agent("pi", "hola")
    assert result["ok"] is False
    assert rig.texts == ["pi"]


def test_qwen_usa_model_y_avisa_si_piden_esfuerzo():
    herdr = Rig().herdr
    launch = herdr.agent_launch("Qwen Code", "qwen3-coder-plus", "high")
    assert launch.argv == ["qwen", "--model", "qwen3-coder-plus"]
    assert any("esfuerzo" in w for w in launch.warnings)


def test_command_line_antepone_variables_de_entorno():
    line = command_line(["opencode", "--standalone"], {"A": "it's"})
    assert line == "$env:A='it''s'; opencode --standalone"


def test_voice_json_viejo_recibe_los_agentes_nuevos(tmp_path):
    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps({"agents": {"claude": ["claude"]}}), encoding="utf-8"
    )
    cfg = VoiceConfig.load(path)
    assert {"claude", "pi", "qwen", "opencode", "codex"} <= set(cfg.agents)


def test_claude_usa_flags_en_la_linea_de_comando():
    rig = Rig(running="claude-code")
    rig.herdr.run_agent("Cloud Code", "plan", model="sonnet", effort="low")
    assert rig.texts == ["claude --model sonnet --effort low", "plan"]


def test_agente_sin_forma_de_fijar_algo_avisa_pero_abre():
    rig = Rig(running="codex")
    rig.herdr.cfg.agent_profiles["codex"]["effort_args"] = []
    result = rig.herdr.run_agent("codex", effort="high")
    assert result["ok"] is True
    assert any("esfuerzo" in w for w in result["warnings"])
    assert rig.texts == ["codex"]


def test_modo_segundo_plano_no_abre_pestana_y_pega_la_tarea():
    rig = Rig()
    rig.herdr._json = lambda args, timeout=20: {"address": "t5"}  # type: ignore[method-assign]
    result = rig.herdr.run_agent("opencode", "busca el dólar", background=True)
    assert result["ok"] is True
    assert result["background"] is True
    assert rig.app_calls == []
    assert rig.texts == ["busca el dólar"]


def test_command_line_cita_lo_que_no_es_simple():
    assert command_line(["claude", "--model", "opus"]) == "claude --model opus"
    assert command_line(["x", "a b", "it's"]) == "x 'a b' 'it''s'"


def test_nombres_dictados_se_resuelven():
    herdr = Rig().herdr
    assert herdr.resolve_agent("Open Code") == "opencode"
    assert herdr.resolve_agent("Cloud Code") == "claude"
    assert herdr.resolve_agent("M code") == "mcode"
    assert herdr.resolve_agent("Pi") == "pi"
    assert herdr.resolve_agent("nada") is None


def test_pi_y_claude_mapean_esfuerzo():
    herdr = Rig().herdr
    assert herdr.agent_argv("pi", "anthropic/x", "high") == [
        "pi",
        "--model",
        "anthropic/x",
        "--thinking",
        "high",
    ]
    assert herdr.agent_argv("claude", "opus", "max")[-2:] == ["--effort", "max"]


def test_valores_peligrosos_o_invalidos_se_rechazan():
    herdr = Rig().herdr
    assert herdr.agent_launch("claude", "--dangerous").error
    assert herdr.agent_launch("claude", "a b").error
    assert "Valores válidos" in herdr.agent_launch("claude", None, "turbo").error
    rig = Rig()
    assert rig.herdr.run_agent("claude", "x", model="a; rm")["ok"] is False
    assert rig.sent == []


def test_agente_desconocido_lista_disponibles():
    result = Rig().herdr.run_agent("zzz", "x")
    assert result["ok"] is False
    assert "pi" in result["available"]


def test_configure_escribe_los_comandos_de_sesion_del_perfil():
    rig = Rig(running="claude-code")
    result = rig.herdr.configure_agent("t3", "opus", "high")
    assert result["ok"] is True
    assert rig.texts == ["/model opus", "/effort high"]


def test_configure_opencode_usa_models_y_ciclo():
    rig = Rig()
    result = rig.herdr.configure_agent("t3", "glm-5.3", "high")
    assert result["ok"] is True
    assert rig.texts == ["/models", "glm 5.3", "/effort"]
    assert rig.keys == [["esc"], ["ctrl+t"], ["ctrl+t"]]


def test_configure_en_agente_sin_comandos_explica():
    rig = Rig(running="codex")
    result = rig.herdr.configure_agent("t3", "gpt-5")
    assert result["ok"] is False
    assert "run" in result["error"]
    assert rig.sent == []


def test_catalogo_texto_incluye_agentes_y_alias():
    text = Rig().herdr.catalog_text()
    for name in ("claude", "opencode", "codex", "mcode", "pi", "open code"):
        assert name in text


def test_prompt_incluye_orquestacion_solo_con_catalogo(tmp_path):
    cfg = AgentConfig.load(tmp_path / "permissions.toml")
    cfg.system_md_path = tmp_path / "system.md"
    plain = SystemPromptBuilder(cfg).build()
    assert ORCHESTRATION_HINT not in plain
    full = SystemPromptBuilder(
        cfg, agents_catalog=lambda: "# Agentes\n- pi"
    ).build()
    assert ORCHESTRATION_HINT in full
    assert "- pi" in full


def test_parse_steps_valida():
    assert parse_steps(None) == []
    steps = parse_steps([{"agent": "a", "task": "t", "model": "m", "x": 1}])
    assert steps == [{"agent": "a", "task": "t", "model": "m"}]
    for bad in ("x", [1], [{"task": "t"}], [{"agent": "a", "task": "t"}] * 6):
        with pytest.raises(ValueError):
            parse_steps(bad)


def test_chain_spec_anida_y_avisa_solo_al_final():
    steps = parse_steps(
        [
            {"agent": "opencode", "task": "implementa", "model": "glm-5.3"},
            {
                "agent": "claude",
                "task": "audita",
                "model": "opus",
                "effort": "high",
            },
        ]
    )
    first = chain_spec("t1", steps, cwd="C:/p", space=None, notify=True)
    assert first["notify_on_done"] is False
    assert first["notify_final"] is True
    assert first["then"][0]["agent"] == "claude"
    last = chain_spec("t2", first["then"], cwd="C:/p", space=None, notify=True)
    assert last["notify_on_done"] is True
    assert "then" not in last


class _Cb:
    def __init__(self) -> None:
        self.chains: list[dict[str, Any]] = []
        self.watched: list[str] = []

    def callbacks(self) -> HerdrCallbacks:
        return HerdrCallbacks(
            watch=self.watched.append,
            request_close=lambda: None,
            mic_mute=lambda m: None,
            last_summary=lambda: "",
            mute=lambda m: None,
            get_last_user_text=lambda: "",
            enqueue_chain=self.chains.append,
        )


def test_run_con_then_encola_la_cadena_y_no_vigila_el_primer_paso():
    rig = Rig(running="claude-code")
    cb = _Cb()
    tools = {t.name: t for t in build_herdr_tools(rig.herdr, cb.callbacks())}
    result = tools["delegate_to_agent"].handler(
        {
            "action": "run",
            "agent": "Cloud Code",
            "task": "planifica",
            "notify_on_done": True,
            "then": [{"agent": "open code", "task": "implementa"}],
        }
    )
    assert result.ok, result.error
    assert len(cb.chains) == 1
    assert cb.chains[0]["source"] == "t9"
    assert cb.chains[0]["notify_on_done"] is True
    assert cb.watched == []


def test_run_sin_tarea_y_agente_inexistente():
    rig = Rig()
    tools = {
        t.name: t for t in build_herdr_tools(rig.herdr, _Cb().callbacks())
    }
    ok = tools["delegate_to_agent"].handler(
        {"action": "run", "agent": "opencode"}
    )
    assert ok.ok, ok.error
    bad = tools["delegate_to_agent"].handler({"action": "run", "agent": "zzz"})
    assert bad.ok is False
    assert "agents" in (bad.error or "")


def test_migracion_elimina_campos_obsoletos_del_perfil_de_opencode(tmp_path):
    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps(
            {
                "agent_profiles_version": 2,
                "agent_profiles": {
                    "opencode": {
                        "models": ["mi-modelo"],
                        "model_command": ["opencode", "mini"],
                        "task_args": ["--prompt", "{task}"],
                        "effort_args": ["--effort", "{effort}"],
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    profile = VoiceConfig.load(path).agent_profiles["opencode"]
    assert profile["effort_args"] == []
    assert "model_command" not in profile
    assert "task_args" not in profile
    assert "deepseek-v4.1-flash" in profile["models"]
    assert profile["model_efforts"]["kimi-k3"] == ["max"]
    assert profile["session_commands"]["model"][0]["send"] == "/models"


def test_las_pruebas_no_pueden_ejecutar_comandos_reales():
    with pytest.raises(AssertionError):
        Herdr(VoiceConfig())._run(["send", "t1", "hola"])


def test_keys_valida_y_solo_pulsa_teclas_nombradas():
    from ezagent.voice.agent_driver import valid_keys

    assert valid_keys(["ctrl+t", "esc", "a"]) == ["ctrl+t", "esc", "a"]
    assert valid_keys(["rm -rf"]) is None
    assert valid_keys(["enter; ls"]) is None
    assert valid_keys([]) is None
    assert valid_keys(["a"] * 13) is None


def test_herramienta_keys_y_guide():
    rig = Rig()
    tools = {
        t.name: t for t in build_herdr_tools(rig.herdr, _Cb().callbacks())
    }
    tool = tools["delegate_to_agent"]
    ok = tool.handler({"action": "keys", "address": "t9", "keys": ["esc"]})
    assert ok.ok, ok.error
    assert rig.keys == [["esc"]]
    assert tool.handler({"action": "keys", "address": "t9"}).ok is False
    guide = tool.handler({"action": "guide", "agent": "Open Code"})
    assert guide.ok, guide.error
    assert "Select variant" in guide.data["guide"]
    assert tool.handler({"action": "guide", "agent": "zzz"}).ok is False


def test_cada_agente_por_defecto_tiene_manual():
    from ezagent.agent.orchestration import load_guide

    for key in VoiceConfig().agents:
        if key == "minimax":  # alias de mcode
            continue
        assert load_guide(key), key
    assert load_guide("../x") is None


def _codex_cache(tmp_path):
    levels = lambda *names: [{"effort": n, "description": ""} for n in names]  # noqa: E731
    models = [
        {
            "slug": "gpt-6-luna",
            "visibility": "list",
            "supported_reasoning_levels": levels("low", "medium", "high", "xhigh", "max"),
        },
        {
            "slug": "gpt-5.6-terra",
            "visibility": "list",
            "supported_reasoning_levels": levels(
                "low", "medium", "high", "xhigh", "max", "ultra"
            ),
        },
        {
            "slug": "oculto",
            "visibility": "hide",
            "supported_reasoning_levels": levels("low"),
        },
    ]
    path = tmp_path / "codex" / "models_cache.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"models": models}), encoding="utf-8")


def test_codex_acepta_todos_los_niveles_de_su_catalogo(tmp_path):
    _codex_cache(tmp_path)
    herdr = Rig().herdr
    for level in ("low", "medium", "high", "xhigh", "max"):
        launch = herdr.agent_launch("codex", None, level)
        assert launch.argv == [
            "codex",
            "-c",
            f"model_reasoning_effort={level}",
        ]
        assert launch.error == ""


def test_codex_valida_nivel_y_modelo_contra_el_catalogo(tmp_path):
    _codex_cache(tmp_path)
    herdr = Rig().herdr
    # ultra solo existe en gpt-5.6-terra.
    assert "ultra" not in herdr.agent_launch("codex", "GPT-6-Luna", "max").error
    error = herdr.agent_launch("codex", "gpt-6-luna", "ultra").error
    assert "low, medium, high, xhigh, max" in error
    ok = herdr.agent_launch("codex", "gpt-5.6-terra", "ultra")
    assert ok.error == ""
    assert ok.argv[:3] == ["codex", "--model", "gpt-5.6-terra"]
    assert "gpt-6-luna" in herdr.agent_launch("codex", "oculto").error
    entry = next(c for c in herdr.agents_catalog() if c["agent"] == "codex")
    assert entry["models"] == ["gpt-6-luna", "gpt-5.6-terra"]


def test_codex_sin_catalogo_no_bloquea_el_lanzamiento():
    launch = Rig().herdr.agent_launch("codex", None, "xhigh")
    assert launch.error == ""


def test_opencode_solo_esfuerzo_usa_el_modelo_por_defecto_sin_ciclar(tmp_path):
    # El caso del log: un paso con effort=low pero sin model hacía ciclar
    # variantes dentro de OpenCode (low -> max -> high...).
    config = tmp_path / "opencode-config"
    config.mkdir()
    (config / "opencode.jsonc").write_text(
        '{\n  // modelo por defecto\n  "model": {"providerID": "opencode-go",'
        ' "model": "deepseek-v4.1-flash"},\n  "x": "https://a.b/c", /* c */\n}\n',
        encoding="utf-8",
    )
    rig = Rig()
    result = rig.herdr.run_agent("opencode", "haz X", effort="low")
    assert result["ok"] is True
    assert "opencode-go/deepseek-v4.1-flash#low" in rig.texts[0]
    assert rig.keys == []
    assert rig.texts[1:] == ["haz X"]


def test_la_shell_nueva_no_usa_pausa_fija(tmp_path):
    rig = Rig()
    rig.screen = "Windows PowerShell\nCopyright\n\nPS C:\\proyecto>"
    assert rig.herdr.wait_shell_prompt("t9") is True


def test_run_asincrono_responde_al_instante_y_termina_aparte():
    import threading

    rig = Rig()
    done = threading.Event()
    results: list[dict] = []

    def on_ready(result: dict) -> None:
        results.append(result)
        done.set()

    first = rig.herdr.run_agent("opencode", "haz X", on_ready=on_ready)
    assert first["ok"] is True
    assert first["launching"] is True
    assert first["address"] == "t9"
    assert done.wait(5)
    assert results[0]["ok"] is True
    assert results[0]["task_sent"] is True
    assert rig.texts[-1] == "haz X"


def test_run_asincrono_avisa_si_el_agente_no_arranca():
    import threading

    rig = Rig(running="powershell.exe")
    done = threading.Event()
    results: list[dict] = []
    rig.herdr.run_agent(
        "opencode", "haz X", on_ready=lambda r: (results.append(r), done.set())
    )
    assert done.wait(5)
    assert results[0]["ok"] is False
    assert "haz X" not in rig.texts


class _AsyncFake:
    """Herdr falso cuyo run_agent llama a on_ready cuando el test lo decide."""

    def __init__(self) -> None:
        self.calls: list[tuple] = []
        self.on_ready = None

    def resolve_agent(self, name: str):
        return name.lower().replace(" ", "")

    def run_agent(self, agent, task="", **kwargs):
        self.calls.append((agent, task, kwargs))
        self.on_ready = kwargs.get("on_ready")
        return {"ok": True, "launching": True, "address": "t7", "terminal": "t7"}


def _async_tools(fake):
    events: list[tuple] = []
    callbacks = HerdrCallbacks(
        watch=lambda a: events.append(("watch", a)),
        request_close=lambda: None,
        mic_mute=lambda m: None,
        last_summary=lambda: "",
        mute=lambda m: None,
        get_last_user_text=lambda: "",
        enqueue_chain=lambda spec: events.append(("chain", spec)),
        remember_task=lambda a, ag, t: events.append(("remember", a)),
        deliver_answer=lambda a: events.append(("deliver", a)),
        prewarm_chain=lambda spec: events.append(("prewarm", spec)),
        async_launch=True,
        launch_failed=lambda a, ag, e: events.append(("failed", a, e)),
    )
    tools = {t.name: t for t in build_herdr_tools(fake, callbacks)}
    return tools["delegate_to_agent"], events


def test_tool_async_no_vigila_hasta_que_el_agente_recibio_la_tarea():
    fake = _AsyncFake()
    tool, events = _async_tools(fake)
    result = tool.handler(
        {"action": "run", "agent": "codex", "task": "pregunta", "notify_on_done": True}
    )
    assert result.ok and result.data["launching"] is True
    assert events == []  # nada registrado: aún está arrancando
    fake.on_ready({"ok": True, "address": "t7"})
    assert ("remember", "t7") in events
    assert ("watch", "t7") in events


def test_tool_async_pipeline_calienta_el_siguiente_antes_y_enlaza_la_cadena():
    fake = _AsyncFake()
    tool, events = _async_tools(fake)
    tool.handler(
        {
            "action": "run",
            "agent": "codex",
            "task": "pregunta",
            "deliver_answer": True,
            "then": [
                {
                    "agent": "opencode",
                    "task": "responde: {output}",
                    "model": "deepseek-v4.1-flash",
                    "effort": "low",
                }
            ],
        }
    )
    kinds = [e[0] for e in events]
    assert kinds == ["prewarm"]  # antes de que Codex termine de arrancar
    warm = events[0][1]
    assert warm["agent"] == "opencode"
    assert warm["effort"] == "low"
    fake.on_ready({"ok": True, "address": "t7"})
    chain = next(e[1] for e in events if e[0] == "chain")
    assert chain["source"] == "t7"
    assert chain["chain_id"] == warm["chain_id"]
    assert chain["deliver_answer"] is True


def test_tool_async_fallo_de_arranque_se_comunica():
    fake = _AsyncFake()
    tool, events = _async_tools(fake)
    tool.handler({"action": "run", "agent": "codex", "task": "x", "notify_on_done": True})
    fake.on_ready({"ok": False, "address": "t7", "error": "no arrancó"})
    assert events == [("failed", "t7", "no arrancó")]


def test_paso_sin_task_pasa_exactamente_la_respuesta_anterior():
    from ezagent.agent.handoff import compose_handoff

    steps = parse_steps([{"agent": "opencode", "effort": "low"}])
    assert steps[0]["task"] == "{output}"
    pregunta = "¿Cuál es el planeta más grande del sistema solar?"
    # Exactamente lo que se quería ver entrar en OpenCode: nada más.
    assert compose_handoff(steps[0]["task"], pregunta) == pregunta
    assert compose_handoff("", pregunta) == pregunta


def test_send_no_marca_el_mensaje_con_la_terminal_de_origen(monkeypatch):
    # `hc send` añade <hc from="t1">...</hc> si ve HC_TERMINAL_ID; EzAgent lo quita.
    seen: dict = {}

    def fake_run(self, command, timeout, env=None):
        seen["command"] = command
        seen["env"] = env
        return SimpleNamespace(returncode=0, stdout="{}", stderr="")

    monkeypatch.setattr(Herdr, "_run_command", fake_run)
    monkeypatch.setenv("HC_TERMINAL_ID", "t1")
    monkeypatch.setenv("HC_SCOPE", "scope-x")
    result = Herdr(VoiceConfig()).send_agent("t5", "¿Cuál es el planeta?")
    assert result["ok"] is True
    assert seen["command"][1:4] == ["send", "t5", "¿Cuál es el planeta?"]
    assert "HC_TERMINAL_ID" not in seen["env"]
    assert seen["env"]["HC_SCOPE"] == "scope-x"  # lo demás se conserva


def test_mensaje_largo_se_entrega_como_archivo_con_hc_put(monkeypatch, tmp_path):
    calls: list[list[str]] = []

    def fake_run(self, command, timeout, env=None):
        calls.append(command)
        if command[1] == "put":
            return SimpleNamespace(
                returncode=0,
                stdout=json.dumps({"path": "C:/herdr/files/t5/mensaje.md"}),
                stderr="",
            )
        return SimpleNamespace(returncode=0, stdout="{}", stderr="")

    monkeypatch.setattr(Herdr, "_run_command", fake_run)
    big = "Plan detallado. " * 4000  # 64 000 caracteres: no cabe en una línea de comandos
    result = Herdr(VoiceConfig()).send_agent("t5", big)
    assert result["ok"] is True
    assert [c[1] for c in calls] == ["put", "send"]
    pasted = calls[1][3]
    assert len(pasted) < 400
    assert "C:/herdr/files/t5/mensaje.md" in pasted
    saved = next((tmp_path / "localappdata" / "EzAgent" / "handoff").glob("mensaje-*.md"))
    assert saved.read_text(encoding="utf-8") == big  # el contenido íntegro


def test_mensaje_normal_se_sigue_pegando_tal_cual(monkeypatch):
    calls: list[list[str]] = []

    def fake_run(self, command, timeout, env=None):
        calls.append(command)
        return SimpleNamespace(returncode=0, stdout="{}", stderr="")

    monkeypatch.setattr(Herdr, "_run_command", fake_run)
    Herdr(VoiceConfig()).send_agent("t5", "x" * 5000)
    assert [c[1] for c in calls] == ["send"]
    assert calls[0][3] == "x" * 5000


TRUST_SCREEN = (
    "Accessing workspace:\nC:/proyecto\n"
    "Quick safety check: Is this a project you created or one you trust?\n"
    "❯ No, exit\nYes, I trust this folder\nEnter to confirm · Esc to cancel\n"
)
CODEX_STUCK = (
    ">_ OpenAI Codex (v0.167.0)\n~/proyecto\n"
    "› Crea el archivo notas.txt con tres lineas, luego muestralo y al\n"
    "final dime en una frase que archivo creaste y donde esta.\n"
    "GPT-6-Luna low · ~/proyecto\n⚠ 1 warning · f2 to view\n"
)
CODEX_DONE = (
    ">_ OpenAI Codex\n› Crea el archivo notas.txt con tres lineas, luego muestralo y al\n"
    "final dime en una frase que archivo creaste y donde esta.\n• Listo.\n"
    "Worked for 2s\n› Ask Codex to do anything\nGPT-6-Luna low\n"
)


def test_no_pulsa_enter_sobre_un_dialogo_de_confianza():
    # Antes el texto + Enter caía en «No, exit» y Claude se cerraba.
    rig = Rig(running="claude-code")
    rig.screen = TRUST_SCREEN
    result = rig.herdr.run_agent("claude", "haz algo")
    assert result["ok"] is False
    assert "confiar" in result["error"]
    assert rig.texts == ["claude"]  # solo el comando; ni tarea ni Enter extra
    assert ["enter"] not in rig.keys


def test_si_la_tarea_quedo_escrita_sin_enviar_se_pulsa_enter_una_vez():
    rig = Rig(running="codex")
    task = (
        "Crea el archivo notas.txt con tres lineas, luego muestralo y al "
        "final dime en una frase que archivo creaste y donde esta."
    )
    rig.screen = CODEX_STUCK

    def keys(address, k):
        rig.keys.append(list(k))
        if k == ["enter"]:
            rig.screen = CODEX_DONE
        return {"ok": True}

    rig.herdr.keys_agent = keys  # type: ignore[method-assign]
    result = rig.herdr.run_agent("codex", task)
    assert result["ok"] is True
    assert rig.keys == [["enter"]]
    assert not result["warnings"]


def test_tarea_ya_enviada_no_recibe_enter_de_mas():
    rig = Rig(running="codex")
    rig.screen = CODEX_DONE
    result = rig.herdr.run_agent(
        "codex",
        "Crea el archivo notas.txt con tres lineas, luego muestralo y al "
        "final dime en una frase que archivo creaste y donde esta.",
    )
    assert result["ok"] is True
    assert rig.keys == []


def test_opencode_con_el_pegado_sin_enviar_tambien_se_detecta():
    rig = Rig(running="opencode")
    rig.screen = "logo\n┃  [Pasted ~1 lines]\n┃  Build · DeepSeek\n"

    def keys(address, k):
        rig.keys.append(list(k))
        if k == ["enter"]:
            rig.screen = "respuesta"
        return {"ok": True}

    rig.herdr.keys_agent = keys  # type: ignore[method-assign]
    rig.herdr.run_agent("opencode", "una tarea cualquiera larga")
    assert rig.keys == [["enter"]]
