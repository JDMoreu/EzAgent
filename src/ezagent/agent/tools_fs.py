"""Herramientas nativas de archivos con política, diario y Papelera.

Todas las rutas pasan por :func:`ezagent.agent.paths.resolve_safe`. Las
escrituras se limitan a ``write_roots`` y las lecturas a ``read_roots``. El
borrado usa siempre ``send2trash`` (import perezoso) y nunca una eliminación
definitiva.

Los resúmenes que ve el usuario los genera este módulo a partir de las rutas y
cantidades reales; el LLM no los redacta.
"""

from __future__ import annotations

import os
import shutil
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from ezagent.agent.config import AgentConfig
from ezagent.agent.journal import Journal
from ezagent.agent.paths import PathDenied, resolve_safe
from ezagent.agent.registry import ToolInfo, ToolResult, ToolSpec

# Tope para contar elementos de una carpeta sin recorrer árboles enormes.
_COUNT_LIMIT = 5000


# ----------------------------------------------------------------------
# Utilidades internas
# ----------------------------------------------------------------------
class _FolderBulk(int):
    """Cantidad real de elementos de una carpeta que siempre exige confirmación fuerte.

    # DECISION: ToolInfo no tiene un campo «strong»; policy.decide solo mira
    # ``bulk > umbral`` y loop.run_turn usa el mismo ``bulk`` como número que
    # debe decir el usuario. Este entero conserva la cantidad real para el
    # resumen, la igualdad y la serialización, pero se considera mayor que
    # cualquier umbral distinto de cero. Así una carpeta pequeña exige
    # «confirmo N» con su cantidad real, sin falsear el resumen.
    """

    def __gt__(self, other: object) -> bool:
        if other == 0:
            return int(self) > 0
        return True


def _expanded(raw: str) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(str(raw)))).resolve()


def _resolve_read(config: AgentConfig, raw: str) -> Path:
    return resolve_safe(
        raw,
        config.workspace,
        config.paths.read_roots,
        config.paths.deny,
        config.paths.deny_globs,
    )


def _resolve_write(config: AgentConfig, raw: str) -> Path:
    return resolve_safe(
        raw,
        config.workspace,
        config.paths.write_roots,
        config.paths.deny,
        config.paths.deny_globs,
    )


def _is_write_root(config: AgentConfig, path: Path) -> bool:
    return any(_expanded(root) == path for root in config.paths.write_roots)


def _guard_not_root(config: AgentConfig, path: Path, action: str) -> None:
    if _is_write_root(config, path):
        raise PathDenied(f"No se puede {action} una carpeta raíz permitida.")


def _send_to_trash(path: Path) -> None:
    """Envía una ruta a la Papelera. Import perezoso para no romper si falta el paquete."""
    try:
        from send2trash import send2trash
    except ImportError as exc:  # pragma: no cover - depende del entorno
        raise RuntimeError(
            "Falta el paquete send2trash; no se puede enviar a la Papelera."
        ) from exc
    send2trash(str(path))


def _discard_temp(path: Path) -> None:
    """Intenta llevar un temporal a la Papelera sin enmascarar el error original."""
    try:
        if path.exists():
            _send_to_trash(path)
    except Exception:
        # DECISION: si la Papelera no está disponible, el temporal se queda;
        # nunca se borra de forma definitiva.
        pass


def _count_items(path: Path) -> int:
    """Cuenta elementos contenidos en ``path`` (sin incluir la propia carpeta)."""
    if not path.is_dir():
        return 1 if path.exists() else 0
    count = 0
    try:
        for _root, dirnames, filenames in os.walk(str(path), topdown=True, followlinks=False):
            count += len(dirnames) + len(filenames)
            if count >= _COUNT_LIMIT:
                return _COUNT_LIMIT
    except OSError:
        return count
    return count


def _count_phrase(count: int) -> str:
    if count >= _COUNT_LIMIT:
        return f"{_COUNT_LIMIT} elementos o más"
    if count == 1:
        return "1 elemento"
    return f"{count} elementos"


def _backup_path(target: Path) -> Path:
    """Devuelve una ruta ``.bak`` libre, sin sobrescribir una existente."""
    plain = target.with_name(target.name + ".bak")
    if not plain.exists():
        return plain
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    candidate = target.with_name(f"{target.name}.{stamp}.bak")
    counter = 1
    while candidate.exists():
        candidate = target.with_name(f"{target.name}.{stamp}-{counter}.bak")
        counter += 1
    return candidate


def _make_backup(target: Path) -> Path:
    backup = _backup_path(target)
    if target.is_dir():
        shutil.copytree(str(target), str(backup))
    else:
        shutil.copy2(str(target), str(backup))
    return backup


def _atomic_write(target: Path, content: str) -> None:
    """Escribe un texto en un temporal de la misma carpeta y lo sustituye con os.replace."""
    fd, raw = tempfile.mkstemp(prefix=".ezagent-", suffix=".tmp", dir=str(target.parent))
    tmp = Path(raw)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, target)
    except BaseException:
        _discard_temp(tmp)
        raise


def _atomic_copy(src: Path, dst: Path) -> None:
    """Copia un archivo a un temporal y sustituye ``dst`` de forma atómica."""
    fd, raw = tempfile.mkstemp(prefix=".ezagent-", suffix=".tmp", dir=str(dst.parent))
    os.close(fd)
    tmp = Path(raw)
    try:
        shutil.copy2(str(src), str(tmp))
        os.replace(tmp, dst)
    except BaseException:
        _discard_temp(tmp)
        raise


def _startfile(target: str) -> None:
    opener = getattr(os, "startfile", None)
    if opener is None:  # pragma: no cover - Windows siempre lo tiene
        raise OSError("Abrir archivos con la aplicación predeterminada solo está disponible en Windows.")
    opener(target)


def _text_arg(args: dict[str, Any], name: str, *, allow_empty: bool = False) -> str:
    value = args.get(name)
    if not isinstance(value, str):
        raise ValueError(f"El parámetro «{name}» debe ser texto.")
    if not allow_empty and not value.strip():
        raise ValueError(f"El parámetro «{name}» no puede estar vacío.")
    return value


def _int_arg(args: dict[str, Any], name: str, default: int) -> int:
    value = args.get(name, default)
    if isinstance(value, bool):
        raise ValueError(f"El parámetro «{name}» debe ser un número entero.")
    if isinstance(value, int):
        return value
    # DECISION: el registro acepta 2.0 como «integer»; aquí se normaliza.
    if isinstance(value, float) and value.is_integer():
        return int(value)
    raise ValueError(f"El parámetro «{name}» debe ser un número entero.")


def _bool_arg(args: dict[str, Any], name: str, default: bool = False) -> bool:
    value = args.get(name, default)
    if not isinstance(value, bool):
        raise ValueError(f"El parámetro «{name}» debe ser booleano.")
    return value


def _error_summary(raw: str, action: str, exc: Exception) -> str:
    return f"No se puede {action} «{raw}»: {exc}"


def _is_http_url(raw: str) -> bool:
    lowered = raw.lower()
    return lowered.startswith("http://") or lowered.startswith("https://")


# ----------------------------------------------------------------------
# Handlers
# ----------------------------------------------------------------------
def build_fs_tools(config: AgentConfig, journal: Journal) -> list[ToolSpec]:
    """Construye las herramientas de archivos, de la más simple a la más sensible."""

    def create_dir(args: dict[str, Any]) -> ToolResult:
        try:
            raw = _text_arg(args, "path")
            target = _resolve_write(config, raw)
            _guard_not_root(config, target, "crear")
            if target.exists():
                if target.is_dir():
                    return ToolResult(ok=True, data={"path": str(target), "existed": True})
                return ToolResult(ok=False, error=f"Ya existe un archivo en «{target}».")
            # DECISION: create_dir no crea padres ausentes; solo write_file lo
            # hace (el encargo lo pide expresamente para los archivos).
            if not target.parent.exists():
                return ToolResult(
                    ok=False,
                    error=f"No existe la carpeta padre «{target.parent}».",
                )
            target.mkdir(parents=False, exist_ok=False)
            journal.record_create(
                target,
                kind="create_dir",
                summary=f"Crear la carpeta «{target}»",
            )
            return ToolResult(ok=True, data={"path": str(target), "existed": False})
        except (PathDenied, OSError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def write_file(args: dict[str, Any]) -> ToolResult:
        try:
            raw = _text_arg(args, "path")
            content = _text_arg(args, "content", allow_empty=True)
            overwrite = _bool_arg(args, "overwrite")
            target = _resolve_write(config, raw)
            _guard_not_root(config, target, "escribir en")
            if target.exists() and target.is_dir():
                return ToolResult(ok=False, error=f"«{target}» es una carpeta, no un archivo.")
            limit_mb = float(config.general.max_file_mb)
            payload = content.encode("utf-8")
            if len(payload) > int(limit_mb * 1024 * 1024):
                return ToolResult(
                    ok=False,
                    error=f"El contenido supera el máximo permitido de {limit_mb:g} MB.",
                )
            if target.exists() and not overwrite:
                return ToolResult(
                    ok=False,
                    error=(
                        f"El archivo «{target}» ya existe. "
                        "Usa overwrite=true para sobrescribirlo."
                    ),
                )
            created_parent: Path | None = None
            if not target.parent.exists():
                target.parent.mkdir(parents=True, exist_ok=True)
                created_parent = target.parent
            backup: Path | None = None
            if target.exists():
                backup = _make_backup(target)
            _atomic_write(target, content)
            if backup is not None:
                journal.record_overwrite(
                    target,
                    backup,
                    summary=f"Sobrescribir el archivo «{target}»",
                )
            else:
                journal.record_create(
                    target,
                    kind="write_file",
                    summary=f"Crear el archivo «{target}»",
                )
            data: dict[str, Any] = {
                "path": str(target),
                "bytes": len(payload),
                "overwritten": backup is not None,
            }
            if backup is not None:
                data["backup"] = str(backup)
            if created_parent is not None:
                data["created_parent"] = str(created_parent)
            return ToolResult(ok=True, data=data)
        except (PathDenied, OSError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def copy_path(args: dict[str, Any]) -> ToolResult:
        try:
            src = _resolve_read(config, _text_arg(args, "src"))
            dst = _resolve_write(config, _text_arg(args, "dst"))
            _guard_not_root(config, dst, "copiar en")
            if not src.exists():
                return ToolResult(ok=False, error=f"No existe «{src}».")
            if src == dst:
                return ToolResult(ok=False, error="El origen y el destino son iguales.")
            if src.is_dir() and dst.is_relative_to(src):
                return ToolResult(ok=False, error="No se puede copiar una carpeta dentro de sí misma.")
            # DECISION: dst es la ruta exacta de destino; no se interpreta
            # como una carpeta contenedora donde copiar el origen.
            backup: Path | None = None
            overwritten = dst.exists()
            if overwritten:
                if src.is_dir() != dst.is_dir():
                    return ToolResult(
                        ok=False,
                        error="El origen y el destino deben ser del mismo tipo.",
                    )
                if dst.is_dir() and src.is_relative_to(dst):
                    return ToolResult(
                        ok=False,
                        error="El destino contiene al origen; elige otra carpeta.",
                    )
                backup = _make_backup(dst)
                if src.is_dir():
                    shutil.copytree(str(src), str(dst), dirs_exist_ok=True)
                else:
                    _atomic_copy(src, dst)
                journal.record_copy(src, dst, overwrite=True, backup=backup)
            else:
                # DECISION: copy_path no crea padres ausentes; solo write_file
                # lo hace según el encargo.
                if not dst.parent.exists():
                    return ToolResult(
                        ok=False,
                        error=f"No existe la carpeta destino «{dst.parent}».",
                    )
                if src.is_dir():
                    shutil.copytree(str(src), str(dst))
                else:
                    shutil.copy2(str(src), str(dst))
                journal.record_copy(src, dst)
            data: dict[str, Any] = {
                "src": str(src),
                "dst": str(dst),
                "overwritten": overwritten,
                "kind": "folder" if src.is_dir() else "file",
            }
            if backup is not None:
                data["backup"] = str(backup)
            return ToolResult(ok=True, data=data)
        except (PathDenied, OSError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def move_path(args: dict[str, Any]) -> ToolResult:
        try:
            src = _resolve_write(config, _text_arg(args, "src"))
            dst = _resolve_write(config, _text_arg(args, "dst"))
            _guard_not_root(config, src, "mover")
            _guard_not_root(config, dst, "mover a")
            if not src.exists():
                return ToolResult(ok=False, error=f"No existe «{src}».")
            if src == dst:
                return ToolResult(ok=False, error="El origen y el destino son iguales.")
            if src.is_dir() and dst.is_relative_to(src):
                return ToolResult(ok=False, error="No se puede mover una carpeta dentro de sí misma.")
            # DECISION: dst es la ruta exacta de destino; no se interpreta
            # como una carpeta contenedora donde mover el origen.
            backup: Path | None = None
            overwritten = dst.exists()
            if overwritten:
                if src.is_dir() != dst.is_dir():
                    return ToolResult(
                        ok=False,
                        error="El origen y el destino deben ser del mismo tipo.",
                    )
                if dst.is_dir() and src.is_relative_to(dst):
                    return ToolResult(
                        ok=False,
                        error="El destino contiene al origen; elige otra carpeta.",
                    )
                backup = _make_backup(dst)
                if src.is_dir():
                    # Para carpetas no hay sustitución atómica: se envía la
                    # destino a la Papelera y se mueve la origen a su sitio.
                    _send_to_trash(dst)
                    shutil.move(str(src), str(dst))
                else:
                    # Los archivos se sustituyen con os.replace, que es atómico
                    # y no necesita la Papelera para el destino anterior.
                    try:
                        os.replace(str(src), str(dst))
                    except OSError:
                        _send_to_trash(dst)
                        shutil.move(str(src), str(dst))
                journal.record_move(src, dst, overwrite=True, backup=backup)
            else:
                # DECISION: move_path tampoco crea padres ausentes.
                if not dst.parent.exists():
                    return ToolResult(
                        ok=False,
                        error=f"No existe la carpeta destino «{dst.parent}».",
                    )
                shutil.move(str(src), str(dst))
                journal.record_move(src, dst)
            data: dict[str, Any] = {
                "src": str(src),
                "dst": str(dst),
                "overwritten": overwritten,
                "kind": "folder" if src.is_dir() else "file",
            }
            if backup is not None:
                data["backup"] = str(backup)
            return ToolResult(ok=True, data=data)
        except (PathDenied, OSError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def delete_path(args: dict[str, Any]) -> ToolResult:
        try:
            target = _resolve_write(config, _text_arg(args, "path"))
            _guard_not_root(config, target, "borrar")
            if not target.exists() and not target.is_symlink():
                return ToolResult(ok=False, error=f"No existe «{target}».")
            kind = "folder" if target.is_dir() else "file"
            items = _count_items(target)
            # Nunca hay borrado definitivo: siempre Papelera.
            _send_to_trash(target)
            return ToolResult(
                ok=True,
                data={"path": str(target), "kind": kind, "items": items},
            )
        except (PathDenied, OSError, RuntimeError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def open_path(args: dict[str, Any]) -> ToolResult:
        try:
            raw = _text_arg(args, "path_or_url").strip()
            if _is_http_url(raw):
                parsed = urlsplit(raw)
                if parsed.scheme.lower() not in {"http", "https"} or not parsed.netloc:
                    return ToolResult(ok=False, error="La URL no es válida.")
                _startfile(raw)
                return ToolResult(ok=True, data={"opened": raw, "kind": "url"})
            if "://" in raw:
                return ToolResult(
                    ok=False,
                    error="Solo se permiten rutas de archivo o URL http(s).",
                )
            target = _resolve_read(config, raw)
            if not target.exists():
                return ToolResult(ok=False, error=f"No existe «{target}».")
            if not target.is_file():
                return ToolResult(
                    ok=False,
                    error=f"Solo se pueden abrir archivos, no carpetas: «{target}».",
                )
            _startfile(str(target))
            return ToolResult(ok=True, data={"opened": str(target), "kind": "file"})
        except (PathDenied, OSError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    def undo_last(args: dict[str, Any]) -> ToolResult:
        try:
            n = _int_arg(args, "n", 1)
            if n < 1:
                return ToolResult(ok=False, error="n debe ser mayor o igual que 1.")

            def validate(path: Path) -> Path:
                resolved = _resolve_write(config, str(path))
                _guard_not_root(config, resolved, "deshacer sobre")
                return resolved

            outcomes = journal.undo_last(n, validate=validate)
            details = [outcome.to_dict() for outcome in outcomes]
            if not outcomes:
                return ToolResult(
                    ok=True,
                    data={
                        "undone": 0,
                        "failed": 0,
                        "details": [],
                        "message": "No hay operaciones que deshacer.",
                    },
                )
            failed = [outcome for outcome in outcomes if not outcome.ok]
            error = None
            if failed:
                reasons = "; ".join(
                    f"{outcome.summary}: {outcome.error}" for outcome in failed
                )
                error = f"No se pudieron deshacer todas las operaciones. {reasons}"
            return ToolResult(
                ok=not failed,
                data={
                    "undone": len(outcomes) - len(failed),
                    "failed": len(failed),
                    "details": details,
                },
                error=error,
            )
        except (PathDenied, OSError, RuntimeError, ValueError) as exc:
            return ToolResult(ok=False, error=str(exc))

    # ------------------------------------------------------------------
    # Descripciones para la política (resumen generado por código)
    # ------------------------------------------------------------------
    def describe_create_dir(args: dict[str, Any]) -> ToolInfo:
        raw = str(args.get("path", ""))
        try:
            target = _resolve_write(config, raw)
            _guard_not_root(config, target, "crear")
            if target.exists() and target.is_dir():
                summary = f"La carpeta «{target}» ya existe; no hay que crearla."
            elif target.exists():
                summary = f"Ya existe un archivo en «{target}»; no se puede crear la carpeta."
            else:
                summary = f"Voy a crear la carpeta «{target}»."
            return ToolInfo(level=1, summary=summary)
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(level=1, summary=_error_summary(raw, "crear la carpeta", exc))

    def describe_write_file(args: dict[str, Any]) -> ToolInfo:
        raw = str(args.get("path", ""))
        content = args.get("content", "")
        overwrite = bool(args.get("overwrite", False))
        chars = len(content) if isinstance(content, str) else 0
        try:
            target = _resolve_write(config, raw)
            _guard_not_root(config, target, "escribir en")
            if target.exists() and target.is_dir():
                return ToolInfo(
                    level=1,
                    summary=f"«{target}» es una carpeta; no se puede escribir como archivo.",
                )
            if target.exists() and overwrite:
                backup = _backup_path(target)
                return ToolInfo(
                    level=2,
                    always_confirm=True,
                    summary=(
                        f"Voy a sobrescribir el archivo «{target}» y guardaré "
                        f"una copia de seguridad en «{backup}»."
                    ),
                )
            if target.exists():
                return ToolInfo(
                    level=1,
                    summary=(
                        f"El archivo «{target}» ya existe; no se sobrescribirá "
                        "sin overwrite=true."
                    ),
                )
            parent = target.parent
            if not parent.exists():
                summary = (
                    f"Voy a crear el archivo «{target}» ({chars} caracteres) "
                    f"y la carpeta «{parent}»."
                )
            else:
                summary = f"Voy a crear el archivo «{target}» ({chars} caracteres)."
            return ToolInfo(level=1, summary=summary)
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(level=1, summary=_error_summary(raw, "escribir el archivo", exc))

    def describe_copy_path(args: dict[str, Any]) -> ToolInfo:
        raw_src = str(args.get("src", ""))
        raw_dst = str(args.get("dst", ""))
        try:
            src = _resolve_read(config, raw_src)
            dst = _resolve_write(config, raw_dst)
            _guard_not_root(config, dst, "copiar en")
            if not src.exists():
                return ToolInfo(
                    level=1,
                    summary=f"No se puede copiar «{src}»: no existe.",
                )
            if src == dst:
                return ToolInfo(
                    level=1,
                    summary="El origen y el destino son iguales.",
                )
            if src.is_dir() and dst.is_relative_to(src):
                return ToolInfo(
                    level=1,
                    summary="No se puede copiar una carpeta dentro de sí misma.",
                )
            if dst.exists() and dst.is_dir() and src.is_relative_to(dst):
                return ToolInfo(
                    level=1,
                    summary="El destino contiene al origen; elige otra carpeta.",
                )
            if src.is_dir():
                count = _count_items(src)
                origin = f"la carpeta «{src}» ({_count_phrase(count)})"
                bulk = count
            else:
                origin = f"el archivo «{src}»"
                bulk = 1
            if dst.exists():
                backup = _backup_path(dst)
                return ToolInfo(
                    level=2,
                    bulk=bulk,
                    always_confirm=True,
                    summary=(
                        f"Voy a copiar {origin} sobre «{dst}» y guardaré una "
                        f"copia de seguridad en «{backup}»."
                    ),
                )
            return ToolInfo(
                level=1,
                bulk=bulk,
                summary=f"Voy a copiar {origin} en «{dst}».",
            )
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(
                level=1,
                summary=_error_summary(f"{raw_src} → {raw_dst}", "copiar", exc),
            )

    def describe_move_path(args: dict[str, Any]) -> ToolInfo:
        raw_src = str(args.get("src", ""))
        raw_dst = str(args.get("dst", ""))
        try:
            src = _resolve_write(config, raw_src)
            dst = _resolve_write(config, raw_dst)
            _guard_not_root(config, src, "mover")
            _guard_not_root(config, dst, "mover a")
            if not src.exists():
                return ToolInfo(
                    level=1,
                    summary=f"No se puede mover «{src}»: no existe.",
                )
            if src == dst:
                return ToolInfo(
                    level=1,
                    summary="El origen y el destino son iguales.",
                )
            if src.is_dir() and dst.is_relative_to(src):
                return ToolInfo(
                    level=1,
                    summary="No se puede mover una carpeta dentro de sí misma.",
                )
            if dst.exists() and dst.is_dir() and src.is_relative_to(dst):
                return ToolInfo(
                    level=1,
                    summary="El destino contiene al origen; elige otra carpeta.",
                )
            if src.is_dir():
                count = _count_items(src)
                origin = f"la carpeta «{src}» ({_count_phrase(count)})"
                bulk = count
            else:
                origin = f"el archivo «{src}»"
                bulk = 1
            if dst.exists():
                backup = _backup_path(dst)
                return ToolInfo(
                    level=2,
                    bulk=bulk,
                    always_confirm=True,
                    summary=(
                        f"Voy a mover {origin} sobre «{dst}» y guardaré una "
                        f"copia de seguridad en «{backup}»."
                    ),
                )
            return ToolInfo(
                level=1,
                bulk=bulk,
                summary=f"Voy a mover {origin} a «{dst}».",
            )
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(
                level=1,
                summary=_error_summary(f"{raw_src} → {raw_dst}", "mover", exc),
            )

    def describe_delete_path(args: dict[str, Any]) -> ToolInfo:
        raw = str(args.get("path", ""))
        try:
            target = _resolve_write(config, raw)
            _guard_not_root(config, target, "borrar")
            if not target.exists() and not target.is_symlink():
                return ToolInfo(
                    level=2,
                    always_confirm=True,
                    summary=f"No se puede borrar «{target}»: no existe.",
                )
            if target.is_dir():
                count = _count_items(target)
                if count == 0:
                    summary = (
                        f"Voy a enviar a la Papelera la carpeta «{target}», "
                        "que está vacía."
                    )
                else:
                    summary = (
                        f"Voy a enviar a la Papelera la carpeta «{target}», "
                        f"con {_count_phrase(count)} dentro."
                    )
                # _FolderBulk conserva la cantidad real y fuerza el umbral.
                bulk = _FolderBulk(count)
                return ToolInfo(
                    level=2,
                    bulk=bulk,
                    always_confirm=True,
                    summary=summary,
                )
            return ToolInfo(
                level=2,
                bulk=1,
                always_confirm=True,
                summary=f"Voy a enviar a la Papelera el archivo «{target}».",
            )
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(
                level=2,
                always_confirm=True,
                summary=_error_summary(raw, "borrar", exc),
            )

    def describe_open_path(args: dict[str, Any]) -> ToolInfo:
        raw = str(args.get("path_or_url", "")).strip()
        if _is_http_url(raw):
            return ToolInfo(level=1, summary=f"Voy a abrir el enlace «{raw}» en el navegador.")
        if "://" in raw:
            return ToolInfo(
                level=1,
                summary="Solo se pueden abrir rutas de archivo o URL http(s).",
            )
        try:
            target = _resolve_read(config, raw)
            if not target.exists():
                return ToolInfo(level=1, summary=f"No se puede abrir «{target}»: no existe.")
            if not target.is_file():
                return ToolInfo(
                    level=1,
                    summary=f"Solo se pueden abrir archivos, no carpetas: «{target}».",
                )
            return ToolInfo(level=1, summary=f"Voy a abrir el archivo «{target}».")
        except (PathDenied, OSError, ValueError) as exc:
            return ToolInfo(level=1, summary=_error_summary(raw, "abrir", exc))

    def describe_undo_last(args: dict[str, Any]) -> ToolInfo:
        try:
            n = _int_arg(args, "n", 1)
        except ValueError:
            n = 1
        if n <= 1:
            summary = "Voy a deshacer la última operación."
        else:
            summary = f"Voy a deshacer las últimas {n} operaciones."
        return ToolInfo(level=1, summary=summary)

    # ------------------------------------------------------------------
    # Especificaciones
    # ------------------------------------------------------------------
    path_property = {
        "type": "string",
        "description": "Ruta de la carpeta o del archivo.",
    }
    return [
        ToolSpec(
            name="create_dir",
            description=(
                "Crea una carpeta dentro de las carpetas de escritura permitidas. "
                "La carpeta padre debe existir."
            ),
            parameters={
                "type": "object",
                "properties": {"path": path_property},
                "required": ["path"],
                "additionalProperties": False,
            },
            level=1,
            handler=create_dir,
            describe_fn=describe_create_dir,
        ),
        ToolSpec(
            name="write_file",
            description=(
                "Crea un archivo de texto. Si el archivo existe, solo lo sobrescribe "
                "con overwrite=true y guarda una copia .bak."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "path": path_property,
                    "content": {
                        "type": "string",
                        "description": "Contenido de texto del archivo.",
                    },
                    "overwrite": {
                        "type": "boolean",
                        "description": "Sobrescribir si ya existe (nivel 2).",
                    },
                },
                "required": ["path", "content"],
                "additionalProperties": False,
            },
            level=1,
            handler=write_file,
            describe_fn=describe_write_file,
        ),
        ToolSpec(
            name="copy_path",
            description=(
                "Copia un archivo o una carpeta. No sobrescribe sin confirmación; "
                "si el destino existe, guarda una copia de seguridad."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "src": {"type": "string", "description": "Ruta de origen."},
                    "dst": {"type": "string", "description": "Ruta de destino."},
                },
                "required": ["src", "dst"],
                "additionalProperties": False,
            },
            level=1,
            handler=copy_path,
            describe_fn=describe_copy_path,
        ),
        ToolSpec(
            name="move_path",
            description=(
                "Mueve o renombra un archivo o una carpeta dentro de las carpetas "
                "de escritura permitidas. Si el destino existe, pide confirmación."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "src": {"type": "string", "description": "Ruta de origen."},
                    "dst": {"type": "string", "description": "Ruta de destino."},
                },
                "required": ["src", "dst"],
                "additionalProperties": False,
            },
            level=1,
            handler=move_path,
            describe_fn=describe_move_path,
        ),
        ToolSpec(
            name="delete_path",
            description=(
                "Envía un archivo o una carpeta a la Papelera de reciclaje. "
                "Nunca borra definitivamente y siempre pide confirmación."
            ),
            parameters={
                "type": "object",
                "properties": {"path": path_property},
                "required": ["path"],
                "additionalProperties": False,
            },
            level=2,
            handler=delete_path,
            describe_fn=describe_delete_path,
        ),
        ToolSpec(
            name="open_path",
            description=(
                "Abre un archivo permitido o una URL http(s) con la aplicación "
                "predeterminada del sistema."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "path_or_url": {
                        "type": "string",
                        "description": "Ruta de archivo o URL http(s).",
                    }
                },
                "required": ["path_or_url"],
                "additionalProperties": False,
            },
            level=1,
            handler=open_path,
            describe_fn=describe_open_path,
        ),
        ToolSpec(
            name="undo_last",
            description=(
                "Revierte las últimas n operaciones de archivos registradas en el "
                "diario (crear, copiar, mover o sobrescribir)."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "n": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Número de operaciones a deshacer.",
                    }
                },
                "required": [],
                "additionalProperties": False,
            },
            level=1,
            handler=undo_last,
            describe_fn=describe_undo_last,
        ),
    ]
