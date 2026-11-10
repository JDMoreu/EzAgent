#!/usr/bin/env python3
"""Herramientas sencillas para hojas de cálculo .xlsx con openpyxl.

Subcomandos:
    summary <xlsx>
    read    <xlsx> [--sheet Nombre] [--max-rows 50]
    create  <xlsx> --data datos.json [--sheet Hoja1]
    append  <xlsx> --data datos.json [--sheet Hoja1]

``openpyxl`` se importa de forma perezosa.  Si falta, el script devuelve un
error claro en español y no instala nada por su cuenta.  ``create`` nunca
sobrescribe un archivo existente; ``append`` modifica el libro indicado.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

MAX_ROWS = 20_000
MAX_COLUMNS = 50
MAX_READ_ROWS = 500


class ToolError(Exception):
    """Error con mensaje apto para el usuario."""


def _load_openpyxl():
    try:
        import openpyxl  # type: ignore[import-not-found]
    except ImportError as exc:
        raise ToolError(
            "Falta openpyxl. Instálalo con «pip install openpyxl»; este script "
            "no instala dependencias por su cuenta."
        ) from exc
    return openpyxl


def _check_input(path: Path) -> None:
    if not path.is_file():
        raise ToolError(f"No existe el archivo: {path}")


def _check_output(path: Path) -> None:
    if path.exists():
        raise ToolError(f"El archivo de salida ya existe: {path}. Usa otro nombre.")


def _read_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ToolError(f"No se pudo leer el JSON de datos: {exc}") from exc


def _normalise_rows(data) -> list[list[object]]:
    if isinstance(data, list):
        rows: list[list[object]] = []
        if not data:
            return rows
        if all(isinstance(item, dict) for item in data):
            headers = list(data[0].keys())
            rows.append(list(headers))
            for item in data:
                rows.append([item.get(header) for header in headers])
            return rows
        for item in data:
            if not isinstance(item, list):
                raise ToolError("Cada fila del JSON debe ser una lista o un objeto.")
            rows.append(list(item))
        return rows
    raise ToolError("El JSON debe ser una lista de filas o un objeto {hoja: filas}.")


def _validate_rows(rows: list[list[object]], sheet: str) -> None:
    if len(rows) > MAX_ROWS:
        raise ToolError(f"La hoja «{sheet}» supera el máximo de {MAX_ROWS} filas.")
    for row in rows:
        if len(row) > MAX_COLUMNS:
            raise ToolError(f"La hoja «{sheet}» supera el máximo de {MAX_COLUMNS} columnas.")


def _collect_sheets(data) -> dict[str, list[list[object]]]:
    if isinstance(data, dict):
        sheets: dict[str, list[list[object]]] = {}
        for name, value in data.items():
            if not isinstance(name, str) or not name.strip():
                raise ToolError("Los nombres de hoja deben ser texto no vacío.")
            rows = _normalise_rows(value)
            _validate_rows(rows, name)
            sheets[name] = rows
        if not sheets:
            raise ToolError("El JSON no contiene ninguna hoja.")
        return sheets
    rows = _normalise_rows(data)
    _validate_rows(rows, "Hoja1")
    return {"Hoja1": rows}


def command_summary(args: argparse.Namespace) -> int:
    openpyxl = _load_openpyxl()
    path = Path(args.xlsx).expanduser()
    _check_input(path)
    workbook = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
    try:
        print(f"Archivo: {path}")
        for sheet in workbook.worksheets:
            print(f"- {sheet.title}: {sheet.max_row or 0} filas x {sheet.max_column or 0} columnas")
            first_row = []
            for cell in next(sheet.iter_rows(min_row=1, max_row=1), ()):
                value = getattr(cell, "value", None)
                if value is not None:
                    first_row.append(str(value))
            if first_row:
                print("  Primera fila: " + " | ".join(first_row[:MAX_COLUMNS]))
    finally:
        workbook.close()
    return 0


def command_read(args: argparse.Namespace) -> int:
    openpyxl = _load_openpyxl()
    path = Path(args.xlsx).expanduser()
    _check_input(path)
    limit = max(1, min(int(args.max_rows), MAX_READ_ROWS))
    workbook = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
    try:
        if args.sheet:
            if args.sheet not in workbook.sheetnames:
                raise ToolError(
                    f"No existe la hoja «{args.sheet}». "
                    f"Hojas disponibles: {', '.join(workbook.sheetnames)}"
                )
            sheet = workbook[args.sheet]
        else:
            sheet = workbook.active
        rows = []
        for index, row in enumerate(sheet.iter_rows(values_only=True)):
            if index >= limit:
                break
            rows.append([_json_value(value) for value in row])
    finally:
        workbook.close()
    print(json.dumps(rows, ensure_ascii=False))
    return 0


def command_create(args: argparse.Namespace) -> int:
    openpyxl = _load_openpyxl()
    destination = Path(args.xlsx).expanduser()
    _check_output(destination)
    data = _read_json(Path(args.data).expanduser())
    sheets = _collect_sheets(data)
    if args.sheet and list(sheets) == ["Hoja1"]:
        sheets = {args.sheet: sheets["Hoja1"]}

    workbook = openpyxl.Workbook()
    default_sheet = workbook.active
    first = True
    for name, rows in sheets.items():
        if first:
            worksheet = default_sheet
            worksheet.title = name
            first = False
        else:
            worksheet = workbook.create_sheet(title=name)
        for row in rows:
            worksheet.append(row)
    destination.parent.mkdir(parents=True, exist_ok=True)
    workbook.save(str(destination))
    print(f"Excel creado: {destination}")
    return 0


def command_append(args: argparse.Namespace) -> int:
    openpyxl = _load_openpyxl()
    destination = Path(args.xlsx).expanduser()
    _check_input(destination)
    data = _read_json(Path(args.data).expanduser())
    rows = _normalise_rows(data)
    _validate_rows(rows, args.sheet)

    workbook = openpyxl.load_workbook(str(destination))
    try:
        if args.sheet in workbook.sheetnames:
            worksheet = workbook[args.sheet]
        else:
            worksheet = workbook.create_sheet(title=args.sheet)
        for row in rows:
            worksheet.append(row)
        workbook.save(str(destination))
    finally:
        workbook.close()
    print(f"Añadidas {len(rows)} filas a «{args.sheet}» en: {destination}")
    return 0


def _json_value(value):
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Herramientas para hojas de cálculo .xlsx.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    summary = subparsers.add_parser("summary", help="Resume hojas y tamaños.")
    summary.add_argument("xlsx")
    summary.set_defaults(func=command_summary)

    read = subparsers.add_parser("read", help="Lee filas como JSON.")
    read.add_argument("xlsx")
    read.add_argument("--sheet", default=None)
    read.add_argument("--max-rows", default=50)
    read.set_defaults(func=command_read)

    create = subparsers.add_parser("create", help="Crea un .xlsx nuevo.")
    create.add_argument("xlsx")
    create.add_argument("--data", required=True, help="JSON con filas u hojas.")
    create.add_argument("--sheet", default=None)
    create.set_defaults(func=command_create)

    append = subparsers.add_parser("append", help="Añade filas a un .xlsx.")
    append.add_argument("xlsx")
    append.add_argument("--data", required=True, help="JSON con filas.")
    append.add_argument("--sheet", default="Hoja1")
    append.set_defaults(func=command_append)
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
