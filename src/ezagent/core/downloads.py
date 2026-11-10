"""Gestor de descargas: recursos externos verificados por SHA256.

Todo se guarda bajo ``data_dir()`` (``%LOCALAPPDATA%\\EzAgent`` o
``EZAGENT_HOME``). Cada descarga va a un archivo ``.part``, se reanuda con
``Range`` si se interrumpió, se verifica el hash completo y solo entonces se
renombra de forma atómica. Un hash incorrecto borra el ``.part``.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import tomllib
import zipfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

import httpx
from loguru import logger

ProgressFn = Callable[[str, int, int], None]  # (asset_id, hecho, total)

_MANIFEST = Path(__file__).with_name("manifest.toml")
_CHUNK = 1024 * 256


class DownloadError(RuntimeError):
    """Fallo de descarga, verificación o extracción."""


@dataclass(frozen=True)
class Asset:
    id: str
    label: str
    url: str
    sha256: str
    size: int
    dest: str
    unzip: bool = False
    strip_prefix: str = ""
    license: str = ""
    optional: bool = False


def data_dir() -> Path:
    """Carpeta de datos de EzAgent (se crea si falta)."""
    explicit = os.getenv("EZAGENT_HOME")
    if explicit:
        root = Path(explicit).expanduser()
    else:
        base = os.getenv("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
        root = Path(base) / "EzAgent"
    root.mkdir(parents=True, exist_ok=True)
    return root


def load_manifest(path: Path | None = None) -> dict[str, Asset]:
    raw = tomllib.loads((path or _MANIFEST).read_text(encoding="utf-8"))
    assets: dict[str, Asset] = {}
    for entry in raw.get("asset", []):
        asset = Asset(
            id=entry["id"],
            label=entry.get("label", entry["id"]),
            url=entry["url"],
            sha256=entry["sha256"].lower(),
            size=int(entry["size"]),
            dest=entry["dest"],
            unzip=bool(entry.get("unzip", False)),
            strip_prefix=entry.get("strip_prefix", ""),
            license=entry.get("license", ""),
            optional=bool(entry.get("optional", False)),
        )
        if not asset.url.startswith("https://"):
            raise DownloadError(f"{asset.id}: solo se permiten URLs https")
        assets[asset.id] = asset
    return assets


def asset_path(asset: Asset, root: Path | None = None) -> Path:
    return (root or data_dir()) / asset.dest


def _marker(asset: Asset, root: Path | None) -> Path:
    return asset_path(asset, root) / f".installed-{asset.sha256[:12]}"


def is_installed(asset: Asset, root: Path | None = None) -> bool:
    target = asset_path(asset, root)
    if asset.unzip:
        return _marker(asset, root).is_file()
    return target.is_file() and target.stat().st_size == asset.size


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(_CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def _fetch(asset: Asset, part: Path, client: httpx.Client, progress: ProgressFn | None) -> None:
    start = part.stat().st_size if part.exists() else 0
    if start >= asset.size:  # parte inválida o completa: se descarta y se re-verifica
        part.unlink(missing_ok=True)
        start = 0
    headers = {"Range": f"bytes={start}-"} if start else {}
    with client.stream("GET", asset.url, headers=headers, follow_redirects=True) as resp:
        if resp.status_code == 200:
            start = 0  # el servidor ignoró Range: empezar de cero
        elif resp.status_code != 206:
            raise DownloadError(f"{asset.id}: HTTP {resp.status_code}")
        done = start
        with part.open("ab" if start else "wb") as out:
            for chunk in resp.iter_bytes(_CHUNK):
                out.write(chunk)
                done += len(chunk)
                if progress:
                    progress(asset.id, done, asset.size)


def _extract(asset: Asset, archive: Path, root: Path | None) -> None:
    target = asset_path(asset, root)
    target.mkdir(parents=True, exist_ok=True)
    base = target.resolve()
    with zipfile.ZipFile(archive) as zf:
        for info in zf.infolist():
            name = info.filename
            if asset.strip_prefix:
                if not name.startswith(asset.strip_prefix):
                    continue
                name = name[len(asset.strip_prefix) :]
            if not name or name.endswith("/"):
                continue
            out = (target / name).resolve()
            if base not in out.parents:
                raise DownloadError(f"{asset.id}: ruta insegura en el zip: {info.filename}")
            out.parent.mkdir(parents=True, exist_ok=True)
            with zf.open(info) as src, out.open("wb") as dst:
                shutil.copyfileobj(src, dst)
    _marker(asset, root).write_text(asset.sha256, encoding="utf-8")


def download(
    asset: Asset,
    *,
    root: Path | None = None,
    client: httpx.Client | None = None,
    progress: ProgressFn | None = None,
    force: bool = False,
) -> Path:
    """Descarga y verifica ``asset``; devuelve su ruta. Reintenta una vez."""
    if is_installed(asset, root) and not force:
        return asset_path(asset, root)
    base = root or data_dir()
    own_client = client is None
    http = client or httpx.Client(timeout=httpx.Timeout(30.0, read=60.0))
    final = base / (asset.dest + ".zip" if asset.unzip else asset.dest)
    final.parent.mkdir(parents=True, exist_ok=True)
    part = final.with_suffix(final.suffix + ".part")
    try:
        for attempt in (1, 2):
            try:
                _fetch(asset, part, http, progress)
            except httpx.HTTPError as exc:
                raise DownloadError(f"{asset.id}: {exc}") from exc
            if _sha256(part) == asset.sha256:
                break
            part.unlink(missing_ok=True)
            logger.warning(f"{asset.id}: hash incorrecto (intento {attempt})")
            if attempt == 2:
                raise DownloadError(f"{asset.id}: el hash no coincide; descarga descartada")
        part.replace(final)
        if asset.unzip:
            try:
                _extract(asset, final, root)
            finally:
                final.unlink(missing_ok=True)
        return asset_path(asset, root)
    finally:
        if own_client:
            http.close()


def adopt(asset: Asset, source: Path, *, root: Path | None = None) -> bool:
    """Copia un archivo existente (p. ej. una caché antigua) si su hash es el oficial.

    Devuelve True si quedó instalado. No toca ``source``.
    """
    if asset.unzip or not source.is_file() or source.stat().st_size != asset.size:
        return False
    if _sha256(source) != asset.sha256:
        return False
    final = asset_path(asset, root)
    final.parent.mkdir(parents=True, exist_ok=True)
    part = final.with_suffix(final.suffix + ".part")
    shutil.copyfile(source, part)
    part.replace(final)
    return True


def ensure(
    asset_id: str,
    *,
    root: Path | None = None,
    manifest: dict[str, Asset] | None = None,
    **kwargs,
) -> Path:
    assets = manifest or load_manifest()
    if asset_id not in assets:
        raise DownloadError(f"Recurso desconocido: {asset_id}")
    return download(assets[asset_id], root=root, **kwargs)


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="python -m ezagent.core.downloads")
    parser.add_argument("--list", action="store_true", help="Muestra el estado")
    parser.add_argument("--all", action="store_true", help="Descarga lo necesario")
    parser.add_argument("--with-optional", action="store_true")
    parser.add_argument("ids", nargs="*")
    args = parser.parse_args(argv)
    assets = load_manifest()
    if args.list or not (args.all or args.ids):
        for a in assets.values():
            state = "instalado" if is_installed(a) else "falta"
            extra = " (opcional)" if a.optional else ""
            print(f"{a.id:22} {state:10} {a.size / 1e6:8.1f} MB  {a.label}{extra}")
        return 0
    wanted = args.ids or [a.id for a in assets.values() if args.with_optional or not a.optional]
    last = {"pct": -1}

    def show(asset_id: str, done: int, total: int) -> None:
        pct = int(done * 100 / max(total, 1))
        if pct != last["pct"] and pct % 10 == 0:
            last["pct"] = pct
            print(f"  {asset_id}: {pct}%", flush=True)

    for asset_id in wanted:
        print(f"{asset_id}…", flush=True)
        try:
            path = ensure(asset_id, manifest=assets, progress=show)
        except DownloadError as exc:
            print(f"ERROR: {exc}")
            return 1
        print(f"  listo: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
