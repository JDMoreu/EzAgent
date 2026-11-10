"""Imágenes y documentos del chat: qué se puede mostrar y qué se puede abrir.

El modelo decide qué enlaces escribe, pero el usuario decide qué se abre:

* Las imágenes se muestran solo si son archivos locales, dentro de las carpetas
  de lectura permitidas, de un tipo de imagen y de un tamaño razonable. Las de la
  web **no se descargan solas** (una petición revelaría la IP y podría rastrear).
* Los documentos son tarjetas con nombre, tipo, tamaño y fecha, y una vista previa
  de texto para formatos de texto. «Abrir» solo funciona con tipos de documento
  conocidos y con un clic; nunca con ejecutables ni scripts.
* Aquí no se borra ni se modifica nada, y nada se ejecuta por su cuenta.
"""

from __future__ import annotations

import csv
import io
import os
import subprocess
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Final, Literal
from urllib.parse import unquote, urlparse

from loguru import logger
from PIL import Image

IMAGE_EXTENSIONS: Final[frozenset[str]] = frozenset(
    {".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"}
)
TEXT_EXTENSIONS: Final[frozenset[str]] = frozenset(
    {".md", ".markdown", ".txt", ".csv", ".tsv", ".json", ".log", ".toml", ".yaml", ".yml"}
)
OFFICE_EXTENSIONS: Final[frozenset[str]] = frozenset(
    {".pdf", ".docx", ".doc", ".xlsx", ".xls", ".pptx", ".ppt", ".odt", ".ods", ".odp", ".rtf"}
)
# Lo único que «Abrir» acepta: nunca .exe, .bat, .cmd, .ps1, .lnk, .js, .vbs…
OPENABLE_EXTENSIONS: Final[frozenset[str]] = IMAGE_EXTENSIONS | TEXT_EXTENSIONS | OFFICE_EXTENSIONS
MAX_IMAGE_BYTES: Final[int] = 25 * 1024 * 1024
MAX_IMAGE_PIXELS: Final[int] = 40_000_000  # frente a «bombas» de descompresión
MAX_PREVIEW_BYTES: Final[int] = 1024 * 1024
PREVIEW_CHARS: Final[int] = 1800
PREVIEW_ROWS: Final[int] = 8

Kind = Literal["image", "text", "office", "other"]


@dataclass(frozen=True, slots=True)
class FileInfo:
    """Datos de un archivo local para pintar su tarjeta (sin leer su contenido)."""

    path: Path
    name: str
    ext: str
    size: int
    mtime: float
    kind: Kind
    allowed: bool  # dentro de las carpetas de lectura permitidas
    openable: bool  # tipo seguro de abrir con un clic


def resolve_target(target: str, base_dirs: Iterable[Path] = ()) -> Path | None:
    """Ruta absoluta de un enlace local (``C:\\x``, ``file:///x``, ``~/x``, relativo)."""
    text = str(target or "").strip().strip("<>")
    if not text:
        return None
    if text.lower().startswith("file:"):
        parsed = urlparse(text)
        raw = unquote(parsed.path)
        if parsed.netloc and parsed.netloc != "localhost":
            raw = f"//{parsed.netloc}{raw}"  # \\servidor\recurso
        elif len(raw) > 2 and raw[0] == "/" and raw[2] == ":":
            raw = raw[1:]  # /C:/x -> C:/x
        text = raw
    path = Path(text).expanduser()
    if path.is_absolute():
        return path
    for base in base_dirs:
        candidate = Path(base) / path
        if candidate.exists():
            return candidate
    return None


def is_within(path: Path, roots: Iterable[Path | str]) -> bool:
    """¿Está ``path`` dentro de alguna de las carpetas ``roots``? (resuelve enlaces y ``..``)."""
    try:
        resolved = path.resolve()
    except OSError:
        return False
    for root in roots:
        try:
            resolved.relative_to(Path(root).expanduser().resolve())
            return True
        except (ValueError, OSError):
            continue
    return False


def kind_of(ext: str) -> Kind:
    ext = ext.lower()
    if ext in IMAGE_EXTENSIONS:
        return "image"
    if ext in TEXT_EXTENSIONS:
        return "text"
    if ext in OFFICE_EXTENSIONS:
        return "office"
    return "other"


def file_info(path: Path | None, roots: Iterable[Path | str] | None = None) -> FileInfo | None:
    """Ficha del archivo, o ``None`` si no existe o no es un archivo normal."""
    if path is None:
        return None
    try:
        if not path.is_file():
            return None
        stat = path.stat()
    except OSError:
        return None
    ext = path.suffix.lower()
    allowed = True if roots is None else is_within(path, roots)
    return FileInfo(
        path=path,
        name=path.name,
        ext=ext,
        size=int(stat.st_size),
        mtime=float(stat.st_mtime),
        kind=kind_of(ext),
        allowed=allowed,
        openable=ext in OPENABLE_EXTENSIONS,
    )


def human_size(size: int) -> str:
    """``1,4 MB`` / ``820 KB`` / ``12 B``."""
    value = float(max(0, size))
    for unit in ("B", "KB", "MB", "GB"):
        if value < 1024 or unit == "GB":
            text = (
                f"{value:.0f}" if unit == "B" or value >= 100 else f"{value:.1f}".replace(".", ",")
            )
            return f"{text} {unit}"
        value /= 1024
    return f"{size} B"


def load_image(info: FileInfo | None) -> Image.Image | None:
    """Imagen lista para dibujar (RGBA), o ``None`` si no se puede o no se debe mostrar."""
    if info is None or info.kind != "image" or not info.allowed or info.size > MAX_IMAGE_BYTES:
        return None
    try:
        with Image.open(info.path) as probe:
            width, height = probe.size
            if width * height > MAX_IMAGE_PIXELS or width < 1 or height < 1:
                return None
            probe.seek(0)  # GIF animado: solo el primer fotograma
            return probe.convert("RGBA")
    except Exception as exc:  # noqa: BLE001
        logger.warning("No pude abrir la imagen {}: {}", info.name, exc)
        return None


def text_preview(info: FileInfo | None) -> tuple[str, bool] | None:
    """``(texto, hay_más)`` con el principio de un archivo de texto; ``None`` si no aplica."""
    if info is None or info.kind != "text" or not info.allowed or info.size > MAX_PREVIEW_BYTES * 8:
        return None
    try:
        with info.path.open("rb") as handle:
            raw = handle.read(MAX_PREVIEW_BYTES)
    except OSError:
        return None
    text = raw.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")
    text = "".join(ch for ch in text if ch == "\n" or ch == "\t" or ord(ch) >= 32)
    if info.ext in (".csv", ".tsv"):
        return _table_preview(text, "\t" if info.ext == ".tsv" else None, len(raw) < info.size)
    more = len(text) > PREVIEW_CHARS or len(raw) < info.size
    return text[:PREVIEW_CHARS].rstrip(), more


def _table_preview(text: str, delimiter: str | None, truncated: bool) -> tuple[str, bool]:
    """Un CSV se previsualiza como tabla de markdown (las primeras filas)."""
    try:
        dialect = csv.Sniffer().sniff(text[:2000], delimiters=delimiter or ",;\t|")
    except csv.Error:
        dialect = csv.excel
    rows = []
    for index, row in enumerate(csv.reader(io.StringIO(text), dialect)):
        if index > PREVIEW_ROWS:
            truncated = True
            break
        rows.append([cell.replace("|", "\\|").strip() for cell in row])
    if not rows or not rows[0]:
        return text[:PREVIEW_CHARS], truncated
    width = max(len(row) for row in rows)
    rows = [row + [""] * (width - len(row)) for row in rows]
    lines = ["| " + " | ".join(rows[0]) + " |", "|" + "---|" * width]
    lines += ["| " + " | ".join(row) + " |" for row in rows[1:]]
    return "\n".join(lines), truncated


def open_file(info: FileInfo | None) -> bool:
    """Abre un documento con la aplicación predeterminada. Solo tipos seguros."""
    if info is None or not info.openable or not info.allowed:
        return False
    try:
        os.startfile(str(info.path))  # type: ignore[attr-defined]  # noqa: S606
        return True
    except (OSError, AttributeError) as exc:
        logger.warning("No pude abrir {}: {}", info.name, exc)
        return False


def reveal_file(info: FileInfo | None) -> bool:
    """Muestra el archivo en el Explorador (lista de argumentos, sin shell)."""
    if info is None or not info.allowed:
        return False
    try:
        subprocess.Popen(["explorer", f"/select,{info.path}"])  # noqa: S603,S607
        return True
    except OSError as exc:
        logger.warning("No pude mostrar {}: {}", info.name, exc)
        return False
