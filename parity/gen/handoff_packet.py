"""Fixtures del paquete de traspaso (dominio `handoff_packet`, F1-02).

Cubre `has_work`, `outline`, `build_packet` y `packet_text`. Los callbacks
`select` y `save` se describen en JSON con cadenas:
- `select`: "none" | "raise" | "zero" | "negative" | "float" | "str" |
  "none_value" | "out_of_range" | "len" | un entero (línea 1-based).
- `save`: "none" | "raise" | "path" (devuelve `C:/tmp/registro.md`).
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "handoff_packet"
SAVED_PATH = "C:/tmp/registro.md"

WORK_LOG = (
    "\n".join(f"• Ran comando {index}\nsalida del comando {index}" for index in range(40))
    + "\n• Wrote C:/web/index.html\n"
    + "● Creé index.html en C:/web y quedó listo para publicar.\n"
)

PLAN = "# Plan\n" + "\n".join(f"{index}. Paso número {index} del plan" for index in range(1, 400))
SHORT_PLAN = "palabra " * 700  # 5600 caracteres, por debajo de DOCUMENT_FILE_CHARS


def _select_callback(spec: Any) -> Callable[[list[str]], int | None] | None:
    if spec == "none":
        return None
    if spec == "raise":

        def boom(lines: list[str]) -> int | None:
            raise RuntimeError("modelo caído")

        return boom
    fixed = {
        "zero": lambda lines: 0,
        "negative": lambda lines: -3,
        "float": lambda lines: 2.5,
        "str": lambda lines: "inyección",
        "none_value": lambda lines: None,
        "out_of_range": lambda lines: 10_000,
        "len": lambda lines: len(lines),
    }
    if spec in fixed:
        return fixed[spec]  # type: ignore[return-value]
    if isinstance(spec, int) and not isinstance(spec, bool):
        return lambda lines: spec
    raise ValueError(f"select desconocido: {spec!r}")


def _save_callback(spec: str, calls: list[str]) -> Callable[[str], str | None] | None:
    if spec == "none":
        return None
    if spec == "raise":

        def boom(text: str) -> str | None:
            raise RuntimeError("disco lleno")

        return boom

    def save(text: str) -> str:
        calls.append(text)
        return SAVED_PATH

    return save


def _has_work_cases() -> list[str]:
    return [
        "Ran 1 shell command",
        "Ran 12 shell commands",
        "• Ran uv sync",
        "× Ran algo",
        "● Wrote a.txt",
        "• Read archivo.py",
        "• Write b.txt",
        "• Edit c.py",
        "• Edited d.py",
        "• Added línea",
        "• Updated algo",
        "• Deleted temporal",
        "• Explored src",
        "• Searched TODO",
        "• Listed archivos",
        "• Thought algo",
        "• Create archivo",
        "• Created archivo",
        "# Wrote /tmp/a",
        "# Read /tmp/b",
        "# Edit /tmp/c",
        "# Created /tmp/d",
        "# Deleted /tmp/e",
        "# Patched /tmp/f",
        "→ Read x",
        "↳ siguiente paso",
        "← anterior",
        "+ Show details",
        "+ Thought about it",
        "Explored: src",
        "  • Ran con sangría  ",
        "$ uv sync",
        "12 archivo",
        "⎿ resultado",
        "└ salida",
        "Hice un plan con varios pasos.\n1. uno\n2. dos",
        "Ran shell command",
        "• Run uv sync",
        "",
        "   ",
        "Unicode: ñandú, emoji 🙂",
        "Ran 0 shell commands",
    ]


def _outline_cases() -> list[tuple[str, int]]:
    duplicate = (
        "Plan de la landing\n\nObjetivo\nTexto largo que termina en punto.\nObjetivo\nPublicar\n"
    )
    many = "\n".join(f"Paso {index}" for index in range(1, 40))
    return [
        ("", 24),
        ("   \n\t\n", 24),
        ("Una sola línea", 24),
        ("\n\n  Primer párrafo  \n\nSegundo párrafo", 24),
        ("# Título\n\n## Sección\nTexto.", 24),
        ("1. Uno\n2. Dos\n3) Tres\nTexto final.", 24),
        ("Objetivo\nObjetivo\nPublicar", 24),
        (duplicate, 24),
        ("Título\nlinea que termina en punto.\nOtra Cosa", 24),
        ("A" * 91 + "\nCorto", 24),
        ("Texto " * 100, 24),
        (many, 3),
        (many, 1),
        (many, 0),
        ("ÁÉÍÓÚÑ Órgano\nÑandú", 24),
        ("emoji 🙂🚀\nTítulo con tilde: ñ", 24),
        ("Plan\n" + "\n".join(f"Paso {i}" for i in range(30)), 5),
        (PLAN, 24),
        ("Primero\nCola sin punto\n\n# Encabezado\nCuerpo.", 2),
        ("  # sangrado\n  Texto", 24),
        ("www.ejemplo.com\nhttps://ejemplo.com", 24),
        ("Línea " + "x" * 80 + "\nOtra", 24),
    ]


def _plain_build_cases(short_chars: int, file_chars: int) -> list[tuple[str, Any, str]]:
    return [
        ("", "none", "none"),
        ("   ", "none", "none"),
        ("Rojo\nAzul\nAmarillo", "none", "none"),
        ("a" * short_chars, "none", "none"),
        ("a" * (short_chars + 1), "none", "none"),
        (SHORT_PLAN, "none", "path"),
        (PLAN, "none", "path"),
        (PLAN, "none", "none"),
        ("x " * file_chars, "none", "path"),
        ("Plan con {output} literal", "none", "none"),
        ("Texto con ñandú y emoji 🙂", "none", "none"),
        ("Documento\n\n# Sección\nTexto que termina en punto.", "none", "path"),
        ("Documento\n\n# Sección\nTexto que termina en punto.", "none", "none"),
        ("", "none", "raise"),
        ("  ", "none", "raise"),
    ]


def _work_build_cases() -> list[tuple[str, Any, str]]:
    lines = [f"linea {index}" for index in range(1, 31)]
    numbered = "Ran 1 shell command\n" + "\n".join(lines)
    hostile = "\n".join(f"• Ran c{index}\nl{index}" for index in range(15)) + "\n• Wrote x\n● Fin\n"
    long_tail = "Ran 1 shell command\n" + "\n".join(
        f"resultado {index:04d}" for index in range(400)
    )
    short_brief = "Ran 1 shell command\n● Listo.\n"
    weak = "Ran 1 shell command\n$ uv sync\ntodo listo\n"
    return [
        (WORK_LOG, "none", "path"),
        (WORK_LOG, "none", "none"),
        (short_brief, "none", "path"),
        (weak, "none", "none"),
        ("Ran 1 shell command", "none", "none"),
        ("Ran 1 shell command", "none", "path"),
        (numbered, 25, "none"),
        (numbered, "none", "none"),
        (numbered, "zero", "none"),
        (numbered, "negative", "none"),
        (numbered, "float", "none"),
        (numbered, "str", "none"),
        (numbered, "none_value", "none"),
        (numbered, "raise", "none"),
        (numbered, "out_of_range", "none"),
        (numbered, "len", "none"),
        (hostile, "none", "none"),
        (hostile, "str", "none"),
        (long_tail, "none", "none"),
        ("Ran 1 shell command\n● Uno\n● Dos\n● Tres", "none", "none"),
        ("● Created archivo.txt\nCreé archivo.txt.", "none", "none"),
        ("Unicode: ñandú 🙂\nRan 1 shell command\nFin con tilde: ñ", "none", "none"),
    ]


def _packet_text_cases() -> list[dict[str, str]]:
    return [
        {"kind": "short", "text": "hola", "spoken": "hola", "full": "hola", "saved_path": ""},
        {"kind": "short", "text": "", "spoken": "", "full": "", "saved_path": ""},
        {"kind": "report", "text": "Fin.", "spoken": "Fin.", "full": "log\nFin.", "saved_path": ""},
        {"kind": "document", "text": "Índice", "spoken": "Índice", "full": "x", "saved_path": ""},
        {
            "kind": "report",
            "text": "Fin.",
            "spoken": "Fin.",
            "full": "log\nFin.",
            "saved_path": SAVED_PATH,
        },
        {
            "kind": "document",
            "text": "Índice",
            "spoken": "Índice",
            "full": "x" * 100,
            "saved_path": SAVED_PATH,
        },
        {
            "kind": "short",
            "text": "hola",
            "spoken": "hola",
            "full": "hola",
            "saved_path": SAVED_PATH,
        },
        {
            "kind": "otro",
            "text": "cosa",
            "spoken": "cosa",
            "full": "cosa",
            "saved_path": "C:/tmp/otro.md",
        },
        {"kind": "report", "text": "", "spoken": "", "full": "", "saved_path": "C:/tmp/x.md"},
        {
            "kind": "document",
            "text": "Línea ñ\nSegunda",
            "spoken": "Línea ñ Segunda",
            "full": "Línea ñ\nSegunda",
            "saved_path": "C:/tmp/ñ.md",
        },
        {
            "kind": "report",
            "text": "Emoji 🙂",
            "spoken": "Emoji 🙂",
            "full": "Emoji 🙂",
            "saved_path": "C:/tmp/emoji.md",
        },
        {
            "kind": "report",
            "text": "a" * 500,
            "spoken": "a" * 500,
            "full": "b" * 900,
            "saved_path": "C:/tmp/largo.md",
        },
        {
            "kind": "document",
            "text": "Contenido:\n- Uno\n- Dos",
            "spoken": "Contenido: - Uno - Dos",
            "full": "Contenido:\n- Uno\n- Dos",
            "saved_path": " ",
        },
        {
            "kind": "report",
            "text": "Texto\ncon\nsaltos",
            "spoken": "Texto con saltos",
            "full": "Texto\ncon\nsaltos",
            "saved_path": "C:/tmp/a b.md",
        },
        {
            "kind": "document",
            "text": '{"json": true}',
            "spoken": '{"json": true}',
            "full": '{"json": true}',
            "saved_path": "C:/tmp/d.json",
        },
        {
            "kind": "report",
            "text": "Fin.",
            "spoken": "Fin.",
            "full": "log",
            "saved_path": "",
        },
        {
            "kind": "document",
            "text": "Plan",
            "spoken": "Plan",
            "full": "Plan",
            "saved_path": "C:/tmp/plan.md",
        },
        {
            "kind": "short",
            "text": "x",
            "spoken": "x",
            "full": "x",
            "saved_path": "C:/tmp/x.md",
        },
        {
            "kind": "report",
            "text": "",
            "spoken": "",
            "full": "",
            "saved_path": "",
        },
        {
            "kind": "document",
            "text": 'Texto con \\ barra y "comillas"',
            "spoken": 'Texto con \\ barra y "comillas"',
            "full": 'Texto con \\ barra y "comillas"',
            "saved_path": "C:/tmp/comillas.md",
        },
        {
            "kind": "report",
            "text": "Uno\nDos\nTres",
            "spoken": "Uno Dos Tres",
            "full": "log",
            "saved_path": "C:/tmp/tres.md",
        },
        {
            "kind": "document",
            "text": "Índice",
            "spoken": "Índice",
            "full": "x",
            "saved_path": "C:/tmp/índice.md",
        },
    ]


def _packet(packet: Any) -> dict[str, Any]:
    return {
        "kind": packet.kind,
        "text": packet.text,
        "spoken": packet.spoken,
        "full": packet.full,
        "saved_path": packet.saved_path,
    }


def _emit_has_work(out: FixtureFile, module: Any) -> None:
    for index, text in enumerate(_has_work_cases(), start=1):
        out.add(
            f"handoff_packet/work/{index:03d}",
            "has_work",
            {"text": text},
            module.has_work(text),
        )


def _emit_outline(out: FixtureFile, module: Any) -> None:
    for index, (text, limit) in enumerate(_outline_cases(), start=1):
        out.add(
            f"handoff_packet/outline/{index:03d}",
            "outline",
            {"text": text, "limit": limit},
            {"text": module.outline(text, limit)},
        )


def _emit_build(out: FixtureFile, module: Any) -> None:
    cases = _plain_build_cases(module.SHORT_CHARS, module.DOCUMENT_FILE_CHARS)
    cases += _work_build_cases()
    for index, (answer, select_spec, save_spec) in enumerate(cases, start=1):
        calls: list[str] = []
        kwargs: dict[str, Any] = {}
        select = _select_callback(select_spec)
        if select is not None:
            kwargs["select"] = select
        save = _save_callback(save_spec, calls)
        if save is not None:
            kwargs["save"] = save
        result = module.build_packet(answer, **kwargs)
        out.add(
            f"handoff_packet/build/{index:03d}",
            "build_packet",
            {"answer": answer, "select": select_spec, "save": save_spec},
            {**_packet(result), "saved_calls": calls},
        )


def _emit_packet_text(out: FixtureFile, module: Any) -> None:
    for index, fields in enumerate(_packet_text_cases(), start=1):
        out.add(
            f"handoff_packet/text/{index:03d}",
            "packet_text",
            fields,
            {"text": module.packet_text(module.Packet(**fields))},
        )


def _emit_errors(out: FixtureFile, module: Any) -> None:
    out.add_error(
        "handoff_packet/work/err/none",
        "has_work",
        {"text": None},
        _error_text(module.has_work, None),
    )
    out.add_error(
        "handoff_packet/work/err/int",
        "has_work",
        {"text": 7},
        _error_text(module.has_work, 7),
    )
    out.add_error(
        "handoff_packet/outline/err/none",
        "outline",
        {"text": None, "limit": 24},
        _error_text(module.outline, None, 24),
    )
    out.add_error(
        "handoff_packet/text/err/none",
        "packet_text",
        {"packet": None},
        _error_text(module.packet_text, None),
    )
    out.add_error(
        "handoff_packet/build/err/save-document",
        "build_packet",
        {"answer": PLAN, "select": "none", "save": "raise"},
        _error_text(module.build_packet, PLAN, save=_save_callback("raise", [])),
    )
    log_answer = "\n".join(f"• Ran c{index}\nsalida {index}" for index in range(30)) + "\n● Fin."
    out.add_error(
        "handoff_packet/build/err/save-report",
        "build_packet",
        {"answer": log_answer, "select": "none", "save": "raise"},
        _error_text(module.build_packet, log_answer, save=_save_callback("raise", [])),
    )


def generate(out: FixtureFile) -> None:
    from ezagent.agent import handoff_packet as module

    _emit_has_work(out, module)
    _emit_outline(out, module)
    _emit_build(out, module)
    _emit_packet_text(out, module)
    _emit_errors(out, module)


def _error_text(fn: Callable[..., Any], *args: Any, **kwargs: Any) -> str:
    try:
        fn(*args, **kwargs)
    except Exception as error:  # noqa: BLE001 - el oráculo guarda el mensaje real
        return str(error)
    raise AssertionError(f"{fn.__name__} no falló")


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
