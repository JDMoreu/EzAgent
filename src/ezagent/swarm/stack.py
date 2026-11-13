"""Todo el enjambre en un objeto: tablero, servicio, servidor local y supervisor.

La app de voz solo crea un ``SwarmStack``, lo arranca y lo detiene. Mientras no
haya enjambres activos no hace nada: el supervisor duerme y el servidor solo
ocupa un puerto local.
"""

from __future__ import annotations

import shutil
import sys
from collections.abc import Callable
from html import escape
from pathlib import Path
from typing import Any

from .server import SwarmServer
from .service import SwarmService
from .store import SwarmStore
from .supervisor import Supervisor


def find_command_dir(command: str = "ezs") -> str:
    """Carpeta del ejecutable ``ezs`` (junto a Python o en el PATH), o ''."""
    beside = Path(sys.executable).parent
    for name in (f"{command}.exe", command):
        if (beside / name).is_file():
            return str(beside)
    found = shutil.which(command)
    return str(Path(found).parent) if found else ""


class SwarmStack:
    """Punto único de montaje del modo enjambre."""

    def __init__(
        self,
        base_dir: Path,
        herdr: Any,
        *,
        announce: Callable[[str], None],
        command: str = "ezs",
    ) -> None:
        self.base_dir = Path(base_dir)
        self.herdr = herdr
        self.store = SwarmStore(self.base_dir / "board.db")
        self.service = SwarmService(self.store, command=command)
        self.server = SwarmServer(self.service, self.base_dir / "endpoint.json")
        self.command_dir = find_command_dir(command)
        self.supervisor = Supervisor(
            self.service,
            herdr,
            seat_dir=self.base_dir / "seats",
            announce=announce,
            command_dir=self.command_dir,
        )

    def start(self) -> None:
        self.server.start()
        self.supervisor.start()

    def stop(self) -> None:
        self.supervisor.stop()
        self.server.stop()

    def active(self) -> bool:
        return bool(self.store.active())

    def prompt_block(self) -> str:
        """Estado de los enjambres activos para el prompt (dato, no instrucción)."""
        active = self.store.active()
        if not active:
            return ""
        lines = ["<swarms>"]
        for info in active[-3:]:
            goal = escape(" ".join(info.goal.split())[:100], quote=False)
            spoken = escape(self.service.status(info.id).spoken(), quote=False)
            lines.append(f"- enjambre {info.id} · «{goal}» · {spoken}")
        lines.append("</swarms>")
        return "\n".join(lines)


__all__ = ["SwarmStack", "find_command_dir"]
