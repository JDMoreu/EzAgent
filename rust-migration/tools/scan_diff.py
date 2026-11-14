"""Reglas de `rust/gate.ps1` que se pueden escribir como código puro (F0-05).

Reglas:
  scope      los archivos del diff están en la lista de la tarea
  forbidden  patrones prohibidos en las líneas añadidas
  sizes      ningún .rs supera 600 líneas
  fixtures   toda prueba de paridad afirma cuántos casos leyó (`cases_run`)

`--selftest` comprueba que cada regla rechaza su caso de ejemplo.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
MAX_LINES = 600

UNSAFE_MODULES = (
    "rust/crates/ez-hud/src/win/",
    "rust/crates/ez-audio/src/capture",
    "rust/crates/ez-tts/src/phonemes",
    "rust/crates/ez-app/src/tray",
    "rust/crates/ez-app/src/hotkeys",
)
PRINTLN_MODULES = (
    "rust/crates/ez-swarm/src/cli.rs",
    "rust/crates/ez-swarm/src/bin/",
    "rust/crates/ez-app/src/app/main.rs",
    "rust/crates/ez-app/src/bin/",
    "rust/crates/ez-fakes/src/bin/",
)

FORBIDDEN = [
    ("marcador sin implementar", re.compile(r"\b(todo!|unimplemented!)")),
    ("prueba ignorada", re.compile(r"#\[ignore")),
    ("TODO", re.compile(r"\bTODO\b")),
    ("shell cmd", re.compile(r'"cmd"')),
    ("shell cmd /c", re.compile(r'"/c"')),
    ("powershell -Command", re.compile(r"-Command")),
    ("borrado recursivo", re.compile(r"remove_dir_all")),
    ("println!", re.compile(r"\bprintln!")),
]


def added_lines(diff: str) -> list[tuple[str, str]]:
    """(archivo, línea) de las líneas añadidas del diff unificado."""
    out: list[tuple[str, str]] = []
    current = ""
    for raw in diff.splitlines():
        if raw.startswith("+++ b/"):
            current = raw[6:].strip()
        elif raw.startswith("+") and not raw.startswith("+++"):
            out.append((current, raw[1:]))
    return out


def check_forbidden(diff: str) -> list[str]:
    problems: list[str] = []
    added = added_lines(diff)
    for index, (path, line) in enumerate(added):
        for label, pattern in FORBIDDEN:
            if not pattern.search(line):
                continue
            if label == "println!" and path.startswith(PRINTLN_MODULES):
                continue
            in_test = "/tests/" in path or "/src/bin/" in path
            if label == "borrado recursivo" and in_test:
                continue
            if label == "borrado recursivo" and "temp" in line.lower():
                continue
            problems.append(f"{path}: {label}: {line.strip()}")
        if "unsafe" not in line:
            continue
        if not path.startswith(UNSAFE_MODULES):
            problems.append(f"{path}: unsafe fuera de los módulos permitidos")
            continue
        # el comentario de seguridad va en la línea o en las tres anteriores del mismo archivo
        window = [text for other, text in added[max(0, index - 3) : index + 1] if other == path]
        if not any("// SAFETY:" in text for text in window):
            problems.append(f"{path}: bloque unsafe sin // SAFETY:")
    return problems


def check_unwrap(diff: str) -> list[str]:
    problems: list[str] = []
    for path, line in added_lines(diff):
        if ".unwrap()" not in line:
            continue
        if "/tests/" in path or path.startswith("rust/crates/ez-parity/") or path.startswith("rust/crates/ez-fakes/"):
            continue
        if "DECISION" in line or "SAFETY" in line:
            continue
        problems.append(f"{path}: .unwrap() sin justificar: {line.strip()}")
    return problems


def check_sizes(root: pathlib.Path) -> list[str]:
    problems: list[str] = []
    for path in root.glob("rust/crates/**/*.rs"):
        total = len(path.read_text(encoding="utf-8", errors="replace").splitlines())
        if total > MAX_LINES:
            problems.append(f"{path.relative_to(root)}: {total} líneas (máximo {MAX_LINES})")
    return problems


def check_fixtures(root: pathlib.Path, task: dict) -> list[str]:
    fixtures = task.get("fixtures") or []
    if not fixtures:
        return []
    problems: list[str] = []
    for item in task["files"]:
        if "/tests/" not in str(item):
            continue
        path = root / str(item)
        if not path.exists():
            problems.append(f"{item}: no existe la prueba de paridad de la tarea")
        elif "cases_run" not in path.read_text(encoding="utf-8", errors="replace"):
            problems.append(f"{item}: no afirma cases_run == casos del archivo")
    return problems


def check_scope(changed: list[str], task: dict) -> list[str]:
    allowed = [str(item).rstrip("/") for item in task["files"]]
    allowed += ["rust-migration/reports/", "rust-migration/briefs/"]
    if not allowed or task["id"].startswith(("F0", "F1", "F7", "F8", "F9")):
        return []
    problems: list[str] = []
    for path in changed:
        if not any(path == a or path.startswith(a + "/") for a in allowed):
            problems.append(f"{path}: fuera del brief {task['id']}")
    return problems


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def default_base() -> str:
    """Base de la tarea: el commit anterior, o el tag congelado si es el primero.

    En esta rama cada tarea aterriza como un commit sobre `rust/main`, así que
    `rust/main...HEAD` siempre da vacío: la base correcta es `HEAD^`.
    """
    for candidate in ("HEAD^", "py-baseline"):
        if git("rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}").returncode == 0:
            return candidate
    raise SystemExit("no hay base git utilizable (ni HEAD^ ni py-baseline)")


def check_base(base: str) -> str:
    """Falla si la base no existe: un `git diff` roto devolvía un diff vacío."""
    result = git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}")
    if result.returncode != 0:
        raise SystemExit(f"base git inexistente: {base!r} (usa -Base <ref> válido)")
    return base


def empty_diff_problem(changed: list[str], base: str) -> str | None:
    """Un diff vacio casi siempre significa base equivocada, no tarea limpia."""
    if changed:
        return None
    return (
        f"base {base!r}: el diff está vacío; pasa -Base <commit> si la tarea ya "
        "está commiteada"
    )


def git_diff(base: str) -> str:
    return git("diff", f"{base}...HEAD", "--", "rust/", "parity/").stdout


def git_changed(base: str) -> list[str]:
    return [line.strip() for line in git("diff", "--name-only", f"{base}...HEAD", "--", "rust/", "parity/").stdout.splitlines() if line.strip()]


def selftest() -> int:
    import tempfile

    failures: list[str] = []
    sample = {
        "id": "F3-07",
        "files": ["rust/crates/ez-agent/src/tools_fs.rs", "rust/crates/ez-agent/tests/tools_fs.rs"],
        "fixtures": ["tools_fs"],
    }

    def expect(rule: str, problems: list[str]) -> None:
        if not problems:
            failures.append(f"{rule}: no rechazó su caso de ejemplo")

    expect("scope", check_scope(["rust/crates/ez-agent/src/memory.rs"], sample))
    expect("forbidden", check_forbidden("+++ b/rust/x.rs\n+    let a = todo!();\n"))
    expect("forbidden", check_forbidden('+++ b/rust/x.rs\n+    Command::new("cmd");\n'))
    expect("forbidden", check_forbidden('+++ b/rust/x.rs\n+    println!("hola");\n'))
    expect("forbidden", check_forbidden("+++ b/rust/x.rs\n+    unsafe { f(); }\n"))
    expect("forbidden", check_forbidden("+++ b/rust/crates/ez-hud/src/win/a.rs\n+    unsafe { f(); }\n"))
    expect("unwrap", check_unwrap("+++ b/rust/crates/ez-core/src/lib.rs\n+    let v = map.get(&k).unwrap();\n"))
    expect("forbidden", check_forbidden("+++ b/rust/crates/ez-core/src/lib.rs\n+    remove_dir_all(path);\n"))

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        big = root / "rust" / "crates" / "ez-core" / "src" / "big.rs"
        big.parent.mkdir(parents=True)
        big.write_text("\n".join("// línea" for _ in range(MAX_LINES + 1)), encoding="utf-8")
        expect("sizes", check_sizes(root))

        fixture_example = root / "rust" / "crates" / "ez-agent" / "tests" / "tools_fs.rs"
        fixture_example.parent.mkdir(parents=True)
        fixture_example.write_text("#[test]\nfn p() {}\n", encoding="utf-8")
        task = {
            "id": "F3-07",
            "files": ["rust/crates/ez-agent/tests/tools_fs.rs"],
            "fixtures": ["tools_fs"],
        }
        expect("fixtures", check_fixtures(root, task))

    # casos que deben pasar
    if check_scope(["rust/crates/ez-agent/src/tools_fs.rs"], sample):
        failures.append("scope: rechazó un archivo del brief")
    if check_forbidden("+++ b/rust/crates/ez-swarm/src/bin/ezs.rs\n+    println!(\"ok\");\n"):
        failures.append("forbidden: rechazó println! permitido en ezs")
    if check_forbidden(
        "+++ b/rust/crates/ez-hud/src/win/a.rs\n+    // SAFETY: hwnd válido\n+    unsafe { f(); }\n"
    ):
        failures.append("forbidden: rechazó unsafe con // SAFETY: en el módulo permitido")

    if failures:
        print("\n".join(f"FALLO {f}" for f in failures))
        return 1
    print("selftest ok: cada regla rechaza su caso de ejemplo")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task")
    parser.add_argument("--base", default=None)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv[1:])

    if args.selftest:
        return selftest()

    if not args.task:
        parser.error("falta --task <ID>")

    import yaml

    data = yaml.safe_load((ROOT / "rust-migration" / "tasks.yaml").read_text(encoding="utf-8"))
    task = next((t for t in data["tasks"] if t["id"] == args.task), None)
    if task is None:
        print(f"tarea desconocida: {args.task}")
        return 2

    base = check_base(args.base or default_base())
    changed = git_changed(base)
    empty = empty_diff_problem(changed, base)
    if empty:
        print(f"FALLO {empty}")
        return 1
    diff = git_diff(base)
    problems = (
        check_scope(changed, task)
        + check_forbidden(diff)
        + check_unwrap(diff)
        + check_sizes(ROOT)
        + check_fixtures(ROOT, task)
    )
    if problems:
        print("\n".join(f"FALLO {p}" for p in problems))
        return 1
    print(f"reglas de codigo de {args.task}: ok ({len(changed)} archivos contra {base})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
