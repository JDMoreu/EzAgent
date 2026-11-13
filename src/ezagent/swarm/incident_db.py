"""Registro de incidentes del enjambre en el tablero (tabla ``incident``).

Un incidente activo por miembro como máximo: si el mismo agente vuelve a
fallar, se actualiza el existente en lugar de abrir otro. Estados:

- ``open``: espera una decisión del usuario.
- ``waiting``: se decidió esperar (o se espera solo a la hora de
  restablecimiento); el supervisor lo retoma cuando el agente vuelve.
- ``resolved``: cerrado, con la acción que se tomó.
"""

from __future__ import annotations

from .db import Db
from .models import MAX_RESULT, Incident, SwarmError, clip

ACTIVE = ("open", "waiting")
INCIDENT_STATES = frozenset({"open", "waiting", "resolved"})


class IncidentMixin(Db):
    """Alta, consulta y cierre de incidentes."""

    def open_incident(
        self, swarm: str, member: str, kind: str, detail: str = "", *, resets: float = 0.0
    ) -> tuple[Incident, bool]:
        """Abre (o actualiza) el incidente activo del miembro; True si es nuevo."""
        text, _ = clip(detail, MAX_RESULT)
        now = self._clock()
        state = "waiting" if resets > 0 else "open"
        with self._write() as db:
            self._require_member(db, swarm, member)
            row = db.execute(
                "SELECT * FROM incident WHERE swarm = ? AND member = ? AND state IN (?, ?) "
                "ORDER BY id DESC LIMIT 1",
                (swarm, member, *ACTIVE),
            ).fetchone()
            if row is not None:
                same = row["kind"] == kind
                db.execute(
                    "UPDATE incident SET kind = ?, detail = ?, resets = ?, updated = ? WHERE id = ?",
                    (kind, text, float(resets or (row["resets"] if same else 0.0)), now, row["id"]),
                )
                return self._incident(self._incident_row(db, row["id"])), not same
            cursor = db.execute(
                "INSERT INTO incident (swarm, member, kind, detail, resets, state, created, updated) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (swarm, member, kind, text, float(resets), state, now, now),
            )
            return self._incident(self._incident_row(db, int(cursor.lastrowid or 0))), True

    def incident(self, incident_id: int) -> Incident | None:
        with self._read() as db:
            row = db.execute("SELECT * FROM incident WHERE id = ?", (int(incident_id),)).fetchone()
        return self._incident(row) if row else None

    def incidents(self, swarm: str, *, active: bool = True) -> list[Incident]:
        query = "SELECT * FROM incident WHERE swarm = ?"
        params: tuple[object, ...] = (swarm,)
        if active:
            query += " AND state IN (?, ?)"
            params += ACTIVE
        with self._read() as db:
            rows = db.execute(query + " ORDER BY id", params).fetchall()
        return [self._incident(row) for row in rows]

    def member_incident(self, swarm: str, member: str) -> Incident | None:
        """Incidente activo del miembro, o None."""
        with self._read() as db:
            row = db.execute(
                "SELECT * FROM incident WHERE swarm = ? AND member = ? AND state IN (?, ?) "
                "ORDER BY id DESC LIMIT 1",
                (swarm, member, *ACTIVE),
            ).fetchone()
        return self._incident(row) if row else None

    def set_incident(self, incident_id: int, state: str, action: str = "") -> Incident:
        if state not in INCIDENT_STATES:
            raise SwarmError(f"Estado de incidente desconocido: {state!r}.")
        with self._write() as db:
            row = self._incident_row(db, int(incident_id))
            db.execute(
                "UPDATE incident SET state = ?, action = ?, updated = ? WHERE id = ?",
                (state, clip(action, 40)[0] or row["action"], self._clock(), row["id"]),
            )
            return self._incident(self._incident_row(db, row["id"]))

    def _incident_row(self, db, incident_id: int):  # noqa: ANN001, ANN202 - fila sqlite3
        row = db.execute("SELECT * FROM incident WHERE id = ?", (incident_id,)).fetchone()
        if row is None:
            raise SwarmError(f"No existe el incidente I{incident_id}.")
        return row

    @staticmethod
    def _incident(row) -> Incident:  # noqa: ANN001 - fila sqlite3
        return Incident(
            row["id"],
            row["swarm"],
            row["member"],
            row["kind"],
            row["detail"],
            row["resets"],
            row["state"],
            row["action"],
            row["created"],
            row["updated"],
        )
