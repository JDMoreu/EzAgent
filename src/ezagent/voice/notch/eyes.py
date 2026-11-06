"""Ojos del notch: poses, SDF de cápsulas, arco y onda de voz.

El dibujo se hace con SDF analítico, anti-alias de 1 px lógico y numpy, y solo
sobre la caja de cada ojo/arco. El buffer destino es float32 ``(H, W, 4)`` RGBA
premultiplicado. Este módulo no anima: recibe ``level``, ``t``, ``offset`` y
``blink`` ya calculados por la capa de animación para que sea puro y testeable.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

from .tokens import (
    AA_RAMP,
    Color,
    EYE_ARC_POINTS,
    EYE_ARC_STROKE,
    WAVE_BAR_COUNT,
    WAVE_BAR_W,
    WAVE_HEIGHT_BASE,
    WAVE_HEIGHT_SPAN,
    WAVE_MOD_AMP,
    WAVE_MOD_BASE,
    WAVE_PROFILE,
    WAVE_SIN_FREQ,
    WAVE_SIN_PHASE_STEP,
    WAVE_SLOT,
    WAVE_W,
    glow_max,
    palette,
)


@dataclass(frozen=True)
class EyePose:
    w: float
    h: float
    rot: float
    gap: float
    glow: float
    wave_w: float
    shape: str       # 'capsule' o 'arc'
    color_key: str   # nombre del token de color
    gain: float = 0.0  # px extra de alto por unidad de nivel (0..1)


POSES: dict[str, EyePose] = {
    'open':   EyePose(11, 3, 0, 14, 0, 0, 'capsule', 'EYE_IDLE'),
    'idle':   EyePose(7, 11, 0, 14, 6, 0, 'capsule', 'EYE_IDLE'),
    'listen': EyePose(8, 15, 0, 16, 10, 0, 'capsule', 'MINT', gain=7),
    'think':  EyePose(8, 8, 0, 14, 6, 0, 'capsule', 'WARM'),
    'speak':  EyePose(8, 10, 0, 10, 6, 27, 'capsule', 'MINT', gain=3),
    'notice': EyePose(11, 6, 0, 14, 10, 0, 'arc', 'MINT'),
    'mute':   EyePose(11, 3, 0, 14, 0, 0, 'capsule', 'EYE_MUTE'),
    'error':  EyePose(8, 12, 18, 14, 8, 0, 'capsule', 'ERR'),
    'close':  EyePose(11, 3, 0, 14, 0, 0, 'capsule', 'EYE_IDLE'),
}

POSE_WORD = {'idle': 'ESPERA', 'listen': 'ESCUCHO', 'think': 'PIENSO',
             'speak': 'HABLO', 'notice': 'AVISO', 'mute': 'PAUSA', 'error': 'ERROR'}


def _resolve_pose(pose: EyePose | str) -> EyePose:
    if isinstance(pose, EyePose):
        return pose
    try:
        return POSES[pose]
    except KeyError as exc:
        raise ValueError(f"pose de ojos desconocida: {pose!r}") from exc


def _box(
    cx: float,
    cy: float,
    reach: float,
    height: int,
    width: int,
) -> tuple[int, int, int, int] | None:
    x0 = max(0, int(math.floor(cx - reach)))
    y0 = max(0, int(math.floor(cy - reach)))
    x1 = min(width, int(math.ceil(cx + reach)) + 1)
    y1 = min(height, int(math.ceil(cy + reach)) + 1)
    if x1 <= x0 or y1 <= y0:
        return None
    return x0, y0, x1, y1


def _capsule_distance(
    x0: int,
    y0: int,
    x1: int,
    y1: int,
    cx: float,
    cy: float,
    w: float,
    h: float,
    rot_deg: float,
) -> np.ndarray:
    """SDF de una cápsula rotada, solo sobre su caja.

    La spec describe la cápsula vertical (r = ancho/2). Para las rayas
    horizontales de 11 x 3 (open/close/mute) esa fórmula degenera en un
    círculo; la referencia HTML y la intención visual piden una píldora
    horizontal. Por eso el radio sigue el eje corto.
    """
    xs = np.arange(x0, x1, dtype=np.float32) + 0.5
    ys = np.arange(y0, y1, dtype=np.float32) + 0.5
    dx = xs[None, :] - cx
    dy = ys[:, None] - cy
    angle = math.radians(rot_deg)
    ca, sa = math.cos(angle), math.sin(angle)
    x = dx * ca + dy * sa
    y = -dx * sa + dy * ca
    if w > h:
        radius = h / 2.0
        half = max(w / 2.0 - radius, 0.0)
        qx = np.clip(x, -half, half)
        return np.hypot(x - qx, y) - radius
    radius = w / 2.0
    half = max(h / 2.0 - radius, 0.0)
    qy = np.clip(y, -half, half)
    return np.hypot(x, y - qy) - radius


def _arc_distance(
    x0: int,
    y0: int,
    x1: int,
    y1: int,
    cx: float,
    cy: float,
    scale: float,
    blink: float,
) -> np.ndarray:
    """SDF de un arco ^ de dos segmentos con extremos redondeados."""
    xs = np.arange(x0, x1, dtype=np.float32) + 0.5
    ys = np.arange(y0, y1, dtype=np.float32) + 0.5
    dx = xs[None, :] - cx
    dy = ys[:, None] - cy
    points = [
        (px * scale, py * blink * scale) for px, py in EYE_ARC_POINTS
    ]
    stroke = EYE_ARC_STROKE * scale
    result = np.full(dx.shape, np.inf, dtype=np.float32)
    for (ax, ay), (bx, by) in zip(points[:-1], points[1:]):
        vx = bx - ax
        vy = by - ay
        length2 = vx * vx + vy * vy
        if length2 <= 1e-9:
            distance = np.hypot(dx - ax, dy - ay)
        else:
            t = np.clip(((dx - ax) * vx + (dy - ay) * vy) / length2, 0.0, 1.0)
            px = ax + vx * t
            py = ay + vy * t
            distance = np.hypot(dx - px, dy - py)
        result = np.minimum(result, distance)
    return result - stroke / 2.0


def _coverage(distance: np.ndarray, aa: float) -> np.ndarray:
    return np.clip(0.5 - distance / aa, 0.0, 1.0)


def _glow(
    distance: np.ndarray,
    radius: float,
    maximum: float,
) -> np.ndarray:
    if radius <= 0.0 or maximum <= 0.0:
        return np.zeros_like(distance)
    halo = np.exp(-(np.maximum(distance, 0.0) / radius) ** 2) * maximum
    return np.minimum(halo, maximum)


def _over(
    region: np.ndarray,
    coverage: np.ndarray,
    rgb: np.ndarray,
    alpha: float,
) -> None:
    a = np.clip(coverage * alpha, 0.0, 1.0)
    region[..., :3] = (
        rgb[None, None, :] * a[..., None]
        + region[..., :3] * (1.0 - a[..., None])
    )
    region[..., 3] = a + region[..., 3] * (1.0 - a)


def _add_glow(
    region: np.ndarray,
    distance: np.ndarray,
    radius: float,
    maximum: float,
    rgb: np.ndarray,
    strength: float,
) -> None:
    if radius <= 0.0 or maximum <= 0.0 or strength <= 0.0:
        return
    halo = _glow(distance, radius, maximum) * strength
    # El glow también suma alfa: así el buffer sigue siendo premultiplicado
    # incluso si se dibuja sobre un fondo transparente. Sobre BODY (alfa 1)
    # el resultado visual es idéntico.
    region[..., :3] += rgb[None, None, :] * halo[..., None]
    region[..., 3] = np.minimum(1.0, region[..., 3] + halo)
    np.minimum(region[..., :3], region[..., 3:4], out=region[..., :3])


def _draw_capsule(
    buf: np.ndarray,
    cx: float,
    cy: float,
    pose: EyePose,
    w: float,
    h: float,
    rgb: np.ndarray,
    color_alpha: float,
    scale: float,
    own_alpha: float,
    group_alpha: float,
    theme: str,
    rotation: float,
) -> None:
    physical_w = max(0.0, w) * scale
    physical_h = max(0.0, h) * scale
    if physical_w <= 0.0 or physical_h <= 0.0 or own_alpha <= 0.0:
        return
    aa = AA_RAMP * scale
    glow_radius = pose.glow * scale
    reach = math.hypot(physical_w, physical_h) / 2.0 + glow_radius + 2.0 * aa
    height, width = buf.shape[:2]
    box = _box(cx, cy, reach, height, width)
    if box is None:
        return
    x0, y0, x1, y1 = box
    region = buf[y0:y1, x0:x1]
    distance = _capsule_distance(
        x0, y0, x1, y1, cx, cy, physical_w, physical_h, rotation
    )
    strength = own_alpha * group_alpha * color_alpha
    _add_glow(
        region,
        distance,
        glow_radius,
        glow_max(theme),
        rgb,
        strength,
    )
    _over(region, _coverage(distance, aa), rgb, strength)


def _draw_arc(
    buf: np.ndarray,
    cx: float,
    cy: float,
    pose: EyePose,
    rgb: np.ndarray,
    color_alpha: float,
    scale: float,
    own_alpha: float,
    group_alpha: float,
    theme: str,
    blink: float,
) -> None:
    if own_alpha <= 0.0:
        return
    aa = AA_RAMP * scale
    glow_radius = pose.glow * scale
    local = [(x * scale, y * blink * scale) for x, y in EYE_ARC_POINTS]
    reach = max(math.hypot(px, py) for px, py in local)
    reach += EYE_ARC_STROKE * scale / 2.0 + glow_radius + 2.0 * aa
    height, width = buf.shape[:2]
    box = _box(cx, cy, reach, height, width)
    if box is None:
        return
    x0, y0, x1, y1 = box
    region = buf[y0:y1, x0:x1]
    distance = _arc_distance(x0, y0, x1, y1, cx, cy, scale, blink)
    strength = own_alpha * group_alpha * color_alpha
    _add_glow(
        region,
        distance,
        glow_radius,
        glow_max(theme),
        rgb,
        strength,
    )
    _over(region, _coverage(distance, aa), rgb, strength)


def _draw_wave(
    buf: np.ndarray,
    wave_cx: float,
    cy: float,
    pose: EyePose,
    level: float,
    t: float,
    rgb: np.ndarray,
    color_alpha: float,
    scale: float,
    wave_alpha: float,
    wave_width: float,
    group_alpha: float,
) -> None:
    if pose.wave_w <= 0.0 or wave_width <= 0.0 or wave_alpha <= 0.0:
        return
    compression = max(0.0, min(1.0, wave_width / max(WAVE_W, 1e-6)))
    if compression <= 0.0:
        return
    aa = AA_RAMP * scale
    height, width = buf.shape[:2]
    # Se recorren las cinco barras; cada SDF se calcula solo sobre su caja.
    for index in range(WAVE_BAR_COUNT):
        profile = WAVE_PROFILE[index]
        modulation = WAVE_MOD_BASE + WAVE_MOD_AMP * math.sin(
            WAVE_SIN_FREQ * t + WAVE_SIN_PHASE_STEP * index
        )
        logical_h = WAVE_HEIGHT_BASE + level * profile * WAVE_HEIGHT_SPAN * modulation
        logical_w = WAVE_BAR_W * compression
        if logical_w <= 0.0 or logical_h <= 0.0:
            continue
        offset = (index - (WAVE_BAR_COUNT - 1) / 2.0) * WAVE_SLOT * compression
        bx = (wave_cx + offset * scale)
        by = cy
        physical_w = logical_w * scale
        physical_h = logical_h * scale
        reach = math.hypot(physical_w, physical_h) / 2.0 + 2.0 * aa
        box = _box(bx, by, reach, height, width)
        if box is None:
            continue
        x0, y0, x1, y1 = box
        region = buf[y0:y1, x0:x1]
        distance = _capsule_distance(
            x0, y0, x1, y1, bx, by, physical_w, physical_h, 0.0
        )
        _over(
            region,
            _coverage(distance, aa),
            rgb,
            wave_alpha * group_alpha * color_alpha,
        )


def draw_eyes(
    buf: np.ndarray,
    cx: float,
    cy: float,
    pose: EyePose | str,
    color: Color | None,
    scale: float,
    level: float = 0.0,
    t: float = 0.0,
    theme: str = "dark",
    *,
    blink: float = 1.0,
    offset: tuple[float, float] = (0.0, 0.0),
    capsule_alpha: float | None = None,
    arc_alpha: float | None = None,
    wave_alpha: float = 1.0,
    wave_width: float | None = None,
    alpha: float = 1.0,
    gap_override: float | None = None,
    wave_below: bool = False,
) -> None:
    """Dibuja una pose de ojos centrada en ``(cx, cy)`` lógicos.

    - ``blink`` escala la altura del grupo (0,08 durante el parpadeo).
    - ``offset`` permite deriva, barrido, saltitos y sacudida sin recalcular la
      pose.
    - ``capsule_alpha`` y ``arc_alpha`` permiten el fundido de forma; por
      defecto se deduce de ``pose.shape``.
    - ``wave_width`` anima el ancho de la onda; por defecto usa ``pose.wave_w``.
    - ``theme`` elige la paleta y el límite de glow (0 en claro).
    - ``color`` puede imponer un color de transición; ``None`` usa
      ``pose.color_key`` de la paleta del tema.
    # DECISION: ``cx``/``cy`` son lógicos y ``scale`` los convierte a físicos.
    # Los extras son keyword-only para no romper la llamada de referencia.
    """
    if buf.ndim != 3 or buf.shape[2] != 4 or buf.dtype != np.float32:
        raise TypeError("buf debe ser float32 con forma (H, W, 4)")
    if scale <= 0.0:
        raise ValueError("scale debe ser mayor que cero")
    # ``off`` no está en POSES (12.3) porque la isla está retirada; dibujarlo
    # como no-op evita romper llamadas de la máquina de estados.
    if pose == "off":
        return
    resolved = _resolve_pose(pose)
    group_alpha = max(0.0, min(1.0, alpha))
    if group_alpha <= 0.0:
        return
    level = max(0.0, min(1.0, level))
    blink = max(0.0, min(1.0, blink))
    if color is None:
        base = palette(theme)[resolved.color_key]
        rgb = np.asarray(base[:3], dtype=np.float32) / 255.0
        color_alpha = 1.0
    else:
        rgb = np.asarray(color[:3], dtype=np.float32) / 255.0
        color_alpha = (color[3] / 255.0) if len(color) == 4 else 1.0

    if capsule_alpha is None:
        capsule_alpha = 1.0 if resolved.shape == "capsule" else 0.0
    if arc_alpha is None:
        arc_alpha = 1.0 if resolved.shape == "arc" else 0.0
    capsule_alpha = max(0.0, min(1.0, capsule_alpha))
    arc_alpha = max(0.0, min(1.0, arc_alpha))

    width = resolved.w
    gap = resolved.gap if gap_override is None else float(gap_override)
    wave_active = resolved.wave_w > 0.0
    current_wave_w = 0.0
    if wave_active:
        current_wave_w = (
            resolved.wave_w if wave_width is None else max(0.0, wave_width)
        )
        current_wave_w = min(current_wave_w, resolved.wave_w)
    horizontal_wave = wave_active and not wave_below
    total = (
        width * 2.0
        + gap * (2.0 if horizontal_wave else 1.0)
        + (current_wave_w if horizontal_wave else 0.0)
    )
    x0 = cx + offset[0] - total / 2.0
    eye_y = cy + offset[1]
    left_cx = x0 + width / 2.0
    if horizontal_wave:
        wave_cx = x0 + width + gap + current_wave_w / 2.0
        right_cx = x0 + width + gap + current_wave_w + gap + width / 2.0
    else:
        wave_cx = cx + offset[0]
        right_cx = x0 + width + gap + width / 2.0

    height_eye = resolved.h + resolved.gain * level
    left_cx_px = left_cx * scale
    right_cx_px = right_cx * scale
    eye_y_px = eye_y * scale

    # Glow y formas. El ojo izquierdo gira +rot y el derecho -rot.
    _draw_capsule(
        buf, left_cx_px, eye_y_px, resolved, width, height_eye * blink,
        rgb, color_alpha, scale, capsule_alpha, group_alpha, theme,
        resolved.rot,
    )
    _draw_capsule(
        buf, right_cx_px, eye_y_px, resolved, width, height_eye * blink,
        rgb, color_alpha, scale, capsule_alpha, group_alpha, theme,
        -resolved.rot,
    )
    _draw_arc(
        buf, left_cx_px, eye_y_px, resolved, rgb, color_alpha, scale,
        arc_alpha, group_alpha, theme, blink,
    )
    _draw_arc(
        buf, right_cx_px, eye_y_px, resolved, rgb, color_alpha, scale,
        arc_alpha, group_alpha, theme, blink,
    )
    if wave_active:
        wave_y = eye_y
        if wave_below:
            # En el acople lateral la onda baja debajo de los ojos: el ancho
            # de la píldora no cabe a los lados.
            wave_y += height_eye * blink / 2.0 + 8.0
        _draw_wave(
            buf, wave_cx * scale, wave_y * scale, resolved, level, t, rgb,
            color_alpha, scale, wave_alpha, current_wave_w, group_alpha,
        )


__all__ = [
    "EyePose",
    "POSES",
    "POSE_WORD",
    "draw_eyes",
]
