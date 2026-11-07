"""Registro persistente de tareas que el usuario quiere vigilar.

El watcher de Herdr vive en memoria; este registro guarda en
``%APPDATA%/EzAgent/tasks.json`` qué agente se está vigilando y **qué tarea**
pidió el usuario, para que:

- el aviso de finalización pueda decir qué tarea terminó;
- la tarea sobreviva a un reinicio de EzAgent;
- el prompt del LLM pueda incluir las tareas pendientes.

El archivo es del usuario y editable; el agente no lo toca. La escritura es
atómica y el acceso está protegido por un cerrojo porque lo usan el hilo de
respuesta, el watcher y el hilo de UI.
"""

from __future__ import annotations

import json
import os
import threading
import time
from pathlib import Path
from typing import Any


class TaskRegistry:
    """Tareas pendientes indexadas por la dirección del agente de Herdr."""

    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        self._lock = threading.RLock()
        self._tasks: dict[str, dict[str, Any]] = {}
        self._chains: list[dict[str, Any]] = []
        self._load()

    # ------------------------------------------------------------- lectura
    def _load(self) -> None:
        try:
            raw = self.path.read_text(encoding="utf-8")
            data = json.loads(raw)
        except Exception:
            data = {}
        if isinstance(data, dict) and isinstance(data.get("tasks"), list):
            entries = data["tasks"]
        elif isinstance(data, list):
            entries = data
        elif isinstance(data, dict):
            entries = list(data.values())
        else:
            entries = []
        tasks: dict[str, dict[str, Any]] = {}
        for item in entries:
            if not isinstance(item, dict):
                continue
            address = str(item.get("address") or "").strip()
            if not address:
                continue
            try:
                created = float(item.get("created_at") or time.time())
            except (TypeError, ValueError):
                created = time.time()
            tasks[address] = {
                "address": address,
                "agent": str(item.get("agent") or ""),
                "task": str(item.get("task") or ""),
                "created_at": created,
            }
        chains: list[dict[str, Any]] = []
        raw_chains = (
            data.get("chains")
            if isinstance(data, dict) and isinstance(data.get("chains"), list)
            else []
        )
        for item in raw_chains:
            if not isinstance(item, dict):
                continue
            source = str(item.get("source") or "").strip()
            agent = str(item.get("agent") or "").strip()
            if not source or not agent:
                continue
            spec = dict(item)
            spec["source"] = source
            spec["agent"] = agent
            try:
                spec["created_at"] = float(
                    item.get("created_at") or time.time()
                )
            except (TypeError, ValueError):
                spec["created_at"] = time.time()
            chains.append(spec)
        with self._lock:
            self._tasks = tasks
            self._chains = chains

    def all(self) -> list[dict[str, Any]]:
        with self._lock:
            values = [dict(item) for item in self._tasks.values()]
        return sorted(values, key=lambda item: item.get("created_at", 0.0))

    def get(self, address: str) -> dict[str, Any] | None:
        key = str(address or "").strip()
        if not key:
            return None
        with self._lock:
            item = self._tasks.get(key)
            return dict(item) if item is not None else None

    # ------------------------------------------------------------ escritura
    def add(
        self,
        address: str,
        agent: str = "",
        task: str = "",
        *,
        created_at: float | None = None,
    ) -> dict[str, Any] | None:
        key = str(address or "").strip()
        if not key:
            return None
        entry = {
            "address": key,
            "agent": str(agent or ""),
            "task": str(task or "").strip(),
            "created_at": float(
                created_at if created_at is not None else time.time()
            ),
        }
        with self._lock:
            self._tasks[key] = entry
            self._write_locked()
        return dict(entry)

    def remove(self, address: str) -> dict[str, Any] | None:
        key = str(address or "").strip()
        if not key:
            return None
        with self._lock:
            item = self._tasks.pop(key, None)
            if item is not None:
                self._write_locked()
            return dict(item) if item is not None else None

    def clear(self) -> None:
        with self._lock:
            if self._tasks or self._chains:
                self._tasks.clear()
                self._chains.clear()
                self._write_locked()

    # ------------------------------------------------------------- cadenas
    def add_chain(self, spec: dict[str, Any]) -> dict[str, Any] | None:
        if not isinstance(spec, dict):
            return None
        source = str(spec.get("source") or "").strip()
        agent = str(spec.get("agent") or "").strip()
        if not source or not agent:
            return None
        entry = dict(spec)
        entry["source"] = source
        entry["agent"] = agent
        entry["task"] = str(spec.get("task") or "").strip()
        try:
            entry["created_at"] = float(
                spec.get("created_at") or time.time()
            )
        except (TypeError, ValueError):
            entry["created_at"] = time.time()
        with self._lock:
            self._chains.append(entry)
            self._write_locked()
        return dict(entry)

    def chains(self) -> list[dict[str, Any]]:
        with self._lock:
            values = [dict(item) for item in self._chains]
        return sorted(values, key=lambda item: item.get("created_at", 0.0))

    def chains_for(self, source: str) -> list[dict[str, Any]]:
        key = str(source or "").strip()
        if not key:
            return []
        return [
            item for item in self.chains()
            if item.get("source") == key
        ]

    def remove_chains_for(self, source: str) -> list[dict[str, Any]]:
        key = str(source or "").strip()
        if not key:
            return []
        with self._lock:
            removed = [
                dict(item) for item in self._chains
                if item.get("source") == key
            ]
            if removed:
                self._chains = [
                    item for item in self._chains
                    if item.get("source") != key
                ]
                self._write_locked()
            return removed

    def _write_locked(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "version": 1,
            "tasks": self.all(),
            "chains": [dict(item) for item in self._chains],
        }
        text = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
        tmp = self.path.with_name(self.path.name + ".tmp")
        tmp.write_text(text, encoding="utf-8")
        os.replace(tmp, self.path)

    # -------------------------------------------------------------- prompt
    def prompt_block(self) -> str:
        tasks = self.all()
        chains = self.chains()
        if not tasks and not chains:
            return ""
        lines = ["<pending_tasks>"]
        for item in tasks:
            address = item.get("address") or "?"
            agent = item.get("agent") or "agente"
            task = str(item.get("task") or "").strip()
            if len(task) > 220:
                task = task[:219].rstrip() + "…"
            task = (
                task.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
            )
            lines.append(f"- {address} · {agent} · «{task or 'tarea sin describir'}»")
        for chain in chains:
            source = chain.get("source") or "?"
            agent = chain.get("agent") or "agente"
            task = str(chain.get("task") or "").strip()
            if len(task) > 220:
                task = task[:219].rstrip() + "…"
            task = (
                task.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
            )
            lines.append(
                f"- cadena: cuando termine {source} → {agent} · "
                f"«{task or 'tarea sin describir'}»"
            )
        lines.append("</pending_tasks>")
        return "\n".join(lines)


__all__ = ["TaskRegistry"]
