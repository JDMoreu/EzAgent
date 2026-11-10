"""Tokens visuales del notch de EzAgent.

Este módulo es la **única** fuente de verdad para colores, medidas, tipografía,
movimiento y coordenadas de la base visual. Ningún otro módulo de la interfaz
debe contener valores visuales literales.

Convenciones:
- Todas las medidas están en píxeles lógicos a 96 DPI. Al dibujar se multiplican
  por ``scale = dpi / 96``.
- Los colores son tuplas RGB 0-255. ``LINE`` y ``LINE_2`` son RGBA porque
  incluyen alfa.
- Los comentarios y nombres de la UI van en español; las interfaces públicas,
  en inglés, como el resto del proyecto.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Final

# --------------------------------------------------------------------------
# Colores (spec 3.1)
# --------------------------------------------------------------------------

RGB = tuple[int, int, int]
RGBA = tuple[int, int, int, int]
Color = RGB | RGBA

DARK: Final[dict[str, Color]] = {
    "BODY": (20, 18, 16),  # #0E0F13
    "GROUP": (30, 27, 24),  # #1C1C1F
    "GROUP_HOVER": (40, 36, 32),  # #26262A
    "TRACK": (52, 48, 43),  # #2F2F34
    "TEXT": (244, 239, 232),  # #F5F5F7
    "TEXT_2": (194, 186, 174),  # #B6B6BF
    "TEXT_3": (156, 148, 136),  # #94949E
    "LINE": (244, 239, 232, 23),  # rgba(255,255,255,.09)
    "LINE_2": (244, 239, 232, 15),  # rgba(255,255,255,.06)
    "MINT": (212, 242, 90),  # #5EEAC4
    "WARM": (255, 178, 62),  # #FFC24A
    "ERR": (255, 93, 82),  # #FF453A
    "MINT_BTN": (212, 242, 90),  # #5EEAC4
    "MINT_INK": (20, 18, 16),  # #0B1220
    "WARM_BTN": (255, 178, 62),  # #FFC24A
    "WARM_INK": (26, 16, 0),  # #1A1200
    "ERR_INK": (20, 18, 16),  # #0E0F13
    "EYE_IDLE": (140, 160, 63),  # #3E927D
    "EYE_MUTE": (183, 140, 58),  # #B78C3A
    # Variantes de texto de la palabra de estado (AA sobre BODY). En oscuro
    # coinciden con el color del ojo; en claro se oscurecen para el texto.
    "WORD_IDLE": (140, 160, 63),
    "WORD_MINT": (212, 242, 90),
    "WORD_WARM": (255, 178, 62),
    "WORD_MUTE": (183, 140, 58),
    "WORD_ERR": (255, 93, 82),
}

LIGHT: Final[dict[str, Color]] = {
    "BODY": (250, 248, 245),  # #FFFFFF
    "GROUP": (241, 237, 231),  # #F5F5F7
    "GROUP_HOVER": (232, 227, 219),  # #EBEBEF
    "TRACK": (214, 208, 198),  # #D1D1D6
    "TEXT": (26, 23, 20),  # #1D1D1F
    "TEXT_2": (78, 72, 64),  # #52525A
    "TEXT_3": (111, 103, 92),  # #6E6E73
    "LINE": (26, 23, 20, 26),  # rgba(0,0,0,.10)
    "LINE_2": (26, 23, 20, 15),  # rgba(0,0,0,.06)
    "MINT": (77, 107, 0),  # #0C917E
    "WARM": (163, 95, 0),  # #C2760A
    "ERR": (200, 36, 26),  # #D70015
    "MINT_BTN": (92, 127, 0),  # #0A7A6A
    "MINT_INK": (255, 255, 255),  # #FFFFFF
    "WARM_BTN": (163, 95, 0),  # #B25000
    "WARM_INK": (255, 255, 255),  # #FFFFFF
    "ERR_INK": (255, 255, 255),  # #FFFFFF
    # Ajuste AA sobre la spec: #6DBDB2 (2,19:1) y #D49F54 (2,36:1) no
    # alcanzaban ni el 3:1 gráfico sobre blanco. Se conserva la temperatura
    # mint/ámbar, un paso más profunda.
    "EYE_IDLE": (111, 138, 31),  # #2F7F74
    "EYE_MUTE": (163, 95, 0),  # #A06412
    "WORD_IDLE": (86, 110, 18),
    "WORD_MINT": (77, 107, 0),  # #0A7A6A
    "WORD_WARM": (163, 95, 0),  # #8A5207
    "WORD_MUTE": (163, 95, 0),
    "WORD_ERR": (200, 36, 26),
}

COLOR_KEYS: Final[tuple[str, ...]] = tuple(DARK)

GLOW_MAX_DARK: Final[float] = 0.10
GLOW_MAX_LIGHT: Final[float] = 0.0


def palette(theme: str) -> dict[str, Color]:
    """Devuelve la paleta del tema.

    ``auto`` se resuelve en la capa de configuración; si llega aquí sin
    resolver se asume oscuro, que es el tema base del notch.
    """
    # DECISION: ``auto`` no se consulta al registro desde este módulo puro;
    # la resolución pertenece a la capa de configuración. Aquí cae a oscuro.
    if theme == "light":
        return LIGHT
    if theme in ("dark", "auto"):
        return DARK
    raise ValueError(f"tema desconocido: {theme!r}")


def glow_max(theme: str) -> float:
    """Alfa máximo del glow aditivo de los ojos (spec 3.4)."""
    return GLOW_MAX_LIGHT if theme == "light" else GLOW_MAX_DARK


# --------------------------------------------------------------------------
# Tipografía (spec 3.2)
# --------------------------------------------------------------------------


@dataclass(frozen=True, slots=True)
class FontStyle:
    """Estilo tipográfico cerrado del notch."""

    token: str
    family: str
    size: float
    weight: int
    tracking_em: float = 0.0
    uppercase: bool = False

    @property
    def mono(self) -> bool:
        return self.family == "GeistMono"


FS_TITLE = FontStyle("FS_TITLE", "Geist", 16, 600)
FS_ROW = FontStyle("FS_ROW", "Geist", 14, 400)
FS_BODY = FontStyle("FS_BODY", "Geist", 13, 400)
FS_SECONDARY = FontStyle("FS_SECONDARY", "Geist", 12, 600)
FS_MICRO = FontStyle("FS_MICRO", "GeistMono", 11, 500, tracking_em=0.10, uppercase=True)

FONT_STYLES: Final[dict[str, FontStyle]] = {
    "FS_TITLE": FS_TITLE,
    "FS_ROW": FS_ROW,
    "FS_BODY": FS_BODY,
    "FS_SECONDARY": FS_SECONDARY,
    "FS_MICRO": FS_MICRO,
}

# Rutas del proyecto. Se resuelven desde este archivo para no depender del cwd.
FONT_DIR: Final[Path] = Path(__file__).resolve().parents[1] / "assets" / "fonts"

FONT_FILES: Final[dict[str, dict[int, Path]]] = {
    "Geist": {
        400: FONT_DIR / "Geist-Regular.ttf",
        500: FONT_DIR / "Geist-Medium.ttf",
        600: FONT_DIR / "Geist-SemiBold.ttf",
    },
    "GeistMono": {
        400: FONT_DIR / "GeistMono-Regular.ttf",
        500: FONT_DIR / "GeistMono-Medium.ttf",
        600: FONT_DIR / "GeistMono-Medium.ttf",
    },
}

# Respaldos del sistema si el proyecto se distribuye sin las fuentes Geist.
# Consolas se usa para Geist Mono y Segoe UI (Semibold en 600) para Geist.
# DECISION: el proyecto incluye Geist; los respaldos solo cubren una instalación
# incompleta. Pillow escoge el primero que exista en cada arranque.
FONT_FALLBACKS: Final[dict[str, dict[int, tuple[Path, ...]]]] = {
    "Geist": {
        400: (Path("C:/Windows/Fonts/segoeui.ttf"),),
        500: (Path("C:/Windows/Fonts/segoeui.ttf"),),
        600: (
            Path("C:/Windows/Fonts/seguisb.ttf"),
            Path("C:/Windows/Fonts/segoeui.ttf"),
        ),
    },
    "GeistMono": {
        400: (Path("C:/Windows/Fonts/consola.ttf"),),
        500: (Path("C:/Windows/Fonts/consola.ttf"),),
        600: (Path("C:/Windows/Fonts/consola.ttf"),),
    },
}


# --------------------------------------------------------------------------
# Espaciado y radios (spec 3.3)
# --------------------------------------------------------------------------

SP_1: Final[int] = 2
SP_2: Final[int] = 4
SP_3: Final[int] = 6
SP_4: Final[int] = 8
SP_5: Final[int] = 12
SP_6: Final[int] = 16
SP_7: Final[int] = 20
SP_8: Final[int] = 24

R_MENU: Final[int] = 18
R_GROUP: Final[int] = 12
FLARE: Final[int] = 7


# --------------------------------------------------------------------------
# Geometría del menú
# --------------------------------------------------------------------------

W: Final[int] = 320
H_ISLAND: Final[int] = 36
BODY_X0: Final[float] = float(FLARE)
BODY_X1: Final[float] = float(W - FLARE)

# Alto de referencia del menú completo (spec 7.3). Se usa para interpolar el
# radio inferior de la silueta abierta 14 -> 18.
H_MENU: Final[int] = 560

# La imagen se dibuja con una rampa anti-alias de 1 px lógico (spec 4.4).
AA_RAMP: Final[float] = 1.0


@dataclass(frozen=True, slots=True)
class Rect:
    """Rectángulo en píxeles lógicos, origen en la ventana."""

    x0: float
    y0: float
    x1: float
    y1: float

    @property
    def width(self) -> float:
        return self.x1 - self.x0

    @property
    def height(self) -> float:
        return self.y1 - self.y0

    @property
    def center(self) -> tuple[float, float]:
        return ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)

    def overlaps(self, other: Rect) -> bool:
        """True si ambos rectángulos comparten área positiva."""
        return (
            self.x0 < other.x1 and other.x0 < self.x1 and self.y0 < other.y1 and other.y0 < self.y1
        )


# --------------------------------------------------------------------------
# Movimiento (spec 3.5)
# --------------------------------------------------------------------------

T_MICRO: Final[int] = 120
T_FAST: Final[int] = 180
T_BASE: Final[int] = 240
T_SLOW: Final[int] = 320
T_CLOSE_RETRACT: Final[int] = 260

VOICE_ATTACK: Final[float] = 45.0
VOICE_RELEASE: Final[float] = 200.0
FPS_ACTIVE: Final[int] = 60
FPS_IDLE: Final[int] = 30

# Puntos de control CSS cubic-bezier(x1, y1, x2, y2). Las funciones EASE_* que
# consumen estos valores viven en anim.py; así la curva no se duplica.
BEZIER_OUT: Final[tuple[float, float, float, float]] = (0.2, 0.8, 0.2, 1.0)
BEZIER_IN: Final[tuple[float, float, float, float]] = (0.5, 0.0, 0.9, 0.4)
BEZIER_INOUT: Final[tuple[float, float, float, float]] = (0.4, 0.0, 0.2, 1.0)
BEZIER_SPRING: Final[tuple[float, float, float, float]] = (0.34, 1.56, 0.64, 1.0)


# --------------------------------------------------------------------------
# Iconos (spec 6.3)
# --------------------------------------------------------------------------

ICON_GRID: Final[float] = 24.0
ICON_STROKE: Final[float] = 1.8
ICON_STROKE_SMALL: Final[float] = 1.6
ICON_SMALL_THRESHOLD: Final[float] = 18.0
ICON_SSAA: Final[int] = 4  # supermuestreo interno del rasterizador vectorial


__all__ = [
    "AA_RAMP",
    "BEZIER_IN",
    "BEZIER_INOUT",
    "BEZIER_OUT",
    "BEZIER_SPRING",
    "BODY_X0",
    "BODY_X1",
    "COLOR_KEYS",
    "DARK",
    "FLARE",
    "FONT_DIR",
    "FONT_FALLBACKS",
    "FONT_FILES",
    "FONT_STYLES",
    "FPS_ACTIVE",
    "FPS_IDLE",
    "FS_BODY",
    "FS_MICRO",
    "FS_ROW",
    "FS_SECONDARY",
    "FS_TITLE",
    "FontStyle",
    "GLOW_MAX_DARK",
    "GLOW_MAX_LIGHT",
    "H_ISLAND",
    "H_MENU",
    "ICON_GRID",
    "ICON_SMALL_THRESHOLD",
    "ICON_SSAA",
    "ICON_STROKE",
    "ICON_STROKE_SMALL",
    "LIGHT",
    "R_GROUP",
    "R_MENU",
    "Rect",
    "SP_1",
    "SP_2",
    "SP_3",
    "SP_4",
    "SP_5",
    "SP_6",
    "SP_7",
    "SP_8",
    "T_BASE",
    "T_CLOSE_RETRACT",
    "T_FAST",
    "T_MICRO",
    "T_SLOW",
    "VOICE_ATTACK",
    "VOICE_RELEASE",
    "W",
    "glow_max",
    "palette",
]
