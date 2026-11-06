"""Silueta de la isla y del menú abierto.

Dibuja la muesca de 320 px de ancho con anti-alias SDF de 1 px lógico y salida
BGRA premultiplicada. En tema oscuro conserva los hombros cóncavos de 7 px y
extiende los lados rectos hasta cualquier alto. En tema claro no hay hombros:
el canto superior es recto, el cuerpo va de x=7 a x=313 y lleva un hairline
``LINE`` de 1 px.

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
    H_SIDE,
    R_FLOAT,
    R_ISLAND,
    R_MENU,
    R_SIDE,
    W,
    W_COMPACT,
    W_SIDE,
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


def _sd_round_box_lr(
    x: np.ndarray,
    y: np.ndarray,
    cx: float,
    cy: float,
    hw: float,
    hh: float,
    r_left: float,
    r_right: float,
) -> np.ndarray:
    """Caja redondeada con radios independientes izquierda/derecha."""
    radius = np.where(x < cx, r_left, r_right)
    qx = np.abs(x - cx) - (hw - radius)
    qy = np.abs(y - cy) - (hh - radius)
    return (
        np.sqrt(np.maximum(qx, 0.0) ** 2 + np.maximum(qy, 0.0) ** 2)
        + np.minimum(np.maximum(qx, qy), 0.0)
        - radius
    )


def _side_tab_distance(
    x: np.ndarray,
    y: np.ndarray,
    tab_width: float,
    tab_height: float,
    attachment: str,
    theme: str,
    radius: float = R_SIDE,
) -> np.ndarray:
    """Pestaña lateral: franja llena en el borde y cuello girado en las puntas."""
    if attachment == "right":
        x = tab_width - x
    if theme == "light":
        return _sd_round_box_lr(
            x, y, tab_width / 2.0, tab_height / 2.0,
            tab_width / 2.0, tab_height / 2.0, 0.0, R_SIDE,
        )
    # Rotación exacta del cuello superior: una franja pegada al borde recorre
    # toda la altura y el cuerpo retrocede 7 px en cada punta. El cuello queda
    # lleno contra el borde, sin los huecos de la versión anterior.
    body = _sd_round_box_lr(
        x, y, tab_width / 2.0, tab_height / 2.0,
        tab_width / 2.0, max(0.0, tab_height / 2.0 - FLARE),
        0.0, radius,
    )
    spine = _sd_box(
        x, y, FLARE / 2.0, tab_height / 2.0,
        FLARE / 2.0, tab_height / 2.0,
    )
    dl = np.hypot(x - FLARE, y)
    dr = np.hypot(x - FLARE, y - tab_height)
    return np.maximum(
        np.minimum(body, spine), FLARE - np.minimum(dl, dr)
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
    # Coordenadas locales; el acople inferior es un espejo vertical del
    # superior, así el cuello mira siempre al borde de la pantalla.
    ty = tab_height - y if attachment == "bottom" else y
    if attachment == "float":
        return _sd_round_box(
            x, ty, tab_width / 2.0, tab_height / 2.0,
            tab_width / 2.0, tab_height / 2.0, R_FLOAT, R_FLOAT,
        )
    if theme == "light":
        return _sd_round_box(
            x, ty, tab_width / 2.0, tab_height / 2.0,
            (tab_width - 2.0 * FLARE) / 2.0, tab_height / 2.0,
            0.0, R_ISLAND,
        )
    return _sd_compact_notch(
        x, ty, tab_width, tab_height, FLARE, R_ISLAND
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
    ty = height - y if attachment == "bottom" else y
    if theme == "light":
        return _sd_round_box(
            x, ty, width / 2.0, height / 2.0,
            width / 2.0, height / 2.0, 0.0, R_MENU,
        )
    return _sd_compact_notch(x, ty, width, height, FLARE, R_MENU)


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
    if mask.shape[0] > 0:
        if attachment == "top" and tab_y0 <= 0.01:
            mask[0] = np.where(mask[0] >= 0.5, 1.0, 0.0)
        if attachment == "bottom" and tab_y0 + tab_height >= height - 0.01:
            mask[-1] = np.where(mask[-1] >= 0.5, 1.0, 0.0)

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


_SIDE_CACHE: dict[tuple, np.ndarray] = {}
_SIDE_CACHE_LIMIT: Final[int] = 16


def _render_side_surface_base(
    width: float,
    height: float,
    theme: str,
    scale: float,
    attachment: str,
    tab_width: float,
    tab_height: float,
    panel_width: float,
    panel_height: float,
) -> np.ndarray:
    physical_w = max(1, int(round(width * scale)))
    physical_h = max(1, int(round(height * scale)))
    ys, xs = np.mgrid[0:physical_h, 0:physical_w].astype(np.float32)
    x = ((xs + 0.5) / scale).astype(np.float32)
    y = ((ys + 0.5) / scale).astype(np.float32)

    if panel_height > 0.0 and theme == "dark":
        # Abierto: el mismo cuello del notch superior, girado. La franja del
        # borde recorre la altura completa y el panel se ensancha hacia dentro
        # como una sola pieza.
        distance = _side_tab_distance(
            x, y, width, height, attachment, theme, R_MENU
        )
    elif panel_height > 0.0:
        left_r = 0.0 if attachment == "left" else R_MENU
        right_r = R_MENU if attachment == "left" else 0.0
        distance = _sd_round_box_lr(
            x, y, width / 2.0, height / 2.0,
            width / 2.0, height / 2.0, left_r, right_r,
        )
    else:
        tab_x0 = 0.0 if attachment == "left" else width - tab_width
        distance = _side_tab_distance(
            x - tab_x0, y, tab_width, height, attachment, theme
        )

    mask = np.clip(0.5 - distance / AA_RAMP, 0.0, 1.0).astype(np.float32)
    # El canto pegado al borde físico queda opaco, sin línea gris.
    if mask.shape[1] > 0:
        if attachment == "left":
            mask[:, 0] = np.where(mask[:, 0] >= 0.5, 1.0, 0.0)
        if attachment == "right":
            mask[:, -1] = np.where(mask[:, -1] >= 0.5, 1.0, 0.0)

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


def render_side_surface(
    width: float,
    height: float,
    theme: str = "dark",
    scale: float = 1.0,
    *,
    attachment: str = "left",
    tab_width: float = W_SIDE,
    tab_height: float = H_SIDE,
    panel_width: float = W,
    panel_height: float = 0.0,
) -> np.ndarray:
    """Silueta de una isla acoplada a la izquierda o derecha.

    La pestaña cerrada mide 36×186 y conserva el cuello girado del notch
    superior. Si ``panel_height > 0``, el mismo cuello se extiende a lo largo
    del borde y el panel del menú crece hacia el interior como una sola pieza.
    """
    if width <= 0 or height <= 0 or scale <= 0:
        raise ValueError("width, height y scale deben ser mayores que cero")
    if attachment not in ("left", "right"):
        raise ValueError(f"acople lateral desconocido: {attachment!r}")
    if tab_width <= 0 or tab_height <= 0:
        raise ValueError("la pestaña lateral no puede estar vacía")
    key = (
        round(float(width), 3),
        round(float(height), 3),
        "light" if theme == "light" else "dark",
        round(float(scale), 4),
        attachment,
        round(float(tab_width), 3),
        round(float(tab_height), 3),
        round(float(panel_width), 3),
        round(float(panel_height), 3),
    )
    cached = _SIDE_CACHE.get(key)
    if cached is None:
        cached = _render_side_surface_base(*key)
        if len(_SIDE_CACHE) >= _SIDE_CACHE_LIMIT:
            _SIDE_CACHE.pop(next(iter(_SIDE_CACHE)))
        _SIDE_CACHE[key] = cached
    return cached.copy()


def render_surface(
    width: float,
    height: float,
    theme: str = "dark",
    scale: float = 1.0,
    *,
    tab_width: float = W_COMPACT,
    tab_height: float = H_ISLAND,
    attachment: str = "float",
    menu_h: float = 0.0,
    menu_above: bool = False,
    body_width: float | None = None,
) -> np.ndarray:
    """Silueta BGRA premultiplicada de la isla compacta y su menú.

    ``attachment``: ``top`` (hombros arriba), ``bottom`` (hombros abajo) o
    ``float`` (píldora). Con ``menu_h > 0`` el cuerpo del menú se une a la
    pestaña; si ``tab_width`` y ``body_width`` coinciden se dibuja una sola
    pieza continua, sin escalón en la unión. ``body_width`` por defecto es el
    ancho completo de la ventana.
    """
    if width <= 0 or height <= 0 or scale <= 0:
        raise ValueError("width, height y scale deben ser mayores que cero")
    if attachment not in ("top", "bottom", "float"):
        raise ValueError(f"acople desconocido: {attachment!r}")
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
    _SIDE_CACHE.clear()


__all__ = [
    "bottom_radius",
    "clear_cache",
    "render_side_surface",
    "render_silhouette",
    "render_surface",
]
