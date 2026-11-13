"""Base SQLite del tablero: conexiones, esquema y conversión de filas.

Persistencia en SQLite (WAL). Las escrituras usan ``BEGIN IMMEDIATE`` más un
lock de proceso, así son atómicas aunque otro proceso abra la misma base.
"""

from __future__ import annotations

import json
import sqlite3
import threading
import time
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path

from .models import Member, Message, SwarmError, Task

_DDL = """
CREATE TABLE IF NOT EXISTS swarm (
    id TEXT PRIMARY KEY, goal TEXT NOT NULL, cwd TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active', created REAL NOT NULL, updated REAL NOT NULL);
CREATE TABLE IF NOT EXISTS member (
    swarm TEXT NOT NULL, name TEXT NOT NULL, role TEXT NOT NULL DEFAULT '',
    agent TEXT NOT NULL DEFAULT '', address TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'idle', seen REAL NOT NULL,
    token TEXT NOT NULL DEFAULT '', model TEXT NOT NULL DEFAULT '',
    effort TEXT NOT NULL DEFAULT '', briefed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (swarm, name));
CREATE INDEX IF NOT EXISTS idx_member_token ON member(token);
CREATE TABLE IF NOT EXISTS task (
    id INTEGER PRIMARY KEY AUTOINCREMENT, swarm TEXT NOT NULL, title TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT '', owner TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'pending', deps TEXT NOT NULL DEFAULT '[]',
    parent INTEGER NOT NULL DEFAULT 0, result TEXT NOT NULL DEFAULT '',
    rounds INTEGER NOT NULL DEFAULT 0, created REAL NOT NULL, updated REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_task_swarm ON task(swarm, state);
CREATE TABLE IF NOT EXISTS message (
    id INTEGER PRIMARY KEY AUTOINCREMENT, swarm TEXT NOT NULL, sender TEXT NOT NULL,
    recipient TEXT NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL,
    ref TEXT NOT NULL DEFAULT '', ts REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_message_swarm ON message(swarm, id);
CREATE TABLE IF NOT EXISTS cursor (
    swarm TEXT NOT NULL, member TEXT NOT NULL, last INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (swarm, member));
CREATE TABLE IF NOT EXISTS lease (
    swarm TEXT NOT NULL, path TEXT NOT NULL, member TEXT NOT NULL, expires REAL NOT NULL,
    PRIMARY KEY (swarm, path));
"""


class Db:
    """Conexiones y utilidades comunes del tablero."""

    def __init__(self, path: Path, *, clock: Callable[[], float] = time.time) -> None:
        self.path = Path(path)
        self._clock = clock
        self._lock = threading.Lock()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        connection = sqlite3.connect(str(self.path), timeout=5.0)
        try:
            connection.execute("PRAGMA journal_mode=WAL")
            connection.executescript(_DDL)
        finally:
            connection.close()

    @contextmanager
    def _read(self) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(str(self.path), timeout=5.0)
        connection.row_factory = sqlite3.Row
        try:
            yield connection
        finally:
            connection.close()

    @contextmanager
    def _write(self) -> Iterator[sqlite3.Connection]:
        with self._lock:
            connection = sqlite3.connect(str(self.path), timeout=5.0, isolation_level=None)
            connection.row_factory = sqlite3.Row
            try:
                connection.execute("BEGIN IMMEDIATE")
                yield connection
                connection.execute("COMMIT")
            except BaseException:
                try:
                    connection.execute("ROLLBACK")
                except sqlite3.Error:
                    pass
                raise
            finally:
                connection.close()

    # ------------------------------------------------------- comprobaciones
    def _require_swarm(self, db: sqlite3.Connection, swarm: str) -> None:
        if db.execute("SELECT 1 FROM swarm WHERE id = ?", (swarm,)).fetchone() is None:
            raise SwarmError(f"No existe el enjambre {swarm!r}.")

    def _require_member(self, db: sqlite3.Connection, swarm: str, name: str) -> sqlite3.Row:
        self._require_swarm(db, swarm)
        row = db.execute(
            "SELECT * FROM member WHERE swarm = ? AND name = ?", (swarm, name)
        ).fetchone()
        if row is None:
            raise SwarmError(f"{name!r} no es miembro del enjambre {swarm}.")
        return row

    def _task_row(self, db: sqlite3.Connection, swarm: str, task_id: int) -> sqlite3.Row:
        row = db.execute(
            "SELECT * FROM task WHERE swarm = ? AND id = ?", (swarm, task_id)
        ).fetchone()
        if row is None:
            raise SwarmError(f"No existe la tarea T{task_id}.")
        return row

    def _insert_message(
        self,
        db: sqlite3.Connection,
        swarm: str,
        sender: str,
        recipient: str,
        kind: str,
        body: str,
        ref: str = "",
    ) -> Message:
        now = self._clock()
        cursor = db.execute(
            "INSERT INTO message (swarm, sender, recipient, kind, body, ref, ts) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (swarm, sender, recipient, kind, body, ref, now),
        )
        db.execute("UPDATE swarm SET updated = ? WHERE id = ?", (now, swarm))
        return Message(int(cursor.lastrowid or 0), sender, recipient, kind, body, ref, now)

    # ------------------------------------------------------------ conversión
    @staticmethod
    def _member(row: sqlite3.Row) -> Member:
        return Member(
            row["name"],
            row["role"],
            row["agent"],
            row["address"],
            row["state"],
            row["seen"],
            row["model"],
            row["effort"],
        )

    @staticmethod
    def _task(row: sqlite3.Row) -> Task:
        deps = tuple(int(dep) for dep in json.loads(row["deps"] or "[]"))
        return Task(
            row["id"],
            row["title"],
            row["role"],
            row["owner"],
            row["state"],
            deps,
            row["parent"],
            row["result"],
            row["rounds"],
            row["updated"],
        )

    @staticmethod
    def _message(row: sqlite3.Row) -> Message:
        return Message(
            row["id"],
            row["sender"],
            row["recipient"],
            row["kind"],
            row["body"],
            row["ref"],
            row["ts"],
        )
