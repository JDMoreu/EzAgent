"""Fixtures de las herramientas de skills (dominio `tools_skills`, F1-14).

Cubre `src/ezagent/agent/tools_skills.py` sobre una biblioteca real de skills
creada en el perfil temporal: carga del cuerpo y de archivos auxiliares,
validación de rutas, aprobaciones por hash y ejecución de scripts con
`subprocess.run` sustituido por un doble determinista (nunca se lanza Python).

El intérprete se sustituye por `C:/Python/python.exe` o por `None`; las rutas
del perfil se normalizan a `<tmp>`.
"""

from __future__ import annotations

import logging
import os
import subprocess
from dataclasses import asdict
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tools_skills"

INTERPRETER = "C:/Python/python.exe"

SKILLS: dict[str, dict[str, Any]] = {
    "demo": {
        "front": "name: demo\ndescription: Skill de prueba para scripts.\n",
        "body": "Cuerpo de la demo.\n\nSegunda línea.\n",
        "files": {
            "scripts/ok.py": "print('ok')\n",
            "scripts/fail.py": "raise SystemExit(2)\n",
            "scripts/rc3.py": "raise SystemExit(3)\n",
            "scripts/big.py": "print('x')\n",
            "scripts/slow.py": "import time\n",
            "scripts/slowstr.py": "import time\n",
            "scripts/boom.py": "print('boom')\n",
            "scripts/empty.py": "pass\n",
            "scripts/MAYUS.PY": "print('mayus')\n",
            "scripts/note.txt": "no es python\n",
            "references/guia.md": "Guía de la skill.\n",
            "references/crlf.md": "uno\r\ndos\r\n",
            "references/grande.md": "h" * 20010,
            "assets/binario.dat": b"\xff\xfe roto",
        },
    },
    "sin-scripts": {
        "front": "name: sin-scripts\ndescription: Skill sin carpeta scripts.\n",
        "body": "Sin scripts.\n",
        "files": {"references/guia.md": "guia\n"},
    },
    "vacia": {
        "front": "name: vacia\ndescription: Skill con cuerpo vacío.\n",
        "body": "   \n\n",
    },
    "grande": {
        "front": "name: grande\ndescription: Cuerpo enorme.\n",
        "body": "g" * 20010,
    },
    "crlf": {
        "front": "name: crlf\ndescription: Editada en Windows.\n",
        "raw": "---\r\nname: crlf\r\ndescription: Editada en Windows.\r\n---\r\nLínea uno.\r\nLínea dos.\r\n",
    },
}


def _write_skills(base: Path) -> None:
    for folder, spec in SKILLS.items():
        root = base / folder
        root.mkdir(parents=True, exist_ok=True)
        if "raw" in spec:
            (root / "SKILL.md").write_bytes(spec["raw"].encode("utf-8"))
        else:
            text = f"---\n{spec['front']}---\n{spec.get('body', '')}"
            (root / "SKILL.md").write_bytes(text.encode("utf-8"))
        for relative, content in spec.get("files", {}).items():
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(content, bytes):
                target.write_bytes(content)
            else:
                target.write_bytes(content.encode("utf-8"))


def _config(home: Path, label: str, allow_scripts: bool) -> Any:
    from ezagent.agent.config import (
        AgentConfig,
        ConfirmConfig,
        GeneralConfig,
        PathsConfig,
        SkillsConfig,
        ToolsDConfig,
        WebConfig,
    )

    work = home / "skills-tools-cases" / label
    return AgentConfig(
        general=GeneralConfig(),
        paths=PathsConfig(read_roots=[str(work)], write_roots=[str(work)], deny=[], deny_globs=[]),
        confirm=ConfirmConfig(accept=[], reject=[]),
        web=WebConfig(),
        skills=SkillsConfig(allow_scripts=allow_scripts),
        tools_d=ToolsDConfig(allowed_executables=[]),
        app_dir=work / "app",
        workspace=work / "workspace",
        permissions_path=work / "app" / "permissions.toml",
        system_md_path=work / "app" / "system.md",
        memory_dir=work / "app" / "memory",
        history_path=work / "app" / "memory" / "history.db",
        audit_path=work / "app" / "logs" / "audit.jsonl",
        journal_dir=work / "app" / "journal",
        skills_dir=work / "skills",
        global_skills_dir=work / "global-skills",
        tools_d_dir=work / "app" / "tools.d",
    )


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        text = value
        for prefix in (str(home), home.as_posix()):
            text = text.replace(prefix, "<tmp>")
        return text.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _expand(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return value.replace("<tmp>", str(home))
    if isinstance(value, list):
        return [_expand(item, home) for item in value]
    if isinstance(value, dict):
        return {key: _expand(item, home) for key, item in value.items()}
    return value


class _Replies:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def run(self, argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
        self.calls.append(
            {
                "argv": [Path(item).name if index == 1 else item for index, item in enumerate(argv)],
                "argv_full": list(argv),
                "shell": kwargs.get("shell"),
                "cwd": kwargs.get("cwd"),
                "timeout": kwargs.get("timeout"),
                "encoding": kwargs.get("encoding"),
                "errors": kwargs.get("errors"),
                "text": kwargs.get("text"),
                "capture_output": kwargs.get("capture_output"),
                "check": kwargs.get("check"),
                "env_keys": sorted(kwargs.get("env") or {}),
            }
        )
        name = Path(argv[1]).name
        if name == "slow.py":
            raise subprocess.TimeoutExpired(argv, kwargs.get("timeout") or 0, output=b"parcial\n")
        if name == "slowstr.py":
            raise subprocess.TimeoutExpired(argv, kwargs.get("timeout") or 0, output="texto\n")
        if name == "boom.py":
            raise OSError("no se encontró el intérprete")
        replies = {
            "ok.py": (0, "hola desde el script\n", ""),
            "MAYUS.PY": (0, "mayus\n", ""),
            "fail.py": (2, "algo\n", "  falló  \n"),
            "rc3.py": (3, "", "   \n"),
            "big.py": (0, "z" * 9000, "e" * 9000),
            "empty.py": (0, None, None),
        }
        rc, stdout, stderr = replies.get(name, (0, "", ""))
        return subprocess.CompletedProcess(argv, rc, stdout, stderr)


def _approvals_text(mode: str, hash_value: str) -> str | None:
    if mode == "missing":
        return None
    if mode == "match":
        return f'{{\n  "demo": "{hash_value}"\n}}\n'
    if mode == "stale":
        return '{\n  "demo": "hash-viejo"\n}\n'
    if mode == "corrupt":
        return "{no es json"
    if mode == "non-object":
        return '["lista"]'
    return '{\n  "demo": 5,\n  "otra": "x"\n}\n'


def _run(
    out: FixtureFile,
    home: Path,
    label: str,
    *,
    allow_scripts: bool = True,
    interpreter: str | None = INTERPRETER,
    approvals: str = "missing",
    fail_write: bool = False,
    calls: list[tuple[str, dict[str, Any]]],
) -> None:
    from ezagent.agent import tools_skills as module
    from ezagent.agent.skills import SkillLibrary, hash_folder

    config = _config(home, label, allow_scripts)
    _write_skills(config.skills_dir)
    library = SkillLibrary(config)
    hash_value = hash_folder(config.skills_dir / "demo")
    approvals_path = Path(config.app_dir) / "skills_approved.json"
    rendered = _approvals_text(approvals, hash_value)
    if rendered is not None:
        approvals_path.parent.mkdir(parents=True, exist_ok=True)
        approvals_path.write_bytes(rendered.encode("utf-8"))

    replies = _Replies()
    original_interpreter = module._python_interpreter
    original_run = module.subprocess.run
    original_write = module._write_approvals

    def fake_write(path: Path, values: dict[str, str]) -> None:
        if fail_write:
            raise OSError("disco lleno")
        original_write(path, values)

    module._python_interpreter = lambda: interpreter
    module.subprocess.run = replies.run
    module._write_approvals = fake_write
    try:
        tools = {tool.name: tool for tool in module.build_skill_tools(config, library)}
        results: list[Any] = []
        infos: list[Any] = []
        for name, args in calls:
            tool = tools[name]
            expanded = _expand(args, home)
            results.append(tool.handler(expanded).to_dict())
            infos.append(asdict(tool.describe(expanded)))
    finally:
        module._python_interpreter = original_interpreter
        module.subprocess.run = original_run
        module._write_approvals = original_write

    final = approvals_path.read_text(encoding="utf-8") if approvals_path.is_file() else None
    out.add(
        f"tools_skills/call/{label}",
        "build_skill_tools",
        {
            "calls": [{"tool": name, "args": args} for name, args in calls],
            "allow_scripts": allow_scripts,
            "interpreter": interpreter,
            "approvals": approvals,
            "skills": "default",
            "config": "default",
        },
        _norm(
            {
                "results": results,
                "infos": infos,
                "approvals": final,
                "approvals_exists": approvals_path.is_file(),
                "calls": replies.calls,
            },
            home,
        ),
    )


LOAD_CASES: list[tuple[str, list[tuple[str, dict[str, Any]]]]] = [
    ("load-body", [("load_skill", {"name": "demo"})]),
    ("load-file", [("load_skill", {"name": "demo", "file": "references/guia.md"})]),
    ("load-file-backslash", [("load_skill", {"name": "demo", "file": "references\\guia.md"})]),
    ("load-file-blank", [("load_skill", {"name": "demo", "file": "   "})]),
    ("load-file-none", [("load_skill", {"name": "demo", "file": None})]),
    ("load-crlf", [("load_skill", {"name": "crlf"})]),
    ("load-crlf-file", [("load_skill", {"name": "demo", "file": "references/crlf.md"})]),
    ("load-big", [("load_skill", {"name": "grande"})]),
    ("load-big-file", [("load_skill", {"name": "demo", "file": "references/grande.md"})]),
    ("load-empty-body", [("load_skill", {"name": "vacia"})]),
    ("load-case-insensitive", [("load_skill", {"name": "DEMO"})]),
    ("load-spaced-name", [("load_skill", {"name": "  demo  "})]),
    ("load-missing-name", [("load_skill", {})]),
    ("load-blank-name", [("load_skill", {"name": "   "})]),
    ("load-non-text-name", [("load_skill", {"name": 5})]),
    ("load-non-text-file", [("load_skill", {"name": "demo", "file": 5})]),
    ("load-missing-skill", [("load_skill", {"name": "no-existe"})]),
    ("load-missing-file", [("load_skill", {"name": "demo", "file": "missing.md"})]),
    ("load-dot-dot", [("load_skill", {"name": "demo", "file": "../secret"})]),
    ("load-inner-dot-dot", [("load_skill", {"name": "demo", "file": "references/../scripts/ok.py"})]),
    ("load-dot-only", [("load_skill", {"name": "demo", "file": "."})]),
    ("load-absolute", [("load_skill", {"name": "demo", "file": "/etc/passwd"})]),
    ("load-backslash-absolute", [("load_skill", {"name": "demo", "file": "\\\\servidor\\x"})]),
    ("load-drive", [("load_skill", {"name": "demo", "file": "C:/x"})]),
    ("load-not-utf8", [("load_skill", {"name": "demo", "file": "assets/binario.dat"})]),
    ("load-no-scripts", [("load_skill", {"name": "sin-scripts"})]),
]

RUN_CASES: list[tuple[str, bool, str | None, str, bool, list[tuple[str, dict[str, Any]]]]] = [
    ("run-disabled", False, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-ok", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    (
        "run-args",
        True,
        INTERPRETER,
        "missing",
        False,
        [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": ["uno", "dos"]})],
    ),
    ("run-null-args", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": None})]),
    ("run-missing-skill-name", True, INTERPRETER, "missing", False, [("run_skill_script", {"script": "scripts/ok.py"})]),
    ("run-blank-skill-name", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "  ", "script": "scripts/ok.py"})]),
    ("run-missing-script-name", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo"})]),
    ("run-blank-script-name", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": " "})]),
    ("run-non-text-skill", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": 5, "script": "scripts/ok.py"})]),
    ("run-unknown-skill", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "no-existe", "script": "scripts/ok.py"})]),
    ("run-no-scripts-dir", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "sin-scripts", "script": "scripts/ok.py"})]),
    ("run-absolute", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "/scripts/ok.py"})]),
    ("run-drive", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "C:/x.py"})]),
    ("run-dot-dot", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/../scripts/ok.py"})]),
    ("run-outside-scripts", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "references/guia.md"})]),
    ("run-dot-only", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "."})]),
    ("run-missing-file", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/no.py"})]),
    ("run-not-python", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/note.txt"})]),
    ("run-uppercase", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/MAYUS.PY"})]),
    ("run-no-interpreter", True, None, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-failure", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/fail.py"})]),
    ("run-failure-no-stderr", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/rc3.py"})]),
    ("run-big-output", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/big.py"})]),
    ("run-timeout-bytes", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/slow.py"})]),
    ("run-timeout-text", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/slowstr.py"})]),
    ("run-oserror", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/boom.py"})]),
    ("run-empty-output", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/empty.py"})]),
    ("run-args-not-list", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": "x"})]),
    ("run-args-non-text", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": ["x", 5]})]),
    ("run-args-too-many", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": ["x"] * 65})]),
    ("run-arg-too-long", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": ["x" * 4001]})]),
    ("run-arg-null", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py", "args": ["a\x00b"]})]),
    ("run-approval-write", True, INTERPRETER, "missing", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-match", True, INTERPRETER, "match", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-stale", True, INTERPRETER, "stale", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-corrupt", True, INTERPRETER, "corrupt", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-non-object", True, INTERPRETER, "non-object", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-mixed", True, INTERPRETER, "mixed", False, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-approval-write-fails", True, INTERPRETER, "missing", True, [("run_skill_script", {"skill": "demo", "script": "scripts/ok.py"})]),
    ("run-describe", True, INTERPRETER, "missing", False, [("run_skill_script", {})]),
]


def _setup_case(out: FixtureFile, home: Path) -> None:
    entries: dict[str, Any] = {}
    for folder, spec in SKILLS.items():
        files: dict[str, Any] = {}
        if "raw" in spec:
            files["SKILL.md"] = spec["raw"]
        else:
            files["SKILL.md"] = f"---\n{spec['front']}---\n{spec.get('body', '')}"
        for relative, content in spec.get("files", {}).items():
            if isinstance(content, bytes):
                files[relative] = {"bytes_hex": content.hex()}
            else:
                files[relative] = content
        entries[folder] = files
    out.add(
        "tools_skills/setup/0001",
        "fixture_tree",
        {"skills": "default", "interpreter": INTERPRETER},
        {"skills": entries},
    )


def _specs(out: FixtureFile, home: Path) -> None:
    from ezagent.agent import tools_skills as module
    from ezagent.agent.skills import SkillLibrary

    config = _config(home, "specs", True)
    _write_skills(config.skills_dir)
    library = SkillLibrary(config)
    tools = []
    for tool in module.build_skill_tools(config, library):
        tools.append(
            {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "level": tool.level,
                "untrusted_output": tool.untrusted_output,
                "timeout_s": tool.timeout_s,
                "schema": tool.schema(),
                "info": _norm(asdict(tool.describe({})), home),
            }
        )
    out.add(
        "tools_skills/build_skill_tools/specs",
        "build_skill_tools",
        {"allow_scripts": True, "skills": "default", "config": "default"},
        {"tools": tools},
    )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    previous = logging.root.manager.disable
    logging.disable(logging.CRITICAL)
    try:
        _setup_case(out, home)
        _specs(out, home)
        for label, calls in LOAD_CASES:
            _run(out, home, label, calls=calls)
        for label, allow, interpreter, approvals, fail_write, calls in RUN_CASES:
            _run(
                out,
                home,
                label,
                allow_scripts=allow,
                interpreter=interpreter,
                approvals=approvals,
                fail_write=fail_write,
                calls=calls,
            )
    finally:
        logging.disable(previous)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
