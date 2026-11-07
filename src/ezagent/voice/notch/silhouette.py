"""Silueta de la isla y del menú abierto.

Dibuja la muesca superior y la píldora flotante con anti-alias SDF de 1 px
lógico y salida BGRA premultiplicada. La muesca (``top``) conserva los hombros
cóncavos de 7 px en tema oscuro; la píldora (``float``) es una cápsula de radio
18. En tema claro la muesca no tiene hombros: el canto superior es recto y el
cuerpo va de x=7 a x=313 con un hairline ``LINE`` de 1 px.

La capa estática se cachea por ``(alto, tema, escala, sx)``; ``dy`` y
``opacity`` se aplican después sobre una copia, para que las animaciones de
despertar/retirada no contaminen la caché.
"""

from __future__ import annotations

from typing import Final

import numpy as np

from .tokens import (
    AA_RAMP,
    BODY_WIDTH,
    FLARE,
    H_ISLAND,
    R_FLOAT,
    R_ISLAND,
    R_MENU,
    W,
    W_COMPACT,
    WINDOW_OPEN_H,
    palette,
)

_CACHE: dict[tuple[float, str, float, float], np.ndarray] = {}
_CACHE_LIMIT: Final[int] = 12


def bottom_radius(height: float) -> float:
    """Radio inferior interpolado 14 -> 18 con el progreso de apertura.

    # DECISION: el progreso se mide entre 36 y 596 px (isla + menú completo,
    # sección 7.3). Es la única referencia de altura abierta disponible.
    """
    span = max(1.0, float(WINDOW_OPEN_H - H_ISLAND))
    progress = max(0.0, min(1.0, (height - H_ISLAND) / span))
    return float(R_ISLAND) + (float(R_MENU) - float(R_ISLAND)) * progress


def _sd_box(
    x: np.ndarray,
    y: np.ndarray,
    cx: float,
    cy: float,
    hw: float,
    hh: float,
) -> np.ndarray:
    qx = np.abs(x - cx) - hw
    qy = np.abs(y - cy) - hh
    return (
        np.sqrt(np.maximum(qx, 0.0) ** 2 + np.maximum(qy, 0.0) ** 2)
        + np.minimum(np.maximum(qx, qy), 0.0)
    )


def _sd_round_box(
    x: np.ndarray,
    y: np.ndarray,
    cx: float,
    cy: float,
    hw: float,
    hh: float,
    r_top: float,
    r_bottom: float,
) -> np.ndarray:
    radius = np.where(y < cy, r_top, r_bottom)
    qx = np.abs(x - cx) - (hw - radius)
    qy = np.abs(y - cy) - (hh - radius)
    return (
        np.sqrt(np.maximum(qx, 0.0) ** 2 + np.maximum(qy, 0.0) ** 2)
        + np.minimum(np.maximum(qx, qy), 0.0)
        - radius
    )


def _dark_distance(
    x: np.ndarray, y: np.ndarray, height: float, radius: float
) -> np.ndarray:
    body = _sd_round_box(
        x,
        y,
        W / 2.0,
        height / 2.0,
        BODY_WIDTH / 2.0,
        height / 2.0,
        0.0,
        radius,
    )
    top = _sd_box(x, y, W / 2.0, FLARE / 2.0, W / 2.0, FLARE / 2.0)
    dl = np.hypot(x, y - FLARE)
    dr = np.hypot(x - W, y - FLARE)
    return np.maximum(
        np.minimum(body, top), FLARE - np.minimum(dl, dr)
    )


def _light_distance(
    x: np.ndarray, y: np.ndarray, height: float, radius: float
) -> np.ndarray:
    return _sd_round_box(
        x,
        y,
        W / 2.0,
        height / 2.0,
        BODY_WIDTH / 2.0,
        height / 2.0,
        0.0,
        radius,
    )


def _ring_coverage(distance: np.ndarray) -> np.ndarray:
    """Anillo de 1 px lógico centrado en el borde del cuerpo."""
    half_width = 0.5
    band_distance = np.abs(distance) - half_width
    return np.clip(0.5 - band_distance / AA_RAMP, 0.0, 1.0)


def _render_base(
    height: float,
    theme: str,
    scale: float,
    sx: float,
) -> np.ndarray:
    physical_h = max(1, int(round(height * scale)))
    physical_w = max(1, int(round(W * scale)))
    ys, xs = np.mgrid[0:physical_h, 0:physical_w].astype(np.float32)
    # Centros de píxel -> coordenadas lógicas -> corrección de escala X.
    xl = (xs + 0.5) / scale
    yl = (ys + 0.5) / scale
    if abs(sx - 1.0) > 1e-9:
        xl = (xl - W / 2.0) / sx + W / 2.0
    x = xl.astype(np.float32)
    y = yl.astype(np.float32)

    radius = bottom_radius(height)
    aa = AA_RAMP  # las coordenadas ya están en px lógicos
    if theme == "light":
        distance = _light_distance(x, y, height, radius)
    else:
        distance = _dark_distance(x, y, height, radius)
    mask = np.clip(0.5 - distance / aa, 0.0, 1.0).astype(np.float32)

    # El canto superior debe quedar opaco: la rampa de AA solo se aplica a los
    # lados y a la esquina inferior, nunca deja translúcida la primera fila.
    # DECISION: en la primera fila se conserva el píxel si su cobertura supera
    # 0,5; así el borde recto no se ve gris y las esquinas siguen recortadas.
    if mask.shape[0] > 0:
        top = mask[0]
        mask[0] = np.where(top >= 0.5, 1.0, 0.0)

    colors = palette(theme)
    body = np.asarray(colors["BODY"][:3], dtype=np.float32) / 255.0
    out_a = mask
    out_c = body[None, None, :] * mask[..., None]

    if theme == "light":
        line = colors["LINE"]
        line_alpha = (line[3] / 255.0) if len(line) == 4 else 1.0
        ring = _ring_coverage(distance) * line_alpha
        line_rgb = np.asarray(line[:3], dtype=np.float32) / 255.0
        out_c = line_rgb[None, None, :] * ring[..., None] + out_c * (
            1.0 - ring[..., None]
        )
        out_a = ring + out_a * (1.0 - ring)

    out_c = np.minimum(out_c, out_a[..., None])
    bgra = np.empty((physical_h, physical_w, 4), dtype=np.uint8)
    bgra[..., 0] = np.clip(out_c[..., 2] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 1] = np.clip(out_c[..., 1] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 2] = np.clip(out_c[..., 0] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 3] = np.clip(out_a * 255.0 + 0.5, 0, 255).astype(np.uint8)
    return bgra


def _base(
    height: float,
    theme: str,
    scale: float,
    sx: float,
) -> np.ndarray:
    key = (
        round(float(height), 3),
        "light" if theme == "light" else "dark",
        round(float(scale), 4),
        round(float(sx), 3),
    )
    cached = _CACHE.get(key)
    if cached is None:
        cached = _render_base(
            key[0], key[1], key[2], key[3]
        )
        if len(_CACHE) >= _CACHE_LIMIT:
            _CACHE.pop(next(iter(_CACHE)))
        _CACHE[key] = cached
    return cached


def render_silhouette(
    height: float,
    theme: str = "dark",
    scale: float = 1.0,
    *,
    sx: float = 1.0,
    dy: float = 0.0,
    opacity: float = 1.0,
) -> np.ndarray:
    """Silueta BGRA premultiplicada ``(alto, ancho, 4)``.

    ``height`` es el alto total visible de la ventana en px lógicos (36 para la
    isla cerrada; 36 + alto del menú para la abierta). ``dy`` desplaza el
    contenido verticalmente y ``opacity`` lo atenúa; ninguno de los dos entra en
    la caché. ``sx`` escala horizontalmente alrededor del centro para A-01/A-02.
    """
    if height <= 0:
        raise ValueError("height debe ser mayor que cero")
    if scale <= 0:
        raise ValueError("scale debe ser mayor que cero")
    if sx <= 0:
        raise ValueError("sx debe ser mayor que cero")
    opacity = max(0.0, min(1.0, float(opacity)))
    base = _base(height, theme, scale, sx)
    out = base.copy()

    shift = int(round(dy * scale))
    if shift:
        shifted = np.zeros_like(out)
        h = out.shape[0]
        if shift > 0:
            if shift < h:
                shifted[shift:] = out[: h - shift]
        else:
            amount = -shift
            if amount < h:
                shifted[: h - amount] = out[amount:]
        out = shifted

    if opacity < 1.0:
        out = np.clip(
            out.astype(np.float32) * opacity + 0.5, 0, 255
        ).astype(np.uint8)
    return out


_SURFACE_CACHE: dict[tuple, np.ndarray] = {}
_SURFACE_LIMIT: Final[int] = 16


def _sd_compact_notch(
    x: np.ndarray,
    y: np.ndarray,
    width: float,
    height: float,
    flare: float,
    radius: float,
) -> np.ndarray:
    body_w = width - 2.0 * flare
    body = _sd_round_box(
        x, y, width / 2.0, height / 2.0, body_w / 2.0, height / 2.0,
        0.0, radius,
    )
    top = _sd_box(x, y, width / 2.0, flare / 2.0, width / 2.0, flare / 2.0)
    dl = np.hypot(x, y - flare)
    dr = np.hypot(x - width, y - flare)
    return np.maximum(np.minimum(body, top), flare - np.minimum(dl, dr))


def _tab_distance(
    x: np.ndarray,
    y: np.ndarray,
    tab_width: float,
    tab_height: float,
    attachment: str,
    theme: str,
) -> np.ndarray:
    if attachment == "float":
        return _sd_round_box(
            x, y, tab_width / 2.0, tab_height / 2.0,
            tab_width / 2.0, tab_height / 2.0, R_FLOAT, R_FLOAT,
        )
    if theme == "light":
        return _sd_round_box(
            x, y, tab_width / 2.0, tab_height / 2.0,
            (tab_width - 2.0 * FLARE) / 2.0, tab_height / 2.0,
            0.0, R_ISLAND,
        )
    return _sd_compact_notch(
        x, y, tab_width, tab_height, FLARE, R_ISLAND
    )


def _continuous_distance(
    x: np.ndarray,
    y: np.ndarray,
    width: float,
    height: float,
    attachment: str,
    theme: str,
) -> np.ndarray:
    """Cuerpo continuo del menú: la pestaña y el panel son una sola pieza."""
    if attachment == "float":
        return _sd_round_box(
            x, y, width / 2.0, height / 2.0,
            width / 2.0, height / 2.0, R_MENU, R_MENU,
        )
    if theme == "light":
        return _sd_round_box(
            x, y, width / 2.0, height / 2.0,
            width / 2.0, height / 2.0, 0.0, R_MENU,
        )
    return _sd_compact_notch(x, y, width, height, FLARE, R_MENU)


def _render_surface_base(
    width: float,
    height: float,
    theme: str,
    scale: float,
    tab_width: float,
    tab_height: float,
    attachment: str,
    menu_h: float,
    menu_above: bool,
    body_width: float,
) -> np.ndarray:
    physical_w = max(1, int(round(width * scale)))
    physical_h = max(1, int(round(height * scale)))
    ys, xs = np.mgrid[0:physical_h, 0:physical_w].astype(np.float32)
    x = ((xs + 0.5) / scale).astype(np.float32)
    y = ((ys + 0.5) / scale).astype(np.float32)

    tab_x0 = (width - tab_width) / 2.0
    tab_y0 = menu_h if (menu_h > 0.0 and menu_above) else 0.0
    if menu_h > 0.0 and abs(tab_width - body_width) < 0.51:
        # La pestaña y el menú comparten ancho: se dibuja una sola silueta,
        # sin escalón ni costura en la unión.
        body_x = x - (width - body_width) / 2.0
        distance = _continuous_distance(
            body_x, y, body_width, height, attachment, theme
        )
    else:
        tab_x = x - tab_x0
        tab_y = y - tab_y0
        distance = _tab_distance(
            tab_x, tab_y, tab_width, tab_height, attachment, theme
        )
        if menu_h > 0.0:
            menu_y0 = 0.0 if menu_above else tab_height
            menu_y1 = menu_y0 + menu_h
            menu_distance = _sd_round_box(
                x, y, width / 2.0, (menu_y0 + menu_y1) / 2.0,
                body_width / 2.0, menu_h / 2.0, R_MENU, R_MENU,
            )
            distance = np.minimum(distance, menu_distance)

    mask = np.clip(0.5 - distance / AA_RAMP, 0.0, 1.0).astype(np.float32)
    # El canto pegado al borde físico queda opaco, sin línea gris.
    if mask.shape[0] > 0 and attachment == "top" and tab_y0 <= 0.01:
        mask[0] = np.where(mask[0] >= 0.5, 1.0, 0.0)

    colors = palette(theme)
    body = np.asarray(colors["BODY"][:3], dtype=np.float32) / 255.0
    out_a = mask
    out_c = body[None, None, :] * mask[..., None]

    if theme == "light":
        line = colors["LINE"]
        line_alpha = (line[3] / 255.0) if len(line) == 4 else 1.0
        ring = _ring_coverage(distance) * line_alpha
        line_rgb = np.asarray(line[:3], dtype=np.float32) / 255.0
        out_c = line_rgb[None, None, :] * ring[..., None] + out_c * (
            1.0 - ring[..., None]
        )
        out_a = ring + out_a * (1.0 - ring)

    out_c = np.minimum(out_c, out_a[..., None])
    bgra = np.empty((physical_h, physical_w, 4), dtype=np.uint8)
    bgra[..., 0] = np.clip(out_c[..., 2] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 1] = np.clip(out_c[..., 1] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 2] = np.clip(out_c[..., 0] * 255.0 + 0.5, 0, 255).astype(np.uint8)
    bgra[..., 3] = np.clip(out_a * 255.0 + 0.5, 0, 255).astype(np.uint8)
    return bgra


def render_surface(
    width: float,
    height: float,
    theme: str = "dark",
    scale: float = 1.0,
    *,
    tab_width: float | None = None,
    tab_height: float = H_ISLAND,
    attachment: str = "float",
    menu_h: float = 0.0,
    menu_above: bool = False,
    body_width: float | None = None,
) -> np.ndarray:
    """Silueta BGRA premultiplicada de la isla y su menú.

    ``attachment``: ``top`` (muesca con hombros) o ``float`` (píldora). Con
    ``menu_h > 0`` el cuerpo del menú se une a la pestaña; si ``tab_width`` y
    ``body_width`` coinciden se dibuja una sola pieza continua, sin escalón en
    la unión. ``tab_width`` por defecto es el ancho de la pestaña cerrada
    (169 px, o el ancho de la ventana si es menor). ``body_width`` por defecto
    es el ancho completo de la ventana.
    """
    if width <= 0 or height <= 0 or scale <= 0:
        raise ValueError("width, height y scale deben ser mayores que cero")
    if attachment not in ("top", "float"):
        raise ValueError(f"acople desconocido: {attachment!r}")
    if tab_width is None:
        tab_width = min(float(width), float(W_COMPACT))
    if tab_width <= 0 or tab_height <= 0 or tab_width > width:
        raise ValueError("la pestaña no cabe en el ancho")
    if body_width is None:
        body_width = width
    if body_width <= 0 or body_width > width:
        raise ValueError("el cuerpo del menú no cabe en el ancho")
    menu_h = max(0.0, float(menu_h))
    key = (
        round(float(width), 3),
        round(float(height), 3),
        "light" if theme == "light" else "dark",
        round(float(scale), 4),
        round(float(tab_width), 3),
        round(float(tab_height), 3),
        attachment,
        round(menu_h, 3),
        bool(menu_above),
        round(float(body_width), 3),
    )
    cached = _SURFACE_CACHE.get(key)
    if cached is None:
        cached = _render_surface_base(*key)
        if len(_SURFACE_CACHE) >= _SURFACE_LIMIT:
            _SURFACE_CACHE.pop(next(iter(_SURFACE_CACHE)))
        _SURFACE_CACHE[key] = cached
    return cached.copy()


def clear_cache() -> None:
    """Vacía la caché de siluetas (cambio de DPI, tema o contenido)."""
    _CACHE.clear()
    _SURFACE_CACHE.clear()


__all__ = [
    "bottom_radius",
    "clear_cache",
    "render_silhouette",
    "render_surface",
]
