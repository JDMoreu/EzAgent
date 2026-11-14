"""Cuadros dorados y números de la vista rápida de actividad (F1-10c).

Las tarjetas salen de ``peek_paint.paint_rows`` + ``paint_marks`` + ``peek.compose``
(el mismo camino que usa la tarjeta real, sin ventana ni Win32). Cada PNG va a
``parity/golden/hud/peek/`` y cada caso describe el estado exacto que lo produjo.

También se registran las medidas y textos puros: ``age``, ``meta_line``, ``fit``,
``card_height``, ``baselines``, ``place_card`` y ``row_at``/``dismiss_at``.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

import numpy as np
from _common import GOLDEN, FixtureFile, main_guard
from PIL import Image

from ezagent.voice.activity import (
    DONE,
    FAILED,
    NEEDS_YOU,
    STALE,
    WAITING,
    WORKING,
    ActivityItem,
)
from ezagent.voice.hud import peek, peek_paint

DOMAIN = "hud_peek"
OUT = GOLDEN / "hud" / "peek"
NOW = 1735689600.0


def plain(value: Any) -> Any:
    if isinstance(value, np.ndarray):
        return [plain(item) for item in value.tolist()]
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, dict):
        return {str(key): plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(item) for item in value]
    return value


def add(out: FixtureFile, name: str, fn: str, data: Any, value: Any) -> None:
    out.add(f"hud_peek/{name}", fn, plain(data), plain(value))


def item_dict(item: ActivityItem) -> dict[str, Any]:
    return {
        "key": item.key,
        "title": item.title,
        "agent": item.agent,
        "state": item.state,
        "since": item.since,
        "unseen": item.unseen,
        "live": item.live,
    }


def make_items() -> tuple[ActivityItem, ...]:
    return (
        ActivityItem("a", "Revisar el PR de la migración", "opencode", WORKING,
                     1735689500.0, False, True),
        ActivityItem("b", "¿Qué hago con el conflicto?", "claude", NEEDS_YOU,
                     1735689400.0, True, False),
        ActivityItem("c", "Falló la suite de pruebas", "codex", FAILED,
                     1735689000.0, False, False),
        ActivityItem("d", "Esperando al enjambre", "enjambre", WAITING,
                     1735688800.0, False, True),
        ActivityItem("e", "Tarea terminada con un nombre largo de verdad", "otro",
                     DONE, 1735686000.0, True, False),
        ActivityItem("f", "Sin actualización reciente", "", STALE,
                     1735680000.0, False, False),
    )


def window_size(rows: int, more: int, scale: float) -> tuple[int, int]:
    return (
        round((peek_paint.PEEK_W + 2 * peek_paint.MARGIN) * scale),
        round((peek_paint.card_height(max(1, rows), bool(more)) + 2 * peek_paint.MARGIN)
              * scale),
    )


def straight_bgra(pixels: np.ndarray) -> Image.Image:
    """BGRA premultiplicado uint8 (contrato del HUD) -> RGBA con alfa directo."""
    a = pixels[..., 3].astype(np.float32) / 255.0
    safe = np.maximum(a, 1e-6)[..., None]
    rgb = np.clip(np.where(a[..., None] > 0.0,
                           pixels[..., [2, 1, 0]].astype(np.float32) / 255.0 / safe, 0.0),
                  0.0, 1.0)
    data = np.empty((*a.shape, 4), np.uint8)
    data[..., :3] = np.round(rgb * 255.0).astype(np.uint8)
    data[..., 3] = np.round(np.clip(a, 0.0, 1.0) * 255.0).astype(np.uint8)
    return Image.fromarray(data, "RGBA")


def card_frame(
    out: FixtureFile,
    name: str,
    items: tuple[ActivityItem, ...],
    more: int,
    scale: float,
    theme: str,
    hover: int | None,
    *,
    now: float = NOW,
    marks_now: float | None = None,
    reduced: bool = False,
    progress: float = 1.0,
    above: bool = True,
) -> None:
    width, height = window_size(len(items), more, scale)
    buf = peek_paint.paint_rows(items, more, width, height, scale, theme, hover, now)
    peek_paint.paint_marks(buf, items, scale, theme,
                           marks_now if marks_now is not None else now, reduced)
    frame = peek.compose(buf, progress, above, scale, reduced)
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.png"
    straight_bgra(frame).save(path, format="PNG")
    add(
        out,
        f"frames/{name}",
        "peek.compose",
        {
            "items": [item_dict(item) for item in items],
            "more": more,
            "width": width,
            "height": height,
            "scale": scale,
            "theme": theme,
            "hover": hover,
            "now": now,
            "marks_now": marks_now if marks_now is not None else now,
            "reduced": reduced,
            "progress": progress,
            "above": above,
        },
        {"png": f"hud/peek/{path.name}"},
    )


def frame_cases(out: FixtureFile) -> None:
    items = make_items()
    head = items[:4]
    for scale in (1.0, 1.25, 1.5, 2.0):
        for theme in ("light", "dark"):
            card_frame(out, f"row-1-{theme}-{int(scale * 100)}", items[:1], 0, scale,
                       theme, None)
    for scale in (1.0, 2.0):
        for theme in ("light", "dark"):
            card_frame(out, f"rows-4-more-2-{theme}-{int(scale * 100)}", head, 2, scale,
                       theme, None)
    for hover in range(4):
        card_frame(out, f"hover-row-{hover}-light-150", head, 2, 1.5, "light", hover)
    card_frame(out, "unseen-light-150", (items[1], items[4]), 0, 1.5, "light", None)
    card_frame(out, "unseen-dark-150", (items[1], items[4]), 0, 1.5, "dark", None)
    for step in (0, 6, 12, 18):
        card_frame(out, f"working-spin-{step:02d}-dark-150", (items[0],), 0, 1.5, "dark",
                   None, marks_now=NOW + step / peek_paint.SPIN_STEPS / 1.0)
    for progress in (0.25, 0.5, 0.75):
        for above in (True, False):
            side = "above" if above else "below"
            card_frame(out, f"compose-{int(progress * 100):02d}-{side}", head[:2], 0, 1.0,
                       "dark", None, progress=progress, above=above)
    card_frame(out, "compose-reduced-light", head[:2], 0, 1.0, "light", None,
               progress=0.5, reduced=True)
    card_frame(out, "toast-one-row-dark", (items[2],), 0, 1.5, "dark", None)
    card_frame(out, "rows-4-more-1-light-125", items[:4], 1, 1.25, "light", None)
    card_frame(out, "rows-6-more-0-dark-100", items, 0, 1.0, "dark", None)


def text_cases(out: FixtureFile) -> None:
    for index, seconds in enumerate(
        (-10.0, 0.0, 1.0, 59.0, 59.999, 60.0, 61.0, 119.9, 120.0, 3599.0, 3600.0,
         3660.0, 7200.0, 86400.0),
        start=1,
    ):
        add(out, f"text/age/{index:02d}", "peek.age", {"seconds": seconds},
            {"text": peek_paint.age(seconds)})
    items = [
        (ActivityItem("k", "Tarea", "opencode", WORKING, NOW - 45, False, True), NOW),
        (ActivityItem("k", "Tarea", "OpenCode", DONE, NOW - 60, False, False), NOW),
        (ActivityItem("k", "Tarea", "  codex  ", DONE, NOW - 3600, False, False), NOW),
        (ActivityItem("k", "Tarea", "CLAUDE", NEEDS_YOU, NOW - 5, False, False), NOW),
        (ActivityItem("k", "Tarea", "enjambre", WAITING, NOW, False, True), NOW),
        (ActivityItem("k", "Tarea", "desconocido", FAILED, NOW - 61, False, False), NOW),
        (ActivityItem("k", "Tarea", "", STALE, NOW - 90000, False, False), NOW),
        (ActivityItem("k", "Tarea", "   ", DONE, NOW - 59, False, False), NOW),
        (ActivityItem("k", "Tarea", "ñandú", DONE, NOW + 30, False, False), NOW),
        (ActivityItem("k", "Tarea", "Opencode", WORKING, NOW - 7200), NOW),
    ]
    for index, (item, now) in enumerate(items, start=1):
        add(out, f"text/meta/{index:02d}", "peek.meta_line",
            {"item": item_dict(item), "now": now}, {"text": peek_paint.meta_line(item, now)})
    fits = [
        ("Revisar el PR de la migración", peek_paint.FS_TASK, 220.0, 1.0),
        ("Revisar el PR de la migración", peek_paint.FS_TASK, 100.0, 1.0),
        ("Revisar el PR de la migración", peek_paint.FS_TASK, 0.0, 1.0),
        ("", peek_paint.FS_TASK, 100.0, 1.0),
        ("ñandú cantando", peek_paint.FS_TASK, 55.0, 1.0),
        ("abcdefghij", peek_paint.FS_META, 40.0, 1.5),
        ("  espacios  ", peek_paint.FS_TASK, 45.0, 1.0),
        ("Emoji 🚀 de prueba", peek_paint.FS_TASK, 70.0, 1.0),
    ]
    for index, (text, style, width, scale) in enumerate(fits, start=1):
        add(
            out,
            f"text/fit/{index:02d}",
            "peek.fit",
            {"text": text, "style": style.token, "max_w": width, "scale": scale},
            {"text": peek_paint.fit(text, style, width, scale)},
        )
    for index, (rows, more) in enumerate(
        [(1, False), (1, True), (2, False), (4, True), (6, False), (0, True)],
        start=1,
    ):
        add(out, f"text/card-height/{index:02d}", "peek.card_height",
            {"rows": rows, "more": more}, {"height": peek_paint.card_height(rows, more)})
    for index, scale in enumerate((0.5, 1.0, 1.25, 1.5, 2.0), start=1):
        first, second = peek_paint.baselines(scale)
        add(out, f"text/baselines/{index:02d}", "peek.baselines",
            {"scale": scale}, {"first": first, "second": second})
    add(
        out,
        "text/constants",
        "peek.constants",
        {},
        {
            "PEEK_W": peek_paint.PEEK_W,
            "ROW_H": peek_paint.ROW_H,
            "PAD_X": peek_paint.PAD_X,
            "PAD_Y": peek_paint.PAD_Y,
            "MARK_COL": peek_paint.MARK_COL,
            "RADIUS": peek_paint.RADIUS,
            "ROW_INSET": peek_paint.ROW_INSET,
            "LINE_GAP": peek_paint.LINE_GAP,
            "MARK_ICON": peek_paint.MARK_ICON,
            "MARGIN": peek_paint.MARGIN,
            "GAP": peek_paint.GAP,
            "MAX_ROWS": peek_paint.MAX_ROWS,
            "DISMISS_W": peek_paint.DISMISS_W,
            "MORE_H": peek_paint.MORE_H,
            "HOVER_DELAY": peek_paint.HOVER_DELAY,
            "LEAVE_GRACE": peek_paint.LEAVE_GRACE,
            "OPEN_SECS": peek_paint.OPEN_SECS,
            "CLOSE_SECS": peek_paint.CLOSE_SECS,
            "RISE": peek_paint.RISE,
            "TOAST_SECS": peek_paint.TOAST_SECS,
            "REVEAL_SECS": peek_paint.REVEAL_SECS,
            "SPIN_STEPS": peek_paint.SPIN_STEPS,
            "AGENT_NAMES": peek_paint.AGENT_NAMES,
            "FS_TASK": {
                "token": peek_paint.FS_TASK.token,
                "family": peek_paint.FS_TASK.family,
                "size": peek_paint.FS_TASK.size,
                "weight": peek_paint.FS_TASK.weight,
            },
            "FS_META": {
                "token": peek_paint.FS_META.token,
                "family": peek_paint.FS_META.family,
                "size": peek_paint.FS_META.size,
                "weight": peek_paint.FS_META.weight,
            },
        },
    )


def placement_cases(out: FixtureFile) -> None:
    hubs = [
        (906.0, 981.0, 1014.0, 1019.0),
        (900.0, 950.0, 1116.0, 1040.0),
        (0.0, 0.0, 108.0, 38.0),
        (1812.0, 1042.0, 1920.0, 1080.0),
        (0.0, 1042.0, 108.0, 1080.0),
        (960.0, 20.0, 1068.0, 58.0),
        (20.0, 540.0, 128.0, 578.0),
        (1800.0, 540.0, 1908.0, 578.0),
        (-100.0, -100.0, 8.0, -62.0),
        (2000.0, 1200.0, 2108.0, 1238.0),
    ]
    for index, hub in enumerate(hubs, start=1):
        for rows, more, scale in ((1, 0, 1.0), (4, 2, 1.5)):
            width, height = window_size(rows, more, scale)
            add(
                out,
                f"placement/place/{index:02d}-{rows}-{int(scale * 100)}",
                "peek.place_card",
                {"hub": hub, "size": [width, height], "work": [0, 0, 1920, 1080],
                 "scale": scale},
                {"place": peek.place_card(hub, (width, height), (0, 0, 1920, 1080), scale)},
            )
        width, height = window_size(2, 0, 1.0)
        add(
            out,
            f"placement/place-small-work/{index:02d}",
            "peek.place_card",
            {"hub": hub, "size": [width, height], "work": [100, 50, 500, 400],
             "scale": 1.0},
            {"place": peek.place_card(hub, (width, height), (100, 50, 500, 400), 1.0)},
        )
    rows = make_items()[:4]
    for scale in (1.0, 1.5, 2.0):
        fake = SimpleNamespace(s=scale, bs=scale)
        controller = peek.Peek(fake)
        controller._rows = (rows, 2)
        for index, (x, y) in enumerate(
            [(0, 0), (10, 10), (100, 60), (100, 14 * scale + 1), (259 * scale, 100),
             (10 * scale, 38 * scale), (10 * scale, 39 * scale), (-1, 10),
             (1000, 1000), (14 * scale, 14 * scale + 20)],
            start=1,
        ):
            add(
                out,
                f"placement/row-at/{int(scale * 100)}-{index:02d}",
                "peek.row_at",
                {"scale": scale, "rows": [item_dict(item) for item in rows], "more": 2,
                 "x": x, "y": y},
                {"row": controller.row_at(x, y)},
            )
        for index, (x, y) in enumerate(
            [(232 * scale, 14 * scale + 19 * scale),
             (250 * scale, 14 * scale + 19 * scale),
             (232 * scale, 14 * scale + 19 * scale - 5),
             (10 * scale, 14 * scale + 19 * scale),
             (250 * scale, 14 * scale + 38 * scale * 2 + 19 * scale),
             (250 * scale, 14 * scale + 38 * scale * 4 + 19 * scale)],
            start=1,
        ):
            add(
                out,
                f"placement/dismiss-at/{int(scale * 100)}-{index:02d}",
                "peek.dismiss_at",
                {"scale": scale, "rows": [item_dict(item) for item in rows], "more": 2,
                 "x": x, "y": y},
                {"key": controller.dismiss_at(x, y)},
            )


def generate(out: FixtureFile) -> None:
    text_cases(out)
    placement_cases(out)
    frame_cases(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
