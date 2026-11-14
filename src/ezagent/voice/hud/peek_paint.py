"""Dibujo de la vista rápida: medidas, textos y capas (sin Win32).

Separated from ``peek`` so neither module passes 600 lines; ``peek`` keeps the
window and the controller and re-exports what callers use.
"""

from __future__ import annotations

import math
from functools import lru_cache

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

from ..activity import (
    DONE,
    FAILED,
    NEEDS_YOU,
    STALE,
    WAITING,
    WORKING,
    ActivityItem,
    dismissible,
)
from .bar_theme import card_palette
from .icons import colorize, draw_icon, segment_distance
from .marks import ink as mark_ink
from .text import baseline_top, cap_height, draw_text, text_size
from .tokens import FontStyle

# Medidas en unidades de la barra (se escalan con ella, ``DropUI.bs``): una fila mide lo
# que la píldora (38) y el radio es su mitad, así que un aviso de una fila es una cápsula
# hermana de la píldora; el ancho queda entre la píldora (108) y la fila completa (~284).
PEEK_W = 232.0  # ancho lógico de la tarjeta
ROW_H = 38.0  # = PILL_H
PAD_X = 10.0
PAD_Y = 0.0
MARK_COL = 24.0  # columna del indicador de estado
RADIUS = 19.0  # = PILL_H / 2
ROW_INSET = 3.0  # el resaltado de la fila bajo el puntero, dentro de la cápsula
LINE_GAP = 13.0  # de la línea base del título a la de la línea de estado
MARK_ICON = 15.0
MARGIN = 14.0  # aire alrededor de la tarjeta para la sombra
GAP = 10.0  # separación entre la forma y la tarjeta
MAX_ROWS = 4
DISMISS_W = 26.0  # zona de la × al final de la fila (px lógicos)
MORE_H = 22.0
HOVER_DELAY = 0.28  # reposo sobre la forma antes de abrir
LEAVE_GRACE = 0.35  # margen para alcanzar la tarjeta al salir de la forma
OPEN_SECS = 0.2
CLOSE_SECS = 0.13
RISE = 6.0  # cuánto se desplaza al aparecer (px lógicos)
TOAST_SECS = {DONE: 3.2, NEEDS_YOU: 6.0, FAILED: 6.0, STALE: 4.5}
REVEAL_SECS = 8.0  # «¿qué tareas tengo pendientes?»: la lista completa un rato

FS_TASK = FontStyle("FS_TASK", "Geist", 12.5, 500)
FS_META = FontStyle("FS_META", "Geist", 10.5, 400)
AGENT_NAMES = {"opencode": "OpenCode", "codex": "Codex", "claude": "Claude", "enjambre": "Enjambre"}
_MARK = {
    NEEDS_YOU: ("alert", "amber"),
    FAILED: ("close", "red"),
    WAITING: ("clock", "muted"),
    STALE: ("minus", "muted"),
    DONE: ("check", "mint"),
}


# ------------------------------------------------------------------- textos
def age(seconds: float) -> str:
    minutes = int(max(0.0, seconds) // 60)
    if minutes < 1:
        return "ahora"
    if minutes < 60:
        return f"{minutes} min"
    return f"{minutes // 60} h"


def meta_line(item: ActivityItem, now: float) -> str:
    """«Estado · Agente · tiempo»: lo vivo cuenta desde que empezó; lo demás, «hace»."""
    parts = [item.label]
    if item.agent:
        agent = item.agent.strip()
        parts.append(AGENT_NAMES.get(agent.lower(), agent[:1].upper() + agent[1:]))
    elapsed = age(now - item.since)
    parts.append(elapsed if item.live or elapsed == "ahora" else f"hace {elapsed}")
    return " · ".join(parts)


def fit(text: str, style: FontStyle, max_w: float, scale: float) -> str:
    """Recorta con «…» para que quepa en ``max_w`` px lógicos."""
    if text_size(text, style, scale)[0] <= max_w:
        return text
    lo, hi = 0, len(text)
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if text_size(text[:mid].rstrip() + "…", style, scale)[0] <= max_w:
            lo = mid
        else:
            hi = mid - 1
    return text[:lo].rstrip() + "…"


def card_height(rows: int, more: bool) -> float:
    return PAD_Y * 2 + rows * ROW_H + (MORE_H if more else 0.0)


# ------------------------------------------------------------------- dibujo
@lru_cache(maxsize=12)
def _body(w: int, h: int, scale: float, theme: str) -> np.ndarray:
    """Sombra + cuerpo redondeado: float32 RGBA premultiplicado, solo lectura."""
    pal = card_palette(theme)
    ss = 2
    m, r = MARGIN * scale * ss, RADIUS * scale * ss
    box = (m, m, w * ss - m - 1, h * ss - m - 1)
    shadow = Image.new("RGBA", (w * ss, h * ss), (0, 0, 0, 0))
    lift = 3 * scale * ss
    ImageDraw.Draw(shadow).rounded_rectangle(
        (box[0], box[1] + lift, box[2], box[3] + lift),
        radius=r,
        fill=(0, 0, 0, 110 if theme == "dark" else 46),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(7 * scale * ss))
    card = Image.new("RGBA", (w * ss, h * ss), (0, 0, 0, 0))
    line = pal["LINE"]
    ImageDraw.Draw(card).rounded_rectangle(
        box,
        radius=r,
        fill=(*pal["BODY"][:3], 255),
        outline=(*line[:3], int(line[3]) if len(line) > 3 else 40),
        width=max(1, round(scale * ss)),
    )
    out = Image.alpha_composite(shadow, card).convert("RGBa").reduce(ss)
    arr = np.asarray(out).astype(np.float32) / 255.0
    arr.setflags(write=False)
    return arr


def _rgb(color) -> tuple[int, int, int]:
    return tuple(int(c) for c in color[:3])  # type: ignore[return-value]


def baselines(scale: float) -> tuple[float, float]:
    """Líneas base (título, estado) dentro de una fila: el bloque queda centrado.

    El aire sobre las mayúsculas del título es el mismo que bajo la línea de estado.
    """
    cap = cap_height(FS_TASK, scale)
    first = (ROW_H - LINE_GAP + cap) / 2
    return first, first + LINE_GAP


def paint_rows(
    items: tuple[ActivityItem, ...],
    more: int,
    w: int,
    h: int,
    scale: float,
    theme: str,
    hover: int | None,
    now: float,
) -> np.ndarray:
    """Capa estática: cuerpo, filas, separadores, el punto de «sin ver» y la ×."""
    pal = card_palette(theme)
    buf = np.array(_body(w, h, scale, theme), copy=True)
    x0 = MARGIN + PAD_X
    first, second = baselines(scale)
    for i, item in enumerate(items):
        y = MARGIN + PAD_Y + i * ROW_H
        # La × solo ocupa sitio en la fila bajo el puntero.
        text_w = PEEK_W - PAD_X * 2 - MARK_COL - (DISMISS_W if hover == i else 10)
        if hover == i:
            inset = ROW_INSET
            _fill_round(
                buf,
                MARGIN + inset,
                y + inset,
                PEEK_W - 2 * inset,
                ROW_H - 2 * inset,
                RADIUS - inset,
                pal["GROUP_HOVER"],
                scale,
            )
        title = fit(item.title, FS_TASK, text_w, scale)
        top = baseline_top(title, FS_TASK, y + first, scale)
        draw_text(buf, title, FS_TASK, _rgb(pal["TEXT"]), x0 + MARK_COL, top, scale)
        meta = fit(meta_line(item, now), FS_META, text_w, scale)
        top = baseline_top(meta, FS_META, y + second, scale)
        draw_text(buf, meta, FS_META, _rgb(pal["TEXT_2"]), x0 + MARK_COL, top, scale)
        if hover == i and dismissible(item):
            cx = MARGIN + PEEK_W - PAD_X - 6
            draw_icon(buf, "close", cx, y + ROW_H / 2, 13.0, _rgb(pal["TEXT_2"]), scale)
        elif item.unseen:
            cx, cy = MARGIN + PEEK_W - PAD_X - 2.5, y + ROW_H / 2
            _fill_round(buf, cx - 2.5, cy - 2.5, 5, 5, 2.5, mark_ink(theme)["mint"], scale)
        if i < len(items) - 1 and hover not in (i, i + 1):
            line = pal["LINE"]
            rule_w = PEEK_W - PAD_X * 2 - MARK_COL
            _fill_round(buf, x0 + MARK_COL, y + ROW_H - 0.5, rule_w, 1 / scale, 0, line, scale)
    if more:
        label = f"y {more} más" if more > 1 else "y 1 más"
        base = MARGIN + PAD_Y + len(items) * ROW_H + (MORE_H + cap_height(FS_META, scale)) / 2
        top = baseline_top(label, FS_META, base - 3, scale)
        draw_text(buf, label, FS_META, _rgb(pal["TEXT_3"]), x0 + MARK_COL, top, scale)
    return buf


def _fill_round(buf, x, y, w, h, r, color, scale) -> None:
    """Rectángulo redondeado relleno, con borde analítico, sobre ``buf``."""
    x0, y0 = int(math.floor(x * scale)), int(math.floor(y * scale))
    x1, y1 = int(math.ceil((x + w) * scale)), int(math.ceil((y + h) * scale))
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(buf.shape[1], x1), min(buf.shape[0], y1)
    if x1 <= x0 or y1 <= y0:
        return
    xs = (np.arange(x0, x1, dtype=np.float32) + 0.5) / scale
    ys = (np.arange(y0, y1, dtype=np.float32) + 0.5) / scale
    gx, gy = np.meshgrid(xs, ys)
    hx, hy = w / 2 - r, h / 2 - r
    qx = np.abs(gx - (x + w / 2)) - hx
    qy = np.abs(gy - (y + h / 2)) - hy
    d = np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r
    alpha = (color[3] / 255.0) if len(color) > 3 else 1.0
    a = (np.clip(0.5 - d * scale, 0, 1) * alpha)[..., None]
    rgb = np.asarray(color[:3], np.float32) / 255.0
    region = buf[y0:y1, x0:x1]
    region[..., :3] = rgb * a + region[..., :3] * (1 - a)
    region[..., 3:] = a + region[..., 3:] * (1 - a)


_SPIN: dict[tuple, np.ndarray] = {}
SPIN_STEPS = 24


def spinner(scale: float, color, angle_step: int, size: float = 16.0) -> np.ndarray:
    """Arco de 270° que gira (tarea en curso), cacheado por paso de ángulo."""
    key = (round(scale, 3), tuple(color), angle_step % SPIN_STEPS, size)
    sprite = _SPIN.get(key)
    if sprite is not None:
        return sprite
    side = max(4, round(size * scale))
    axis = (np.arange(side, dtype=np.float32) + 0.5) / scale - side / scale / 2
    gx, gy = (g.ravel() for g in np.meshgrid(axis, axis))
    start = 360.0 * (angle_step % SPIN_STEPS) / SPIN_STEPS
    radius = size * 0.32
    pts = [
        (
            radius * math.cos(math.radians(start + 270 * k / 18)),
            radius * math.sin(math.radians(start + 270 * k / 18)),
        )
        for k in range(19)
    ]
    segs = np.asarray([(*a, *b) for a, b in zip(pts, pts[1:], strict=False)], np.float32)
    d = segment_distance(gx, gy, segs) * scale
    cover = np.clip(0.9 * scale - d + 0.5, 0, 1).reshape(side, side)
    sprite = colorize(cover, color)
    if len(_SPIN) > 96:
        _SPIN.clear()
    _SPIN[key] = sprite
    return sprite


def paint_marks(
    buf: np.ndarray, items, scale: float, theme: str, now: float, reduced: bool
) -> None:
    """Indicadores de estado (los únicos que cambian de un cuadro a otro)."""
    ink = mark_ink(theme)
    cx = MARGIN + PAD_X + 8
    for i, item in enumerate(items):
        cy = MARGIN + PAD_Y + i * ROW_H + ROW_H / 2
        if item.state == WORKING:
            step = 0 if reduced else int(now * SPIN_STEPS) % SPIN_STEPS
            sprite = spinner(scale, ink["mint"], step, MARK_ICON - 1)
            _blit(buf, sprite, cx * scale, cy * scale)
            continue
        name, tone = _MARK.get(item.state, ("minus", "muted"))
        draw_icon(buf, name, cx, cy, MARK_ICON, ink[tone], scale)


def _blit(buf: np.ndarray, sprite: np.ndarray, cx: float, cy: float) -> None:
    sh, sw = sprite.shape[:2]
    x0, y0 = int(round(cx - sw / 2)), int(round(cy - sh / 2))
    if x0 < 0 or y0 < 0 or x0 + sw > buf.shape[1] or y0 + sh > buf.shape[0]:
        return
    region = buf[y0 : y0 + sh, x0 : x0 + sw]
    region[:] = sprite + region * (1.0 - sprite[..., 3:4])


def ease_out(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return 1.0 - (1.0 - t) ** 3
