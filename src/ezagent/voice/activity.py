"""Actividad de los agentes delegados: un solo modelo para la órbita y la vista rápida.

Two sources are merged into one ordered snapshot:

* **Live** items, read from what other threads already computed (watched
  terminals and their last watcher state, chained stages waiting on a source,
  active swarms). Building the snapshot never calls Herdr or the disk.
* **Recent outcomes** kept by :class:`ActivityLog` (finished, needs you, failed,
  lost). They stay visible for a while and are *unseen* until the user attends.

States are honest: nothing is marked finished just because output stopped, and
there are no percentages. A terminal that vanished is "Sin actualización reciente".
"""

from __future__ import annotations

import threading
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass, replace
from typing import Any, Final

from .herdr import visible_name

NEEDS_YOU: Final = "needs_you"
FAILED: Final = "failed"
WORKING: Final = "working"
WAITING: Final = "waiting"
STALE: Final = "stale"
DONE: Final = "done"

# Lo que pide atención va primero; lo terminado, al final.
STATE_ORDER: Final[dict[str, int]] = {
    NEEDS_YOU: 0,
    FAILED: 1,
    WORKING: 2,
    WAITING: 3,
    STALE: 4,
    DONE: 5,
}
STATE_LABEL: Final[dict[str, str]] = {
    NEEDS_YOU: "Necesita tu respuesta",
    FAILED: "Falló",
    WORKING: "En curso",
    WAITING: "En espera",
    STALE: "Sin actualización reciente",
    DONE: "Completada",
}
# Motivo de un aviso del vigilante → estado.
_REASON_STATE: Final[dict[str, str]] = {
    "done": DONE,
    "idle": DONE,
    "blocked": NEEDS_YOU,
    "error": FAILED,
    "lost": STALE,
}
# Último estado de Herdr de una terminal vigilada → estado.
_LIVE_STATE: Final[dict[str, str]] = {"working": WORKING, "blocked": NEEDS_YOU, "error": FAILED}

TITLE_LIMIT: Final = 120
KEEP_SECS: Final = 30 * 60  # cuánto sigue a la vista un desenlace ya visto
LOG_LIMIT: Final = 8
ORBIT_LIMIT: Final = 5


@dataclass(frozen=True)
class ActivityItem:
    key: str
    title: str
    agent: str
    state: str
    since: float
    unseen: bool = False
    live: bool = False

    @property
    def label(self) -> str:
        return STATE_LABEL.get(self.state, "")

    @property
    def marked(self) -> bool:
        """¿Se dibuja en la órbita? Lo vivo, y lo terminado que aún no se ha visto."""
        return self.live or self.unseen


def chain_key(spec: dict) -> str:
    """Clave de una etapa de cadena en espera (la app la usa para encontrarla)."""
    return f"chain:{spec.get('source')}:{spec.get('agent')}:{spec.get('created_at')}"


def dismissible(item: ActivityItem) -> bool:
    """¿Se puede quitar de la vista? Todo menos un enjambre en marcha."""
    return not (item.live and item.key.startswith("swarm:"))


def clean_title(text: str, fallback: str = "Tarea") -> str:
    title = " ".join(str(text or "").split())
    if len(title) > TITLE_LIMIT:
        title = title[: TITLE_LIMIT - 1].rstrip() + "…"
    return title or fallback


class ActivityLog:
    """Desenlaces recientes; seguro entre hilos (la UI lee, los avisos escriben)."""

    def __init__(self, clock: Callable[[], float] = time.time) -> None:
        self._clock = clock
        self._lock = threading.Lock()
        self._items: dict[str, ActivityItem] = {}

    def record(self, key: str, agent: str, title: str, reason: str) -> ActivityItem:
        state = _REASON_STATE.get(str(reason), NEEDS_YOU)
        item = ActivityItem(
            key=str(key),
            title=clean_title(title),
            agent=str(agent or ""),
            state=state,
            since=self._clock(),
            unseen=True,
        )
        with self._lock:
            self._items.pop(item.key, None)
            self._items[item.key] = item
            while len(self._items) > LOG_LIMIT:
                self._items.pop(next(iter(self._items)))
        return item

    def seen_all(self) -> bool:
        """Marca todo como visto; True si algo cambió."""
        with self._lock:
            changed = any(item.unseen for item in self._items.values())
            self._items = {k: replace(v, unseen=False) for k, v in self._items.items()}
        return changed

    def forget(self, key: str) -> None:
        with self._lock:
            self._items.pop(str(key), None)

    def items(self) -> tuple[ActivityItem, ...]:
        now = self._clock()
        with self._lock:
            expired = [
                k for k, v in self._items.items() if not v.unseen and now - v.since > KEEP_SECS
            ]
            for key in expired:
                self._items.pop(key, None)
            return tuple(self._items.values())


def order(items: Iterable[ActivityItem]) -> tuple[ActivityItem, ...]:
    """Prioridad: estado, luego lo no visto, luego lo más reciente."""
    return tuple(sorted(items, key=lambda i: (STATE_ORDER.get(i.state, 9), not i.unseen, -i.since)))


def merge(live: Iterable[ActivityItem], recent: Iterable[ActivityItem]) -> tuple[ActivityItem, ...]:
    """Une lo vivo y lo reciente; lo vivo gana si comparten clave."""
    out = {item.key: item for item in recent}
    out.update({item.key: item for item in live})
    return order(out.values())


def orbit(items: Iterable[ActivityItem], limit: int = ORBIT_LIMIT) -> tuple[str, ...]:
    """Estados de las marcas de la órbita, en orden de prioridad."""
    return tuple(item.state for item in order(i for i in items if i.marked))[:limit]


# --------------------------------------------------------------- desde la app
def _safe(getter: Callable[[], Any], default: Any) -> Any:
    try:
        return getter()
    except Exception:  # noqa: BLE001 - otro hilo cambió la colección justo ahora
        return default


def live_items(app: object, now: float | None = None) -> list[ActivityItem]:
    """Tareas vivas de la app, sin llamar a Herdr (solo estados ya calculados)."""
    now = time.time() if now is None else now
    tasks = getattr(app, "tasks", None)
    state_of = getattr(getattr(app, "watcher", None), "state_of", None)
    label_for = getattr(getattr(app, "herdr", None), "label_for", None)
    watched = _safe(lambda: tuple(getattr(app, "watched", ()) or ()), ())
    items: list[ActivityItem] = []
    for address in watched:
        info = (_safe(lambda a=address: tasks.get(a), None) if tasks is not None else None) or {}
        raw = state_of(address) if callable(state_of) else "working"
        if raw in ("idle", "done"):
            continue  # el aviso de fin está en camino; no se adelanta
        # Title = the tab the user sees, never their own request read back to them.
        shown = label_for(address) if callable(label_for) else ""
        items.append(
            ActivityItem(
                key=str(address),
                title=clean_title(shown or visible_name({"agent": info.get("agent")})),
                agent="",
                state=_LIVE_STATE.get(str(raw), STALE),
                since=float(info.get("created_at") or now),
                live=True,
            )
        )
    chains = _safe(lambda: tasks.chains(), []) if tasks is not None else []
    for spec in chains or ():
        title = spec.get("name") or spec.get("task") or spec.get("agent")
        items.append(
            ActivityItem(
                key=chain_key(spec),
                title=clean_title(str(title)),
                agent=str(spec.get("agent") or ""),
                state=WAITING,
                since=float(spec.get("created_at") or now),
                live=True,
            )
        )
    supervisor = getattr(getattr(app, "swarm", None), "supervisor", None)
    for swarm_id, goal in tuple(getattr(supervisor, "active_view", ()) or ()):
        items.append(
            ActivityItem(
                key=f"swarm:{swarm_id}",
                title=clean_title(goal, "Enjambre"),
                agent="enjambre",
                state=WORKING,
                since=now,
                live=True,
            )
        )
    return items


def snapshot(app: object, log: ActivityLog | None) -> tuple[ActivityItem, ...]:
    return merge(live_items(app), log.items() if log is not None else ())


__all__ = [
    "DONE",
    "FAILED",
    "NEEDS_YOU",
    "STALE",
    "STATE_LABEL",
    "WAITING",
    "WORKING",
    "ActivityItem",
    "ActivityLog",
    "chain_key",
    "clean_title",
    "dismissible",
    "live_items",
    "merge",
    "orbit",
    "order",
    "snapshot",
]
