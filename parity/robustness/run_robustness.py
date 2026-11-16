"""Arnés de robustez de EzAgent (F7-05).

Corre cada escenario de `parity/robustness/scenarios/` con el arnés de Python
(`parity/replay/run_py.py`) y con el de Rust (`ezagent-replay`) y compara la
conducta observable: frases dichas, mensajes del chat, transcripciones, estado
final y que ninguno se cuelgue. Además ejecuta la prueba de fondo de 2 h
(`soak`): repite los escenarios en un proceso de Rust, pinta el HUD fuera de
pantalla y muestrea RSS, hilos y handles cada minuto.

Uso:
    python parity/robustness/run_robustness.py compare [--case R01]...
    python parity/robustness/run_robustness.py soak --minutes 120
    python parity/robustness/run_robustness.py --python-case r01-whisper-cut --out caso.json

Los textos que llevan el detalle de la librería HTTP (httpx en Python, reqwest
en Rust) se normalizan antes de comparar: la conducta debe coincidir, no la
redacción del error de red.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
REPLAY = ROOT / "parity" / "replay"
SCENARIOS = Path(__file__).resolve().parent / "scenarios"
RUST = ROOT / "rust"
FAKES = RUST / "target" / "debug"
REPLAY_EXE = FAKES / "ezagent-replay.exe"

CASE_TIMEOUT_SECS = 420.0
SAMPLE_SECS = 60.0
# Texto que solo depende del cliente HTTP (httpx vs reqwest): se compara la
# conducta, no el detalle del error de red.
DETAIL_RULES = (
    (
        re.compile(r"whisper-server no respondió a tiempo \([^)]*\): .*"),
        "whisper-server no respondió a tiempo (<url>): <detalle>",
    ),
    (re.compile(r"whisper-server: .*"), "whisper-server: <detalle>"),
    (
        re.compile(
            r"No puedo conectar con whisper-server\. Arranca scripts\\start-whisper\.ps1\..*"
        ),
        "No puedo conectar con whisper-server. Arranca scripts\\start-whisper.ps1. <detalle>",
    ),
)


# ------------------------------------------------------------------ escenarios
def case_paths(selection: list[str]) -> list[Path]:
    paths = sorted(SCENARIOS.glob("*.yaml"))
    if not selection:
        return paths
    chosen = []
    matched: set[str] = set()
    for path in paths:
        data = load_case(path)
        names = {path.stem, str(data.get("id") or "")}
        if names & set(selection):
            chosen.append(path)
            matched |= names & set(selection)
    missing = set(selection) - matched
    if missing:
        raise SystemExit(f"casos desconocidos: {sorted(missing)}")
    return chosen


def load_case(path: Path) -> dict[str, Any]:
    import yaml

    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    data["id"] = str(data.get("id") or path.stem.split("-")[0])
    return data


def rust_root(work: Path, paths: list[Path]) -> Path:
    """Copia los escenarios y el audio a un `parity/replay` que Rust pueda leer."""
    replay = work / "parity" / "replay"
    scenarios = replay / "scenarios"
    scenarios.mkdir(parents=True, exist_ok=True)
    shutil.copytree(REPLAY / "audio", replay / "audio", dirs_exist_ok=True)
    for path in paths:
        shutil.copy2(path, scenarios / path.name)
    return work


# ---------------------------------------------------------------------- Python
def run_python_case(stem: str, out: Path) -> int:
    """Corre un caso con el arnés de Python en un proceso propio."""
    sys.path.insert(0, str(REPLAY))
    import harness_ext  # noqa: F401 - pasos 16-25 y `wait_drained`
    import run_py

    patch_python_seams(run_py)
    path = SCENARIOS / f"{stem}.yaml"
    try:
        lines, snapshot, errors = run_py.run_scenario(run_py.load_scenario(path))
        payload: dict[str, Any] = {
            "ok": True,
            "lines": lines,
            "snapshot": snapshot,
            "errors": errors,
        }
    except Exception as exc:  # noqa: BLE001 - el arnés informa y sale con error
        payload = {"ok": False, "error": f"{type(exc).__name__}: {exc}"}
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
    return 0


def patch_python_seams(run_py: Any) -> None:
    """`mic_fail`: la captura pierde el dispositivo a mitad de llamada."""
    next_chunk = run_py.ReplayMic._next_chunk

    def failing_next_chunk(self: Any) -> bytes:
        reason = getattr(self, "robust_fail", None)
        if reason:
            raise RuntimeError(reason)
        return next_chunk(self)

    run_py.ReplayMic._next_chunk = failing_next_chunk
    apply_step = run_py.Driver._apply

    def apply_with_mic_fail(self: Any, step: dict[str, Any]) -> Any:
        if "mic_fail" in step:
            self.mic.robust_fail = str(step["mic_fail"])
            return run_py._always
        return apply_step(self, step)

    run_py.Driver._apply = apply_with_mic_fail


def python_case(stem: str, work: Path) -> dict[str, Any]:
    out = work / f"{stem}.py.json"
    command = [
        sys.executable,
        str(Path(__file__).resolve()),
        "--python-case",
        stem,
        "--out",
        str(out),
    ]
    try:
        result = subprocess.run(
            command,
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            timeout=CASE_TIMEOUT_SECS,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {"ok": False, "error": f"el arnés de Python no terminó en {CASE_TIMEOUT_SECS:.0f} s"}
    if not out.exists():
        tail = (result.stdout + result.stderr).strip().splitlines()[-6:]
        return {"ok": False, "error": "sin salida del arnés: " + " | ".join(tail)}
    return json.loads(out.read_text(encoding="utf-8"))


# ------------------------------------------------------------------------ Rust
def rust_case(stem: str, root: Path, work: Path) -> dict[str, Any]:
    trace = work / f"{stem}.rs.jsonl"
    env = dict(os.environ)
    env["EZ_REPLAY_ROOT"] = str(root)
    env["EZ_REPLAY_FAKES"] = str(FAKES)
    try:
        result = subprocess.run(
            [str(REPLAY_EXE), "--scenario", stem, "--trace-out", str(trace)],
            cwd=str(RUST),
            env=env,
            capture_output=True,
            text=True,
            timeout=CASE_TIMEOUT_SECS,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {"ok": False, "error": f"el arnés de Rust no terminó en {CASE_TIMEOUT_SECS:.0f} s"}
    if not trace.exists():
        tail = (result.stdout + result.stderr).strip().splitlines()[-6:]
        return {"ok": False, "error": "sin traza: " + " | ".join(tail)}
    lines = trace.read_text(encoding="utf-8").splitlines()
    errors = [
        line.strip()[2:] for line in result.stdout.splitlines() if line.strip().startswith("- ")
    ]
    return {
        "ok": result.returncode == 0,
        "code": result.returncode,
        "lines": lines,
        "errors": errors,
        "output": result.stdout.strip().splitlines()[-3:],
    }


# -------------------------------------------------------------------- conducta
def behavior(lines: list[str], home: Path | None) -> dict[str, Any]:
    events = []
    for line in lines:
        if not line.strip():
            continue
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    speech = [
        normalize(event.get("text", ""), home)
        for event in events
        if event.get("event") == "speech.say"
    ]
    messages = [
        (str(event.get("role") or ""), normalize(str(event.get("text") or ""), home))
        for event in events
        if event.get("event") == "chat.snapshot"
    ]
    transcripts = [
        normalize(str(event.get("text") or ""), home)
        for event in events
        if event.get("event") == "stt.transcript"
    ]
    end = next((event for event in events if event.get("event") == "app.end"), {})
    return {
        "state": end.get("state"),
        "in_call": end.get("in_call"),
        "muted": end.get("muted"),
        "speech": speech,
        "messages": messages,
        "transcripts": transcripts,
    }


def normalize(text: str, home: Path | None) -> str:
    if home is not None:
        for raw in (str(home), str(home).replace("\\", "/")):
            if len(raw) > 2:
                text = text.replace(raw, "<tmp>")
    text = re.sub(r"127\.0\.0\.1:\d+", "<url>", text)
    for pattern, replacement in DETAIL_RULES:
        if pattern.fullmatch(text):
            return replacement
    return text


def differences(python: dict[str, Any], rust: dict[str, Any]) -> list[str]:
    if not python.get("ok"):
        return [f"Python falló: {python.get('error')}"]
    if not rust.get("ok") and not rust.get("lines"):
        return [f"Rust falló: {rust.get('error')}"]
    left = python_behavior(python)
    right = behavior(rust["lines"], None)
    problems: list[str] = []
    for key in ("state", "in_call", "muted"):
        if left[key] != right[key]:
            problems.append(f"final {key}: Python {left[key]!r} != Rust {right[key]!r}")
    # Las locuciones salen de hilos distintos (voz y error); la traza escrita va
    # ordenada por hilo, no por reloj, así que se comparan como conjunto.
    for key in ("speech", "transcripts"):
        if sorted(left[key]) != sorted(right[key]):
            problems.append(f"{key}: Python {left[key]!r} != Rust {right[key]!r}")
    if left["messages"] != right["messages"]:
        problems.append(f"messages: Python {left['messages']!r} != Rust {right['messages']!r}")
    if rust.get("errors"):
        problems.append(f"aserciones del YAML en Rust: {rust['errors']}")
    if python.get("errors"):
        problems.append(f"aserciones del YAML en Python: {python['errors']}")
    return problems


def python_behavior(payload: dict[str, Any]) -> dict[str, Any]:
    snapshot = payload.get("snapshot") or {}
    messages = [
        (str(item.get("role") or ""), normalize(str(item.get("text") or ""), None))
        for item in snapshot.get("messages") or []
    ]
    speech = [normalize(str(item), None) for item in snapshot.get("speech") or []]
    transcripts = [normalize(str(item), None) for item in snapshot.get("transcripts") or []]
    return {
        "state": snapshot.get("state"),
        "in_call": snapshot.get("in_call"),
        "muted": snapshot.get("muted"),
        "speech": speech,
        "messages": messages,
        "transcripts": transcripts,
    }


# ------------------------------------------------------------------ comparación
def compare(selection: list[str], work: Path) -> int:
    paths = case_paths(selection)
    if not paths:
        raise SystemExit("no hay casos")
    root = rust_root(work, paths)
    failures = 0
    print(f"{len(paths)} caso(s); salida en {work}")
    for path in paths:
        stem = path.stem
        started = time.monotonic()
        python = python_case(stem, work)
        rust = rust_case(stem, root, work)
        problems = differences(python, rust)
        seconds = time.monotonic() - started
        if problems:
            failures += 1
            print(f"{stem}: DIFERENTE ({seconds:.1f} s)")
            for problem in problems:
                print(f"  - {problem}")
        else:
            print(f"{stem}: igual ({seconds:.1f} s)")
        if not python.get("ok"):
            print(f"  · Python: {python.get('error')}")
        if python.get("errors"):
            print(f"  · aserciones del YAML en Python: {python['errors']}")
        if not rust.get("ok") and rust.get("error"):
            print(f"  · Rust: {rust['error']}")
        elif not rust.get("ok"):
            print(f"  · salida de Rust: {rust.get('output')}")
    print(f"\n{len(paths) - failures}/{len(paths)} casos con la misma conducta")
    return 1 if failures else 0


# ------------------------------------------------------------------------- soak
class Counters:
    """Contadores del proceso que da Windows (sin dependencias externas)."""

    def __init__(self) -> None:
        import ctypes
        import ctypes.wintypes as wintypes

        self.ctypes = ctypes
        self.wintypes = wintypes
        self.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.psapi = ctypes.WinDLL("psapi", use_last_error=True)
        self.kernel32.OpenProcess.restype = wintypes.HANDLE
        self.kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        self.psapi.GetProcessMemoryInfo.argtypes = [
            wintypes.HANDLE,
            ctypes.c_void_p,
            wintypes.DWORD,
        ]

    def open(self, pid: int) -> Any:
        handle = self.kernel32.OpenProcess(0x0410, False, pid)  # QUERY_INFORMATION | VM_READ
        if not handle:
            raise OSError(self.ctypes.get_last_error(), "OpenProcess")
        return handle

    def close(self, handle: Any) -> None:
        self.kernel32.CloseHandle(handle)

    def memory(self, handle: Any) -> int:
        counters = self.ctypes.create_string_buffer(80)
        self.ctypes.memset(counters, 0, 80)
        self.ctypes.c_uint32.from_buffer(counters).value = 80
        if not self.psapi.GetProcessMemoryInfo(handle, counters, 80):
            raise OSError(self.ctypes.get_last_error(), "GetProcessMemoryInfo")
        # PeakWorkingSetSize (offset 8) y WorkingSetSize (offset 16) son size_t.
        return int(self.ctypes.c_size_t.from_buffer(counters, 16).value)

    def handles(self, handle: Any) -> int:
        count = self.wintypes.DWORD(0)
        if not self.kernel32.GetProcessHandleCount(handle, self.ctypes.byref(count)):
            raise OSError(self.ctypes.get_last_error(), "GetProcessHandleCount")
        return int(count.value)

    def threads(self, pid: int) -> int:
        snapshot = self.kernel32.CreateToolhelp32Snapshot(0x4, 0)  # TH32CS_SNAPTHREAD
        if snapshot in (0, self.wintypes.HANDLE(-1).value):
            return -1
        entry = self.ctypes.create_string_buffer(64)
        # `dwSize` debe ser exactamente `sizeof(THREADENTRY32)` = 28; Windows
        # rechaza otro valor con ERROR_BAD_LENGTH.
        self.ctypes.c_uint32.from_buffer(entry).value = 28
        count = 0
        try:
            ok = self.kernel32.Thread32First(snapshot, entry)
            while ok:
                # THREADENTRY32: dwSize(0), cntUsage(4), th32ThreadID(8),
                # th32OwnerProcessID(12).
                owner = self.ctypes.c_uint32.from_buffer(entry, 12).value
                if owner == pid:
                    count += 1
                ok = self.kernel32.Thread32Next(snapshot, entry)
        finally:
            self.kernel32.CloseHandle(snapshot)
        return count


def sample(counters: Counters, pid: int) -> dict[str, Any]:
    handle = counters.open(pid)
    try:
        return {
            "at": round(time.time(), 3),
            "rss": counters.memory(handle),
            "handles": counters.handles(handle),
            "threads": counters.threads(pid),
        }
    finally:
        counters.close(handle)


def soak(selection: list[str], minutes: float, warmup: float, work: Path, detach: bool) -> int:
    if not REPLAY_EXE.is_file():
        raise SystemExit(
            f"falta {REPLAY_EXE}; compílalo con: cargo build -p ez-app --features replay --bin ezagent-replay"
        )
    if detach:
        return launch_detached(selection, minutes, warmup, work)
    return run_soak(selection, minutes, warmup, work)


def launch_detached(selection: list[str], minutes: float, warmup: float, work: Path) -> int:
    """Un proceso aparte sobrevive a la sesión; el veredicto queda en `soak.json`."""
    command = [
        sys.executable,
        str(Path(__file__).resolve()),
        "soak",
        "--minutes",
        str(minutes),
        "--warmup",
        str(warmup),
        "--work",
        str(work),
    ]
    for case in selection:
        command += ["--case", case]
    handle = open(work / "sampler.log", "w", encoding="utf-8")
    flags = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    subprocess.Popen(
        command,
        cwd=str(ROOT),
        stdout=handle,
        stderr=subprocess.STDOUT,
        creationflags=flags,
    )
    print(f"soak lanzado en segundo plano; verá el veredicto en {work / 'soak.json'}")
    return 0


def run_soak(selection: list[str], minutes: float, warmup: float, work: Path) -> int:
    paths = case_paths(selection)
    root = rust_root(work, paths)
    command = [str(REPLAY_EXE), "--soak", str(minutes)]
    for path in paths:
        command += ["--scenario", path.stem]
    env = dict(os.environ)
    env["EZ_REPLAY_ROOT"] = str(root)
    env["EZ_REPLAY_FAKES"] = str(FAKES)
    log = work / "soak.log"
    out = work / "soak.json"
    counters = Counters()
    print(f"soak de {minutes:.0f} min con {len(paths)} escenario(s); registro en {log}")
    samples, code = follow_soak(command, env, counters, log, minutes)
    payload = {
        "code": code,
        "minutes": minutes,
        "warmup": warmup,
        "samples": samples,
        "scenarios": [path.stem for path in paths],
    }
    payload["verdict"] = judge(samples, warmup)
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"soak: {out}")
    for line in payload["verdict"]["notes"]:
        print("  " + line)
    return 0 if code == 0 and payload["verdict"]["stable"] else 1


def follow_soak(
    command: list[str], env: dict[str, str], counters: "Counters", log: Path, minutes: float
) -> tuple[list[dict[str, Any]], int]:
    """Arranca el soak y muestrea el proceso hasta que termina."""
    samples: list[dict[str, Any]] = []
    deadline = time.monotonic() + minutes * 60.0 + 120.0
    with open(log, "w", encoding="utf-8") as handle:
        proc = subprocess.Popen(
            command, cwd=str(RUST), env=env, stdout=handle, stderr=subprocess.STDOUT
        )
        try:
            while proc.poll() is None:
                try:
                    samples.append(sample(counters, proc.pid))
                    print("  " + json.dumps(samples[-1]))
                except OSError as error:
                    print(f"  no pude muestrear: {error}")
                if time.monotonic() > deadline:
                    proc.terminate()
                    break
                _sleep_until_next()
        except KeyboardInterrupt:
            proc.terminate()
        return samples, proc.wait(timeout=120)


def _sleep_until_next() -> None:
    end = time.monotonic() + SAMPLE_SECS
    while time.monotonic() < end:
        time.sleep(min(5.0, max(0.0, end - time.monotonic())))


def judge(samples: list[dict[str, Any]], warmup: float) -> dict[str, Any]:
    """RSS estable ±5 % tras el calentamiento; hilos y handles sin crecer."""
    if not samples:
        return {"stable": False, "notes": ["sin muestras"]}
    start = samples[0]["at"] + warmup * 60.0
    steady = [item for item in samples if item["at"] >= start]
    if len(steady) < 2:
        steady = samples[-2:] if len(samples) >= 2 else samples
    rss = [item["rss"] for item in steady]
    low, high = min(rss), max(rss)
    spread = (high - low) / low if low else 0.0
    threads = [item["threads"] for item in steady]
    handles = [item["handles"] for item in steady]
    thread_growth = threads[-1] - max(threads[:-1] or threads)
    handle_growth = handles[-1] - max(handles[:-1] or handles)
    stable = spread <= 0.05 and thread_growth <= 2 and handle_growth <= 16
    notes = [
        f"RSS tras el calentamiento: {low / 1048576:.1f}–{high / 1048576:.1f} MiB (±{spread * 100:.1f} %)",
        f"hilos: {min(threads)}–{max(threads)} (final {threads[-1]}, +{thread_growth})",
        f"handles: {min(handles)}–{max(handles)} (final {handles[-1]}, +{handle_growth})",
        f"muestras: {len(samples)} ({len(steady)} tras el calentamiento)",
    ]
    return {"stable": stable, "rss_spread": spread, "notes": notes}


# ------------------------------------------------------------------------- CLI
def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Robustez de EzAgent: Python y Rust lado a lado")
    parser.add_argument("--python-case", help="interno: corre un caso con el arnés de Python")
    parser.add_argument("--out", type=Path, help="interno: salida JSON del caso de Python")
    sub = parser.add_subparsers(dest="mode")
    compare_parser = sub.add_parser("compare", help="compara la conducta de cada caso")
    soak_parser = sub.add_parser("soak", help="prueba de fondo de 2 h")
    for child in (compare_parser, soak_parser):
        child.add_argument("--case", action="append", default=[], help="id o nombre del YAML")
        child.add_argument("--work", type=Path, help="carpeta de salida (por defecto, temporal)")
    soak_parser.add_argument("--minutes", type=float, default=120.0)
    soak_parser.add_argument("--warmup", type=float, default=10.0, help="minutos descartados")
    soak_parser.add_argument("--detach", action="store_true", help="lanza el soak aparte y sale")
    args = parser.parse_args(argv)
    if args.python_case:
        if args.out is None:
            raise SystemExit("--python-case necesita --out")
        return run_python_case(args.python_case, args.out)
    work = args.work or Path(tempfile.mkdtemp(prefix="ezrobustness-"))
    work.mkdir(parents=True, exist_ok=True)
    if args.mode == "soak":
        return soak(args.case, args.minutes, args.warmup, work, args.detach)
    return compare(args.case, work)


if __name__ == "__main__":
    sys.exit(main())
