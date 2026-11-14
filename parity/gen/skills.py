"""Fixtures de skills locales (dominio `skills`, F1-04).

Fija el descubrimiento (frontmatter válido, inválido y vacío), el catálogo que
ve el LLM, la carga del cuerpo y de archivos auxiliares, las colisiones entre
carpetas y el hash de carpeta. Todo vive en el HOME temporal del oráculo.
"""

from __future__ import annotations

import base64
import os
import pathlib
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "skills"

LONG = 120 * "g"

LOCAL_SKILLS: list[dict[str, Any]] = [
    {
        "folder": "pdf",
        "front": "name: pdf\ndescription: \"Procesa documentos PDF: extrae texto, une archivos y rellena formularios.\"\n",
        "body": "Guía de la skill.\n\nPaso 1.\nPaso 2.\n",
        "files": {
            "scripts/run.py": "print('hola')\n",
            "references/guide.md": "# Guía\n\nDetalle.\n",
            "assets/logo.txt": "logo\n",
            "assets/binary.dat": {"__b64__": base64.b64encode(b"\xff\xfe roto").decode()},
            "scripts/big.txt": "z" * 20010,
        },
    },
    {
        "folder": "web-search",
        "front": "name: web-search\ndescription: >\n  Busca en la web\n  y resume resultados.\n",
        "body": "Abre el navegador y busca.\n",
    },
    {
        "folder": "no-name",
        "front": "description: Skill sin nombre declarado.\n",
        "body": "Cuerpo de la skill sin name.\n",
    },
    {
        "folder": "mismatch",
        "front": "name: otro-nombre\ndescription: El name no coincide con la carpeta.\n",
        "body": "x\n",
    },
    {
        "folder": "BadName",
        "front": "description: Carpeta con nombre inválido.\n",
        "body": "x\n",
    },
    {
        "folder": "long-desc",
        "front": f"name: long-desc\ndescription: {'d' * 1030}\n",
        "body": "x\n",
    },
    {
        "folder": "catalog-clip",
        "front": f"name: catalog-clip\ndescription: {LONG}\n",
        "body": "x\n",
    },
    {
        "folder": "crlf",
        "front": "name: crlf\ndescription: Skill editada en Windows.\n",
        "raw": "---\r\nname: crlf\r\ndescription: Skill editada en Windows.\r\n---\r\nLínea uno.\r\nLínea dos.\r\n",
    },
    {
        "folder": "broken-yaml",
        "front": "name: [unclosed\ndescription: roto\n",
        "body": "x\n",
    },
    {
        "folder": "not-a-map",
        "front": "- uno\n- dos\n",
        "body": "x\n",
    },
    {
        "folder": "empty-front",
        "front": "",
        "body": "x\n",
    },
    {"folder": "no-front", "raw": "# Sin frontmatter\n\nTexto.\n"},
    {"folder": "no-desc", "front": "name: no-desc\n", "body": "x\n"},
    {
        "folder": "bad-utf8",
        "raw_bytes": b"---\nname: bad-utf8\n---\n\xff\xfe roto",
    },
    {"folder": "_proposed", "front": "name: _proposed\ndescription: No revisada.\n", "body": "x\n"},
    {"folder": ".hidden", "front": "name: .hidden\ndescription: Oculta.\n", "body": "x\n"},
    {"folder": "empty-folder"},
]

GLOBAL_SKILLS: list[dict[str, Any]] = [
    {
        "folder": "global-one",
        "front": "name: global-one\ndescription: Skill global del perfil.\n",
        "body": "global\n",
    },
    {
        "folder": "pdf",
        "front": "name: pdf\ndescription: Copia global que debe perder.\n",
        "body": "global\n",
    },
]


def _config(root: pathlib.Path, name: str, local: pathlib.Path, global_dir: pathlib.Path) -> Any:
    from ezagent.agent.config import (
        AgentConfig,
        ConfirmConfig,
        GeneralConfig,
        PathsConfig,
        SkillsConfig,
        ToolsDConfig,
        WebConfig,
    )

    app = root / name
    return AgentConfig(
        general=GeneralConfig(),
        paths=PathsConfig(read_roots=[], write_roots=[], deny=[], deny_globs=[]),
        confirm=ConfirmConfig(accept=[], reject=[]),
        web=WebConfig(),
        skills=SkillsConfig(),
        tools_d=ToolsDConfig(allowed_executables=[]),
        app_dir=app,
        workspace=app / "workspace",
        permissions_path=app / "permissions.toml",
        system_md_path=app / "system.md",
        memory_dir=app / "memory",
        history_path=app / "history" / "history.db",
        audit_path=app / "logs" / "audit.jsonl",
        journal_dir=app / "journal",
        skills_dir=local,
        global_skills_dir=global_dir,
        tools_d_dir=app / "tools.d",
    )


def _write_skill(folder: pathlib.Path, spec: dict[str, Any]) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    if "raw_bytes" in spec:
        (folder / "SKILL.md").write_bytes(spec["raw_bytes"])
    elif "raw" in spec:
        (folder / "SKILL.md").write_text(spec["raw"], encoding="utf-8", newline="")
    elif "front" in spec:
        text = f"---\n{spec['front']}---\n{spec.get('body', '')}"
        (folder / "SKILL.md").write_text(text, encoding="utf-8", newline="\n")
    for relative, content in spec.get("files", {}).items():
        target = folder / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, dict):
            target.write_bytes(base64.b64decode(content["__b64__"]))
        else:
            target.write_text(content, encoding="utf-8", newline="\n")


def _write_tree(base: pathlib.Path, specs: list[dict[str, Any]]) -> None:
    base.mkdir(parents=True, exist_ok=True)
    for spec in specs:
        _write_skill(base / spec["folder"], spec)


def _norm_text(text: str, root: pathlib.Path) -> str:
    return text.replace(str(root), "<root>").replace("\\", "/")


def _norm(value: Any, root: pathlib.Path) -> Any:
    if isinstance(value, str):
        return _norm_text(value, root)
    if isinstance(value, dict):
        return {key: _norm(item, root) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, root) for item in value]
    if isinstance(value, pathlib.Path):
        return _norm_text(str(value), root)
    return value


def _skill_info(skill: Any, root: pathlib.Path) -> dict[str, Any]:
    return {
        "name": skill.name,
        "description": skill.description,
        "body": skill.body,
        "files": list(skill.files),
        "scripts": list(skill.scripts),
        "warnings": list(skill.warnings),
        "declared_name": skill.declared_name,
        "source": skill.source,
        "path": _norm_text(str(skill.path), root),
    }


def _content_info(content: Any, root: pathlib.Path) -> dict[str, Any]:
    return {
        "name": content.name,
        "text": content.text,
        "files": list(content.files),
        "path": _norm_text(content.path, root),
        "file": content.file,
        "truncated": content.truncated,
    }


def _catalog_entries(out: FixtureFile, library: Any, root: pathlib.Path, audit: list[str]) -> None:
    out.add(
        "skills/catalog/names",
        "names",
        {"local": "local/*", "global": "global/*"},
        {"names": library.names()},
    )
    out.add("skills/catalog/text", "catalog_text", {}, {"text": library.catalog_text()})
    out.add(
        "skills/catalog/diagnostics",
        "warnings",
        {},
        {
            "warnings": _norm(library.warnings, root),
            "collisions": _norm(library.collisions, root),
            "audit": _norm(audit, root),
        },
    )
    out.add(
        "skills/catalog/keywords",
        "keywords",
        {},
        {"keywords": sorted(library.keywords())},
    )
    out.add(
        "skills/catalog/skills",
        "all",
        {},
        {"skills": [_skill_info(skill, root) for skill in library.all()]},
    )
    for index, name in enumerate(["pdf", "PDF", "web-search", "no-existe", 5], start=1):
        skill = library.get(name)
        out.add(
            f"skills/catalog/get/{index:03d}",
            "get",
            {"name": name},
            {"skill": None if skill is None else _skill_info(skill, root)},
        )


def _load_entries(out: FixtureFile, library: Any, root: pathlib.Path) -> None:
    loads = [
        ("pdf", None),
        ("pdf", "scripts/run.py"),
        ("pdf", "references\\guide.md"),
        ("pdf", "   "),
        ("crlf", None),
        ("web-search", None),
        ("pdf", "scripts/big.txt"),
    ]
    for index, (name, file) in enumerate(loads, start=1):
        content = library.load_skill(name, file)
        out.add(
            f"skills/load/{index:03d}",
            "load_skill",
            {"name": name, "file": file},
            {"content": _content_info(content, root)},
        )


def _error_entries(out: FixtureFile, library: Any, root: pathlib.Path, SkillError: Any) -> None:
    loads = [
        ("skills/load-error/missing-skill", "load_skill", "nope", None),
        ("skills/load-error/dot-dot", "load_skill", "pdf", "../secret"),
        ("skills/load-error/inner-dot-dot", "load_skill", "pdf", "references/../scripts/run.py"),
        ("skills/load-error/dot-only", "load_skill", "pdf", "."),
        ("skills/load-error/absolute", "load_skill", "pdf", "/etc/passwd"),
        ("skills/load-error/backslash-absolute", "load_skill", "pdf", "\\references\\guide.md"),
        ("skills/load-error/drive", "load_skill", "pdf", "C:/x"),
        ("skills/load-error/missing-file", "load_skill", "pdf", "missing.md"),
        ("skills/load-error/not-utf8", "load_skill", "pdf", "assets/binary.dat"),
        ("skills/load-error/folder-hash", "folder_hash", "no-existe", None),
    ]
    for case, fn_name, name, file in loads:
        try:
            if fn_name == "folder_hash":
                output = {"hash": library.folder_hash(name)}
            else:
                output = {"content": _content_info(library.load_skill(name, file), root)}
        except SkillError as error:
            out.add_error(case, fn_name, {"name": name, "file": file}, _norm_text(str(error), root))
        else:
            out.add(case, fn_name, {"name": name, "file": file}, output)


def _hash_entries(out: FixtureFile, library: Any, root: pathlib.Path, hash_folder: Any) -> None:
    out.add(
        "skills/hash/installed",
        "folder_hash",
        {"name": "pdf"},
        {"hash": library.folder_hash("pdf")},
    )
    out.add(
        "skills/hash/folder",
        "hash_folder",
        {"folder": "<root>/local/pdf"},
        {"hash": hash_folder(root / "local" / "pdf")},
    )
    empty = root / "empty-hash"
    empty.mkdir(parents=True, exist_ok=True)
    out.add(
        "skills/hash/empty",
        "hash_folder",
        {"folder": "<root>/empty-hash"},
        {"hash": hash_folder(empty)},
    )


def _tokens_entries(out: FixtureFile, skill_tokens: Any) -> None:
    samples = [
        "Páginas web y PDF",
        "correos, correo",
        "",
        "VALE: un_2 y guiones-rotos",
        "Cómo hacer una página",
    ]
    for index, text in enumerate(samples, start=1):
        out.add(
            f"skills/tokens/{index:03d}",
            "skill_tokens",
            {"text": text},
            {"tokens": sorted(skill_tokens(text))},
        )


def _ensure_entries(out: FixtureFile, ensure: Any, root: pathlib.Path) -> None:
    source = root / "bundled"
    _write_tree(
        source,
        [
            {"folder": "alpha", "front": "name: alpha\ndescription: Primera.\n", "body": "a\n"},
            {"folder": "beta", "front": "name: beta\ndescription: Segunda.\n", "body": "b\n"},
            {"folder": "_skip", "front": "name: _skip\ndescription: Oculta.\n", "body": "x\n"},
            {"folder": ".hidden", "front": "name: .hidden\ndescription: Oculta.\n", "body": "x\n"},
            {"folder": "plain"},
        ],
    )
    target = root / "ensure-target"
    config = _config(root, "ensure-app", target, root / "unused-global")
    first = ensure(config, source_dir=source)
    second = ensure(config, source_dir=source)
    out.add(
        "skills/ensure/copies-once",
        "ensure_default_skills",
        {"source": "<root>/bundled", "target": "<root>/ensure-target"},
        {
            "first": sorted(first),
            "second": sorted(second),
            "installed": sorted(item.name for item in target.iterdir()),
        },
    )
    out.add(
        "skills/ensure/missing-source",
        "ensure_default_skills",
        {"source": "<root>/no-existe"},
        {"names": ensure(config, source_dir=root / "no-existe")},
    )


def _max_catalog_entries(out: FixtureFile, SkillLibrary: Any, root: pathlib.Path) -> None:
    local = root / "max-local"
    specs = [
        {"folder": name, "front": f"name: {name}\ndescription: Skill {name}.\n", "body": "x\n"}
        for name in ("alfa", "beta", "gamma", "delta")
    ]
    _write_tree(local, specs)
    config = _config(root, "max-app", local, root / "max-global")
    for limit in (3, 0):
        library = SkillLibrary(config, max_catalog=limit)
        out.add(
            f"skills/max-catalog/{limit}",
            "catalog_text",
            {"max_catalog": limit, "skills": 4},
            {
                "text": library.catalog_text(),
                "warnings": _norm(library.warnings, root),
            },
        )


def _refresh_entries(out: FixtureFile, SkillLibrary: Any, root: pathlib.Path) -> None:
    local = root / "refresh-local"
    _write_tree(
        local,
        [{"folder": "uno", "front": "name: uno\ndescription: Primera.\n", "body": "x\n"}],
    )
    config = _config(root, "refresh-app", local, root / "refresh-global")
    library = SkillLibrary(config)
    before = library.names()
    same = library.refresh()
    _write_tree(
        local,
        [{"folder": "dos", "front": "name: dos\ndescription: Segunda.\n", "body": "x\n"}],
    )
    changed = library.refresh()
    out.add(
        "skills/refresh/detects-changes",
        "refresh",
        {"skills": ["uno", "dos"]},
        {
            "before": before,
            "unchanged": same,
            "changed": changed,
            "after": library.names(),
            "forced": library.refresh(force=True),
        },
    )


def generate(out: FixtureFile) -> None:
    from ezagent.agent.skills import (
        SkillError,
        SkillLibrary,
        ensure_default_skills,
        hash_folder,
        skill_tokens,
    )

    root = pathlib.Path(os.environ["HOME"]) / "skills-cases"
    local = root / "local"
    global_dir = root / "global"
    _write_tree(local, LOCAL_SKILLS)
    _write_tree(global_dir, GLOBAL_SKILLS)

    audit: list[str] = []
    config = _config(root, "main-app", local, global_dir)
    library = SkillLibrary(config, audit_warn=audit.append)

    _catalog_entries(out, library, root, audit)
    _load_entries(out, library, root)
    _error_entries(out, library, root, SkillError)
    _hash_entries(out, library, root, hash_folder)
    _tokens_entries(out, skill_tokens)
    _ensure_entries(out, ensure_default_skills, root)
    _max_catalog_entries(out, SkillLibrary, root)
    _refresh_entries(out, SkillLibrary, root)

    out.add(
        "skills/constants",
        "module_constants",
        {},
        _constants(),
    )


def _constants() -> dict[str, Any]:
    from ezagent.agent import skills

    return {
        "max_catalog": skills.MAX_CATALOG,
        "max_description_chars": skills.MAX_DESCRIPTION_CHARS,
        "max_catalog_description_chars": skills.MAX_CATALOG_DESCRIPTION_CHARS,
        "max_skill_chars": skills.MAX_SKILL_CHARS,
        "max_name_chars": skills.MAX_NAME_CHARS,
        "name_regex": skills._NAME_RE.pattern,
    }


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
