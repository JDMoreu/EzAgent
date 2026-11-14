"""Fixtures de las herramientas de archivos (dominio `tools_fs`, F1-14).

Cubre `src/ezagent/agent/tools_fs.py` de punta a punta: los siete manejadores,
sus descripciones para la política, los efectos en disco y las entradas del
diario. Todo ocurre en el perfil temporal del oráculo.

Dobles deterministas:

- `send2trash` se sustituye por un movimiento a `<tmp>/trash/…` que registra la
  ruta enviada; el flujo de Python (comprobar existencia y llamar al envío) no
  cambia.
- `os.startfile` se sustituye por un registro de la ruta o URL.
- El reloj se congela en `2025-01-01T00:00:00+00:00` para que las copias
  `.YYYYmmdd-HHMMSS.bak` y las entradas del diario sean estables.
- Los identificadores del diario se normalizan a `<id>` y las rutas del perfil
  a `<tmp>`.
"""

from __future__ import annotations

import json
import os
import re
import shutil
from dataclasses import asdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tools_fs"

FIXED = datetime(2025, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
ENTRY_ID = re.compile(r"\d{20}-\d{8}-[0-9a-f]{12}")


class _FrozenDatetime(datetime):
    @classmethod
    def now(cls, tz: Any = None) -> datetime:  # noqa: D102 - parche del oráculo
        if tz is None:
            return FIXED.replace(tzinfo=None)
        return FIXED.astimezone(tz)

    def astimezone(self, tz: Any = None) -> datetime:  # noqa: D102
        if tz is None:
            return FIXED
        return super().astimezone(tz)


def _config(home: Path, label: str) -> Any:
    from ezagent.agent.config import (
        AgentConfig,
        ConfirmConfig,
        GeneralConfig,
        PathsConfig,
        SkillsConfig,
        ToolsDConfig,
        WebConfig,
    )

    work = home / "fs-cases" / label
    return AgentConfig(
        general=GeneralConfig(max_file_mb=0.001),
        paths=PathsConfig(
            read_roots=[str(work / "workspace"), str(home / "docs-root")],
            write_roots=[str(work / "workspace")],
            deny=[str(work / "workspace" / "secretos")],
            deny_globs=["**/.env", "**/*.pem"],
        ),
        confirm=ConfirmConfig(accept=[], reject=[]),
        web=WebConfig(),
        skills=SkillsConfig(),
        tools_d=ToolsDConfig(allowed_executables=[]),
        app_dir=work / "app",
        workspace=work / "workspace",
        permissions_path=work / "app" / "permissions.toml",
        system_md_path=work / "app" / "system.md",
        memory_dir=work / "app" / "memory",
        history_path=work / "app" / "memory" / "history.db",
        audit_path=work / "app" / "logs" / "audit.jsonl",
        journal_dir=work / "app" / "journal",
        skills_dir=work / "app" / "skills",
        global_skills_dir=work / "app" / "global-skills",
        tools_d_dir=work / "app" / "tools.d",
    )


def _write(files: dict[str, str | None], workspace: Path, home: Path) -> None:
    for relative, content in sorted(files.items()):
        target = home / relative[len("<tmp>/") :] if relative.startswith("<tmp>/") else workspace / relative
        if content is None:
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content.encode("utf-8"))


def _norm_text(value: str, home: Path) -> str:
    text = ENTRY_ID.sub("<id>", value)
    for prefix in (str(home), home.as_posix()):
        text = text.replace(prefix, "<tmp>")
    return text.replace("\\", "/")


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        return _norm_text(value, home)
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


def _tree(root: Path) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    if not root.is_dir():
        return items
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative == "app" or relative.startswith("app/"):
            continue
        if path.is_dir():
            items.append({"path": relative, "content": None})
        else:
            items.append(
                {
                    "path": relative,
                    "content": path.read_bytes().decode("utf-8", errors="replace"),
                }
            )
    return items


def _journal_entries(directory: Path) -> tuple[list[dict[str, Any]], list[str]]:
    entries: list[dict[str, Any]] = []
    names: list[str] = []
    if not directory.is_dir():
        return entries, names
    for path in sorted(directory.glob("*.json")):
        names.append(ENTRY_ID.sub("<id>", path.name))
        try:
            entries.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            entries.append({"unreadable": True})
    return entries, names


def _info_dict(info: Any) -> dict[str, Any]:
    """`asdict(ToolInfo)` más la marca de `_FolderBulk` (umbral forzado)."""
    value = asdict(info)
    value["bulk_forced"] = type(info.bulk).__name__ == "_FolderBulk"
    return value


class _Recorder:
    """Dobles de Papelera y de apertura del sistema, con registro."""

    def __init__(self, home: Path, label: str) -> None:
        self.home = home
        self.trash_dir = home / "trash" / label
        self.trashed: list[str] = []
        self.opened: list[str] = []

    def trash(self, path: Path) -> None:
        self.trashed.append(str(path))
        self.trash_dir.mkdir(parents=True, exist_ok=True)
        target = self.trash_dir / f"{len(self.trashed):03d}-{path.name}"
        shutil.move(str(path), str(target))

    def open(self, target: str) -> None:
        self.opened.append(target)


def _run(out: FixtureFile, home: Path, label: str, files: dict[str, str | None], calls: list[tuple[str, dict[str, Any]]]) -> None:
    from ezagent.agent import journal as journal_module
    from ezagent.agent import tools_fs as module
    from ezagent.agent.journal import Journal

    config = _config(home, label)
    _write(files, config.workspace, home)
    journal = Journal(config.journal_dir)
    recorder = _Recorder(home, label)
    tools = {tool.name: tool for tool in module.build_fs_tools(config, journal)}

    original_tools_trash = module._send_to_trash
    original_journal_trash = journal_module._send_to_trash
    original_startfile = module._startfile
    module._send_to_trash = recorder.trash
    journal_module._send_to_trash = recorder.trash
    module._startfile = recorder.open
    try:
        results: list[Any] = []
        infos: list[Any] = []
        for name, args in calls:
            tool = tools[name]
            expanded = _expand(args, home)
            # La política describe el estado previo: se evalúa antes del manejador.
            infos.append(_info_dict(tool.describe(expanded)))
            results.append(tool.handler(expanded).to_dict())
    finally:
        module._send_to_trash = original_tools_trash
        journal_module._send_to_trash = original_journal_trash
        module._startfile = original_startfile

    entries, names = _journal_entries(config.journal_dir)
    out.add(
        f"tools_fs/call/{label}",
        "build_fs_tools",
        {
            "calls": [{"tool": name, "args": args} for name, args in calls],
            "files": files,
            "config": "default",
        },
        _norm(
            {
                "results": results,
                "infos": infos,
                "tree": _tree(config.app_dir.parent),
                "journal": entries,
                "journal_files": names,
                "trashed": recorder.trashed,
                "opened": recorder.opened,
            },
            home,
        ),
    )


BASE: dict[str, str | None] = {
    "docs": None,
    "docs/a.txt": "hola",
    "docs/sub": None,
    "docs/sub/b.txt": "bb",
    "src.txt": "origen",
    "dst.txt": "destino",
    "dst.txt.bak": "respaldo",
    "dst.txt.20250101-000000.bak": "respaldo dos",
    "carpeta": None,
    "carpeta/uno.txt": "1",
    "carpeta/dos.txt": "2",
    "carpeta/hijo": None,
    "carpeta/hijo/tres.txt": "3",
    "vacía": None,
    "secretos": None,
    "secretos/x.txt": "x",
    ".env": "SECRETO=1\n",
    "clave.pem": "-----BEGIN-----\n",
    "ñandú.txt": "ñ\n",
    "comillas'raras.txt": "x",
    "sub": None,
    "sub/anidado": None,
}

LIMIT_BASE = {**BASE, "grande.txt": "z" * 1048, "demasiado.txt": "z" * 1049}

CASES: list[tuple[str, dict[str, str | None], list[tuple[str, dict[str, Any]]]]] = [
    # ---------------------------------------------------------- create_dir
    ("create-dir-new", BASE, [("create_dir", {"path": "nueva"})]),
    ("create-dir-nested", BASE, [("create_dir", {"path": "docs/nueva"})]),
    ("create-dir-exists", BASE, [("create_dir", {"path": "docs"})]),
    ("create-dir-file", BASE, [("create_dir", {"path": "src.txt"})]),
    ("create-dir-missing-parent", BASE, [("create_dir", {"path": "no/existe"})]),
    ("create-dir-root", BASE, [("create_dir", {"path": "<tmp>/fs-cases/create-dir-root/workspace"})]),
    ("create-dir-outside", BASE, [("create_dir", {"path": ".."})]),
    ("create-dir-denied", BASE, [("create_dir", {"path": "secretos/nueva"})]),
    ("create-dir-glob", BASE, [("create_dir", {"path": ".env"})]),
    ("create-dir-empty", BASE, [("create_dir", {"path": ""})]),
    ("create-dir-blank", BASE, [("create_dir", {"path": "   "})]),
    ("create-dir-non-text", BASE, [("create_dir", {"path": 5})]),
    ("create-dir-reserved", BASE, [("create_dir", {"path": "CON"})]),
    ("create-dir-unc", BASE, [("create_dir", {"path": "\\\\servidor\\share"})]),
    ("create-dir-unicode", BASE, [("create_dir", {"path": "ñandú"})]),
    ("create-dir-deep", BASE, [("create_dir", {"path": "sub/anidado/nuevo"})]),
    # ---------------------------------------------------------- write_file
    ("write-new", BASE, [("write_file", {"path": "nuevo.txt", "content": "contenido"})]),
    (
        "write-new-parent",
        BASE,
        [("write_file", {"path": "nueva/carpeta/archivo.txt", "content": "hola"})],
    ),
    ("write-empty-content", BASE, [("write_file", {"path": "vacío.txt", "content": ""})]),
    ("write-exists", BASE, [("write_file", {"path": "src.txt", "content": "nuevo"})]),
    (
        "write-overwrite",
        BASE,
        [("write_file", {"path": "src.txt", "content": "nuevo", "overwrite": True})],
    ),
    (
        "write-overwrite-bak",
        {**BASE, "src.txt.bak": "viejo"},
        [("write_file", {"path": "src.txt", "content": "nuevo", "overwrite": True})],
    ),
    (
        "write-overwrite-stamped",
        BASE,
        [("write_file", {"path": "dst.txt", "content": "nuevo", "overwrite": True})],
    ),
    (
        "write-directory",
        BASE,
        [("write_file", {"path": "docs", "content": "x", "overwrite": True})],
    ),
    ("write-missing-path", BASE, [("write_file", {"content": "x"})]),
    ("write-content-non-text", BASE, [("write_file", {"path": "x.txt", "content": 5})]),
    (
        "write-overwrite-non-bool",
        BASE,
        [("write_file", {"path": "x.txt", "content": "x", "overwrite": "true"})],
    ),
    ("write-root", BASE, [("write_file", {"path": "<tmp>/fs-cases/write-root/workspace", "content": "x"})]),
    ("write-outside", BASE, [("write_file", {"path": "..", "content": "x"})]),
    ("write-denied", BASE, [("write_file", {"path": "secretos/x.txt", "content": "x"})]),
    ("write-glob", BASE, [("write_file", {"path": ".env", "content": "x"})]),
    ("write-limit-exact", LIMIT_BASE, [("write_file", {"path": "grande.txt", "content": "z" * 1048})]),
    ("write-limit-over", LIMIT_BASE, [("write_file", {"path": "grande2.txt", "content": "z" * 1049})]),
    (
        "write-unicode",
        BASE,
        [("write_file", {"path": "ñandú/emoji-🙂.txt", "content": "ñandú 🙂\n"})],
    ),
    (
        "write-crlf",
        BASE,
        [("write_file", {"path": "crlf.txt", "content": "uno\r\ndos\r\n"})],
    ),
    ("write-quotes", BASE, [("write_file", {"path": "comillas'raras.txt", "content": "x"})]),
    ("write-reserved", BASE, [("write_file", {"path": "CON", "content": "x"})]),
    # ----------------------------------------------------------- copy_path
    ("copy-file", BASE, [("copy_path", {"src": "src.txt", "dst": "copia.txt"})]),
    ("copy-file-overwrite", BASE, [("copy_path", {"src": "src.txt", "dst": "dst.txt"})]),
    ("copy-folder", BASE, [("copy_path", {"src": "carpeta", "dst": "carpeta-copia"})]),
    ("copy-folder-overwrite", BASE, [("copy_path", {"src": "carpeta", "dst": "docs"})]),
    ("copy-missing", BASE, [("copy_path", {"src": "no.txt", "dst": "copia.txt"})]),
    ("copy-same", BASE, [("copy_path", {"src": "src.txt", "dst": "src.txt"})]),
    ("copy-same-case", BASE, [("copy_path", {"src": "src.txt", "dst": "SRC.TXT"})]),
    ("copy-into-itself", BASE, [("copy_path", {"src": "carpeta", "dst": "carpeta/hijo"})]),
    ("copy-type-mismatch", BASE, [("copy_path", {"src": "src.txt", "dst": "docs"})]),
    (
        "copy-dst-contains-src",
        {"carpeta": None, "carpeta/src": None, "carpeta/src/a.txt": "a"},
        [("copy_path", {"src": "carpeta/src", "dst": "carpeta"})],
    ),
    ("copy-missing-parent", BASE, [("copy_path", {"src": "src.txt", "dst": "no/hay.txt"})]),
    ("copy-root-dst", BASE, [("copy_path", {"src": "src.txt", "dst": "<tmp>/fs-cases/copy-root-dst/workspace"})]),
    ("copy-outside-src", BASE, [("copy_path", {"src": "..", "dst": "copia"})]),
    ("copy-denied-dst", BASE, [("copy_path", {"src": "src.txt", "dst": "secretos/x.txt"})]),
    ("copy-empty-src", BASE, [("copy_path", {"src": "", "dst": "x"})]),
    ("copy-unicode", BASE, [("copy_path", {"src": "ñandú.txt", "dst": "ñandú-2.txt"})]),
    # ----------------------------------------------------------- move_path
    ("move-file", BASE, [("move_path", {"src": "src.txt", "dst": "mudado.txt"})]),
    ("move-file-overwrite", BASE, [("move_path", {"src": "src.txt", "dst": "dst.txt"})]),
    ("move-folder", BASE, [("move_path", {"src": "carpeta", "dst": "carpeta-2"})]),
    ("move-folder-overwrite", BASE, [("move_path", {"src": "carpeta", "dst": "docs"})]),
    ("move-missing", BASE, [("move_path", {"src": "no.txt", "dst": "x.txt"})]),
    ("move-same", BASE, [("move_path", {"src": "src.txt", "dst": "src.txt"})]),
    ("move-into-itself", BASE, [("move_path", {"src": "carpeta", "dst": "carpeta/hijo"})]),
    ("move-type-mismatch", BASE, [("move_path", {"src": "src.txt", "dst": "docs"})]),
    ("move-missing-parent", BASE, [("move_path", {"src": "src.txt", "dst": "no/hay.txt"})]),
    ("move-root-src", BASE, [("move_path", {"src": "<tmp>/fs-cases/move-root-src/workspace", "dst": "x"})]),
    ("move-root-dst", BASE, [("move_path", {"src": "src.txt", "dst": "<tmp>/fs-cases/move-root-dst/workspace"})]),
    ("move-denied", BASE, [("move_path", {"src": "secretos/x.txt", "dst": "x.txt"})]),
    # --------------------------------------------------------- delete_path
    ("delete-file", BASE, [("delete_path", {"path": "src.txt"})]),
    ("delete-empty-folder", BASE, [("delete_path", {"path": "vacía"})]),
    ("delete-folder", BASE, [("delete_path", {"path": "carpeta"})]),
    ("delete-missing", BASE, [("delete_path", {"path": "no.txt"})]),
    ("delete-root", BASE, [("delete_path", {"path": "<tmp>/fs-cases/delete-root/workspace"})]),
    ("delete-outside", BASE, [("delete_path", {"path": ".."})]),
    ("delete-denied", BASE, [("delete_path", {"path": "secretos/x.txt"})]),
    ("delete-glob", BASE, [("delete_path", {"path": ".env"})]),
    # ----------------------------------------------------------- open_path
    ("open-url", BASE, [("open_path", {"path_or_url": "https://example.com/ruta?x=1"})]),
    ("open-url-http", BASE, [("open_path", {"path_or_url": "http://example.com"})]),
    ("open-url-upper", BASE, [("open_path", {"path_or_url": "HTTPS://EXAMPLE.COM/A"})]),
    ("open-url-no-host", BASE, [("open_path", {"path_or_url": "http://"})]),
    ("open-url-credentials", BASE, [("open_path", {"path_or_url": "https://user:pass@example.com/x"})]),
    ("open-ftp", BASE, [("open_path", {"path_or_url": "ftp://example.com/x"})]),
    ("open-file", BASE, [("open_path", {"path_or_url": "src.txt"})]),
    ("open-missing", BASE, [("open_path", {"path_or_url": "no.txt"})]),
    ("open-folder", BASE, [("open_path", {"path_or_url": "docs"})]),
    ("open-outside", BASE, [("open_path", {"path_or_url": ".."})]),
    ("open-javascript", BASE, [("open_path", {"path_or_url": "javascript:alert(1)"})]),
    ("open-empty", BASE, [("open_path", {"path_or_url": "   "})]),
    ("open-non-text", BASE, [("open_path", {"path_or_url": 5})]),
    ("open-absolute", BASE, [("open_path", {"path_or_url": "<tmp>/docs-root/x.txt"})]),
    # ------------------------------------------------------------ undo_last
    ("undo-empty", BASE, [("undo_last", {})]),
    (
        "undo-create-file",
        BASE,
        [
            ("write_file", {"path": "nuevo.txt", "content": "x"}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-create-dir",
        BASE,
        [("create_dir", {"path": "nueva"}), ("undo_last", {})],
    ),
    (
        "undo-copy",
        BASE,
        [
            ("copy_path", {"src": "src.txt", "dst": "copia.txt"}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-copy-overwrite",
        BASE,
        [
            ("copy_path", {"src": "src.txt", "dst": "dst.txt"}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-move",
        BASE,
        [
            ("move_path", {"src": "src.txt", "dst": "mudado.txt"}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-move-overwrite",
        BASE,
        [
            ("move_path", {"src": "src.txt", "dst": "dst.txt"}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-overwrite",
        BASE,
        [
            ("write_file", {"path": "src.txt", "content": "nuevo", "overwrite": True}),
            ("undo_last", {}),
        ],
    ),
    (
        "undo-n-two",
        BASE,
        [
            ("write_file", {"path": "a.txt", "content": "a"}),
            ("write_file", {"path": "b.txt", "content": "b"}),
            ("undo_last", {"n": 2}),
        ],
    ),
    (
        "undo-more-than-entries",
        BASE,
        [("write_file", {"path": "a.txt", "content": "a"}), ("undo_last", {"n": 10})],
    ),
    (
        "undo-four-times",
        BASE,
        [
            ("write_file", {"path": "a.txt", "content": "a"}),
            ("undo_last", {}),
            ("undo_last", {}),
            ("undo_last", {}),
        ],
    ),
    ("undo-n-zero", BASE, [("undo_last", {"n": 0})]),
    ("undo-n-negative", BASE, [("undo_last", {"n": -2})]),
    ("undo-n-bool", BASE, [("undo_last", {"n": True})]),
    ("undo-n-text", BASE, [("undo_last", {"n": "2"})]),
    ("undo-n-float", BASE, [("undo_last", {"n": 1.5})]),
    ("undo-n-null", BASE, [("undo_last", {"n": None})]),
    # ------------------------------------------------- documents outside home
    (
        "read-outside-workspace",
        {"<tmp>/docs-root/externo.txt": "externo"},
        [("open_path", {"path_or_url": "<tmp>/docs-root/externo.txt"})],
    ),
    (
        "write-denied-parent",
        BASE,
        [("write_file", {"path": "docs/.env", "content": "x"})],
    ),
    (
        "copy-folder-empty",
        {"v": None},
        [("copy_path", {"src": "v", "dst": "v-copia"})],
    ),
    (
        "move-folder-empty",
        {"v": None},
        [("move_path", {"src": "v", "dst": "v-2"})],
    ),
]


def _count_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.agent import tools_fs as module

    root = home / "count-cases"
    many = root / "many"
    many.mkdir(parents=True, exist_ok=True)
    for index in range(5001):
        (many / f"f{index:04d}.txt").touch()
    tree = root / "tree"
    (tree / "uno").mkdir(parents=True)
    (tree / "dos").mkdir()
    (tree / "a.txt").touch()
    (tree / "uno" / "b.txt").touch()
    (tree / "dos" / "c.txt").touch()
    single = root / "single.txt"
    single.touch()
    for label, path, entries in [
        ("limit", many, 5001),
        ("tree", tree, 3),
        ("file", single, 1),
        ("missing", root / "no-existe", 0),
    ]:
        out.add(
            f"tools_fs/_count_items/{label}",
            "_count_items",
            {"entries": entries},
            {"count": module._count_items(path)},
        )
    for count in (0, 1, 2, 4999, 5000, 6000):
        out.add(
            f"tools_fs/_count_phrase/{count}",
            "_count_phrase",
            {"count": count},
            {"text": module._count_phrase(count)},
        )


def _describe_cases(out: FixtureFile, home: Path) -> None:
    """Descripciones para la política en estados que un caso normal no crea."""
    from ezagent.agent import tools_fs as module
    from ezagent.agent.journal import Journal

    entries: list[dict[str, Any]] = []
    for name, args in [
        ("create_dir", {"path": "nueva"}),
            ("create_dir", {"path": "docs"}),
            ("create_dir", {"path": "src.txt"}),
            ("create_dir", {"path": "secretos/x"}),
            ("create_dir", {"path": ""}),
            ("write_file", {"path": "nueva/carpeta/a.txt", "content": "ñandú"}),
            ("write_file", {"path": "docs/a.txt", "content": "x"}),
            ("write_file", {"path": "src.txt", "content": "x"}),
            ("write_file", {"path": "src.txt", "content": "x", "overwrite": True}),
            ("write_file", {"path": "dst.txt", "content": "x", "overwrite": True}),
            ("write_file", {"path": "docs", "content": "x"}),
            ("write_file", {"path": "secretos/x.txt", "content": "x"}),
            ("copy_path", {"src": "src.txt", "dst": "copia.txt"}),
            ("copy_path", {"src": "src.txt", "dst": "dst.txt"}),
            ("copy_path", {"src": "carpeta", "dst": "carpeta-2"}),
            ("copy_path", {"src": "vacía", "dst": "vacía-2"}),
            ("copy_path", {"src": "no.txt", "dst": "x.txt"}),
            ("copy_path", {"src": "src.txt", "dst": "src.txt"}),
            ("copy_path", {"src": "carpeta", "dst": "carpeta/hijo"}),
            ("copy_path", {"src": "carpeta", "dst": "docs"}),
            ("copy_path", {"src": "secretos/x.txt", "dst": "x"}),
            ("move_path", {"src": "src.txt", "dst": "dst.txt"}),
            ("move_path", {"src": "carpeta", "dst": "docs"}),
            ("move_path", {"src": "no.txt", "dst": "x"}),
            ("move_path", {"src": "secretos/x.txt", "dst": "x"}),
            ("delete_path", {"path": "src.txt"}),
            ("delete_path", {"path": "vacía"}),
            ("delete_path", {"path": "carpeta"}),
            ("delete_path", {"path": "no.txt"}),
            ("delete_path", {"path": "<tmp>/fs-cases/describe/workspace"}),
            ("delete_path", {"path": "secretos/x.txt"}),
            ("open_path", {"path_or_url": "https://example.com/x"}),
            ("open_path", {"path_or_url": "src.txt"}),
            ("open_path", {"path_or_url": "docs"}),
            ("open_path", {"path_or_url": "no.txt"}),
            ("open_path", {"path_or_url": "ftp://x/y"}),
            ("open_path", {"path_or_url": "secretos/x.txt"}),
            ("undo_last", {}),
            ("undo_last", {"n": 3}),
            ("undo_last", {"n": 0}),
            ("undo_last", {"n": "x"}),
        ]:
        config = _config(home, "describe")
        _write(BASE, config.workspace, home)
        journal = Journal(config.journal_dir)
        tool = next(item for item in module.build_fs_tools(config, journal) if item.name == name)
        expanded = _expand(args, home)
        entries.append(
            {
                "tool": name,
                "args": args,
                "info": _info_dict(tool.describe(expanded)),
            }
        )
    out.add(
        "tools_fs/describe/0001",
        "describe",
        {"files": BASE, "config": "default"},
        _norm({"cases": entries}, home),
    )


def _specs(out: FixtureFile, home: Path) -> None:
    from ezagent.agent import tools_fs as module
    from ezagent.agent.journal import Journal

    config = _config(home, "specs")
    journal = Journal(config.journal_dir)
    tools = []
    for tool in module.build_fs_tools(config, journal):
        tools.append(
            {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "level": tool.level,
                "untrusted_output": tool.untrusted_output,
                "timeout_s": tool.timeout_s,
                "schema": tool.schema(),
                "info": _norm(_info_dict(tool.describe({})), home),
            }
        )
    out.add(
        "tools_fs/build_fs_tools/specs",
        "build_fs_tools",
        {"config": "default"},
        {"tools": tools},
    )


def generate(out: FixtureFile) -> None:
    from ezagent.agent import journal as journal_module
    from ezagent.agent import tools_fs as module

    home = Path(os.environ["HOME"])
    original_tools = module.datetime
    original_journal = journal_module.datetime
    original_ns = journal_module.time.time_ns
    counter = [1_735_689_600_000_000_000]

    def clock_ns() -> int:
        counter[0] += 1_000_000
        return counter[0]

    module.datetime = _FrozenDatetime
    journal_module.datetime = _FrozenDatetime
    journal_module.time.time_ns = clock_ns
    try:
        _specs(out, home)
        _count_cases(out, home)
        _describe_cases(out, home)
        for label, files, calls in CASES:
            _run(out, home, label, files, calls)
    finally:
        module.datetime = original_tools
        journal_module.datetime = original_journal
        journal_module.time.time_ns = original_ns


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
