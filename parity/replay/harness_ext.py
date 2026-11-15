"""Extensiones del arnés para los escenarios 16-25 (F1-12c).

`run_py.py` importa este módulo al final y aquí se enganchan, sin reescribir el
arnés de F1-12a, los pasos y aserciones que necesitan los escenarios nuevos:
panel de chat, guardado del menú, degradaciones y el enjambre con `ezs`.

El enjambre corre con el `SwarmStack` real (servidor y supervisor), pero el
supervisor avanza a mano (`swarm_tick`) y con un reloj de prueba: así la traza
no depende del reloj de pared. Los terminales son de `fake-hc`; las llamadas
`ezs` las lanza el arnés con el asiento del miembro, igual que un agente falso.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Any, Callable

import __main__

# `run_py.py` se ejecuta como script: sus funciones viven en `__main__`, no en
# un módulo `run_py` aparte. Al importarlo desde pruebas se usa el módulo.
run_py = __main__ if hasattr(__main__, "Driver") and hasattr(__main__, "Trace") else __import__("run_py")

ACTIVE: dict[str, Any] = {}
_ALWAYS = run_py._always
_SCRUB = "<swarm>"

# ------------------------------------------------------------------ utilidades
def _scrub(value: Any) -> Any:
    swarm_id = str(ACTIVE.get("swarm_id") or "")
    if isinstance(value, str):
        return value.replace(swarm_id, _SCRUB) if swarm_id else value
    if isinstance(value, dict):
        return {str(key): _scrub(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_scrub(item) for item in value]
    return value

_orig_record = run_py.Trace.record

def _record(self: Any, event: str, **data: Any) -> None:
    _orig_record(self, event, **_scrub(data))

run_py.Trace.record = _record

def _stack(driver: Any) -> Any:
    stack = getattr(driver.app, "swarm", None)
    if stack is None:
        raise RuntimeError("el escenario no montó el enjambre")
    return stack

def _hc_state() -> dict[str, Any]:
    return json.loads(Path(os.environ["FAKE_HC_STATE"]).read_text(encoding="utf-8"))

def _hc_save(data: dict[str, Any]) -> None:
    Path(os.environ["FAKE_HC_STATE"]).write_text(
        json.dumps(data, ensure_ascii=False), encoding="utf-8"
    )

def _address(stack: Any, member: str) -> str:
    found = next(
        (item for item in stack.store.members(ACTIVE["swarm_id"]) if item.name == member),
        None,
    )
    if found is None or not found.address:
        raise RuntimeError(f"{member} no tiene terminal en el enjambre")
    return found.address

def _terminal(member: str, **fields: Any) -> None:
    stack = ACTIVE["stack"]
    address = _address(stack, member)
    data = _hc_state()
    terminal = next((item for item in data["terminals"] if item["address"] == address), None)
    if terminal is None:
        raise RuntimeError(f"fake-hc no tiene la terminal {address}")
    terminal.update(fields)
    _hc_save(data)

def _settle_launches(stack: Any, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        with stack.supervisor._lock:
            launching = bool(stack.supervisor._launching)
        if not launching:
            return
        time.sleep(0.02)
    raise TimeoutError("el supervisor no terminó de abrir a los agentes")

# ------------------------------------------------------------------- enjambre
class Clock:
    """Reloj de prueba: el supervisor y el tablero comparten estos segundos."""

    def __init__(self) -> None:
        self.wall = 1735732800.0  # 2025-01-01 12:00:00 UTC
        self.mono = 1000.0

    def advance(self, seconds: float) -> None:
        self.wall += seconds
        self.mono += seconds

def _make_create_swarm(trace: Any) -> Callable[[Any], Any]:
    def create(self: Any) -> Any:
        from ezagent.swarm.stack import SwarmStack

        base = self.tasks.path.with_name("swarm")
        stack = SwarmStack(
            base,
            self.herdr,
            announce=lambda text: self.events.put(("swarm_notice", text, False)),
            ask=lambda text: self.events.put(("swarm_notice", text, True)),
        )
        clock = Clock()
        stack.store._clock = lambda: clock.wall
        stack.service._clock = lambda: clock.wall
        stack.supervisor._clock = lambda: clock.mono
        stack.supervisor.recovery._wall = lambda: clock.wall
        stack.supervisor.recovery._clock = lambda: clock.mono
        stack.start = lambda: stack.server.start()
        stack.stop = lambda: stack.server.stop()
        ACTIVE.update({"stack": stack, "clock": clock, "launches": []})
        return stack

    return create

def _swarm_start(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    from ezagent.swarm.service import MemberSpec

    block = dict(step["swarm_start"] or {})
    stack = _stack(driver)
    specs = [
        MemberSpec(
            name=str(item["name"]),
            role=str(item.get("role") or ""),
            agent=str(item.get("agent") or "opencode"),
            model=str(item.get("model") or ""),
            effort=str(item.get("effort") or ""),
            task=str(item.get("task") or ""),
            parallel=bool(item.get("parallel")),
        )
        for item in block.get("members") or []
    ]
    stack.server.start()
    swarm_id, _ = stack.service.start(str(block.get("goal") or ""), specs)
    ACTIVE["swarm_id"] = swarm_id
    driver.trace.record(
        "swarm.start", goal=str(block.get("goal") or ""), members=[spec.name for spec in specs]
    )
    return _ALWAYS

def _swarm_tick(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    stack = _stack(driver)
    times = int(step["swarm_tick"].get("times", 1)) if isinstance(step["swarm_tick"], dict) else 1
    before = {
        (item.name, item.address) for item in stack.store.members(ACTIVE["swarm_id"]) if item.address
    }
    for _ in range(max(1, times)):
        stack.supervisor.tick()
        _settle_launches(stack)
        after = set()
        fresh: list[str] = []
        for item in stack.store.members(ACTIVE["swarm_id"]):
            if not item.address:
                continue
            after.add((item.name, item.address))
            if (item.name, item.address) not in before:
                fresh.append(item.name)
        if fresh:
            driver.trace.record("swarm.launched", members=fresh)
            ACTIVE.setdefault("launches", []).append(fresh)
        before = after
    return _ALWAYS

def _swarm_agent(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["swarm_agent"] or {})
    member, argv = str(spec["member"]), [str(item) for item in spec.get("argv") or []]
    stack = _stack(driver)
    seat = stack.base_dir / "seats" / f"{ACTIVE['swarm_id']}-{member}.seat"
    if not seat.is_file():
        raise RuntimeError(f"{member} no tiene asiento en {seat.name}")
    exe = Path(sys.executable).with_name("ezs.exe")
    if not exe.is_file():
        exe = Path("ezs")
    env = dict(os.environ)
    env["EZS_SEAT"] = str(seat)
    env["EZS_ENDPOINT"] = str(stack.base_dir / "endpoint.json")
    result = subprocess.run(
        [str(exe), *argv],
        cwd=str(run_py.ROOT),
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=150,
    )
    text = (result.stdout or result.stderr).strip()
    driver.trace.record(
        "swarm.ezs", member=member, argv=" ".join(argv), ok=result.returncode == 0, text=text
    )
    if result.returncode != 0:
        raise RuntimeError(f"ezs {member} {argv[0]}: {text}")
    if argv[:1] and argv[0].lower() in ("done", "fix", "fail"):
        _terminal(member, state="idle")
    return _ALWAYS

def _swarm_screen(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["swarm_screen"] or {})
    member = str(spec["member"])
    fields = {key: spec[key] for key in ("state", "screen", "agent") if key in spec}
    _terminal(member, **fields)
    driver.trace.record("swarm.screen", member=member, state=str(spec.get("state") or ""))
    return _ALWAYS

def _swarm_lost(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["swarm_lost"] or {})
    member, mode = str(spec["member"]), str(spec.get("mode") or "remove")
    if mode == "remove":
        data = _hc_state()
        address = _address(ACTIVE["stack"], member)
        data["terminals"] = [item for item in data["terminals"] if item["address"] != address]
        _hc_save(data)
    elif mode == "shell":
        _terminal(member, agent="powershell.exe", state="unknown", screen="PS C:\\> ")
    else:
        _terminal(member, state="error", screen=str(spec.get("screen") or "error"))
    driver.trace.record("swarm.lost", member=member, mode=mode)
    return _ALWAYS

def _swarm_resolve(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["swarm_resolve"] or {})
    stack = _stack(driver)
    incident = int(spec.get("incident") or 0)
    if not incident:
        found = stack.store.incidents(ACTIVE["swarm_id"])
        if not found:
            raise RuntimeError("no hay incidente abierto")
        incident = found[0].id
    choice = str(spec["choice"])
    text = stack.recovery.resolve(incident, choice)
    driver.trace.record("swarm.resolve", incident=incident, choice=choice, text=text)
    return _ALWAYS

def _swarm_clock(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["swarm_clock"] or {})
    seconds = float(spec.get("advance", 0.0))
    ACTIVE["clock"].advance(seconds)
    driver.trace.record("swarm.clock", advanced=seconds)
    return _ALWAYS

# ------------------------------------------------------------ pasos de la app
def _hud(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    spec = dict(step["hud"] or {})
    action = str(spec["action"])
    callback = getattr(driver.app.hud, f"on_{action}", None)
    if not callable(callback):
        raise RuntimeError(f"el HUD no expone on_{action}")
    if action == "chat_send":
        callback(str(spec.get("text") or ""))
    elif action in ("chat_switch", "chat_delete"):
        callback(int(spec["session"]))
    else:
        callback()
    driver.trace.record("hud.panel", action=action)
    return _ALWAYS

def _menu_save(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    values = dict(step["menu_save"] or {})
    settings = driver.app._hud_menu_settings()
    if settings is None:
        raise RuntimeError("el menú no está disponible")
    for key, value in values.items():
        settings.set(key, value)
    driver.app._hud_save_settings(settings.snapshot())
    driver.trace.record("menu.save", keys=sorted(values))
    return _ALWAYS

def _predicate(timeout: float, check: Callable[[], bool], what: str) -> Callable[[], bool]:
    deadline = time.monotonic() + timeout

    def poll() -> bool:
        if check():
            return True
        if time.monotonic() > deadline:
            raise TimeoutError(f"{what} no ocurrió en {timeout:.0f} s")
        return False

    return poll

def _wait_config(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    want = dict(step["wait_config"] or {})
    timeout = float(dict(step).get("timeout") or 20.0)

    def check() -> bool:
        return all(getattr(driver.app.cfg, key, None) == value for key, value in want.items())

    return _predicate(timeout, check, "los ajustes esperados")

def _wait_voice(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    want = dict(step["wait_voice"] or {})
    timeout = float(dict(step).get("timeout") or 20.0)

    def check() -> bool:
        try:
            data = json.loads((Path(os.environ["APPDATA"]) / "EzAgent" / "voice.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return False
        return all(data.get(key) == value for key, value in want.items())

    return _predicate(timeout, check, "voice.json con los valores esperados")

def _drained(driver: Any, step: dict[str, Any]) -> Callable[[], bool]:
    timeout = float(step["wait_drained"] or 20)
    deadline = time.monotonic() + timeout
    since: list[float | None] = [None]

    def check() -> bool:
        now = time.monotonic()
        app = driver.app
        busy = any(
            thread.name.startswith(
                (
                    "ezagent-speech",
                    "ezagent-answer",
                    "ezagent-chat",
                    "ezagent-tts",
                    "ezagent-stt",
                    "ezagent-swarm-launch",
                )
            )
            and thread.is_alive()
            for thread in threading.enumerate()
        )
        quiet = (
            app.events.empty()
            and app.menu_events.empty()
            and not app._announcements()
            and not busy
            and app.state not in ("thinking", "speaking")
        )
        if not quiet:
            since[0] = None
            return False
        if since[0] is None:
            since[0] = now
        if now - since[0] >= 0.3:
            return True
        if now > deadline:
            raise TimeoutError(f"la app no quedó quieta en {timeout:.0f} s")
        return False

    return check

_HANDLERS = {
    "swarm_start": _swarm_start,
    "swarm_tick": _swarm_tick,
    "swarm_agent": _swarm_agent,
    "swarm_screen": _swarm_screen,
    "swarm_lost": _swarm_lost,
    "swarm_resolve": _swarm_resolve,
    "swarm_clock": _swarm_clock,
    "hud": _hud,
    "menu_save": _menu_save,
    "wait_config": _wait_config,
    "wait_voice": _wait_voice,
    "wait_drained": _drained,
}

_orig_apply = run_py.Driver._apply

def _apply(self: Any, step: dict[str, Any]) -> Callable[[], bool]:
    for key, handler in _HANDLERS.items():
        if key in step:
            return handler(self, step)
    return _orig_apply(self, step)

run_py.Driver._apply = _apply

# ------------------------------------------------------------------ parcheo
def _break_speaker(speaker: Any, trace: Any) -> None:
    reason = "No encuentro el modelo de Kokoro; revisa la voz o la ruta."

    def synthesize(_text: str) -> Any:
        trace.record("degraded.kokoro")
        raise RuntimeError(reason)

    def speak(*_args: Any, **_kwargs: Any) -> Any:
        trace.record("degraded.kokoro")
        raise RuntimeError(reason)

    speaker.synthesize = synthesize
    speaker.speak = speak

def _broken_smart_turn(trace: Any) -> Any:
    class BrokenSmartTurn:
        def __init__(self, *_args: Any, **_kwargs: Any) -> None:
            trace.record("degraded.smart_turn")
            raise RuntimeError("No encuentro el modelo de Smart Turn.")

    return BrokenSmartTurn

_orig_install = run_py.install_patches

def _install(
    module: Any, hud: Any, root: Any, mic: Any, speaker: Any, trace: Any, *rest: Any
) -> None:
    original_context_text = module.Herdr.context_text
    _orig_install(module, hud, root, mic, speaker, trace, *rest)
    ACTIVE["trace"] = trace
    scenario = dict(ACTIVE.get("scenario") or {})
    if scenario.get("swarm"):
        # F1-12b anula `Herdr.context_text` para sus avisos; el enjambre
        # (F1-12c) depende del texto real.
        module.Herdr.context_text = original_context_text
    if scenario.get("swarm"):
        module.VoiceApp._create_swarm = _make_create_swarm(trace)
    degrade = set(scenario.get("degrade") or [])
    if "kokoro" in degrade:
        _break_speaker(speaker, trace)
    if "smart_turn" in degrade:
        module.SmartTurn = _broken_smart_turn(trace)

run_py.install_patches = _install

_orig_prepare = run_py.prepare_home

def _prepare(data: dict[str, Any], home: Path, settings: dict[str, Any]) -> Path:
    path = _orig_prepare(data, home, settings)
    # `apply_env` deja que LLM_MODEL del entorno pise el modelo guardado: el
    # escenario manda también ahí para poder medir la corrección del id.
    os.environ["LLM_MODEL"] = str(settings.get("llm_model") or "fake-model")
    if data.get("swarm"):
        profiles = settings.setdefault("agent_profiles", {})
        profiles.setdefault("opencode", {})["command"] = [
            str(run_py.FAKES / "fake-agent.exe"),
            "--scene",
            "working",
        ]
        path.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
            newline="\n",
        )
    return path

run_py.prepare_home = _prepare

_orig_run = run_py.run_scenario

def _run(data: dict[str, Any]) -> Any:
    ACTIVE.clear()
    ACTIVE["scenario"] = data
    ACTIVE["launches"] = []
    return _orig_run(data)

run_py.run_scenario = _run

# --------------------------------------------------------------- instantánea
def _chat_history(app: Any) -> list[str]:
    store = app.chat.store
    if store is None:
        return []
    out: list[str] = []
    for info in sorted(store.sessions(), key=lambda item: item.session):
        for entry in store.entries(info.session):
            out.append(f"{entry.session}:{entry.role}:{entry.text}")
    return out

def _swarm_view(stack: Any) -> dict[str, Any]:
    swarm_id = ACTIVE["swarm_id"]
    info = stack.store.get(swarm_id)
    return {
        "status": info.status if info is not None else "",
        "tasks": [
            f"T{task.id} {task.state} {task.owner or '-'}"
            for task in stack.store.tasks(swarm_id)
        ],
        "members": [
            f"{member.name} {member.state}"
            for member in stack.store.members(swarm_id)
        ],
        "incidents": [
            f"I{item.id} {item.member} {item.kind} {item.state} {item.action}"
            for item in stack.store.incidents(swarm_id, active=False)
        ],
        "launches": [list(group) for group in ACTIVE.get("launches", [])],
        "resumes_sent": sum(
            1
            for terminal in _hc_state().get("terminals", [])
            for sent in terminal.get("sent", [])
            if sent.startswith("[EzAgent] Ya se restableció tu uso.")
        ),
    }

_orig_snapshot = run_py.Driver.snapshot

def _snapshot(self: Any) -> dict[str, Any]:
    snap = _orig_snapshot(self)
    app = self.app
    cfg = app.cfg
    snap["config"] = {
        key: getattr(cfg, key, None)
        for key in (
            "voice",
            "speed",
            "llm_provider",
            "llm_model",
            "llm_api_style",
            "hotkey",
            "chat_voice",
            "smart_turn",
            "silence_secs",
        )
    }
    snap["history"] = _chat_history(app)
    stack = getattr(app, "swarm", None)
    if stack is not None and ACTIVE.get("swarm_id"):
        snap["swarm"] = _swarm_view(stack)
    return _scrub(snap)

run_py.Driver.snapshot = _snapshot

# ---------------------------------------------------------------- aserciones
def _voice_json() -> dict[str, Any]:
    path = Path(os.environ["APPDATA"]) / "EzAgent" / "voice.json"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}

_orig_check = run_py.check_expect

def _check(
    expect: dict[str, Any], snapshot: dict[str, Any], trace: Any, requests: int, *rest: Any
) -> list[str]:
    errors = _orig_check(expect, snapshot, trace, requests, *rest)
    swarm = snapshot.get("swarm") or {}
    for key in ("swarm_tasks", "swarm_members", "swarm_incidents", "launches"):
        wanted = expect.get(key)
        name = {
            "swarm_tasks": "tasks",
            "swarm_members": "members",
            "swarm_incidents": "incidents",
            "launches": "launches",
        }[key]
        if wanted is not None and swarm.get(name) != wanted:
            errors.append(f"enjambre {name}: {swarm.get(name)!r} != {wanted!r}")
    if "swarm_status" in expect and swarm.get("status") != expect["swarm_status"]:
        errors.append(f"enjambre status: {swarm.get('status')!r} != {expect['swarm_status']!r}")
    if "resumes_sent" in expect and swarm.get("resumes_sent") != int(expect["resumes_sent"]):
        errors.append(
            f"avisos de reanudación: {swarm.get('resumes_sent')!r} != {expect['resumes_sent']!r}"
        )
    config = snapshot.get("config") or {}
    for key, value in (expect.get("config") or {}).items():
        if config.get(key) != value:
            errors.append(f"ajuste {key}: {config.get(key)!r} != {value!r}")
    saved = _voice_json()
    for key, value in (expect.get("voice_json") or {}).items():
        if saved.get(key) != value:
            errors.append(f"voice.json {key}: {saved.get(key)!r} != {value!r}")
    if "chat_history" in expect and snapshot.get("history") != expect["chat_history"]:
        errors.append(f"historial: {snapshot.get('history')!r} != {expect['chat_history']!r}")
    degraded = sorted(
        {item["event"] for item in trace.chrono if str(item["event"]).startswith("degraded.")}
    )
    if "degraded" in expect and degraded != expect["degraded"]:
        errors.append(f"degradaciones: {degraded!r} != {expect['degraded']!r}")
    if "tool_failures" in expect:
        failures = sum(
            1
            for item in trace.with_event("tool.result")
            if not bool(item.get("ok"))
        )
        if failures != int(expect["tool_failures"]):
            errors.append(f"herramientas fallidas: {failures} != {expect['tool_failures']}")
    if isinstance(expect.get("files"), list):
        names = list(dict.fromkeys(item["name"] for item in trace.with_event("file.write")))
        if names != expect["files"]:
            errors.append(f"archivos escritos: {names!r} != {expect['files']!r}")
    return errors

run_py.check_expect = _check
