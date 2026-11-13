"""Mensajes breves con cursor por miembro y reservas de archivos."""

from __future__ import annotations

from collections.abc import Sequence

from .db import Db
from .models import (
    BROADCAST,
    DEFAULT_LEASE_SECS,
    MAX_BODY,
    MAX_INBOX,
    MAX_REF,
    MESSAGE_KINDS,
    SYSTEM,
    Lease,
    Message,
    SwarmError,
    clip,
    lease_path,
    member_name,
    overlaps,
)


class MailMixin(Db):
    """Mensajería y reservas del tablero."""

    # ----------------------------------------------------------- mensajes
    def post(
        self,
        swarm: str,
        sender: str,
        body: str,
        *,
        to: str = BROADCAST,
        kind: str = "info",
        ref: str = "",
    ) -> tuple[Message, bool]:
        """Publica un mensaje breve; devuelve el mensaje y si se recortó."""
        if kind not in MESSAGE_KINDS:
            raise SwarmError(f"Tipo de mensaje desconocido: {kind!r}.")
        # DECISION: un mensaje largo se recorta en vez de rechazarse; repetir
        # el envío gastaría más tokens que perder la cola. El aviso va en el
        # resultado para que el agente use `ref` la próxima vez.
        text, truncated = clip(body, MAX_BODY)
        if not text:
            raise SwarmError("El mensaje está vacío.")
        ref_text, _ = clip(ref, MAX_REF)
        with self._write() as db:
            if sender != SYSTEM:
                self._require_member(db, swarm, sender)
            else:
                self._require_swarm(db, swarm)
            target = BROADCAST if to == BROADCAST else member_name(to, "destinatario")
            if target != BROADCAST:
                self._require_member(db, swarm, target)
            message = self._insert_message(db, swarm, sender, target, kind, text, ref_text)
        return message, truncated

    def inbox(
        self, swarm: str, member: str, *, mark: bool = True, limit: int = MAX_INBOX
    ) -> list[Message]:
        """Mensajes no leídos para ``member`` (directos y a todos), sin los propios."""
        with self._write() as db:
            self._require_member(db, swarm, member)
            row = db.execute(
                "SELECT last FROM cursor WHERE swarm = ? AND member = ?", (swarm, member)
            ).fetchone()
            rows = db.execute(
                "SELECT * FROM message WHERE swarm = ? AND id > ? AND sender != ? "
                "AND recipient IN (?, ?) ORDER BY id LIMIT ?",
                (
                    swarm,
                    int(row["last"]) if row else 0,
                    member,
                    member,
                    BROADCAST,
                    max(1, min(limit, 100)),
                ),
            ).fetchall()
            messages = [self._message(item) for item in rows]
            if mark and messages:
                db.execute(
                    "INSERT INTO cursor (swarm, member, last) VALUES (?, ?, ?) "
                    "ON CONFLICT(swarm, member) DO UPDATE SET last = excluded.last",
                    (swarm, member, messages[-1].id),
                )
        return messages

    def unread(self, swarm: str, member: str) -> int:
        with self._read() as db:
            row = db.execute(
                "SELECT COUNT(*) FROM message WHERE swarm = ? AND sender != ? "
                "AND recipient IN (?, ?) AND id > COALESCE("
                "(SELECT last FROM cursor WHERE swarm = ? AND member = ?), 0)",
                (swarm, member, member, BROADCAST, swarm, member),
            ).fetchone()
        return int(row[0]) if row else 0

    def history(self, swarm: str, *, since: int = 0, limit: int = 50) -> list[Message]:
        with self._read() as db:
            rows = db.execute(
                "SELECT * FROM (SELECT * FROM message WHERE swarm = ? AND id > ? "
                "ORDER BY id DESC LIMIT ?) ORDER BY id",
                (swarm, int(since), max(1, min(limit, 500))),
            ).fetchall()
        return [self._message(row) for row in rows]

    # ----------------------------------------------------------- reservas
    def reserve(
        self, swarm: str, member: str, paths: Sequence[str], *, ttl: float = DEFAULT_LEASE_SECS
    ) -> list[Lease]:
        """Reserva rutas (todas o ninguna). Devuelve los choques con otros."""
        wanted = sorted({lease_path(path) for path in paths})
        if not wanted:
            raise SwarmError("No indicaste rutas para reservar.")
        now = self._clock()
        with self._write() as db:
            self._require_member(db, swarm, member)
            db.execute("DELETE FROM lease WHERE swarm = ? AND expires <= ?", (swarm, now))
            others = [
                Lease(row["path"], row["member"], row["expires"])
                for row in db.execute(
                    "SELECT * FROM lease WHERE swarm = ? AND member != ?", (swarm, member)
                ).fetchall()
            ]
            conflicts = [
                lease for lease in others if any(overlaps(lease.path, path) for path in wanted)
            ]
            if conflicts:
                return conflicts
            expires = now + max(30.0, float(ttl))
            db.executemany(
                "INSERT INTO lease (swarm, path, member, expires) VALUES (?, ?, ?, ?) "
                "ON CONFLICT(swarm, path) DO UPDATE SET member = excluded.member, "
                "expires = excluded.expires",
                [(swarm, path, member, expires) for path in wanted],
            )
        return []

    def release(self, swarm: str, member: str, paths: Sequence[str] | None = None) -> int:
        with self._write() as db:
            if paths is None:
                cursor = db.execute(
                    "DELETE FROM lease WHERE swarm = ? AND member = ?", (swarm, member)
                )
            else:
                cursor = db.executemany(
                    "DELETE FROM lease WHERE swarm = ? AND member = ? AND path = ?",
                    [(swarm, member, lease_path(path)) for path in paths],
                )
            return max(0, cursor.rowcount)

    def leases(self, swarm: str) -> list[Lease]:
        with self._read() as db:
            rows = db.execute(
                "SELECT * FROM lease WHERE swarm = ? AND expires > ? ORDER BY path",
                (swarm, self._clock()),
            ).fetchall()
        return [Lease(row["path"], row["member"], row["expires"]) for row in rows]
