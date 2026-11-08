"""Icono de la bandeja del sistema de EzAgent.

El icono vive en su propio hilo (pystray). Los clics del menú no tocan Tk ni el
notch: publican una intención en la cola de la aplicación, que la ejecuta en el
hilo principal. Si pystray no está disponible, la app sigue funcionando sin
bandeja.
"""

from __future__ import annotations

import threading
from pathlib import Path
from typing import Callable

from PIL import Image

try:  # pragma: no cover - depende del entorno de escritorio
    import pystray as _pystray
except Exception:  # pragma: no cover - instalaciones sin bandeja
    _pystray = None

ICON_SIZE = 64
_TITLE = "EzAgent · activo"


def _fallback_image() -> Image.Image:
    """Marca de respaldo: la «e» de un solo trazo con su punto de agente."""
    from .brand import mark_image

    return mark_image(ICON_SIZE)


def load_icon_image(path: Path | None = None) -> Image.Image:
    """Carga el icono del proyecto o dibuja el respaldo."""
    if path is not None:
        try:
            if path.exists():
                return (
                    Image.open(path)
                    .convert("RGBA")
                    .resize((ICON_SIZE, ICON_SIZE), Image.LANCZOS)
                )
        except Exception:
            pass
    return _fallback_image()


class TrayIcon:
    """Bandeja del sistema con Configurar / Reiniciar / Cerrar."""

    def __init__(
        self,
        on_action: Callable[[str], None],
        *,
        image_path: Path | None = None,
        title: str = _TITLE,
    ) -> None:
        self._on_action = on_action
        self._image_path = image_path
        self._title = title
        self._icon = None
        self._thread: threading.Thread | None = None

    @property
    def available(self) -> bool:
        return _pystray is not None

    @property
    def running(self) -> bool:
        return self._icon is not None

    def start(self) -> bool:
        if _pystray is None or self._icon is not None:
            return self._icon is not None
        try:
            menu = _pystray.Menu(
                _pystray.MenuItem(
                    "Configurar",
                    self._menu_config,
                    default=True,
                ),
                _pystray.MenuItem(
                    "Reiniciar EzAgent", self._menu_restart
                ),
                _pystray.Menu.SEPARATOR,
                _pystray.MenuItem("Cerrar", self._menu_close),
            )
            icon = _pystray.Icon(
                "ezagent",
                load_icon_image(self._image_path),
                self._title,
                menu,
            )
        except Exception:
            self._icon = None
            return False
        self._icon = icon
        self._thread = threading.Thread(
            target=self._run,
            name="ezagent-tray",
            daemon=True,
        )
        self._thread.start()
        return True

    def _run(self) -> None:
        icon = self._icon
        if icon is None:
            return
        try:
            icon.run()
        except Exception:
            pass

    def _dispatch(self, action: str) -> None:
        try:
            self._on_action(action)
        except Exception:
            pass

    def _menu_config(self, icon=None, item=None) -> None:
        self._dispatch("config")

    def _menu_restart(self, icon=None, item=None) -> None:
        self._dispatch("restart")

    def _menu_close(self, icon=None, item=None) -> None:
        self._dispatch("quit")

    def set_title(self, title: str) -> None:
        """Cambia el tooltip sin bloquear si la bandeja aún no existe."""
        self._title = str(title)
        icon = self._icon
        if icon is not None:
            try:
                icon.title = self._title
            except Exception:
                pass

    def stop(self) -> None:
        icon, self._icon = self._icon, None
        if icon is not None:
            try:
                icon.stop()
            except Exception:
                pass


__all__ = ["TrayIcon", "load_icon_image", "ICON_SIZE"]
