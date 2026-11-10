"""Memoria persistente de EzAgent.

Tres capas:

- ``USER.md``: quién es el usuario (tope de 1.375 caracteres).
- ``MEMORY.md``: hechos y flujo de trabajo (tope de 2.200 caracteres).
- ``history.db``: historial de conversaciones en SQLite con FTS5.

Solo se guardan mensajes de usuario y asistente; nunca resultados de
herramientas. El historial conserva 180 días.

Los dos archivos Markdown son viñetas ``- [AAAA-MM-DD] texto`` y se pueden
editar a mano. ``memory_block()`` los devuelve dentro de ``<memory>`` para el
prompt del sistema.

Comando de consola (no es una herramienta del LLM)::

    python -m ezagent.agent.memory wipe [--history RUTA] [--yes]
"""

from __future__ import annotations

import argparse
import logging
import os
import re
import shutil
import sqlite3
import sys
import tempfile
import warnings
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable, Iterator, Sequence

if TYPE_CHECKING:
    from ezagent.agent.config import AgentConfig

logger = logging.getLogger(__name__)

USER_LIMIT = 1375
MEMORY_LIMIT = 2200
MAX_ENTRY_CHARS = 200
RECALL_FRAGMENT_CHARS = 300
RETENTION_DAYS = 180
DEFAULT_RECALL_LIMIT = 5

_KIND_FILES: dict[str, str] = {"user": "USER.md", "memory": "MEMORY.md"}
_KIND_LIMITS: dict[str, int] = {"user": USER_LIMIT, "memory": MEMORY_LIMIT}
_KIND_ALIASES: dict[str, str] = {
    "user": "user",
    "usuario": "user",
    "perfil": "user",
    "user.md": "user",
    "user_md": "user",
    "memory": "memory",
    "memoria": "memory",
    "hechos": "memory",
    "memory.md": "memory",
    "memory_md": "memory",
}
_ROLE_ALIASES: dict[str, str] = {
    "user": "user",
    "usuario": "user",
    "assistant": "assistant",
    "asistente": "assistant",
    "ai": "assistant",
    "bot": "assistant",
}
_BULLET_RE = re.compile(r"^\s*-\s*\[(\d{4}-\d{2}-\d{2})\]\s*(.*)$")
_TOKEN_RE = re.compile(r"[^\W_]+", re.UNICODE)
_ACCENT_MAP = str.maketrans(
    "áéíóúüñàèìòùïëÁÉÍÓÚÜÑÀÈÌÒÙÏË",
    "aeiouunaeiouieAEIOUUNaeiouie",
)
_STOPWORDS: frozenset[str] = frozenset(
    {
        "a",
        "al",
        "algo",
        "ante",
        "así",
        "ayer",
        "con",
        "como",
        "cuando",
        "de",
        "del",
        "desde",
        "donde",
        "dónde",
        "el",
        "él",
        "ella",
        "ellas",
        "ellos",
        "en",
        "entre",
        "era",
        "es",
        "esa",
        "ese",
        "eso",
        "esta",
        "está",
        "este",
        "esto",
        "estos",
        "estas",
        "fue",
        "ha",
        "hablamos",
        "han",
        "has",
        "hasta",
        "hay",
        "hoy",
        "la",
        "las",
        "le",
        "les",
        "lo",
        "los",
        "más",
        "me",
        "mi",
        "mis",
        "mucho",
        "muy",
        "no",
        "nos",
        "o",
        "para",
        "pero",
        "por",
        "que",
        "qué",
        "se",
        "ser",
        "si",
        "sí",
        "sin",
        "sobre",
        "son",
        "su",
        "sus",
        "también",
        "te",
        "ti",
        "tu",
        "tus",
        "un",
        "una",
        "unas",
        "uno",
        "unos",
        "y",
        "ya",
        "yo",
    }
)

# DECISION: las palabras «contraseña»/«password» se rechazan siempre; el resto
# de secretos exige una asignación («token: ...») para no bloquear frases
# inocuas como «prefiero tokens cortos».
_PASSWORD_WORD = re.compile(
    r"\b(?:contraseñas?|contrasenas?|passwords?|passwd|pwd)",
    re.IGNORECASE,
)
_ASSIGNED_SECRET = re.compile(
    r"\b(?:"
    r"(?:api[\s_-]?keys?|tokens?|secrets?|secretos?|claves?)"
    r"\s*(?:es|son|:|=|=>)\s*\S+"
    r"|claves?\s+de\s+(?:api|acceso|seguridad|wifi)\b"
    r"|(?:pass|pin)\s*[:=]\s*\S+"
    r")",
    re.IGNORECASE,
)
_KEY_FORMATS: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"\bsk-[A-Za-z0-9_-]{8,}\b"), "claves o tokens"),
    (re.compile(r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}\b"), "claves o tokens"),
    (re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b"), "claves o tokens"),
    (re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b", re.IGNORECASE), "claves o tokens"),
    (re.compile(r"\bAKIA[0-9A-Z]{16}\b"), "claves o tokens"),
    (re.compile(r"\bBearer\s+[A-Za-z0-9._~+/=-]{8,}", re.IGNORECASE), "claves o tokens"),
    (re.compile(r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"), "claves o tokens"),
    (re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"), "claves o tokens"),
    (re.compile(r"\bAIza[0-9A-Za-z_-]{30,}\b"), "claves o tokens"),
)
# DECISION: una tarjeta se detecta por Luhn o por aparecer junto a «tarjeta»;
# así no se confunden teléfonos o fechas con números de tarjeta.
_CARD_SEQUENCE = re.compile(r"(?<!\d)(?:\d[ -]?){12,18}\d(?!\d)")
_CARD_CONTEXT = re.compile(r"\btarjetas?\b[^\n]{0,40}?(?:\d[ -]?){12,18}\d", re.IGNORECASE)
_DNI = re.compile(r"(?<!\w)\d{8}[A-Za-z](?!\w)")
_NIE = re.compile(r"(?<!\w)[XYZxyz]\d{7}[A-Za-z](?!\w)")
_ID_CONTEXT = re.compile(
    r"\b(?:dni|nie|documento(?: nacional)? de identidad|c[eé]dula|pasaporte|passport)\b"
    r"[^\n]{0,30}?\b[A-Z]{0,2}\d{6,9}[A-Za-z]?\b",
    re.IGNORECASE,
)
_IBAN = re.compile(
    r"\b[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]{4}){2,7}(?:[ ]?[A-Z0-9]{1,3})?\b",
    re.IGNORECASE,
)
# DECISION: no se puede conocer la procedencia real del texto; se rechazan
# URLs y enlaces Markdown como heurística de contenido web. El taint del bucle
# sigue siendo la protección autoritativa para el resto de casos.
_URL_RE = re.compile(r"(?:https?://|ftp://|\bwww\.)\S+", re.IGNORECASE)
_MD_LINK_RE = re.compile(r"\[[^\]]*\]\((?:https?://|www\.)[^)]+\)", re.IGNORECASE)
_DOMAIN_RE = re.compile(
    r"\b(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+"
    r"(?:com|org|net|io|es|co|gov|edu|info|dev|ai)\b",
    re.IGNORECASE,
)


def fold_text(value: str) -> str:
    """Minúsculas y sin acentos, conservando la longitud del texto."""
    return str(value).translate(_ACCENT_MAP).lower()


def normalize_entry(text: str) -> str:
    """Normaliza un dato de memoria a una sola línea con espacios simples."""
    return " ".join(str(text).split())


def normalize_for_compare(value: str) -> str:
    """Normaliza para comparar duplicados (sin mayúsculas, acentos ni punto final)."""
    folded = fold_text(normalize_entry(value)).strip()
    while folded and folded[-1] in ".,;:!?¿¡":
        folded = folded[:-1].rstrip()
    return folded


def summarize_text(value: Any, limit: int = 80) -> str:
    """Recorta un texto para mensajes y resúmenes de confirmación."""
    text = normalize_entry(str(value or ""))
    if len(text) > limit:
        text = text[: max(0, limit - 1)].rstrip() + "…"
    return text


def _luhn_ok(digits: str) -> bool:
    total = 0
    for index, char in enumerate(reversed(digits)):
        number = int(char)
        if index % 2 == 1:
            number *= 2
            if number > 9:
                number -= 9
        total += number
    return total % 10 == 0


def _has_card_number(text: str) -> bool:
    for match in _CARD_SEQUENCE.finditer(text):
        digits = re.sub(r"\D", "", match.group())
        if 13 <= len(digits) <= 19 and _luhn_ok(digits):
            return True
    return _CARD_CONTEXT.search(text) is not None


def contains_web_content(text: str) -> bool:
    """Heurística: URLs, dominios o enlaces Markdown propios de contenido web."""
    value = str(text or "")
    return (
        _URL_RE.search(value) is not None
        or _MD_LINK_RE.search(value) is not None
        or _DOMAIN_RE.search(value) is not None
    )


def find_sensitive_text(text: str) -> str | None:
    """Devuelve la categoría sensible detectada, o ``None`` si no hay ninguna.

    Categorías: contraseñas, claves o tokens, números de tarjeta, documentos de
    identidad y datos bancarios (IBAN).
    """
    value = str(text or "")
    if _PASSWORD_WORD.search(value):
        return "contraseñas"
    if _ASSIGNED_SECRET.search(value):
        return "claves o tokens"
    for pattern, category in _KEY_FORMATS:
        if pattern.search(value):
            return category
    if _has_card_number(value):
        return "números de tarjeta"
    if _DNI.search(value) or _NIE.search(value) or _ID_CONTEXT.search(value):
        return "documentos de identidad"
    if _IBAN.search(value):
        return "datos bancarios (IBAN)"
    return None


def normalize_kind(kind: str) -> str | None:
    """Acepta ``user``/``memory`` y sus alias en español; devuelve ``None`` si no vale."""
    if not isinstance(kind, str):
        return None
    return _KIND_ALIASES.get(kind.strip().lower())


@dataclass(frozen=True)
class MemoryResult:
    """Resultado uniforme de las operaciones de memoria."""

    status: str
    message: str
    kind: str | None = None
    text: str | None = None
    proposal: str | None = None
    removed: int = 0
    files: tuple[str, ...] = ()
    backup: str | None = None
    line_count: int = 0

    @property
    def saved(self) -> bool:
        return self.status == "saved"

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {"status": self.status, "message": self.message}
        if self.kind is not None:
            data["kind"] = self.kind
        if self.text is not None:
            data["text"] = self.text
        if self.proposal is not None:
            data["proposal"] = self.proposal
        if self.removed:
            data["removed"] = self.removed
        if self.files:
            data["files"] = list(self.files)
        if self.backup is not None:
            data["backup"] = self.backup
        if self.line_count:
            data["line_count"] = self.line_count
        return data


_MESSAGES_DDL = """
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY,
  ts TEXT NOT NULL,
  session TEXT NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL
)
"""

_FTS_DDL = (
    """
    CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
      content, content='messages', content_rowid='id',
      tokenize='unicode61 remove_diacritics 2'
    )
    """,
    """
    CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
      INSERT INTO messages_fts(messages_fts, rowid, content)
      VALUES ('delete', old.id, old.content);
    END
    """,
)


def _local_now() -> datetime:
    return datetime.now().astimezone()


def _as_utc(value: datetime) -> datetime:
    if value.tzinfo is None:
        return value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc)


def _query_terms(query: str) -> list[str]:
    """Términos útiles de la consulta, sin acentos ni palabras vacías."""
    raw = [fold_text(token) for token in _TOKEN_RE.findall(str(query))]
    meaningful = [token for token in raw if token not in _STOPWORDS and len(token) >= 2]
    if not meaningful:
        meaningful = [token for token in raw if token not in _STOPWORDS]
    return meaningful or raw


def _like_pattern(term: str) -> str:
    escaped = term.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
    return f"%{escaped}%"


def make_snippet(
    text: str,
    terms: Sequence[str],
    max_chars: int = RECALL_FRAGMENT_CHARS,
) -> str:
    """Fragmento de como máximo ``max_chars`` alrededor del primer término."""
    content = str(text or "")
    if len(content) <= max_chars:
        return content
    folded = fold_text(content)
    positions = [folded.find(term) for term in terms if term]
    positions = [position for position in positions if position >= 0]
    index = min(positions) if positions else 0
    start = max(0, index - 80)
    end = min(len(content), start + max_chars)
    if end - start < max_chars:
        start = max(0, end - max_chars)
    prefix = "…" if start > 0 else ""
    suffix = "…" if end < len(content) else ""
    available = max_chars - len(prefix) - len(suffix)
    segment = content[start:end]
    if available <= 0:
        return segment[:max_chars]
    if len(segment) > available:
        segment = segment[:available]
    return (prefix + segment + suffix)[:max_chars]


class MemoryStore:
    """Acceso a ``USER.md``, ``MEMORY.md`` y ``history.db``.

    Se abre una conexión SQLite efímera por operación para no compartir estado
    entre hilos. La comprobación de FTS5 se hace al construir la instancia.
    """

    def __init__(
        self,
        config: AgentConfig,
        *,
        session: str = "default",
        prefer_fts: bool = True,
        retention_days: int = RETENTION_DAYS,
        clock: Callable[[], datetime] | None = None,
    ) -> None:
        self._setup(
            Path(config.memory_dir),
            Path(config.history_path),
            session=session,
            prefer_fts=prefer_fts,
            retention_days=retention_days,
            clock=clock,
        )

    @classmethod
    def from_paths(
        cls,
        memory_dir: str | Path,
        history_path: str | Path,
        *,
        session: str = "default",
        prefer_fts: bool = True,
        retention_days: int = RETENTION_DAYS,
        clock: Callable[[], datetime] | None = None,
    ) -> "MemoryStore":
        """Constructor para pruebas y para el comando de consola."""
        store = cls.__new__(cls)
        store._setup(
            Path(memory_dir),
            Path(history_path),
            session=session,
            prefer_fts=prefer_fts,
            retention_days=retention_days,
            clock=clock,
        )
        return store

    def _setup(
        self,
        memory_dir: Path,
        history_path: Path,
        *,
        session: str,
        prefer_fts: bool,
        retention_days: int,
        clock: Callable[[], datetime] | None,
    ) -> None:
        self.memory_dir = memory_dir
        self.history_path = history_path
        self.session = session or "default"
        self.retention_days = max(1, int(retention_days))
        self._clock = clock or _local_now
        self.memory_dir.mkdir(parents=True, exist_ok=True)
        self.history_path.parent.mkdir(parents=True, exist_ok=True)
        self.user_path = self.memory_dir / _KIND_FILES["user"]
        self.memory_path = self.memory_dir / _KIND_FILES["memory"]
        for path in (self.user_path, self.memory_path):
            if not path.exists():
                path.write_text("", encoding="utf-8")
        self.fts_enabled = False
        self.fts_error: str | None = None
        self.last_backend = "none"
        self._init_db(prefer_fts=prefer_fts)
        self.purge_old()

    # ------------------------------------------------------------------
    # SQLite
    # ------------------------------------------------------------------

    @contextmanager
    def _connect(self) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(str(self.history_path), timeout=5.0)
        connection.row_factory = sqlite3.Row
        try:
            yield connection
            connection.commit()
        except BaseException:
            connection.rollback()
            raise
        finally:
            connection.close()

    def _init_db(self, *, prefer_fts: bool) -> None:
        with self._connect() as connection:
            connection.execute(_MESSAGES_DDL)
            connection.execute("CREATE INDEX IF NOT EXISTS idx_messages_ts ON messages(ts)")
            had_fts = (
                connection.execute(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages_fts'"
                ).fetchone()
                is not None
            )
            had_triggers = all(
                connection.execute(
                    "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
                    (name,),
                ).fetchone()
                is not None
                for name in ("messages_ai", "messages_ad")
            )
            if not prefer_fts:
                warnings.warn(
                    "FTS5 desactivado; recall usará LIKE como respaldo.",
                    RuntimeWarning,
                    stacklevel=2,
                )
                return
            try:
                for statement in _FTS_DDL:
                    connection.execute(statement)
            except sqlite3.OperationalError as exc:
                # DECISION: sin FTS5 se retiran los triggers para que los
                # INSERT sigan funcionando; recall cae a LIKE.
                self.fts_error = str(exc)
                logger.warning("SQLite no ofrece FTS5 (%s); recall usará LIKE.", exc)
                warnings.warn(
                    "SQLite no ofrece FTS5; recall usará LIKE como respaldo.",
                    RuntimeWarning,
                    stacklevel=2,
                )
                for trigger in ("messages_ai", "messages_ad"):
                    connection.execute(f"DROP TRIGGER IF EXISTS {trigger}")
                return
            self.fts_enabled = True
            if not had_fts or not had_triggers:
                connection.execute("INSERT INTO messages_fts(messages_fts) VALUES('rebuild')")

    # ------------------------------------------------------------------
    # Archivos Markdown
    # ------------------------------------------------------------------

    def _today(self) -> str:
        return self._clock().date().isoformat()

    def _path(self, kind: str) -> Path:
        if kind == "user":
            return self.user_path
        return self.memory_path

    def filename(self, kind: str) -> str:
        normalized = normalize_kind(kind)
        return _KIND_FILES.get(normalized or "", "memoria")

    def limit(self, kind: str) -> int:
        normalized = normalize_kind(kind)
        return _KIND_LIMITS.get(normalized or "", 0)

    def _read(self, kind: str) -> str:
        try:
            return self._path(kind).read_text(encoding="utf-8", errors="replace")
        except FileNotFoundError:
            return ""

    # DECISION: escritura atómica con archivo temporal y os.replace; si falla,
    # el temporal puede quedar huérfano (no se usan borrados prohibidos).
    def _atomic_write(self, path: Path, content: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        descriptor, temporary = tempfile.mkstemp(
            prefix=path.name + ".", suffix=".tmp", dir=str(path.parent)
        )
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)

    def read_markdown(self, kind: str) -> str:
        """Contenido completo del archivo, para edición manual o pruebas."""
        normalized = normalize_kind(kind)
        if normalized is None:
            return ""
        return self._read(normalized)

    def entries(self, kind: str) -> list[str]:
        """Viñetas del archivo, sin la fecha ni el guion."""
        normalized = normalize_kind(kind)
        if normalized is None:
            return []
        result: list[str] = []
        for line in self._read(normalized).splitlines():
            match = _BULLET_RE.match(line)
            if match:
                body = match.group(2).strip()
                if body:
                    result.append(body)
                continue
            stripped = line.lstrip()
            if stripped.startswith("- "):
                body = stripped[2:].strip()
                if body:
                    result.append(body)
        return result

    def _find_duplicate(self, kind: str, value: str) -> str | None:
        target = normalize_for_compare(value)
        for entry in self.entries(kind):
            if normalize_for_compare(entry) == target:
                return entry
        return None

    # DECISION: la propuesta no repite el archivo completo (ya viaja en
    # <memory>); así cabe en el límite de 2.000 caracteres del bucle.
    def _compaction_proposal(self, kind: str, value: str) -> str:
        limit = _KIND_LIMITS[kind]
        content = self._read(kind).strip()
        recent = [line for line in content.splitlines() if line.strip()][-5:]
        suggestion = "\n".join(recent)
        if len(suggestion) > 500:
            suggestion = suggestion[-500:]
        return (
            f"{self.filename(kind)} está lleno ({len(content)}/{limit} caracteres). "
            "No he guardado nada. Redacta una versión fusionada del archivo (lo "
            "tienes completo en <memory>) que conserve lo importante, incluya el "
            f"dato nuevo y no pase de {limit} caracteres; después pide confirmación "
            f"para guardarla con compact_memory(kind='{kind}', text=...).\n\n"
            f"Dato nuevo: - [{self._today()}] {value}\n\n"
            f"Últimas líneas como referencia:\n{suggestion}"
        )

    # DECISION: los datos delicados (salud, creencias, finanzas) no se
    # clasifican automáticamente; el requisito es que el usuario confirme de
    # forma explícita en la herramienta remember.
    def validate_entry(self, kind: str, text: str) -> MemoryResult:
        """Valida un dato sin escribirlo. ``status == 'ready'`` si se puede guardar."""
        normalized_kind = normalize_kind(kind)
        if normalized_kind is None:
            return MemoryResult(
                "invalid_kind",
                "Tipo de memoria no válido; usa 'user' o 'memory'.",
            )
        value = normalize_entry(text)
        if not value:
            return MemoryResult("empty", "El texto está vacío.", kind=normalized_kind)
        if len(value) > MAX_ENTRY_CHARS:
            return MemoryResult(
                "too_long",
                f"El texto tiene {len(value)} caracteres; el máximo es {MAX_ENTRY_CHARS}.",
                kind=normalized_kind,
                text=value,
            )
        category = find_sensitive_text(value)
        if category is not None:
            return MemoryResult(
                "secret",
                f"No guardo {category} en la memoria.",
                kind=normalized_kind,
            )
        if contains_web_content(value):
            return MemoryResult(
                "web",
                "No guardo contenido procedente de la web en la memoria.",
                kind=normalized_kind,
            )
        duplicate = self._find_duplicate(normalized_kind, value)
        if duplicate is not None:
            return MemoryResult(
                "duplicate",
                f"Ya está guardado: «{summarize_text(duplicate)}».",
                kind=normalized_kind,
                text=value,
            )
        limit = _KIND_LIMITS[normalized_kind]
        content = self._read(normalized_kind)
        prefix = "" if not content or content.endswith("\n") else "\n"
        line = f"- [{self._today()}] {value}"
        if len(f"{content}{prefix}{line}\n") > limit:
            return MemoryResult(
                "full",
                f"{self.filename(normalized_kind)} está lleno "
                f"({len(content)}/{limit} caracteres); no se ha guardado nada.",
                kind=normalized_kind,
                text=value,
                proposal=self._compaction_proposal(normalized_kind, value),
            )
        return MemoryResult("ready", "Listo para guardar.", kind=normalized_kind, text=value)

    def remember(self, kind: str, text: str) -> MemoryResult:
        """Valida y guarda una viñeta. Nunca guarda secretos ni duplicados."""
        validation = self.validate_entry(kind, text)
        if validation.status != "ready":
            return validation
        assert validation.kind is not None
        assert validation.text is not None
        normalized_kind = validation.kind
        value = validation.text
        limit = _KIND_LIMITS[normalized_kind]
        content = self._read(normalized_kind)
        prefix = "" if not content or content.endswith("\n") else "\n"
        line = f"- [{self._today()}] {value}"
        new_content = f"{content}{prefix}{line}\n"
        if len(new_content) > limit:
            return MemoryResult(
                "full",
                f"{self.filename(normalized_kind)} está lleno "
                f"({len(content)}/{limit} caracteres); no se ha guardado nada.",
                kind=normalized_kind,
                text=value,
                proposal=self._compaction_proposal(normalized_kind, value),
            )
        self._atomic_write(self._path(normalized_kind), new_content)
        return MemoryResult(
            "saved",
            f"Guardado en {self.filename(normalized_kind)}.",
            kind=normalized_kind,
            text=value,
        )

    # DECISION: las líneas planas de una versión compactada se convierten en
    # viñetas con la fecha de hoy; las cabeceras Markdown se conservan para no
    # romper archivos editados a mano.
    def _compaction_lines(self, text: str) -> list[str]:
        today = self._today()
        result: list[str] = []
        for raw_line in str(text or "").splitlines():
            line = raw_line.strip()
            if not line:
                continue
            if line.startswith("#"):
                result.append(line)
                continue
            match = _BULLET_RE.match(line)
            if match:
                body = match.group(2).strip()
                if body:
                    result.append(f"- [{match.group(1)}] {body}")
                continue
            if line.startswith("- "):
                body = line[2:].strip()
                if body:
                    result.append(f"- [{today}] {body}")
                continue
            result.append(f"- [{today}] {line}")
        return result

    def prepare_compaction(self, kind: str, text: str) -> MemoryResult:
        """Valida la versión compactada completa sin escribirla."""
        normalized_kind = normalize_kind(kind)
        if normalized_kind is None:
            return MemoryResult(
                "invalid_kind",
                "Tipo de memoria no válido; usa 'user' o 'memory'.",
            )
        lines = self._compaction_lines(text)
        if not lines:
            return MemoryResult("empty", "La versión compactada está vacía.", kind=normalized_kind)
        content = "\n".join(lines) + "\n"
        limit = _KIND_LIMITS[normalized_kind]
        if len(content) > limit:
            return MemoryResult(
                "too_long",
                f"La versión compactada tiene {len(content)} caracteres; el máximo es {limit}.",
                kind=normalized_kind,
                text=content,
                line_count=len(lines),
            )
        for line in lines:
            category = find_sensitive_text(line)
            if category is not None:
                return MemoryResult(
                    "secret",
                    f"No guardo {category} en la memoria.",
                    kind=normalized_kind,
                )
            if contains_web_content(line):
                return MemoryResult(
                    "web",
                    "No guardo contenido procedente de la web en la memoria.",
                    kind=normalized_kind,
                )
        return MemoryResult(
            "ready",
            "Listo para compactar.",
            kind=normalized_kind,
            text=content,
            line_count=len(lines),
        )

    # DECISION: la copia .bak se crea siempre antes de reescribir, aunque el
    # contenido compactado sea idéntico al actual.
    def compact_memory(self, kind: str, text: str) -> MemoryResult:
        """Reescribe el archivo tras dejar una copia ``.bak`` de la versión previa."""
        prepared = self.prepare_compaction(kind, text)
        if prepared.status != "ready" or prepared.kind is None or prepared.text is None:
            return prepared
        path = self._path(prepared.kind)
        backup = path.with_name(path.name + ".bak")
        if path.exists():
            shutil.copy2(path, backup)
        self._atomic_write(path, prepared.text)
        return MemoryResult(
            "compacted",
            f"{self.filename(prepared.kind)} compactado; copia anterior en {backup.name}.",
            kind=prepared.kind,
            text=prepared.text,
            backup=str(backup),
            line_count=prepared.line_count,
        )

    def forget(self, query: str) -> MemoryResult:
        """Borra las líneas de USER.md y MEMORY.md que contienen la consulta."""
        needle = fold_text(normalize_entry(query))
        if not needle:
            return MemoryResult("empty", "La consulta está vacía.")
        removed = 0
        files: list[str] = []
        for kind in ("user", "memory"):
            content = self._read(kind)
            lines = content.splitlines()
            kept = [line for line in lines if not (line.strip() and needle in fold_text(line))]
            if len(kept) == len(lines):
                continue
            removed += len(lines) - len(kept)
            new_content = ("\n".join(kept) + "\n") if kept else ""
            self._atomic_write(self._path(kind), new_content)
            files.append(self.filename(kind))
        if not removed:
            return MemoryResult(
                "not_found",
                f"No encontré líneas que coincidan con «{summarize_text(query)}».",
            )
        return MemoryResult(
            "forgotten",
            f"Se borraron {removed} línea(s) de {', '.join(files)}.",
            removed=removed,
            files=tuple(files),
        )

    def find_matches(self, query: str) -> dict[str, list[str]]:
        """Líneas que olvidaría ``forget(query)``, por tipo de memoria."""
        needle = fold_text(normalize_entry(query))
        result: dict[str, list[str]] = {}
        if not needle:
            return result
        for kind in ("user", "memory"):
            matches = [
                line
                for line in self._read(kind).splitlines()
                if line.strip() and needle in fold_text(line)
            ]
            if matches:
                result[kind] = matches
        return result

    # DECISION: si el usuario editó a mano y superó el tope, el bloque del
    # prompt se recorta para no crecer por encima de lo previsto; si ambos
    # archivos están vacíos se devuelve "" para no inyectar una etiqueta vacía.
    def memory_block(self) -> str:
        """USER.md y MEMORY.md dentro de ``<memory>``, recortados a sus topes."""
        sections: list[str] = []
        for kind in ("user", "memory"):
            text = self._read(kind).strip()
            if not text:
                continue
            limit = _KIND_LIMITS[kind]
            if len(text) > limit:
                text = text[: max(0, limit - 1)].rstrip() + "…"
            sections.append(f"## {self.filename(kind)}\n{text}")
        if not sections:
            return ""
        return "<memory>\n" + "\n\n".join(sections) + "\n</memory>"

    # ------------------------------------------------------------------
    # Historial de conversaciones
    # ------------------------------------------------------------------

    def _timestamp_iso(self, ts: datetime | str | None) -> str:
        if ts is None:
            return _as_utc(self._clock()).isoformat(timespec="seconds")
        if isinstance(ts, datetime):
            return _as_utc(ts).isoformat(timespec="seconds")
        if isinstance(ts, str):
            return ts.strip()
        raise TypeError("ts debe ser datetime, str o None")

    # DECISION: un rol no admitido (tool, system) se ignora y devuelve False en
    # vez de lanzar, para que un fallo de integración no corte el turno.
    def record(
        self,
        role: str,
        content: str,
        *,
        session: str | None = None,
        ts: datetime | str | None = None,
    ) -> bool:
        """Guarda un mensaje de usuario o asistente. Otros roles se ignoran."""
        normalized_role = _ROLE_ALIASES.get(str(role).strip().lower())
        if normalized_role is None:
            logger.debug("No se guarda en el historial el rol %r", role)
            return False
        if not isinstance(content, str) or not content.strip():
            return False
        timestamp = self._timestamp_iso(ts)
        with self._connect() as connection:
            connection.execute(
                "INSERT INTO messages (ts, session, role, content) VALUES (?, ?, ?, ?)",
                (timestamp, str(session or self.session), normalized_role, content),
            )
        # DECISION: se purga después de insertar para que ni siquiera un mensaje
        # importado con fecha caducada quede en el historial.
        self.purge_old()
        return True

    def record_exchange(
        self,
        user_text: str,
        assistant_text: str,
        *,
        session: str | None = None,
        ts: datetime | str | None = None,
    ) -> tuple[bool, bool]:
        """Guarda un turno completo: primero el usuario y después el asistente."""
        first = self.record("user", user_text, session=session, ts=ts)
        second = self.record("assistant", assistant_text, session=session, ts=ts)
        return first, second

    def purge_old(self, now: datetime | None = None) -> int:
        """Borra mensajes anteriores al periodo de retención y devuelve cuántos."""
        current = _as_utc(now or self._clock())
        cutoff = (current - timedelta(days=self.retention_days)).isoformat(timespec="seconds")
        with self._connect() as connection:
            cursor = connection.execute("DELETE FROM messages WHERE ts < ?", (cutoff,))
            return max(0, cursor.rowcount)

    def count_messages(self) -> int:
        with self._connect() as connection:
            row = connection.execute("SELECT COUNT(*) AS n FROM messages").fetchone()
        return int(row["n"]) if row is not None else 0

    def _result_from_row(
        self, row: sqlite3.Row, terms: Sequence[str], *, score: float = 0.0
    ) -> dict[str, Any]:
        return {
            "id": int(row["id"]),
            "ts": str(row["ts"]),
            "session": str(row["session"]),
            "role": str(row["role"]),
            "snippet": make_snippet(str(row["content"]), terms),
            "score": score,
        }

    def _recall_fts(self, query: str, limit: int) -> list[dict[str, Any]]:
        terms = _query_terms(query)
        if not terms:
            return []
        expression = " OR ".join('"' + term.replace('"', '""') + '"' for term in terms)
        statement = (
            "SELECT m.id, m.ts, m.session, m.role, m.content, "
            "bm25(messages_fts) AS score "
            "FROM messages_fts JOIN messages AS m ON m.id = messages_fts.rowid "
            "WHERE messages_fts MATCH ? "
            "ORDER BY score ASC, m.id DESC LIMIT ?"
        )
        with self._connect() as connection:
            rows = connection.execute(statement, (expression, limit)).fetchall()
        return [self._result_from_row(row, terms, score=float(row["score"])) for row in rows]

    def _recall_like(self, query: str, limit: int) -> list[dict[str, Any]]:
        terms = _query_terms(query) or [fold_text(query)]
        # DECISION: LIKE de SQLite no ignora acentos; se prueban también los
        # tokens originales para no perder coincidencias exactas.
        raw_terms = [token.lower() for token in _TOKEN_RE.findall(str(query))]
        candidates = list(dict.fromkeys([*terms, *raw_terms]))
        patterns = [_like_pattern(term) for term in candidates]
        where = " OR ".join("content LIKE ? ESCAPE '\\'" for _ in patterns)
        statement = (
            "SELECT id, ts, session, role, content FROM messages "
            f"WHERE {where} ORDER BY ts DESC, id DESC LIMIT ?"
        )
        with self._connect() as connection:
            rows = connection.execute(statement, (*patterns, limit)).fetchall()
        return [self._result_from_row(row, terms) for row in rows]

    # DECISION: si FTS5 está disponible pero no devuelve nada, se prueba LIKE
    # para no perder coincidencias por la tokenización.
    def recall(self, query: str, limit: int = DEFAULT_RECALL_LIMIT) -> list[dict[str, Any]]:
        """Busca conversaciones anteriores; fragmentos de 300 caracteres como máximo."""
        text = normalize_entry(query)
        if not text:
            self.last_backend = "none"
            return []
        safe_limit = max(1, min(int(limit), 20))
        if self.fts_enabled:
            try:
                results = self._recall_fts(text, safe_limit)
            except sqlite3.Error:
                logger.warning("Falló la consulta FTS5; se usa LIKE.", exc_info=True)
            else:
                if results:
                    self.last_backend = "fts5"
                    return results
        self.last_backend = "like"
        return self._recall_like(text, safe_limit)

    def wipe(self) -> int:
        """Borra todo el historial de conversaciones. No toca USER.md ni MEMORY.md."""
        with self._connect() as connection:
            cursor = connection.execute("DELETE FROM messages")
            deleted = max(0, cursor.rowcount)
            if self.fts_enabled:
                connection.execute("INSERT INTO messages_fts(messages_fts) VALUES('rebuild')")
        return deleted


def build_parser() -> argparse.ArgumentParser:
    """Parser del comando de consola de memoria."""
    parser = argparse.ArgumentParser(
        prog="python -m ezagent.agent.memory",
        description="Memoria persistente de EzAgent (USER.md, MEMORY.md, history.db).",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    wipe_parser = subparsers.add_parser(
        "wipe",
        help="Borra todo el historial de conversaciones (no toca USER.md ni MEMORY.md).",
    )
    wipe_parser.add_argument(
        "--history",
        type=Path,
        default=None,
        help="Ruta de history.db; por defecto se usa la de AgentConfig.",
    )
    wipe_parser.add_argument(
        "--yes",
        "-y",
        action="store_true",
        help="No pedir confirmación interactiva.",
    )
    return parser


def _wipe_command(args: argparse.Namespace) -> int:
    if args.history is not None:
        history_path = Path(args.history)
        store = MemoryStore.from_paths(history_path.parent, history_path)
    else:
        from ezagent.agent.config import AgentConfig

        store = MemoryStore(AgentConfig.load())
    # DECISION: el comando de consola pide escribir «borrar» salvo --yes; en
    # modo no interactivo sin --yes no borra nada.
    if not args.yes:
        if not sys.stdin.isatty():
            print(
                "Para borrar el historial sin confirmación interactiva usa --yes.",
                file=sys.stderr,
            )
            return 2
        answer = input(
            "Esto borrará todo el historial de conversaciones. Escribe «borrar» para continuar: "
        )
        if answer.strip().lower() != "borrar":
            print("Cancelado. No se borró nada.")
            return 1
    deleted = store.wipe()
    print(f"Historial borrado: {deleted} mensajes.")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    """Punto de entrada de ``python -m ezagent.agent.memory``."""
    args = build_parser().parse_args(argv)
    if args.command == "wipe":
        return _wipe_command(args)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
