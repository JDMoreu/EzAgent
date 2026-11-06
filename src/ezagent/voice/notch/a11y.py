"""Contraste y accesibilidad de la paleta.

Las fórmulas de luminancia y contraste son las de la especificación (12.7).
El validador separa dos niveles:

- **Obligatorio**: texto principal/secundario sobre cuerpos y grupos, tinta de
  botones sobre su fondo y acentos gráficos sobre ``BODY``/``GROUP``. Todos
  cumplen AA en ambos temas.
- **Informativo**: ``MINT``/``WARM`` son acentos gráficos y superan 3:1, pero
  no se usan como texto de estado; si se comparan con el umbral de texto
  aparecen como aviso. ``TEXT_3`` sobre ``TRACK``/``GROUP_HOVER`` corresponde a
  estados deshabilitados o hover sin texto principal. Los ojos y las palabras
  de estado usan ``EYE_*``/``WORD_*`` ajustados para AA en ambos temas.

``LINE``/``LINE_2`` son hairlines decorativos y no se evalúan como texto.

# DECISION: las parejas obligatorias incluyen WORD_*; los acentos MINT/WARM se
# auditan como aviso con umbral de texto porque la spec los usa para la palabra
# de estado, aunque la implementación final usa WORD_MINT/WORD_WARM.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Final, Literal

from .tokens import Color, palette

Kind = Literal["text", "graphic"]
AA_TEXT: Final[float] = 4.5
AA_GRAPHIC: Final[float] = 3.0

# Parejas con requisito AA obligatorio.
_TEXT_PAIRS: Final[tuple[tuple[str, str], ...]] = (
    ("TEXT", "BODY"),
    ("TEXT", "GROUP"),
    ("TEXT", "GROUP_HOVER"),
    ("TEXT", "TRACK"),
    ("TEXT_2", "BODY"),
    ("TEXT_2", "GROUP"),
    ("TEXT_2", "GROUP_HOVER"),
    ("TEXT_3", "BODY"),
    ("TEXT_3", "GROUP"),
    ("MINT_INK", "MINT_BTN"),
    ("WARM_INK", "WARM_BTN"),
    ("ERR_INK", "ERR"),
    ("WORD_IDLE", "BODY"),
    ("WORD_MINT", "BODY"),
    ("WORD_WARM", "BODY"),
    ("WORD_MUTE", "BODY"),
    ("WORD_ERR", "BODY"),
)

_GRAPHIC_PAIRS: Final[tuple[tuple[str, str], ...]] = (
    ("MINT", "BODY"),
    ("WARM", "BODY"),
    ("ERR", "BODY"),
    ("EYE_IDLE", "BODY"),
    ("EYE_MUTE", "BODY"),
    ("MINT", "GROUP"),
    ("WARM", "GROUP"),
    ("ERR", "GROUP"),
    ("MINT_BTN", "BODY"),
    ("WARM_BTN", "BODY"),
)

# Avisos: no bloquean, pero se muestran en el informe. La palabra de estado
# usa WORD_*; estos acentos quedan como gráfico o en estados deshabilitados.
_ADVISORY_TEXT_PAIRS: Final[tuple[tuple[str, str], ...]] = (
    ("MINT", "BODY"),       # ojos ESCUCHO/HABLO en tema claro (gráfico)
    ("WARM", "BODY"),       # ojos PIENSO en tema claro (gráfico)
    ("TEXT_3", "GROUP_HOVER"),
    ("TEXT_3", "TRACK"),    # botones deshabilitados (exentos por WCAG)
)


def _lin(c: int) -> float:
    c = c / 255.0
    return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4


def luminance(rgb: tuple[int, ...]) -> float:
    """Luminancia relativa WCAG de un color RGB 0..255."""
    r, g, b = (_lin(v) for v in rgb[:3])
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: tuple[int, ...], b: tuple[int, ...]) -> float:
    """Relación de contraste WCAG entre dos colores RGB (sin alfa)."""
    la, lb = luminance(a), luminance(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)


@dataclass(frozen=True, slots=True)
class ContrastCheck:
    theme: str
    foreground: str
    background: str
    kind: Kind
    minimum: float
    ratio: float
    advisory: bool = False

    @property
    def passed(self) -> bool:
        return self.ratio >= self.minimum


def _check(
    theme: str,
    foreground: str,
    background: str,
    kind: Kind,
    *,
    advisory: bool,
) -> ContrastCheck:
    colors = palette(theme)
    fg: Color = colors[foreground]
    bg: Color = colors[background]
    return ContrastCheck(
        theme=theme,
        foreground=foreground,
        background=background,
        kind=kind,
        minimum=AA_TEXT if kind == "text" else AA_GRAPHIC,
        ratio=contrast(fg, bg),
        advisory=advisory,
    )


def contrast_report(
    theme: str,
    *,
    include_advisory: bool = False,
) -> list[ContrastCheck]:
    """Lista de comprobaciones de contraste de un tema."""
    checks = [
        _check(theme, fg, bg, "text", advisory=False)
        for fg, bg in _TEXT_PAIRS
    ]
    checks += [
        _check(theme, fg, bg, "graphic", advisory=False)
        for fg, bg in _GRAPHIC_PAIRS
    ]
    if include_advisory:
        checks += [
            _check(theme, fg, bg, "text", advisory=True)
            for fg, bg in _ADVISORY_TEXT_PAIRS
        ]
    return checks


def failures(theme: str, *, include_advisory: bool = False) -> list[ContrastCheck]:
    """Comprobaciones que no alcanzan su mínimo."""
    return [
        check
        for check in contrast_report(
            theme, include_advisory=include_advisory
        )
        if not check.passed
    ]


def validate_palette(theme: str) -> bool:
    """True si todas las parejas obligatorias cumplen AA en ``theme``."""
    return not failures(theme)


def validate_all_palettes() -> dict[str, bool]:
    """Valida las parejas obligatorias en oscuro y claro."""
    return {theme: validate_palette(theme) for theme in ("dark", "light")}


__all__ = [
    "AA_GRAPHIC",
    "AA_TEXT",
    "ContrastCheck",
    "contrast",
    "contrast_report",
    "failures",
    "luminance",
    "validate_all_palettes",
    "validate_palette",
]
