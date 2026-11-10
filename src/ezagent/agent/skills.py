"""Descubrimiento, validación y carga de skills locales.

Una skill es una carpeta con un ``SKILL.md`` (frontmatter YAML con ``name`` y
``description``) y, opcionalmente, ``scripts/``, ``references/`` y ``assets/``.

Orden de búsqueda (gana la primera coincidencia y las colisiones se
registran):

1. ``%APPDATA%/EzAgent/skills/`` (``config.skills_dir``)
2. ``~/.agents/skills/`` (``config.global_skills_dir``)

Nunca se descargan ni se instalan skills desde internet.
"""

from __future__ import annotations

import hashlib
import logging
import re
import shutil
import threading
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ezagent.agent.config import AgentConfig
from ezagent.agent.paths import PathDenied, resolve_safe

logger = logging.getLogger(__name__)

MAX_CATALOG = 20
MAX_DESCRIPTION_CHARS = 1024
MAX_CATALOG_DESCRIPTION_CHARS = 110
MAX_SKILL_CHARS = 20_000  # ~5.000 tokens
MAX_NAME_CHARS = 64

_NAME_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")
_WINDOWS_DRIVE_RE = re.compile(r"^[A-Za-z]:")
_FRONTMATTER_OPEN = "---"


class SkillError(Exception):
    """Error de skills con mensaje apto para el usuario."""


class SkillNotFound(SkillError):
    """No existe una skill con ese nombre."""


class SkillFileError(SkillError):
    """El archivo pedido no pertenece a la skill o no se puede leer."""


@dataclass(frozen=True)
class Skill:
    """Metadatos de una skill descubierta (divulgación progresiva)."""

    name: str
    description: str
    path: Path
    body: str
    files: tuple[str, ...] = ()
    scripts: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()
    declared_name: str = ""
    source: str = "local"


@dataclass(frozen=True)
class SkillContent:
    """Contenido devuelto por ``load_skill``."""

    name: str
    text: str
    files: list[str]
    path: str
    file: str | None = None
    truncated: bool = False


class SkillLibrary:
    """Catálogo de skills descubiertas y accesibles."""

    def __init__(
        self,
        config: AgentConfig,
        *,
        audit_warn: Callable[[str], None] | None = None,
        max_catalog: int = MAX_CATALOG,
    ) -> None:
        self._config = config
        self._audit_warn = audit_warn
        self._max_catalog = max_catalog
        self._skills: dict[str, Skill] = {}
        self._warnings: list[str] = []
        self._collisions: list[str] = []
        self._fingerprint: tuple[tuple[str, int, int], ...] = ()
        self._refresh_lock = threading.Lock()
        self.refresh()

    # ------------------------------------------------------------ catálogo
    def refresh(self, force: bool = False) -> bool:
        """Vuelve a descubrir ambas carpetas si cambiaron o si ``force``.

        Devuelve True si releyó el catálogo. Compara mtime y tamaño de las
        carpetas y archivos relevantes, sin volver a leer los SKILL.md ni
        parsear YAML cuando nada cambió (F26).
        """
        with self._refresh_lock:
            fingerprint = self._fingerprint_tree()
            if not force and fingerprint == self._fingerprint:
                return False
            self._fingerprint = fingerprint
            self._skills = {}
            self._warnings = []
            self._collisions = []

            yaml_module = _import_yaml()
            if yaml_module is None:
                self._warn("PyYAML no está instalado; el catálogo de skills queda vacío.")
                return True

            self._discover(Path(self._config.skills_dir), "local", yaml_module)
            self._discover(Path(self._config.global_skills_dir), "global", yaml_module)
            # Recalcular tras leer evita perder un cambio ocurrido durante el barrido.
            self._fingerprint = self._fingerprint_tree()
            return True

    def _fingerprint_tree(self) -> tuple[tuple[str, int, int], ...]:
        entries: list[tuple[str, int, int]] = []
        for root in (Path(self._config.skills_dir), Path(self._config.global_skills_dir)):
            entries.extend(_tree_signature(root))
        return tuple(entries)

    @property
    def warnings(self) -> list[str]:
        with self._refresh_lock:
            return list(self._warnings)

    @property
    def collisions(self) -> list[str]:
        with self._refresh_lock:
            return list(self._collisions)

    def get(self, name: str) -> Skill | None:
        if not isinstance(name, str):
            return None
        cleaned = name.strip()
        with self._refresh_lock:
            skill = self._skills.get(cleaned)
            if skill is not None:
                return skill
            lowered = cleaned.lower()
            for known, candidate in self._skills.items():
                if known.lower() == lowered:
                    return candidate
        return None

    def names(self) -> list[str]:
        with self._refresh_lock:
            return list(self._skills)

    def all(self) -> list[Skill]:
        with self._refresh_lock:
            return list(self._skills.values())

    def catalog_text(self) -> str:
        """Catálogo compacto para el LLM: máximo 20 skills y 200 caracteres."""
        with self._refresh_lock:
            skills = list(self._skills.values())[: self._max_catalog]
        if not skills:
            return ""
        lines = ["<available_skills>"]
        for skill in skills:
            description = _clip(skill.description, MAX_CATALOG_DESCRIPTION_CHARS)
            lines.append(f"- {skill.name}: {description}")
        lines.append("</available_skills>")
        return "\n".join(lines)

    # -------------------------------------------------------------- carga
    def load_skill(self, name: str, file: str | None = None) -> SkillContent:
        """Devuelve el cuerpo sin frontmatter o un archivo auxiliar de la skill."""
        skill = self.get(name)
        if skill is None:
            raise SkillNotFound(f"No existe la skill «{name}».")

        files = list(skill.files)
        if file is None or not str(file).strip():
            text = skill.body
            truncated = len(text) > MAX_SKILL_CHARS
            if truncated:
                text = text[:MAX_SKILL_CHARS]
            return SkillContent(
                name=skill.name,
                text=text,
                files=files,
                path=str(skill.path),
                truncated=truncated,
            )

        # DECISION: se permite cualquier archivo de texto dentro de la skill
        # (references/, assets/ o scripts/), siempre que la ruta resuelta no
        # salga de la carpeta y no contenga «..».
        relative = _safe_relative_file(str(file))
        candidate = skill.path.joinpath(*relative.split("/"))
        skill_root = skill.path.resolve()
        try:
            resolved = resolve_safe(
                str(candidate),
                base=skill.path,
                roots=[str(skill.path)],
                deny=[],
                deny_globs=[],
            )
        except PathDenied as exc:
            raise SkillFileError(f"No se puede leer ese archivo de la skill: {exc}") from exc

        if resolved != skill_root and not resolved.is_relative_to(skill_root):
            raise SkillFileError("El archivo pedido está fuera de la carpeta de la skill.")
        if not resolved.is_file():
            raise SkillFileError(f"No existe el archivo «{relative}» en la skill «{skill.name}».")
        try:
            raw = resolved.read_bytes()
        except OSError as exc:
            raise SkillFileError(f"No se pudo leer «{relative}»: {exc}") from exc
        try:
            text = raw.decode("utf-8-sig")
        except UnicodeDecodeError as exc:
            raise SkillFileError(f"El archivo «{relative}» no es texto UTF-8.") from exc
        # DECISION: los archivos editados en Windows pueden usar CRLF; el
        # contenido que ve el LLM usa siempre LF para no duplicar caracteres.
        text = text.replace("\r\n", "\n").replace("\r", "\n")

        truncated = len(text) > MAX_SKILL_CHARS
        if truncated:
            text = text[:MAX_SKILL_CHARS]
        return SkillContent(
            name=skill.name,
            text=text,
            files=files,
            path=str(skill.path),
            file=resolved.relative_to(skill_root).as_posix(),
            truncated=truncated,
        )

    def folder_hash(self, name: str) -> str:
        """SHA-256 determinista del contenido de la carpeta de la skill."""
        skill = self.get(name)
        if skill is None:
            raise SkillNotFound(f"No existe la skill «{name}».")
        return hash_folder(skill.path)

    # ------------------------------------------------------------ interno
    def _discover(self, root: Path, source: str, yaml_module: Any) -> None:
        try:
            if not root.is_dir():
                return
            # DECISION: se ignoran carpetas ocultas y las que empiezan por «_»
            # (por ejemplo, skills/_proposed/): son propuestas que el usuario
            # todavía no ha revisado.
            folders = sorted(
                (
                    entry
                    for entry in root.iterdir()
                    if entry.is_dir() and not entry.name.startswith((".", "_"))
                ),
                key=lambda entry: entry.name.lower(),
            )
        except OSError as exc:
            self._warn(f"No se pudo leer la carpeta de skills {root}: {exc}")
            return

        for folder in folders:
            if not (folder / "SKILL.md").is_file():
                continue
            skill = self._read_skill(folder, source, yaml_module)
            if skill is None:
                continue
            existing = self._skills.get(skill.name)
            if existing is not None:
                message = (
                    f"La skill «{skill.name}» de {folder} se ignora: ya existe en {existing.path}."
                )
                self._collisions.append(message)
                self._warn(message)
                continue
            self._skills[skill.name] = skill
            for warning in skill.warnings:
                self._warn(f"Skill «{skill.name}»: {warning}")

        if len(self._skills) > self._max_catalog:
            self._warn(
                f"Hay {len(self._skills)} skills; el catálogo solo mostrará "
                f"las primeras {self._max_catalog}."
            )

    def _read_skill(self, folder: Path, source: str, yaml_module: Any) -> Skill | None:
        skill_md = folder / "SKILL.md"
        try:
            raw = skill_md.read_bytes()
        except OSError as exc:
            self._warn(f"Skill «{folder.name}»: no se pudo leer SKILL.md: {exc}")
            return None
        try:
            text = raw.decode("utf-8-sig")
        except UnicodeDecodeError:
            self._warn(f"Skill «{folder.name}»: SKILL.md no es UTF-8 válido; se descarta.")
            return None
        # DECISION: se normalizan CRLF/CR a LF antes de leer el frontmatter.
        text = text.replace("\r\n", "\n").replace("\r", "\n")

        split = _split_frontmatter(text)
        if split is None:
            self._warn(f"Skill «{folder.name}»: frontmatter YAML ilegible; se descarta.")
            return None
        meta_text, body = split

        try:
            metadata = yaml_module.safe_load(meta_text)
        except Exception as exc:  # noqa: BLE001 - YAML roto = skill descartada
            self._warn(
                f"Skill «{folder.name}»: frontmatter YAML ilegible "
                f"({type(exc).__name__}); se descarta."
            )
            return None
        if not isinstance(metadata, dict):
            self._warn(f"Skill «{folder.name}»: el frontmatter no es un mapa YAML; se descarta.")
            return None

        description = metadata.get("description")
        if not isinstance(description, str) or not description.strip():
            self._warn(f"Skill «{folder.name}»: falta «description»; se descarta.")
            return None
        description = " ".join(description.split())

        warnings: list[str] = []
        if len(description) > MAX_DESCRIPTION_CHARS:
            warnings.append(
                f"la descripción supera {MAX_DESCRIPTION_CHARS} caracteres; se recorta."
            )
            description = description[:MAX_DESCRIPTION_CHARS].rstrip()

        folder_name = folder.name
        declared = metadata.get("name")
        declared_name = declared.strip() if isinstance(declared, str) else ""
        if not declared_name:
            warnings.append("no declara «name»; se usa el nombre de la carpeta.")
        else:
            if declared_name != folder_name:
                warnings.append(
                    f"el name «{declared_name}» no coincide con la carpeta «{folder_name}»."
                )
            if not _valid_name(declared_name):
                warnings.append(
                    "el name no cumple 1-64 caracteres en minúsculas, números y guiones."
                )
        if not _valid_name(folder_name):
            warnings.append(
                "el nombre de la carpeta no cumple 1-64 caracteres en "
                "minúsculas, números y guiones."
            )

        files = _list_files(folder)
        scripts = tuple(item for item in files if item.startswith("scripts/"))
        return Skill(
            name=folder_name,
            description=description,
            path=folder,
            body=body.strip(),
            files=tuple(files),
            scripts=scripts,
            warnings=tuple(warnings),
            declared_name=declared_name,
            source=source,
        )

    def _warn(self, message: str) -> None:
        self._warnings.append(message)
        logger.warning("%s", message)
        if self._audit_warn is None:
            return
        try:
            self._audit_warn(message)
        except Exception as exc:  # noqa: BLE001 - avisos best-effort
            logger.warning("El callback de auditoría de skills falló: %s", exc)


def _tree_signature(root: Path) -> list[tuple[str, int, int]]:
    """mtime y tamaño de la carpeta y de todo su contenido, sin leerlo."""
    signature = [_path_signature(root)]
    try:
        for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
            signature.append(_path_signature(path))
    except OSError:
        pass
    return signature


def _path_signature(path: Path) -> tuple[str, int, int]:
    try:
        stat = path.stat()
        return (str(path), int(stat.st_mtime_ns), int(stat.st_size))
    except OSError:
        return (str(path), -1, -1)


def hash_folder(folder: Path) -> str:
    """SHA-256 de todos los archivos de una carpeta, con rutas relativas."""
    digest = hashlib.sha256()
    root = Path(folder)
    try:
        files = sorted(
            (path for path in root.rglob("*") if path.is_file() and not path.is_symlink()),
            key=lambda path: path.relative_to(root).as_posix(),
        )
    except OSError:
        files = []
    for path in files:
        relative = path.relative_to(root).as_posix()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\x00")
        try:
            digest.update(path.read_bytes())
        except OSError:
            digest.update(b"<unreadable>")
        digest.update(b"\x00")
    return digest.hexdigest()


def ensure_default_skills(config: AgentConfig, *, source_dir: Path | None = None) -> list[str]:
    """Copia las skills empaquetadas a ``config.skills_dir`` si faltan.

    Nunca sobrescribe una carpeta existente: el usuario puede haber editado
    su copia.  Devuelve los nombres copiados.
    """
    source = (
        Path(source_dir)
        if source_dir is not None
        else Path(__file__).resolve().parent / "default_skills"
    )
    if not source.is_dir():
        logger.warning("No existe la carpeta de skills empaquetadas: %s", source)
        return []

    target = Path(config.skills_dir)
    try:
        target.mkdir(parents=True, exist_ok=True)
    except OSError as exc:
        logger.warning("No se pudo preparar la carpeta de skills %s: %s", target, exc)
        return []

    copied: list[str] = []
    try:
        folders = sorted(
            (
                entry
                for entry in source.iterdir()
                if entry.is_dir()
                and (entry / "SKILL.md").is_file()
                and not entry.name.startswith((".", "_"))
            ),
            key=lambda entry: entry.name.lower(),
        )
    except OSError as exc:
        logger.warning("No se pudo leer %s: %s", source, exc)
        return []

    for folder in folders:
        destination = target / folder.name
        if destination.exists():
            continue
        try:
            shutil.copytree(folder, destination)
        except OSError as exc:
            # No se borra una copia parcial (borrar está prohibido en agent/);
            # se registra para que el usuario pueda revisarla.
            logger.warning(
                "No se pudo copiar la skill «%s» a %s: %s",
                folder.name,
                destination,
                exc,
            )
            continue
        copied.append(folder.name)
    return copied


def _import_yaml() -> Any | None:
    try:
        import yaml  # type: ignore[import-not-found]
    except ImportError:
        return None
    return yaml


def _split_frontmatter(text: str) -> tuple[str, str] | None:
    if not text.startswith(_FRONTMATTER_OPEN):
        return None
    lines = text.splitlines(keepends=True)
    if not lines or lines[0].strip() != _FRONTMATTER_OPEN:
        return None
    for index in range(1, len(lines)):
        if lines[index].strip() == _FRONTMATTER_OPEN:
            return "".join(lines[1:index]), "".join(lines[index + 1 :])
    return None


def _valid_name(name: str) -> bool:
    return bool(1 <= len(name) <= MAX_NAME_CHARS and _NAME_RE.fullmatch(name))


def _list_files(folder: Path) -> list[str]:
    try:
        return sorted(
            path.relative_to(folder).as_posix()
            for path in folder.rglob("*")
            if path.is_file() and not path.is_symlink() and path.name != "SKILL.md"
        )
    except OSError:
        return []


def _safe_relative_file(raw: str) -> str:
    normalized = raw.strip().replace("\\", "/")
    if not normalized:
        raise SkillFileError("Falta la ruta del archivo dentro de la skill.")
    if normalized.startswith("/") or _WINDOWS_DRIVE_RE.match(normalized):
        raise SkillFileError("La ruta del archivo debe ser relativa a la carpeta de la skill.")
    parts = [part for part in normalized.split("/") if part not in ("", ".")]
    if not parts:
        raise SkillFileError("La ruta del archivo está vacía.")
    if any(part == ".." for part in parts):
        raise SkillFileError("La ruta del archivo no puede contener «..».")
    return "/".join(parts)


def _clip(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    if limit <= 1:
        return text[:limit]
    return text[: limit - 1] + "…"


__all__ = [
    "MAX_CATALOG",
    "MAX_CATALOG_DESCRIPTION_CHARS",
    "MAX_DESCRIPTION_CHARS",
    "MAX_SKILL_CHARS",
    "Skill",
    "SkillContent",
    "SkillError",
    "SkillFileError",
    "SkillLibrary",
    "SkillNotFound",
    "ensure_default_skills",
    "hash_folder",
]
