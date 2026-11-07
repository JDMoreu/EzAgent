#!/usr/bin/env python3
"""Herramientas sencillas para archivos PDF con pypdf y reportlab.

Subcomandos:
    info    <pdf>
    extract <pdf> [--pages 1-3,5] [--output texto.txt]
    merge   <salida.pdf> <entrada1.pdf> <entrada2.pdf> ...
    split   <pdf> <carpeta_salida>
    create  <salida.pdf> <texto.txt> [--title "Título"]

Las dependencias se importan de forma perezosa.  Si faltan, el script devuelve
un error claro en español y no instala nada por su cuenta.  Nunca sobrescribe
un archivo existente.
"""

from __future__ import annotations

import argparse
import sys
import textwrap
from pathlib import Path

MAX_PAGES = 300


class ToolError(Exception):
    """Error con mensaje apto para el usuario."""


class MissingDependency(ToolError):
    """Falta una dependencia opcional."""


def _load_pypdf():
    try:
        import pypdf  # type: ignore[import-not-found]
    except ImportError as exc:
        raise MissingDependency(
            "Falta pypdf. Instálalo con «pip install pypdf»; este script no "
            "instala dependencias por su cuenta."
        ) from exc
    return pypdf


def _load_reportlab():
    try:
        from reportlab.lib.pagesizes import A4  # type: ignore[import-not-found]
        from reportlab.pdfgen import canvas  # type: ignore[import-not-found]
    except ImportError as exc:
        raise MissingDependency(
            "Falta reportlab. Instálalo con «pip install reportlab»; este "
            "script no instala dependencias por su cuenta."
        ) from exc
    return A4, canvas


def _check_input(path: Path) -> None:
    if not path.is_file():
        raise ToolError(f"No existe el archivo: {path}")


def _check_output(path: Path) -> None:
    if path.exists():
        raise ToolError(
            f"El archivo de salida ya existe: {path}. Usa otro nombre."
        )


def _check_size(path: Path) -> None:
    try:
        size_mb = path.stat().st_size / (1024 * 1024)
    except OSError as exc:
        raise ToolError(f"No se pudo leer {path}: {exc}") from exc
    if size_mb > 50:
        raise ToolError(
            f"El PDF supera el máximo de 50 MB ({size_mb:.1f} MB): {path}"
        )


def _parse_pages(specification: str | None, total: int) -> list[int]:
    if not specification or specification.strip().lower() == "all":
        return list(range(total))
    pages: list[int] = []
    for chunk in specification.split(","):
        chunk = chunk.strip()
        if not chunk:
            continue
        if "-" in chunk:
            start_text, end_text = chunk.split("-", 1)
            try:
                start = int(start_text)
                end = int(end_text)
            except ValueError as exc:
                raise ToolError(f"Rango de páginas inválido: «{chunk}»") from exc
            if start < 1 or end < start or end > total:
                raise ToolError(
                    f"Rango de páginas fuera de 1..{total}: «{chunk}»"
                )
            pages.extend(range(start - 1, end))
        else:
            try:
                page = int(chunk)
            except ValueError as exc:
                raise ToolError(f"Página inválida: «{chunk}»") from exc
            if page < 1 or page > total:
                raise ToolError(f"Página fuera de 1..{total}: «{chunk}»")
            pages.append(page - 1)
    if not pages:
        raise ToolError("No se indicó ninguna página válida.")
    return pages


def command_info(args: argparse.Namespace) -> int:
    pypdf = _load_pypdf()
    path = Path(args.pdf).expanduser()
    _check_input(path)
    _check_size(path)
    reader = pypdf.PdfReader(str(path))
    encrypted = bool(reader.is_encrypted)
    if encrypted:
        try:
            reader.decrypt("")
        except Exception:  # noqa: BLE001 - PDF cifrado sin contraseña vacía
            pass
    pages = len(reader.pages)
    metadata = reader.metadata or {}
    title = metadata.get("/Title", "") if hasattr(metadata, "get") else ""
    print(f"Archivo: {path}")
    print(f"Páginas: {pages}")
    print(f"Cifrado: {'sí' if encrypted else 'no'}")
    if title:
        print(f"Título: {title}")
    return 0


def command_extract(args: argparse.Namespace) -> int:
    pypdf = _load_pypdf()
    path = Path(args.pdf).expanduser()
    _check_input(path)
    _check_size(path)
    reader = pypdf.PdfReader(str(path))
    if reader.is_encrypted:
        try:
            reader.decrypt("")
        except Exception as exc:  # noqa: BLE001
            raise ToolError("El PDF está protegido y no se pudo abrir.") from exc
    if len(reader.pages) > MAX_PAGES:
        raise ToolError(f"El PDF supera el máximo de {MAX_PAGES} páginas.")
    selected = _parse_pages(args.pages, len(reader.pages))
    chunks: list[str] = []
    for index in selected:
        text = reader.pages[index].extract_text() or ""
        chunks.append(f"--- Página {index + 1} ---\n{text}".rstrip())
    result = "\n\n".join(chunks)
    if args.output:
        destination = Path(args.output).expanduser()
        _check_output(destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(result + "\n", encoding="utf-8")
        print(f"Texto extraído en: {destination}")
    else:
        print(result)
    return 0


def command_merge(args: argparse.Namespace) -> int:
    pypdf = _load_pypdf()
    destination = Path(args.output).expanduser()
    _check_output(destination)
    if len(args.inputs) > MAX_PAGES:
        raise ToolError(f"No se pueden unir más de {MAX_PAGES} archivos.")
    writer = pypdf.PdfWriter()
    total_pages = 0
    for raw in args.inputs:
        source = Path(raw).expanduser()
        _check_input(source)
        _check_size(source)
        reader = pypdf.PdfReader(str(source))
        if reader.is_encrypted:
            try:
                reader.decrypt("")
            except Exception as exc:  # noqa: BLE001
                raise ToolError(
                    f"El PDF está protegido y no se pudo abrir: {source}"
                ) from exc
        total_pages += len(reader.pages)
        if total_pages > MAX_PAGES:
            raise ToolError(f"El resultado supera {MAX_PAGES} páginas.")
        for page in reader.pages:
            writer.add_page(page)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("wb") as handle:
        writer.write(handle)
    print(f"PDF unido: {destination} ({total_pages} páginas)")
    return 0


def command_split(args: argparse.Namespace) -> int:
    pypdf = _load_pypdf()
    source = Path(args.pdf).expanduser()
    _check_input(source)
    _check_size(source)
    reader = pypdf.PdfReader(str(source))
    if reader.is_encrypted:
        try:
            reader.decrypt("")
        except Exception as exc:  # noqa: BLE001
            raise ToolError("El PDF está protegido y no se pudo abrir.") from exc
    if len(reader.pages) > MAX_PAGES:
        raise ToolError(f"El PDF supera el máximo de {MAX_PAGES} páginas.")

    output_dir = Path(args.output_dir).expanduser()
    output_dir.mkdir(parents=True, exist_ok=True)
    targets: list[Path] = []
    for index in range(len(reader.pages)):
        target = output_dir / f"{source.stem}_pagina_{index + 1:03d}.pdf"
        _check_output(target)
        targets.append(target)
    for index, target in enumerate(targets):
        writer = pypdf.PdfWriter()
        writer.add_page(reader.pages[index])
        with target.open("wb") as handle:
            writer.write(handle)
    print(f"PDF dividido en {len(targets)} archivos dentro de: {output_dir}")
    return 0


def command_create(args: argparse.Namespace) -> int:
    A4, canvas = _load_reportlab()
    source = Path(args.text).expanduser()
    _check_input(source)
    destination = Path(args.output).expanduser()
    _check_output(destination)
    try:
        content = source.read_text(encoding="utf-8-sig")
    except UnicodeDecodeError as exc:
        raise ToolError("El texto no está en UTF-8 válido.") from exc

    width, height = A4
    margin = 50
    line_height = 15
    destination.parent.mkdir(parents=True, exist_ok=True)
    pdf = canvas.Canvas(str(destination), pagesize=A4)
    if args.title:
        pdf.setTitle(args.title)
        pdf.setFont("Helvetica-Bold", 15)
        pdf.drawString(margin, height - margin, args.title)
        y = height - margin - 30
    else:
        pdf.setTitle("Documento EzAgent")
        y = height - margin

    pdf.setFont("Helvetica", 11)
    max_chars = max(20, int((width - 2 * margin) / 6))
    for raw_line in (content.splitlines() or [""]):
        wrapped = textwrap.wrap(raw_line, width=max_chars) or [""]
        for line in wrapped:
            if y < margin:
                pdf.showPage()
                pdf.setFont("Helvetica", 11)
                y = height - margin
            pdf.drawString(margin, y, line)
            y -= line_height
    pdf.save()
    print(f"PDF creado: {destination}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Herramientas para archivos PDF.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    info = subparsers.add_parser("info", help="Muestra páginas y metadatos.")
    info.add_argument("pdf")
    info.set_defaults(func=command_info)

    extract = subparsers.add_parser("extract", help="Extrae texto de páginas.")
    extract.add_argument("pdf")
    extract.add_argument("--pages", default=None, help="Ejemplo: 1-3,5 (por defecto, todas).")
    extract.add_argument("--output", default=None, help="Archivo .txt de salida.")
    extract.set_defaults(func=command_extract)

    merge = subparsers.add_parser("merge", help="Une varios PDF en uno.")
    merge.add_argument("output")
    merge.add_argument("inputs", nargs="+")
    merge.set_defaults(func=command_merge)

    split = subparsers.add_parser("split", help="Divide un PDF página a página.")
    split.add_argument("pdf")
    split.add_argument("output_dir")
    split.set_defaults(func=command_split)

    create = subparsers.add_parser("create", help="Crea un PDF desde un texto.")
    create.add_argument("output")
    create.add_argument("text")
    create.add_argument("--title", default=None)
    create.set_defaults(func=command_create)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return int(args.func(args))
    except (ToolError, OSError, ValueError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
