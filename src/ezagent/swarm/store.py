"""Tablero del enjambre: estado compartido entre agentes, persistente y ligero.

Los agentes delegados (Claude Code, OpenCode, Codex...) se coordinan aquí sin
gastar contexto en conversar:

- **Mensajes cortos** (``MAX_BODY`` caracteres). El detalle va en un archivo y
  el mensaje solo lleva su ruta en ``ref``.
- **Cursor por miembro**: cada agente solo recibe lo que no ha leído.
- **Tareas por rol, con dependencias y partes**: cada miembro solo puede tomar
  lo que es de su rol (o está a su nombre) y ya tiene las dependencias hechas.
  Una tarea con partes se cierra sola cuando terminan todas.
- **Reapertura** con tope de rondas para el ciclo validar → corregir.
- **Reservas de archivos** (avisos con caducidad) para no pisarse al editar.

Todo lo que escriben los agentes es dato no confiable (AGENTS.md §5): aquí se
valida, se recorta y se guarda; nunca se interpreta.
"""

from __future__ import annotations

import json
import sqlite3
import uuid
from collections.abc import Sequence

from .mail import MailMixin
from .models import (
    BROADCAST,
    MAX_BODY,
    MAX_GOAL,
    MAX_REF,
    MAX_RESULT,
    MAX_ROUNDS,
    MAX_TITLE,
    MEMBER_STATES,
    SWARM_STATES,
    SYSTEM,
    Lease,
    Member,
    Message,
    SwarmError,
    SwarmInfo,
    Task,
    clip,
    member_name,
    required,
    role_name,
)


class SwarmStore(MailMixin):
    """Tablero persistente de uno o varios enjambres."""

    # ---------------------------------------------------------- enjambres
    def create(self, goal: str, *, cwd: str = "", status: str = "active") -> str:
        text = required(goal, MAX_GOAL, "el objetivo")
        if status not in SWARM_STATES:
            raise SwarmError(f"Estado de enjambre desconocido: {status!r}.")
        swarm_id = uuid.uuid4().hex[:8]
        now = self._clock()
        with self._write() as db:
            db.execute(
                "INSERT INTO swarm (id, goal, cwd, status, created, updated) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (swarm_id, text, str(cwd or ""), status, now, now),
            )
        return swarm_id

    def get(self, swarm: str) -> SwarmInfo | None:
        with self._read() as db:
            row = db.execute("SELECT * FROM swarm WHERE id = ?", (swarm,)).fetchone()
        return SwarmInfo(**dict(row)) if row else None

    def active(self) -> list[SwarmInfo]:
        with self._read() as db:
            rows = db.execute(
                "SELECT * FROM swarm WHERE status = 'active' ORDER BY created"
            ).fetchall()
        return [SwarmInfo(**dict(row)) for row in rows]

    def latest(self) -> str:
        """Id del enjambre más reciente, esté activo o no ('' si no hay)."""
        with self._read() as db:
            row = db.execute("SELECT id FROM swarm ORDER BY created DESC LIMIT 1").fetchone()
        return str(row["id"]) if row else ""

    def set_status(self, swarm: str, status: str) -> None:
        if status not in SWARM_STATES:
            raise SwarmError(f"Estado de enjambre desconocido: {status!r}.")
        with self._write() as db:
            self._require_swarm(db, swarm)
            db.execute(
                "UPDATE swarm SET status = ?, updated = ? WHERE id = ?",
                (status, self._clock(), swarm),
            )

    # ----------------------------------------------------------- miembros
    def join(
        self,
        swarm: str,
        name: str,
        *,
        role: str = "",
        agent: str = "",
        address: str = "",
        model: str = "",
        effort: str = "",
    ) -> Member:
        """Alta o actualización de un miembro (idempotente)."""
        member = member_name(name)
        if member in (SYSTEM, "all", "todos"):
            raise SwarmError(f"«{member}» es un nombre reservado.")
        agent_text, _ = clip(agent, 40)
        address_text, _ = clip(address, 96)
        with self._write() as db:
            self._require_swarm(db, swarm)
            db.execute(
                "INSERT INTO member (swarm, name, role, agent, address, state, seen, model, "
                "effort) VALUES (?, ?, ?, ?, ?, 'idle', ?, ?, ?) "
                "ON CONFLICT(swarm, name) DO UPDATE SET role = excluded.role, "
                "agent = excluded.agent, address = excluded.address, seen = excluded.seen, "
                "model = excluded.model, effort = excluded.effort",
                (
                    swarm,
                    member,
                    role_name(role),
                    agent_text,
                    address_text,
                    self._clock(),
                    clip(model, 80)[0],
                    clip(effort, 20)[0],
                ),
            )
            return self._member(self._require_member(db, swarm, member))

    def members(self, swarm: str) -> list[Member]:
        with self._read() as db:
            rows = db.execute(
                "SELECT * FROM member WHERE swarm = ? ORDER BY rowid", (swarm,)
            ).fetchall()
        return [self._member(row) for row in rows]

    def set_member_state(self, swarm: str, name: str, state: str) -> None:
        if state not in MEMBER_STATES:
            raise SwarmError(f"Estado de miembro desconocido: {state!r}.")
        with self._write() as db:
            self._require_member(db, swarm, name)
            db.execute(
                "UPDATE member SET state = ?, seen = ? WHERE swarm = ? AND name = ?",
                (state, self._clock(), swarm, name),
            )

    def set_address(self, swarm: str, name: str, address: str) -> None:
        """Terminal de Herdr donde vive el miembro ('' si aún no se abrió)."""
        with self._write() as db:
            self._require_member(db, swarm, name)
            db.execute(
                "UPDATE member SET address = ?, seen = ? WHERE swarm = ? AND name = ?",
                (clip(address, 96)[0], self._clock(), swarm, name),
            )

    def pending(self, swarm: str, name: str) -> tuple[int, int, tuple[int, ...]]:
        """Lo que justifica despertar a un miembro: (mensajes, último id, tareas listas).

        Solo cuentan los mensajes dirigidos a él y los de EzAgent. Los avisos
        generales de otros («T1 hecha») los verá en su próximo ``next``: no
        merecen gastar un turno de un agente que no tiene nada que hacer.
        """
        with self._read() as db:
            me = self._require_member(db, swarm, name)
            row = db.execute(
                "SELECT COUNT(*), COALESCE(MAX(id), 0) FROM message WHERE swarm = ? "
                "AND sender != ? AND (recipient = ? OR (recipient = ? AND sender = ?)) "
                "AND id > COALESCE("
                "(SELECT last FROM cursor WHERE swarm = ? AND member = ?), 0)",
                (swarm, name, name, BROADCAST, SYSTEM, swarm, name),
            ).fetchone()
            ready = tuple(task["id"] for task in self._ready(db, swarm, me))
        return int(row[0]), int(row[1]), ready

    def touch(self, swarm: str, name: str) -> None:
        """Latido: el miembro dio señales de vida."""
        with self._write() as db:
            db.execute(
                "UPDATE member SET seen = ? WHERE swarm = ? AND name = ?",
                (self._clock(), swarm, name),
            )

    def set_token(self, swarm: str, name: str, digest: str) -> None:
        """Guarda el resumen (nunca el token) con el que se identifica el miembro."""
        with self._write() as db:
            self._require_member(db, swarm, name)
            # Token nuevo = terminal nueva: ese agente aún no conoce su papel.
            db.execute(
                "UPDATE member SET token = ?, briefed = 0 WHERE swarm = ? AND name = ?",
                (digest, swarm, name),
            )

    def mark_briefed(self, swarm: str, name: str) -> bool:
        """Marca que el miembro ya recibió sus instrucciones; True la primera vez."""
        with self._write() as db:
            cursor = db.execute(
                "UPDATE member SET briefed = 1 WHERE swarm = ? AND name = ? AND briefed = 0",
                (swarm, name),
            )
            return cursor.rowcount > 0

    def by_token(self, digest: str) -> tuple[str, Member] | None:
        if not digest:
            return None
        with self._read() as db:
            row = db.execute("SELECT * FROM member WHERE token = ?", (digest,)).fetchone()
        return (row["swarm"], self._member(row)) if row else None

    # ------------------------------------------------------------- tareas
    def add_task(
        self,
        swarm: str,
        title: str,
        *,
        role: str = "",
        owner: str = "",
        deps: Sequence[int] = (),
        parent: int = 0,
    ) -> Task:
        """Crea una tarea; con ``parent`` es una parte de otra tarea."""
        text = required(title, MAX_TITLE, "el título de la tarea")
        role_key = role_name(role)
        now = self._clock()
        with self._write() as db:
            self._require_swarm(db, swarm)
            if owner:
                self._require_member(db, swarm, owner)
            wanted = sorted({int(dep) for dep in deps})
            for dep in wanted:
                self._task_row(db, swarm, dep)
            above = self._task_row(db, swarm, int(parent)) if parent else None
            if above is not None:
                if above["parent"]:
                    raise SwarmError("Las partes solo tienen un nivel.")
                role_key = role_key or above["role"]
                if not role_key and not owner and above["state"] == "pending":
                    owner = above["owner"]
            cursor = db.execute(
                "INSERT INTO task (swarm, title, role, owner, deps, parent, created, updated) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (swarm, text, role_key, owner, json.dumps(wanted), int(parent), now, now),
            )
            if above is not None and above["state"] != "pending":
                # Una tarea con partes ya no se hace directamente.
                self._to_pending(db, swarm, above, owner="")
                if above["state"] == "done":
                    self._reset_dependents(db, swarm, {above["id"]})
            return self._task(self._task_row(db, swarm, int(cursor.lastrowid or 0)))

    def tasks(self, swarm: str) -> list[Task]:
        with self._read() as db:
            rows = db.execute("SELECT * FROM task WHERE swarm = ? ORDER BY id", (swarm,)).fetchall()
        return [self._task(row) for row in rows]

    def ready(self, swarm: str, member: str) -> list[Task]:
        """Tareas que ``member`` podría tomar ahora mismo (sin tomarlas)."""
        with self._read() as db:
            me = self._require_member(db, swarm, member)
            return [self._task(row) for row in self._ready(db, swarm, me)]

    def mine(self, swarm: str, member: str) -> list[Task]:
        """Tareas en curso a nombre de ``member``."""
        with self._read() as db:
            rows = db.execute(
                "SELECT * FROM task WHERE swarm = ? AND owner = ? AND state = 'claimed' "
                "ORDER BY id",
                (swarm, member),
            ).fetchall()
        return [self._task(row) for row in rows]

    def claim(
        self, swarm: str, member: str, task_id: int | None = None, *, every: bool = False
    ) -> list[Task]:
        """Toma la siguiente tarea lista (o todas con ``every``)."""
        with self._write() as db:
            me = self._require_member(db, swarm, member)
            ready = self._ready(db, swarm, me)
            if task_id is not None:
                ready = [row for row in ready if row["id"] == int(task_id)]
            chosen = [row["id"] for row in (ready if every else ready[:1])]
            for chosen_id in chosen:
                db.execute(
                    "UPDATE task SET state = 'claimed', owner = ?, updated = ? WHERE id = ?",
                    (member, self._clock(), chosen_id),
                )
            if chosen:
                self._sync_member(db, swarm, member)
            return [self._task(self._task_row(db, swarm, chosen_id)) for chosen_id in chosen]

    def finish(
        self,
        swarm: str,
        member: str,
        task_id: int,
        result: str = "",
        *,
        ok: bool = True,
        ref: str = "",
    ) -> Task:
        """Cierra una tarea propia y avisa a todos con una línea."""
        summary, _ = clip(result, MAX_RESULT)
        ref_text, _ = clip(ref, MAX_REF)
        with self._write() as db:
            if member != SYSTEM:
                self._require_member(db, swarm, member)
            row = self._task_row(db, swarm, int(task_id))
            if member != SYSTEM:
                if ok and row["state"] == "done" and row["owner"] == member:
                    return self._task(row)  # reintento: ya estaba cerrada
                if row["state"] != "claimed" or row["owner"] != member:
                    raise SwarmError(f"T{row['id']} no está en curso a tu nombre.")
            db.execute(
                "UPDATE task SET state = ?, result = ?, updated = ? WHERE id = ?",
                ("done" if ok else "failed", summary, self._clock(), row["id"]),
            )
            verb = "hecha" if ok else "falló"
            body, _ = clip(f"T{row['id']} {verb}: {summary or row['title']}", MAX_BODY)
            kind = "done" if ok else "block"
            if ok and row["parent"]:
                # Las partes cierran en silencio: avisa una sola vez la tarea madre.
                self._rollup(db, swarm, row["parent"], member)
            else:
                self._insert_message(db, swarm, member, BROADCAST, kind, body, ref_text)
            if row["owner"]:
                self._sync_member(db, swarm, row["owner"], blocked=not ok)
            return self._task(self._task_row(db, swarm, row["id"]))

    def reopen(
        self, swarm: str, task_id: int, *, by: str = SYSTEM, note: str = "", ref: str = ""
    ) -> tuple[Task, bool]:
        """Devuelve una tarea terminada a pendiente; True si agotó las rondas."""
        text, _ = clip(note, MAX_RESULT)
        ref_text, _ = clip(ref, MAX_REF)
        with self._write() as db:
            if by != SYSTEM:
                self._require_member(db, swarm, by)
            row = self._task_row(db, swarm, int(task_id))
            if row["state"] != "done":
                raise SwarmError(f"T{row['id']} no está terminada; escríbele a quien la lleva.")
            rounds = row["rounds"] + 1
            affected = {row["id"]}
            if rounds > MAX_ROUNDS:
                reason, _ = clip(f"Superó {MAX_ROUNDS} rondas de corrección: {text}", MAX_RESULT)
                db.execute(
                    "UPDATE task SET state = 'failed', result = ?, updated = ? WHERE id = ?",
                    (reason, self._clock(), row["id"]),
                )
                body = (
                    f"T{row['id']} superó {MAX_ROUNDS} rondas de corrección; hace falta el usuario."
                )
                self._insert_message(db, swarm, SYSTEM, BROADCAST, "block", body, ref_text)
                self._reset_dependents(db, swarm, affected)
                return self._task(self._task_row(db, swarm, row["id"])), True
            db.execute("UPDATE task SET rounds = ? WHERE id = ?", (rounds, row["id"]))
            target = self._reopen_target(db, swarm, row, text)
            if row["parent"]:
                above = self._task_row(db, swarm, row["parent"])
                if above["state"] == "done":
                    self._to_pending(db, swarm, above)
                    affected.add(above["id"])
            self._reset_dependents(db, swarm, affected)
            body, _ = clip(
                f"T{row['id']} reabierta ({rounds}/{MAX_ROUNDS}): {text or row['title']}", MAX_BODY
            )
            self._insert_message(db, swarm, by, target or BROADCAST, "fix", body, ref_text)
            return self._task(self._task_row(db, swarm, row["id"])), False

    # ---------------------------------------------------------- internos
    def _ready(self, db: sqlite3.Connection, swarm: str, me: sqlite3.Row) -> list[sqlite3.Row]:
        """Tareas pendientes que ``me`` puede tomar: suyas primero, luego por número."""
        rows = db.execute("SELECT * FROM task WHERE swarm = ? ORDER BY id", (swarm,)).fetchall()
        by_id = {row["id"]: row for row in rows}
        done = {row["id"] for row in rows if row["state"] == "done"}
        parents = {row["parent"] for row in rows if row["parent"]}

        def takeable(row: sqlite3.Row) -> bool:
            if row["state"] != "pending" or row["id"] in parents:
                return False
            if row["owner"]:
                if row["owner"] != me["name"]:
                    return False
            elif row["role"] and row["role"] != me["role"]:
                return False
            deps = set(json.loads(row["deps"]))
            if row["parent"] in by_id:
                deps |= set(json.loads(by_id[row["parent"]]["deps"]))
            return deps <= done

        ready = [row for row in rows if takeable(row)]
        return sorted(ready, key=lambda row: (row["owner"] != me["name"], row["id"]))

    def _to_pending(
        self, db: sqlite3.Connection, swarm: str, row: sqlite3.Row, *, owner: str | None = None
    ) -> None:
        db.execute(
            "UPDATE task SET state = 'pending', owner = ?, updated = ? WHERE id = ?",
            (row["owner"] if owner is None else owner, self._clock(), row["id"]),
        )
        if row["owner"] and row["state"] == "claimed":
            self._sync_member(db, swarm, row["owner"])

    def _reopen_target(
        self, db: sqlite3.Connection, swarm: str, row: sqlite3.Row, note: str
    ) -> str:
        """Reabre la tarea (o le añade una parte de corrección); devuelve a quién avisar."""
        has_parts = db.execute("SELECT 1 FROM task WHERE parent = ?", (row["id"],)).fetchone()
        now = self._clock()
        if has_parts:
            title, _ = clip(f"Corregir: {note or row['title']}", MAX_TITLE)
            db.execute(
                "INSERT INTO task (swarm, title, role, deps, parent, created, updated) "
                "VALUES (?, ?, ?, '[]', ?, ?, ?)",
                (swarm, title, row["role"], row["id"], now, now),
            )
            db.execute(
                "UPDATE task SET state = 'pending', owner = '', updated = ? WHERE id = ?",
                (now, row["id"]),
            )
            return ""
        db.execute(
            "UPDATE task SET state = 'pending', result = ?, updated = ? WHERE id = ?",
            (note, now, row["id"]),
        )
        return str(row["owner"])

    def _reset_dependents(self, db: sqlite3.Connection, swarm: str, changed: set[int]) -> None:
        """Lo que dependía de tareas reabiertas vuelve a pendiente (y sus partes)."""
        rows = db.execute("SELECT * FROM task WHERE swarm = ?", (swarm,)).fetchall()
        seen = set(changed)
        queue = list(changed)
        while queue:
            current = queue.pop()
            for row in rows:
                if row["id"] in seen or row["state"] not in ("claimed", "done"):
                    continue
                if current in json.loads(row["deps"]):
                    self._to_pending(db, swarm, row)
                    seen.add(row["id"])
                    queue.append(row["id"])
                    for part in rows:
                        if part["parent"] == row["id"] and part["id"] not in seen:
                            if part["state"] in ("claimed", "done"):
                                self._to_pending(db, swarm, part)
                            seen.add(part["id"])

    def _rollup(self, db: sqlite3.Connection, swarm: str, parent: int, closer: str) -> None:
        """Cierra la tarea madre cuando todas sus partes están hechas."""
        parts = db.execute("SELECT state FROM task WHERE parent = ?", (parent,)).fetchall()
        if not parts or any(part["state"] != "done" for part in parts):
            return
        result = f"{len(parts)} partes hechas"
        db.execute(
            "UPDATE task SET state = 'done', result = ?, updated = ? WHERE id = ?",
            (result, self._clock(), parent),
        )
        self._insert_message(db, swarm, closer, BROADCAST, "done", f"T{parent} hecha: {result}")

    def _sync_member(
        self, db: sqlite3.Connection, swarm: str, name: str, *, blocked: bool = False
    ) -> None:
        """Ajusta el estado del miembro según si tiene tareas en curso."""
        busy = db.execute(
            "SELECT 1 FROM task WHERE swarm = ? AND owner = ? AND state = 'claimed'",
            (swarm, name),
        ).fetchone()
        state = "working" if busy else ("blocked" if blocked else "idle")
        if not busy:  # sin trabajo en curso no tiene sentido retener archivos
            db.execute("DELETE FROM lease WHERE swarm = ? AND member = ?", (swarm, name))
        db.execute(
            "UPDATE member SET state = ?, seen = ? WHERE swarm = ? AND name = ?",
            (state, self._clock(), swarm, name),
        )


__all__ = [
    "BROADCAST",
    "MAX_BODY",
    "MAX_ROUNDS",
    "SYSTEM",
    "Lease",
    "Member",
    "Message",
    "SwarmError",
    "SwarmInfo",
    "SwarmStore",
    "Task",
]
