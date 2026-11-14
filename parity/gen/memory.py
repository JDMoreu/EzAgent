"""Fixtures de memoria persistente y sus herramientas (dominio `memory`, F1-07).

Cubre `src/ezagent/agent/memory.py` y `src/ezagent/agent/tools_memory.py`.

Formato:
- Las funciones puras (`fold_text`, `find_sensitive_text`, `make_snippet`, …)
  reciben su entrada y devuelven su salida tal cual.
- Los escenarios `MemoryStore` / `memory_tools` son secuencias de operaciones
  sobre un store recién creado en un directorio temporal. La salida incluye el
  resultado de cada operación, el texto exacto de `USER.md`/`MEMORY.md`/`.bak`
  y el volcado de `history.db` (esquema, filas de `messages` y de `messages_fts`).

El reloj está congelado en `2025-01-01T00:00:00+00:00` (UTC). Las rutas
absolutas del perfil temporal se normalizan a `<tmp>`.

Se cubren FTS5 y el respaldo `LIKE` (FTS5 desactivado al abrir y FTS5 roto
después de abrir). Los avisos `RuntimeWarning` se capturan como parte de la
salida.
"""

from __future__ import annotations

import os
import sqlite3
import warnings
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "memory"

FROZEN = datetime(2025, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
FROZEN_CLOCK = lambda: FROZEN  # noqa: E731 - el store solo llama


def _norm(value: Any, home: Path) -> Any:
    """Sustituye el perfil temporal y unifica separadores en todo el árbol."""
    if isinstance(value, str):
        text = value.replace("\\\\", "/")
        for prefix in (home.as_posix(), str(home)):
            text = text.replace(prefix, "<tmp>")
        return text.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def _dump_files(memory_dir: Path) -> dict[str, str]:
    if not memory_dir.is_dir():
        return {}
    return {
        path.name: path.read_text(encoding="utf-8", errors="replace")
        for path in sorted(memory_dir.iterdir())
        if path.is_file()
    }


def _dump_db(path: Path) -> dict[str, Any] | None:
    """Esquema y filas de `history.db`, sin tablas internas de FTS5."""
    if not path.is_file():
        return None
    connection = sqlite3.connect(str(path))
    connection.row_factory = sqlite3.Row
    try:
        schema = [
            {"type": row["type"], "name": row["name"], "sql": row["sql"]}
            for row in connection.execute(
                "SELECT type, name, sql FROM sqlite_master "
                "WHERE sql IS NOT NULL AND name NOT LIKE 'messages_fts_%' "
                "ORDER BY type, name"
            )
        ]
        messages = [
            {
                "id": row["id"],
                "ts": row["ts"],
                "session": row["session"],
                "role": row["role"],
                "content": row["content"],
            }
            for row in connection.execute(
                "SELECT id, ts, session, role, content FROM messages ORDER BY id"
            )
        ]
        has_fts = connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages_fts'"
        ).fetchone()
        fts = (
            [
                {"rowid": row["rowid"], "content": row["content"]}
                for row in connection.execute(
                    "SELECT rowid, content FROM messages_fts ORDER BY rowid"
                )
            ]
            if has_fts
            else []
        )
        return {"schema": schema, "messages": messages, "fts": fts}
    finally:
        connection.close()


class _Store:
    """Envoltura del store para acumular avisos y poder reabrirlo."""

    def __init__(
        self,
        memory_dir: Path,
        history_path: Path,
        *,
        prefer_fts: bool = True,
        retention_days: int = 180,
        session: str = "default",
    ) -> None:
        self.memory_dir = memory_dir
        self.history_path = history_path
        self.prefer_fts = prefer_fts
        self.retention_days = retention_days
        self.session = session
        self.warnings: list[dict[str, str]] = []
        self.store = self._open()

    def _open(self) -> Any:
        from ezagent.agent.memory import MemoryStore

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            store = MemoryStore.from_paths(
                self.memory_dir,
                self.history_path,
                session=self.session,
                prefer_fts=self.prefer_fts,
                retention_days=self.retention_days,
                clock=FROZEN_CLOCK,
            )
        self.warnings.extend(
            {"category": item.category.__name__, "message": str(item.message)} for item in caught
        )
        return store

    def reopen(self, options: dict[str, Any]) -> Any:
        if "prefer_fts" in options:
            self.prefer_fts = bool(options["prefer_fts"])
        if "retention_days" in options:
            self.retention_days = int(options["retention_days"])
        if "session" in options:
            self.session = str(options["session"])
        self.store = self._open()
        return self.info()

    def info(self) -> dict[str, Any]:
        return {
            "fts_enabled": self.store.fts_enabled,
            "fts_error": self.store.fts_error,
            "last_backend": self.store.last_backend,
            "session": self.store.session,
        }

    def dump(self, home: Path) -> dict[str, Any]:
        return {
            "files": _norm(_dump_files(self.memory_dir), home),
            "db": _norm(_dump_db(self.history_path), home),
            "store": self.info(),
            "warnings": list(self.warnings),
        }


def _timestamp(value: Any) -> Any:
    if isinstance(value, dict) and "dt" in value:
        return datetime.fromisoformat(str(value["dt"]).replace("Z", "+00:00"))
    return value


def _run_op(wrapper: _Store, op: dict[str, Any]) -> Any:
    store = wrapper.store
    kind = op.get("kind")
    action = op["op"]
    if action == "filename":
        return store.filename(kind)
    if action == "limit":
        return store.limit(kind)
    if action == "validate":
        return store.validate_entry(kind, op.get("text")).to_dict()
    if action == "remember":
        return store.remember(kind, op.get("text")).to_dict()
    if action == "read":
        return store.read_markdown(kind)
    if action == "entries":
        return store.entries(kind)
    if action == "prepare":
        return store.prepare_compaction(kind, op.get("text")).to_dict()
    if action == "compact":
        return store.compact_memory(kind, op.get("text")).to_dict()
    if action == "forget":
        return store.forget(op.get("query")).to_dict()
    if action == "find_matches":
        return store.find_matches(op.get("query"))
    if action == "memory_block":
        return store.memory_block()
    if action == "record":
        return store.record(
            op.get("role"),
            op.get("content"),
            session=op.get("session"),
            ts=_timestamp(op.get("ts")),
        )
    if action == "record_exchange":
        return list(
            store.record_exchange(
                op.get("user"),
                op.get("assistant"),
                session=op.get("session"),
                ts=_timestamp(op.get("ts")),
            )
        )
    if action == "purge":
        return store.purge_old(_timestamp(op.get("now")))
    if action == "count":
        return store.count_messages()
    if action == "recall":
        return {
            "results": store.recall(op.get("query"), op.get("limit", 5)),
            "backend": store.last_backend,
        }
    if action == "wipe":
        return store.wipe()
    if action == "reopen":
        return wrapper.reopen(op.get("options", {}))
    if action == "drop_fts":
        connection = sqlite3.connect(str(wrapper.history_path))
        try:
            connection.execute("DROP TABLE IF EXISTS messages_fts")
            connection.commit()
        finally:
            connection.close()
        return True
    if action == "info":
        return wrapper.info()
    raise ValueError(f"operación desconocida: {action}")


def _run_ops(wrapper: _Store, ops: list[dict[str, Any]]) -> list[Any]:
    results: list[Any] = []
    for op in ops:
        try:
            results.append(_run_op(wrapper, op))
        except Exception as error:  # el oráculo registra el texto exacto
            results.append({"error": f"{type(error).__name__}: {error}"})
    return results


def _scenario(
    out: FixtureFile,
    case: str,
    ops: list[dict[str, Any]],
    *,
    home: Path,
    seed: dict[str, str] | None = None,
    prefer_fts: bool = True,
    retention_days: int = 180,
    session: str = "default",
) -> None:
    work = home / "memory-cases" / case.replace("/", "-")
    memory_dir = work / "memory"
    history_path = work / "history.db"
    for name, text in (seed or {}).items():
        _write(memory_dir / name, text)
    wrapper = _Store(
        memory_dir,
        history_path,
        prefer_fts=prefer_fts,
        retention_days=retention_days,
        session=session,
    )
    results = _run_ops(wrapper, ops)
    out.add(
        f"memory/{case}",
        "MemoryStore",
        {
            "ops": ops,
            "prefer_fts": prefer_fts,
            "retention_days": retention_days,
            "session": session,
            "seed": seed or {},
        },
        _norm({"results": results, **wrapper.dump(home)}, home),
    )


# ---------------------------------------------------------------- funciones puras

FOLD_CASES: list[Any] = [
    "ÁÉÍÓÚÜÑÀÈÌÒÙÏË",
    "áéíóúüñàèìòùïë",
    "Hola Mundo",
    "  espacios  ",
    "ñANDÚ",
    "¿Qué?",
    "",
    5,
    None,
    "İstanbul",
    "ẞ",
    "café\u0301",  # e + acento combinado: translate no lo toca
]

ENTRY_CASES: list[Any] = [
    "  hola   mundo \n nuevo ",
    "una\nsola\tlínea",
    " ",
    "",
    5,
    None,
    "emoji 🙂 texto",
    "a\x0bb\x0cc",
]

COMPARE_CASES: list[Any] = [
    "Hola, mundo!!!",
    "  ¿Qué?  ",
    "dato.",
    "dato...",
    "dato, ; : ! ?",
    "!!!",
    "",
    "ÁRBOL",
    5,
    None,
    "linea\npartida",
]

SUMMARY_CASES: list[dict[str, Any]] = [
    {"value": "  hola   mundo ", "limit": 80},
    {"value": "x" * 80, "limit": 80},
    {"value": "x" * 81, "limit": 80},
    {"value": "x" * 10, "limit": 5},
    {"value": "x" * 10, "limit": 1},
    {"value": "x" * 10, "limit": 0},
    {"value": "  ñandú  🙂 ", "limit": 80},
    {"value": None, "limit": 80},
    {"value": 0, "limit": 80},
    {"value": "", "limit": 80},
    {"value": "a\nb", "limit": 80},
    {"value": "x" * 50, "limit": 49},
    {"value": "x" * 2, "limit": 2},
]

SENSITIVE_CASES: list[str] = [
    "mi contraseña es azul",
    "Las contrasenas son largas",
    "PASSWORD",
    "passwd",
    "pwd",
    "api key: abc",
    "api-key=abc",
    "api_key = abc",
    "el token es abc",
    "tokens son abc",
    "secret: abc",
    "secreto = abc",
    "clave: abc",
    "claves de api",
    "clave de acceso",
    "clave de seguridad",
    "clave de wifi",
    "pass: 1234",
    "pin=1234",
    "sk-abcdefgh12345678",
    "sk-corto",
    "pk_live_abcdefgh1234",
    "rk_test_abcdefgh",
    "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "gho_BBBBBBBBBBBBBBBBBBBB",
    "xoxb-1234567890-abcdef",
    "XOXP-9876543210-ABCDEF",
    "AKIAABCDEFGHIJKLMNOP",
    "Bearer abcdefghij",
    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0",
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN PRIVATE KEY-----",
    "AIzaAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "4111 1111 1111 1111",
    "4111111111111111",
    "4111111111111112",
    "mi tarjeta es 1234 5678 9012 3456",
    "12345678Z",
    "X1234567L",
    "documento de identidad 12345678Z",
    "documento nacional de identidad 12345678Z",
    "cédula 123456789",
    "pasaporte AB1234567",
    "ES91 2100 0418 4502 0005 1332",
    "ES9121000418450200051332",
    "prefiero tokens cortos",
    "+34 612 345 678",
    "2025-01-01",
    "correo@ejemplo.com",
    "mi color favorito es el azul",
    "",
    "1234 5678 9012",
]

WEB_CASES: list[str] = [
    "mira https://ejemplo.com/ruta",
    "http://ejemplo.com",
    "ftp://archivos.ejemplo.com/x",
    "www.ejemplo.com",
    "WWW.EJEMPLO.COM",
    "[enlace](https://ejemplo.com)",
    "[enlace](www.ejemplo.com/x)",
    "texto con ejemplo.com dentro",
    "ejemplo.io",
    "ejemplo.ai",
    "ejemplo.dev",
    "ejemplo.es",
    "ejemplo.co",
    "ejemplo.org",
    "ejemplo.net",
    "ejemplo.gov",
    "ejemplo.edu",
    "ejemplo.info",
    "sub.ejemplo.com",
    "a.b.com",
    "archivo.txt",
    "version 1.2",
    "node.js",
    "esto es un.test",
    "",
    "correo@ejemplo.com",
    None,
]

KIND_CASES: list[Any] = [
    "user",
    " usuario ",
    "USUARIO",
    "perfil",
    "User.MD",
    "user_md",
    "memory",
    "memoria",
    "hechos",
    "MEMORY.MD",
    "memory_md",
    "",
    "otro",
    None,
    5,
    True,
    ["user"],
]

KIND_ALIASES_CASES = ["user", "memory", "usuario", "memoria", "perfil", "hechos"]
KIND_FILENAMES_CASES = ["user", "usuario", "USER.MD", "memory", "memoria", "otro", None]

SNIPPET_CASES: list[dict[str, Any]] = [
    {"text": "corto", "terms": ["corto"], "max_chars": 300},
    {"text": "x" * 300, "terms": ["x"], "max_chars": 300},
    {"text": "x" * 301, "terms": ["x"], "max_chars": 300},
    {"text": "a" * 200 + "aguja" + "b" * 200, "terms": ["aguja"], "max_chars": 300},
    {"text": "aguja" + "b" * 400, "terms": ["aguja"], "max_chars": 300},
    {"text": "b" * 400 + "aguja", "terms": ["aguja"], "max_chars": 300},
    {"text": "b" * 400, "terms": ["aguja"], "max_chars": 300},
    {"text": "b" * 400, "terms": [], "max_chars": 300},
    {"text": "año de la ñandú", "terms": ["año"], "max_chars": 5},
    {"text": "año de la ñandú", "terms": ["nandu"], "max_chars": 5},
    {"text": "x" * 100, "terms": ["x"], "max_chars": 10},
    {"text": "x" * 100, "terms": ["x"], "max_chars": 2},
    {"text": "x" * 100, "terms": ["x"], "max_chars": 1},
    {"text": "x" * 100, "terms": ["x"], "max_chars": 0},
    {"text": "x" * 100, "terms": [""], "max_chars": 20},
    {"text": "uno dos tres", "terms": ["tres", "dos"], "max_chars": 10},
    {"text": "", "terms": ["x"], "max_chars": 300},
    {"text": None, "terms": ["x"], "max_chars": 300},
    {"text": "a" * 90 + "b" * 300, "terms": ["b"], "max_chars": 100},
]

RESULT_CASES: list[dict[str, Any]] = [
    {"status": "saved", "message": "Guardado."},
    {
        "status": "saved",
        "message": "Guardado.",
        "kind": "user",
        "text": "dato",
        "removed": 2,
        "files": ["USER.md", "MEMORY.md"],
        "backup": "<tmp>/x.bak",
        "line_count": 3,
    },
    {"status": "empty", "message": "Vacío.", "kind": "memory", "removed": 0, "line_count": 0},
    {"status": "duplicate", "message": "Ya está.", "proposal": "", "removed": 1},
    {"status": "forgotten", "message": "Borrado.", "files": [], "backup": ""},
]


def _pure_cases(out: FixtureFile, memory: Any) -> None:
    for index, value in enumerate(FOLD_CASES, start=1):
        out.add(
            f"memory/fold/{index:03d}",
            "fold_text",
            {"value": value},
            {"text": memory.fold_text(value)},
        )
    for index, value in enumerate(ENTRY_CASES, start=1):
        out.add(
            f"memory/entry/{index:03d}",
            "normalize_entry",
            {"value": value},
            {"text": memory.normalize_entry(value)},
        )
    for index, value in enumerate(COMPARE_CASES, start=1):
        out.add(
            f"memory/compare/{index:03d}",
            "normalize_for_compare",
            {"value": value},
            {"text": memory.normalize_for_compare(value)},
        )
    for index, case in enumerate(SUMMARY_CASES, start=1):
        out.add(
            f"memory/summarize/{index:03d}",
            "summarize_text",
            case,
            {"text": memory.summarize_text(case["value"], case["limit"])},
        )
    out.add(
        "memory/summarize/default",
        "summarize_text",
        {"value": "x" * 81},
        {"text": memory.summarize_text("x" * 81)},
    )
    for index, value in enumerate(SENSITIVE_CASES, start=1):
        out.add(
            f"memory/sensitive/{index:03d}",
            "find_sensitive_text",
            {"text": value},
            {"category": memory.find_sensitive_text(value)},
        )
    for index, value in enumerate(WEB_CASES, start=1):
        out.add(
            f"memory/web/{index:03d}",
            "contains_web_content",
            {"text": value},
            {"web": memory.contains_web_content(value)},
        )
    for index, value in enumerate(KIND_CASES, start=1):
        out.add(
            f"memory/kind/{index:03d}",
            "normalize_kind",
            {"kind": value},
            {"kind": memory.normalize_kind(value)},
        )
    for index, value in enumerate(SNIPPET_CASES, start=1):
        out.add(
            f"memory/snippet/{index:03d}",
            "make_snippet",
            value,
            {"snippet": memory.make_snippet(value["text"], value["terms"], value["max_chars"])},
        )
    out.add(
        "memory/snippet/default",
        "make_snippet",
        {"text": "x" * 400, "terms": ["x"]},
        {"snippet": memory.make_snippet("x" * 400, ["x"])},
    )
    for index, case in enumerate(RESULT_CASES, start=1):
        result = memory.MemoryResult(
            case["status"],
            case["message"],
            kind=case.get("kind"),
            text=case.get("text"),
            proposal=case.get("proposal"),
            removed=case.get("removed", 0),
            files=tuple(case.get("files", ())),
            backup=case.get("backup"),
            line_count=case.get("line_count", 0),
        )
        out.add(
            f"memory/result/{index:03d}",
            "MemoryResult.to_dict",
            case,
            {"saved": result.saved, "data": result.to_dict()},
        )
    out.add(
        "memory/constants",
        "module_constants",
        {},
        {
            "user_limit": memory.USER_LIMIT,
            "memory_limit": memory.MEMORY_LIMIT,
            "max_entry_chars": memory.MAX_ENTRY_CHARS,
            "recall_fragment_chars": memory.RECALL_FRAGMENT_CHARS,
            "retention_days": memory.RETENTION_DAYS,
            "default_recall_limit": memory.DEFAULT_RECALL_LIMIT,
            "kind_files": dict(memory._KIND_FILES),
            "kind_limits": dict(memory._KIND_LIMITS),
            "kind_aliases": dict(memory._KIND_ALIASES),
            "role_aliases": dict(memory._ROLE_ALIASES),
            "stopwords": sorted(memory._STOPWORDS),
            "kind_aliases_cases": {
                key: memory._KIND_ALIASES.get(key) for key in KIND_ALIASES_CASES
            },
            "kind_filenames_cases": {
                key: memory._KIND_FILES.get(memory.normalize_kind(key) or "", "memoria")
                for key in KIND_FILENAMES_CASES
            },
        },
    )


# ------------------------------------------------------------------- escenarios


def _long_seed(prefix: str, count: int, size: int) -> str:
    return (
        "\n".join(
            f"- [2024-12-{index:02d}] {prefix} {index} " + "x" * size
            for index in range(1, count + 1)
        )
        + "\n"
    )


def _store_scenarios(out: FixtureFile, home: Path) -> None:
    cases: list[tuple[str, dict[str, Any]]] = [
        (
            "empty-store",
            {},
        ),
        (
            "remember-user",
            {
                "ops": [
                    {"op": "remember", "kind": "user", "text": "  Prefiero  café   por la mañana "},
                    {"op": "entries", "kind": "user"},
                    {"op": "filename", "kind": "usuario"},
                    {"op": "limit", "kind": "memoria"},
                    {"op": "memory_block"},
                ]
            },
        ),
        (
            "remember-memory-unicode",
            {
                "ops": [
                    {
                        "op": "remember",
                        "kind": "MEMORIA",
                        "text": "El proyecto usa Rust y 日本語 🙂",
                    },
                    {"op": "read", "kind": "memory"},
                    {"op": "entries", "kind": "memory"},
                ]
            },
        ),
        (
            "remember-mixed",
            {
                "ops": [
                    {"op": "remember", "kind": "user", "text": "Me llamo Ana."},
                    {"op": "remember", "kind": "memory", "text": "Trabajo con Rust."},
                    {"op": "remember", "kind": "usuario", "text": "me llamo ana"},
                    {"op": "remember", "kind": "memory", "text": "trabajo con Rust"},
                    {"op": "validate", "kind": "user", "text": "Me llamo Ana"},
                    {"op": "memory_block"},
                ]
            },
        ),
        (
            "invalid-kinds",
            {
                "ops": [
                    {"op": "validate", "kind": "", "text": "dato"},
                    {"op": "validate", "kind": "otro", "text": "dato"},
                    {"op": "validate", "kind": None, "text": "dato"},
                    {"op": "validate", "kind": 5, "text": "dato"},
                    {"op": "remember", "kind": "otro", "text": "dato"},
                    {"op": "filename", "kind": "otro"},
                    {"op": "limit", "kind": "otro"},
                    {"op": "read", "kind": "otro"},
                    {"op": "entries", "kind": "otro"},
                ]
            },
        ),
        (
            "empty-text",
            {
                "ops": [
                    {"op": "validate", "kind": "user", "text": "   \n\t "},
                    {"op": "validate", "kind": "user", "text": ""},
                    {"op": "validate", "kind": "user", "text": None},
                    {"op": "remember", "kind": "user", "text": "  "},
                ]
            },
        ),
        (
            "too-long",
            {
                "ops": [
                    {"op": "validate", "kind": "user", "text": "y" * 200},
                    {"op": "validate", "kind": "user", "text": "y" * 201},
                    {"op": "validate", "kind": "memory", "text": "ñ" * 201},
                    {"op": "remember", "kind": "memory", "text": "z" * 250},
                ]
            },
        ),
        (
            "secrets",
            {
                "ops": [
                    {"op": "remember", "kind": "user", "text": "mi contraseña es azul"},
                    {"op": "remember", "kind": "memory", "text": "api key: abc"},
                    {"op": "remember", "kind": "user", "text": "sk-abcdefgh12345678"},
                    {"op": "remember", "kind": "user", "text": "mi tarjeta es 4111 1111 1111 1111"},
                    {"op": "remember", "kind": "user", "text": "12345678Z"},
                    {"op": "remember", "kind": "user", "text": "ES91 2100 0418 4502 0005 1332"},
                    {"op": "remember", "kind": "user", "text": "prefiero tokens cortos"},
                ]
            },
        ),
        (
            "web-content",
            {
                "ops": [
                    {"op": "remember", "kind": "user", "text": "mira https://ejemplo.com"},
                    {"op": "remember", "kind": "user", "text": "ejemplo.com"},
                    {"op": "remember", "kind": "user", "text": "[x](https://ejemplo.com)"},
                    {"op": "remember", "kind": "user", "text": "uso Rust"},
                ]
            },
        ),
        (
            "full-user",
            {
                "seed": {"USER.md": _long_seed("dato", 13, 80)},
                "ops": [
                    {
                        "op": "validate",
                        "kind": "user",
                        "text": "un dato nuevo importante " + "q" * 60,
                    },
                    {
                        "op": "remember",
                        "kind": "user",
                        "text": "un dato nuevo importante " + "q" * 60,
                    },
                    {"op": "read", "kind": "user"},
                ],
            },
        ),
        (
            "full-memory",
            {
                "seed": {"MEMORY.md": _long_seed("hecho", 20, 80)},
                "ops": [
                    {"op": "remember", "kind": "memory", "text": "otro hecho " + "r" * 100},
                    {"op": "validate", "kind": "memory", "text": "otro hecho " + "r" * 100},
                ],
            },
        ),
        (
            "compact-valid",
            {
                "seed": {"MEMORY.md": "- [2024-01-01] viejo\nbasura\n"},
                "ops": [
                    {
                        "op": "prepare",
                        "kind": "memory",
                        "text": (
                            "# Preferencias\n"
                            "- [2024-02-02] Café sin azúcar\n"
                            "  - suelto con dos espacios\n"
                            "\n"
                            "línea plana\n"
                            "#\n"
                            "- [2024-03-03]\n"
                        ),
                    },
                    {
                        "op": "compact",
                        "kind": "memory",
                        "text": (
                            "# Preferencias\n"
                            "- [2024-02-02] Café sin azúcar\n"
                            "  - suelto con dos espacios\n"
                            "\n"
                            "línea plana\n"
                            "#\n"
                            "- [2024-03-03]\n"
                        ),
                    },
                    {"op": "read", "kind": "memory"},
                    {"op": "entries", "kind": "memory"},
                ],
            },
        ),
        (
            "compact-twice",
            {
                "ops": [
                    {"op": "compact", "kind": "user", "text": "primera versión"},
                    {"op": "compact", "kind": "user", "text": "segunda versión"},
                    {"op": "read", "kind": "user"},
                ]
            },
        ),
        (
            "compact-invalid",
            {
                "ops": [
                    {"op": "prepare", "kind": "otro", "text": "x"},
                    {"op": "prepare", "kind": "user", "text": ""},
                    {"op": "prepare", "kind": "user", "text": " \n\t "},
                    {"op": "prepare", "kind": "user", "text": "x" * 1376},
                    {"op": "prepare", "kind": "user", "text": "mi contraseña es azul"},
                    {"op": "prepare", "kind": "user", "text": "https://ejemplo.com"},
                    {"op": "compact", "kind": "user", "text": "mi contraseña es azul"},
                ]
            },
        ),
        (
            "forget",
            {
                "seed": {
                    "USER.md": "- [2024-01-01] prefiero café\n- [2024-01-02] vivo en Lima\n",
                    "MEMORY.md": "- [2024-01-03] el proyecto usa Café\n\n- [2024-01-04] otra cosa\n",
                },
                "ops": [
                    {"op": "find_matches", "query": "CAFÉ"},
                    {"op": "forget", "query": "café"},
                    {"op": "forget", "query": "café"},
                    {"op": "forget", "query": "   "},
                    {"op": "read", "kind": "user"},
                    {"op": "read", "kind": "memory"},
                ],
            },
        ),
        (
            "forget-all",
            {
                "seed": {"USER.md": "- [2024-01-01] único dato\n"},
                "ops": [
                    {"op": "forget", "query": "único"},
                    {"op": "read", "kind": "user"},
                    {"op": "memory_block"},
                ],
            },
        ),
        (
            "memory-block",
            {
                "seed": {
                    "USER.md": "  # Perfil\n- [2024-01-01] Ana\n\n",
                    "MEMORY.md": "  hechos  \n",
                },
                "ops": [{"op": "memory_block"}],
            },
        ),
        (
            "memory-block-truncated",
            {
                "seed": {"USER.md": "  " + "x" * 1400 + "  ", "MEMORY.md": "y" * 2300},
                "ops": [{"op": "memory_block"}],
            },
        ),
        (
            "record-basic",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "USER",
                        "content": "hola",
                        "ts": "2024-12-01T10:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "asistente",
                        "content": "buenas",
                        "ts": "2024-12-01T10:00:01+00:00",
                    },
                    {
                        "op": "record",
                        "role": "AI",
                        "content": "  ",
                        "ts": "2024-12-01T10:00:02+00:00",
                    },
                    {
                        "op": "record",
                        "role": "tool",
                        "content": "no",
                        "ts": "2024-12-01T10:00:03+00:00",
                    },
                    {
                        "op": "record",
                        "role": "system",
                        "content": "no",
                        "ts": "2024-12-01T10:00:03+00:00",
                    },
                    {
                        "op": "record",
                        "role": "bot",
                        "content": "claro",
                        "session": "otra",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {"op": "count"},
                ]
            },
        ),
        (
            "record-default-ts",
            {
                "session": "s1",
                "ops": [
                    {"op": "record", "role": "user", "content": "sin ts"},
                    {"op": "record", "role": "assistant", "content": "tampoco"},
                    {"op": "record", "role": "user", "content": "con sesión", "session": "s2"},
                    {"op": "count"},
                ],
            },
        ),
        (
            "record-exchange",
            {
                "ops": [
                    {"op": "record_exchange", "user": "pregunta", "assistant": "respuesta"},
                    {"op": "record_exchange", "user": "  ", "assistant": "solo"},
                    {"op": "record_exchange", "user": "otra", "assistant": ""},
                    {"op": "count"},
                ]
            },
        ),
        (
            "record-datetime",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "aware",
                        "ts": {"dt": "2024-12-01T10:00:00+02:00"},
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "naive",
                        "ts": {"dt": "2024-12-01T10:00:00"},
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "texto",
                        "ts": " 2024-12-01T10:00:00+00:00 ",
                    },
                ]
            },
        ),
        (
            "recall-like-escaping",
            {
                "prefer_fts": False,
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "100%_raro\\ruta",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "100x y raro",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {"op": "recall", "query": "100%_raro\\"},
                    {"op": "recall", "query": "%"},
                    {"op": "recall", "query": "_"},
                    {"op": "recall", "query": "\\"},
                ],
            },
        ),
        (
            "record-errors",
            {
                "ops": [
                    {"op": "record", "role": "user", "content": "x", "ts": 123},
                    {"op": "record", "role": "user", "content": "x", "ts": True},
                    {"op": "record", "role": "user", "content": 5},
                    {"op": "record", "role": None, "content": "x"},
                    {"op": "record", "role": "user", "content": None},
                ]
            },
        ),
        (
            "purge-default",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "viejo",
                        "ts": "2024-06-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "nuevo",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {"op": "purge"},
                    {"op": "count"},
                    {"op": "purge", "now": {"dt": "2025-06-01T00:00:00+00:00"}},
                    {"op": "count"},
                ]
            },
        ),
        (
            "purge-custom",
            {
                "retention_days": 1,
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "ayer",
                        "ts": "2024-12-31T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "hoy",
                        "ts": "2025-01-01T00:00:00+00:00",
                    },
                    {"op": "count"},
                ],
            },
        ),
        (
            "purge-on-insert",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "caducado",
                        "ts": "2020-01-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "vigente",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {"op": "count"},
                ]
            },
        ),
        (
            "recall-fts",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "Hoy hablamos de café y pan",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "assistant",
                        "content": "El café de Lima es dulce",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "user",
                        "content": "Pan integral",
                        "ts": "2024-12-03T00:00:00+00:00",
                    },
                    {"op": "recall", "query": "cafe"},
                    {"op": "recall", "query": "CAFÉ"},
                    {"op": "recall", "query": "pan de lima"},
                    {"op": "recall", "query": "el de la"},
                    {"op": "recall", "query": "zzz"},
                    {"op": "recall", "query": "cafe", "limit": 0},
                    {"op": "recall", "query": "cafe", "limit": 100},
                    {"op": "recall", "query": "cafe", "limit": -3},
                    {"op": "recall", "query": "cafe", "limit": "2"},
                    {"op": "recall", "query": "cafe", "limit": "x"},
                    {"op": "recall", "query": "   "},
                    {"op": "recall", "query": "..."},
                ]
            },
        ),
        (
            "recall-fallback-like",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "Hoy hablamos de café y pan",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "assistant",
                        "content": "El café de Lima es dulce",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {"op": "drop_fts"},
                    {"op": "recall", "query": "cafe"},
                    {"op": "recall", "query": "café"},
                    {"op": "info"},
                ]
            },
        ),
        (
            "recall-disabled",
            {
                "prefer_fts": False,
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "Hoy hablamos de café y pan",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "assistant",
                        "content": "El café de Lima es dulce",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {"op": "recall", "query": "cafe"},
                    {"op": "recall", "query": "CAFÉ"},
                    {"op": "recall", "query": "..."},
                    {"op": "reopen", "options": {}},
                ],
            },
        ),
        (
            "reopen",
            {
                "ops": [
                    {"op": "remember", "kind": "user", "text": "persistente"},
                    {
                        "op": "record",
                        "role": "user",
                        "content": "hola",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {"op": "reopen", "options": {"session": "s9"}},
                    {"op": "entries", "kind": "user"},
                    {"op": "count"},
                ]
            },
        ),
        (
            "wipe",
            {
                "ops": [
                    {
                        "op": "record",
                        "role": "user",
                        "content": "uno",
                        "ts": "2024-12-01T00:00:00+00:00",
                    },
                    {
                        "op": "record",
                        "role": "assistant",
                        "content": "dos",
                        "ts": "2024-12-02T00:00:00+00:00",
                    },
                    {"op": "wipe"},
                    {"op": "count"},
                    {"op": "recall", "query": "uno"},
                    {"op": "wipe"},
                ]
            },
        ),
        (
            "retention-minimum",
            {
                "retention_days": 0,
                "ops": [{"op": "info"}],
            },
        ),
    ]
    for case_id, spec in cases:
        ops = list(spec.get("ops", []))
        if case_id == "reopen":
            ops.append({"op": "count"})
        _scenario(
            out,
            case_id,
            ops,
            home=home,
            seed=spec.get("seed"),
            prefer_fts=spec.get("prefer_fts", True),
            retention_days=spec.get("retention_days", 180),
            session=spec.get("session", "default"),
        )
    _constructor_case(out, home)


def _constructor_case(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.memory import MemoryStore

    work = home / "memory-cases" / "from-config"
    config = SimpleNamespace(
        memory_dir=str(work / "memory"),
        history_path=str(work / "history.db"),
    )
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        store = MemoryStore(config, session="cfg", clock=FROZEN_CLOCK)
    wrapper = _Store(work / "memory", work / "history.db")
    wrapper.store = store
    wrapper.warnings = [
        {"category": item.category.__name__, "message": str(item.message)} for item in caught
    ]
    store.remember("user", "desde AgentConfig")
    out.add(
        "memory/from-config",
        "MemoryStore.__init__",
        {"session": "cfg", "memory_dir": "memory", "history_path": "history.db"},
        _norm({"info": wrapper.info(), **wrapper.dump(home)}, home),
    )


# ---------------------------------------------------------------- herramientas


class _FakeManager:
    """Adaptador mínimo de Confirmación para probar `_resolve_confirmation`."""

    def __init__(self, answer: bool, log: list[tuple[str, bool]]) -> None:
        self.answer = answer
        self.log = log

    def ask(self, summary: str, *, strong: bool) -> bool:
        self.log.append((summary, strong))
        return self.answer


class _Clock:
    def __init__(self) -> None:
        self.value = 1000.0

    def __call__(self) -> float:
        return self.value


def _tool_scenario(
    out: FixtureFile,
    case: str,
    *,
    home: Path,
    ops: list[dict[str, Any]],
    seed: dict[str, str] | None = None,
    history: list[dict[str, Any]] | None = None,
    confirm: Any = "yes",
    manager: bool = False,
    timeout_s: Any = 15.0,
    advance: float = 0.0,
    prefer_fts: bool = True,
) -> None:
    from ezagent.agent import tools_memory

    work = home / "tool-cases" / case.replace("/", "-")
    memory_dir = work / "memory"
    history_path = work / "history.db"
    for name, text in (seed or {}).items():
        _write(memory_dir / name, text)
    memory = _Store(memory_dir, history_path, prefer_fts=prefer_fts)
    history_results = _run_ops(memory, history or [])
    questions: list[dict[str, Any]] = []
    clock = _Clock()

    def ask(summary: str, strong: bool) -> bool:
        questions.append({"summary": summary, "strong": strong})
        if confirm == "raise":
            raise RuntimeError("confirmación rota")
        clock.value += advance
        return bool(confirm)

    config = SimpleNamespace(general=SimpleNamespace(confirm_timeout_s=timeout_s))
    original_monotonic = tools_memory.time.monotonic
    tools_memory.time.monotonic = clock
    try:
        if manager:
            manager_calls: list[tuple[str, bool]] = []
            manager_obj = _FakeManager(bool(confirm), manager_calls)
            tools = tools_memory.build_memory_tools(config, memory.store, confirmation=manager_obj)
        elif confirm == "none":
            tools = tools_memory.build_memory_tools(config, memory.store)
        else:
            tools = tools_memory.build_memory_tools(config, memory.store, confirm=ask)
        by_name = {spec.name: spec for spec in tools}
        results: list[Any] = []
        for op in ops:
            spec = by_name[op["tool"]]
            try:
                result = spec.handler(op.get("args", {}))
                results.append(
                    {
                        "tool": op["tool"],
                        "result": result.to_dict() if hasattr(result, "to_dict") else result,
                        "describe": spec.describe(op.get("args", {})).__dict__,
                    }
                )
            except Exception as error:
                results.append({"tool": op["tool"], "error": f"{type(error).__name__}: {error}"})
        if manager:
            questions = [
                {"summary": summary, "strong": strong} for summary, strong in manager_calls
            ]
    finally:
        tools_memory.time.monotonic = original_monotonic

    payload = {
        "ops": ops,
        "history": history or [],
        "history_results": history_results,
        "confirm": confirm if isinstance(confirm, str) else "callback",
        "questions": questions,
        "results": results,
        "timeout_s": tools_memory._tool_timeout(config),
        **memory.dump(home),
    }
    out.add(
        f"memory/tools/{case}",
        "memory_tools",
        {
            "ops": ops,
            "history": history or [],
            "seed": seed or {},
            "confirm": confirm if isinstance(confirm, str) else "callback",
            "manager": manager,
            "timeout_s": timeout_s,
            "advance": advance,
            "prefer_fts": prefer_fts,
        },
        _norm(payload, home),
    )


def _tool_scenarios(out: FixtureFile, home: Path) -> None:
    seeded = {
        "USER.md": "- [2024-01-01] Prefiero café\n",
        "MEMORY.md": "- [2024-01-02] El proyecto usa Rust\n",
    }
    history_ops = [
        {
            "op": "record",
            "role": "user",
            "content": "Hoy hablamos de café",
            "ts": "2024-12-01T00:00:00+00:00",
        },
        {
            "op": "record",
            "role": "assistant",
            "content": "El café de Lima",
            "ts": "2024-12-02T00:00:00+00:00",
        },
    ]
    cases: list[tuple[str, dict[str, Any]]] = [
        (
            "remember-yes",
            {
                "ops": [
                    {"tool": "remember", "args": {"kind": "user", "text": "  Vivo en   Lima "}},
                    {"tool": "remember", "args": {"kind": "memory", "text": "Uso Rust"}},
                ]
            },
        ),
        (
            "remember-no",
            {
                "confirm": False,
                "ops": [{"tool": "remember", "args": {"kind": "user", "text": "Vivo en Lima"}}],
            },
        ),
        (
            "remember-none",
            {
                "confirm": "none",
                "ops": [{"tool": "remember", "args": {"kind": "user", "text": "Vivo en Lima"}}],
            },
        ),
        (
            "remember-raises",
            {
                "confirm": "raise",
                "ops": [{"tool": "remember", "args": {"kind": "user", "text": "Vivo en Lima"}}],
            },
        ),
        (
            "remember-timeout",
            {
                "advance": 100.0,
                "ops": [{"tool": "remember", "args": {"kind": "user", "text": "Vivo en Lima"}}],
            },
        ),
        (
            "remember-rejects",
            {
                "ops": [
                    {"tool": "remember", "args": {"kind": "otro", "text": "dato"}},
                    {"tool": "remember", "args": {"kind": "user", "text": ""}},
                    {"tool": "remember", "args": {"kind": "user", "text": "y" * 201}},
                    {"tool": "remember", "args": {"kind": "user", "text": "mi contraseña es azul"}},
                    {"tool": "remember", "args": {"kind": "user", "text": "https://ejemplo.com"}},
                    {"tool": "remember", "args": {"kind": "user", "text": "Prefiero café"}},
                    {"tool": "remember", "args": {"kind": "user", "text": "prefiero cafe"}},
                ]
            },
        ),
        (
            "remember-full",
            {
                "seed": {"USER.md": _long_seed("dato", 13, 80)},
                "ops": [
                    {"tool": "remember", "args": {"kind": "user", "text": "nuevo " + "q" * 100}}
                ],
            },
        ),
        (
            "recall",
            {
                "ops": [
                    {"tool": "recall", "args": {"query": "cafe"}},
                    {"tool": "recall", "args": {"query": "CAFÉ", "limit": 1}},
                    {"tool": "recall", "args": {"query": "zzz"}},
                    {"tool": "recall", "args": {"query": ""}},
                    {"tool": "recall", "args": {"query": "cafe", "limit": "x"}},
                    {"tool": "recall", "args": {"query": "cafe", "limit": "2"}},
                    {"tool": "recall", "args": {"query": "cafe", "limit": 0}},
                ]
            },
        ),
        (
            "recall-like",
            {
                "prefer_fts": False,
                "ops": [
                    {"tool": "recall", "args": {"query": "cafe"}},
                    {"tool": "recall", "args": {"query": "dulce", "limit": 20}},
                ],
            },
        ),
        (
            "forget-yes",
            {
                "ops": [{"tool": "forget", "args": {"query": "café"}}],
            },
        ),
        (
            "forget-no",
            {
                "confirm": False,
                "ops": [{"tool": "forget", "args": {"query": "café"}}],
            },
        ),
        (
            "forget-timeout",
            {
                "advance": 100.0,
                "ops": [{"tool": "forget", "args": {"query": "café"}}],
            },
        ),
        (
            "forget-not-found",
            {
                "ops": [
                    {"tool": "forget", "args": {"query": "zzz"}},
                    {"tool": "forget", "args": {"query": ""}},
                ]
            },
        ),
        (
            "compact-yes",
            {
                "seed": {"USER.md": "- [2024-01-01] viejo\n"},
                "ops": [
                    {
                        "tool": "compact_memory",
                        "args": {"kind": "user", "text": "# Perfil\n- [2024-02-02] nuevo\nplano\n"},
                    }
                ],
            },
        ),
        (
            "compact-no",
            {"ops": [{"tool": "compact_memory", "args": {"kind": "user", "text": "solo esto"}}]},
        ),
        (
            "compact-timeout",
            {
                "advance": 100.0,
                "ops": [{"tool": "compact_memory", "args": {"kind": "user", "text": "solo esto"}}],
            },
        ),
        (
            "compact-rejects",
            {
                "ops": [
                    {"tool": "compact_memory", "args": {"kind": "otro", "text": "x"}},
                    {"tool": "compact_memory", "args": {"kind": "user", "text": ""}},
                    {
                        "tool": "compact_memory",
                        "args": {"kind": "user", "text": "mi contraseña es azul"},
                    },
                ]
            },
        ),
        (
            "manager",
            {
                "manager": True,
                "ops": [
                    {"tool": "remember", "args": {"kind": "user", "text": "Vivo en Lima"}},
                    {"tool": "forget", "args": {"query": "café"}},
                ],
            },
        ),
    ]
    for case_id, spec in cases:
        seed = dict(seeded) if case_id not in {"remember-full"} else dict(spec.get("seed", {}))
        seed.update(spec.get("seed", {}))
        _tool_scenario(
            out,
            case_id,
            home=home,
            ops=list(spec.get("ops", [])),
            seed=seed,
            history=[] if case_id == "remember-full" else list(history_ops),
            confirm=spec.get("confirm", True),
            manager=spec.get("manager", False),
            timeout_s=spec.get("timeout_s", 15.0),
            advance=spec.get("advance", 0.0),
            prefer_fts=spec.get("prefer_fts", True),
        )


def _specs_case(out: FixtureFile, home: Path) -> None:
    del home
    from ezagent.agent import tools_memory

    config = SimpleNamespace(general=SimpleNamespace(confirm_timeout_s=15.0))
    work = Path(os.environ["HOME"]) / "specs"
    store = _Store(work / "memory", work / "history.db").store
    tools = tools_memory.build_memory_tools(config, store)
    describe_args: dict[str, list[dict[str, Any]]] = {
        "remember": [
            {"kind": "user", "text": "  dato  con   espacios "},
            {},
            {"kind": "otro"},
        ],
        "recall": [{"query": "  café  ", "limit": 7}, {}, {"query": ""}],
        "forget": [{"query": "  café  "}, {}],
        "compact_memory": [
            {"kind": "user", "text": "# T\nuna\n\n otra "},
            {"kind": "otro"},
        ],
    }
    specs = []
    for spec in tools:
        descriptions = []
        for args in describe_args[spec.name]:
            try:
                descriptions.append({"args": args, "info": spec.describe(args).__dict__})
            except Exception as error:
                descriptions.append({"args": args, "error": f"{type(error).__name__}: {error}"})
        specs.append(
            {
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.parameters,
                "level": spec.level,
                "untrusted_output": spec.untrusted_output,
                "timeout_s": spec.timeout_s,
                "schema": spec.schema(),
                "descriptions": descriptions,
            }
        )
    out.add(
        "memory/tools/specs",
        "build_memory_tools",
        {"confirm_timeout_s": 15.0},
        {"specs": specs, "timeout": tools_memory._tool_timeout(config)},
    )
    out.add(
        "memory/tools/timeout",
        "tool_timeout",
        {"values": [15.0, 25.0, 20.0, 5.0, "40", "x", None]},
        {
            "values": [
                tools_memory._tool_timeout(
                    SimpleNamespace(general=SimpleNamespace(confirm_timeout_s=value))
                )
                for value in (15.0, 25.0, 20.0, 5.0, "40", "x", None)
            ],
            "no_general": tools_memory._tool_timeout(SimpleNamespace()),
            "no_config": tools_memory._tool_timeout(None),
        },
    )


def _cli_cases(out: FixtureFile, home: Path) -> None:
    import contextlib
    import io
    import sys

    from ezagent.agent import memory

    parser = memory.build_parser()
    parse_cases = [
        ["wipe"],
        ["wipe", "--history", str(home / "h.db")],
        ["wipe", "-y"],
        ["wipe", "--history", str(home / "otra.db"), "--yes"],
    ]
    for index, argv in enumerate(parse_cases, start=1):
        args = parser.parse_args(argv)
        out.add(
            f"memory/cli/parse/{index:02d}",
            "build_parser",
            {"argv": _norm(argv, home)},
            {
                "command": args.command,
                "history": _norm(str(args.history), home) if args.history is not None else None,
                "yes": args.yes,
            },
        )

    work = home / "memory-cli" / "wipe"
    history_path = work / "history.db"
    store = memory.MemoryStore.from_paths(work / "memory", history_path, clock=FROZEN_CLOCK)
    store.record("user", "uno", ts="2024-12-01T00:00:00+00:00")
    store.record("assistant", "dos", ts="2024-12-02T00:00:00+00:00")

    class _FakeTTY(io.StringIO):
        def isatty(self) -> bool:
            return True

    original_stdin = sys.stdin
    original_now = memory._local_now
    memory._local_now = lambda: FROZEN
    try:
        for case, argv, stdin_text, tty in (
            ("wipe-yes", ["wipe", "--history", str(history_path), "--yes"], "", False),
            ("wipe-noninteractive", ["wipe", "--history", str(history_path)], "", False),
            ("wipe-cancel", ["wipe", "--history", str(history_path)], "no\n", True),
            ("wipe-confirm", ["wipe", "--history", str(history_path)], "borrar\n", True),
        ):
            store.wipe()
            store.record("user", "uno", ts="2024-12-01T00:00:00+00:00")
            store.record("assistant", "dos", ts="2024-12-02T00:00:00+00:00")
            sys.stdin = _FakeTTY(stdin_text) if tty else io.StringIO()
            out_buffer, err_buffer = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out_buffer), contextlib.redirect_stderr(err_buffer):
                code = memory.main(argv)
            out.add(
                f"memory/cli/{case}",
                "main",
                {"argv": _norm(argv, home), "stdin": stdin_text, "tty": tty},
                _norm(
                    {
                        "code": code,
                        "stdout": out_buffer.getvalue(),
                        "stderr": err_buffer.getvalue(),
                        "count": store.count_messages(),
                    },
                    home,
                ),
            )
    finally:
        sys.stdin = original_stdin
        memory._local_now = original_now


def generate(out: FixtureFile) -> None:
    import os as _os

    from ezagent.agent import memory

    home = Path(_os.environ["HOME"])
    _pure_cases(out, memory)
    _store_scenarios(out, home)
    _tool_scenarios(out, home)
    _specs_case(out, home)
    _cli_cases(out, home)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
