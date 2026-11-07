#!/usr/bin/env python3
"""Convierte Markdown sencillo a un documento .docx.

Uso:
    md_to_docx.py entrada.md salida.docx [--title "Título"]

Solo admite títulos, párrafos, viñetas, listas numeradas, bloques de código y
negritas en línea.  ``python-docx`` se importa de forma perezosa: si falta, el
script devuelve un error claro en español y no intenta instalar nada.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

_HEADING_RE = re.compile(r"^(#{1,6})\s+(.*)$")
_BULLET_RE = re.compile(r"^\s*[-*+]\s+(.*)$")
_NUMBER_RE = re.compile(r"^\s*\d+[.)]\s+(.*)$")
_FENCE_RE = re.compile(r"^\s*```")
_INLINE_RE = re.compile(r"(\*\*.+?\*\*|`[^`]+`)")


class ConversionError(Exception):
    """Error de conversión con mensaje para el usuario."""


def _load_docx():
    try:
        import docx  # type: ignore[import-not-found]
        from docx.shared import Pt  # type: ignore[import-not-found]
    except ImportError as exc:
        raise ConversionError(
            "Falta python-docx. Instálalo con «pip install python-docx»; "
            "este script no instala dependencias por su cuenta."
        ) from exc
    return docx, Pt


def _add_inline(paragraph, text: str) -> None:
    for token in _INLINE_RE.split(text):
        if not token:
            continue
        if token.startswith("**") and token.endswith("**") and len(token) > 4:
            run = paragraph.add_run(token[2:-2])
            run.bold = True
        elif token.startswith("`") and token.endswith("`") and len(token) > 2:
            run = paragraph.add_run(token[1:-1])
            run.font.name = "Consolas"
        else:
            paragraph.add_run(token)


def _add_code(document, lines: list[str], Pt) -> None:
    if not lines:
        return
    for line in lines:
        paragraph = document.add_paragraph(style="No Spacing")
        run = paragraph.add_run(line if line else " ")
        run.font.name = "Consolas"
        run.font.size = Pt(9)


def build_document(markdown: str, title: str | None = None):
    docx, Pt = _load_docx()
    document = docx.Document()
    if title:
        document.core_properties.title = title
        document.add_heading(title, level=0)
    else:
        document.core_properties.title = "Documento EzAgent"

    paragraph_lines: list[str] = []
    code_lines: list[str] = []
    in_code = False

    def flush_paragraph() -> None:
        if not paragraph_lines:
            return
        paragraph = document.add_paragraph()
        _add_inline(paragraph, " ".join(paragraph_lines))
        paragraph_lines.clear()

    for raw_line in markdown.splitlines():
        line = raw_line.rstrip()
        if _FENCE_RE.match(line):
            if in_code:
                _add_code(document, code_lines, Pt)
                code_lines.clear()
                in_code = False
            else:
                flush_paragraph()
                in_code = True
            continue
        if in_code:
            code_lines.append(line)
            continue

        if not line.strip():
            flush_paragraph()
            continue

        heading = _HEADING_RE.match(line)
        if heading:
            flush_paragraph()
            level = min(len(heading.group(1)), 4)
            document.add_heading(heading.group(2).strip(), level=level)
            continue

        bullet = _BULLET_RE.match(line)
        if bullet:
            flush_paragraph()
            paragraph = document.add_paragraph(style="List Bullet")
            _add_inline(paragraph, bullet.group(1).strip())
            continue

        numbered = _NUMBER_RE.match(line)
        if numbered:
            flush_paragraph()
            paragraph = document.add_paragraph(style="List Number")
            _add_inline(paragraph, numbered.group(1).strip())
            continue

        paragraph_lines.append(line.strip())

    flush_paragraph()
    if in_code:
        _add_code(document, code_lines, Pt)
    return document


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Convierte Markdown sencillo a un archivo .docx."
    )
    parser.add_argument("input", help="Archivo Markdown de entrada.")
    parser.add_argument("output", help="Archivo .docx de salida (no debe existir).")
    parser.add_argument("--title", default=None, help="Título opcional del documento.")
    args = parser.parse_args(argv)

    source = Path(args.input).expanduser()
    destination = Path(args.output).expanduser()
    try:
        if not source.is_file():
            raise ConversionError(f"No existe el archivo de entrada: {source}")
        if destination.suffix.lower() != ".docx":
            raise ConversionError("La salida debe tener extensión .docx.")
        if destination.exists():
            raise ConversionError(
                f"El archivo de salida ya existe: {destination}. Usa otro nombre."
            )
        try:
            markdown = source.read_text(encoding="utf-8-sig")
        except UnicodeDecodeError as exc:
            raise ConversionError(
                "El Markdown no está en UTF-8 válido."
            ) from exc
        document = build_document(markdown, args.title)
        destination.parent.mkdir(parents=True, exist_ok=True)
        document.save(str(destination))
    except ConversionError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 2
    except OSError as exc:
        print(f"Error: no se pudo escribir el documento: {exc}", file=sys.stderr)
        return 2

    print(f"Documento creado: {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
