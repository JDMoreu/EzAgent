"""Fixture del ``DropModel`` y la geometría de la barra (F1-10c).

Secuencias de ``update`` con posiciones animadas (centro, escala, opacidad),
``item_at``, ``bar_rect``, ``hub_rect`` y ``near`` con flotantes exactos, más la
geometría pura de ``drop`` (docks, huecos, tamaños y elección de lado).

Entradas: ``steps`` son llamadas reales a ``DropModel.update``; los ``probes`` de
cada caso se evalúan tras cada paso. Nada de Win32 ni de dibujo.
"""

from __future__ import annotations

from functools import partial
from typing import Any

import numpy as np
from _common import FixtureFile, main_guard

from ezagent.voice.hud import drop

DOMAIN = "hud_drop_model"
BUTTONS = list(drop.BUTTONS)
DT = 1.0 / 60.0


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
    out.add(f"hud_drop_model/{name}", fn, plain(data), plain(value))


def add_error(out: FixtureFile, name: str, fn: str, data: Any, error: Exception) -> None:
    out.add_error(f"hud_drop_model/{name}", fn, plain(data), str(error))


def step(
    now: float,
    dt: float = DT,
    *,
    pose: str = "idle",
    rms: float = 0.0,
    theme: str = "light",
    hover: bool = False,
    asking: bool = False,
    menu_open: bool = False,
    reduced: bool = False,
    buttons: str = "hover",
    muted: bool = False,
    hover_item: str | None = None,
    pressed: str | None = None,
    in_call: bool = True,
) -> dict[str, Any]:
    return {
        "now": now,
        "dt": dt,
        "pose": pose,
        "rms": rms,
        "theme": theme,
        "hover": hover,
        "asking": asking,
        "menu_open": menu_open,
        "reduced": reduced,
        "buttons": buttons,
        "muted": muted,
        "hover_item": hover_item,
        "pressed": pressed,
        "in_call": in_call,
    }


def run_step(model: drop.DropModel, values: dict[str, Any]) -> None:
    model.update(
        values["now"],
        values["dt"],
        values["pose"],
        values["rms"],
        values["theme"],
        values["hover"],
        values["asking"],
        values["menu_open"],
        values["reduced"],
        values["buttons"],
        muted=values["muted"],
        hover_item=values["hover_item"],
        pressed=values["pressed"],
        in_call=values["in_call"],
    )


def default_probes(layout: str, dock: str | None) -> list[tuple[float, float]]:
    """Puntos alrededor del hub: centro, cada hueco, bordes y fuera."""
    slots = drop.slots(layout, 1.0, dock)
    probes: list[tuple[float, float]] = []
    for name in ("pill", *drop.BUTTONS):
        cx, cy = slots[name]
        probes += [(cx, cy), (cx + 17.0, cy), (cx - 17.0, cy)]
    hub = drop.DropModel(layout, dock).hub_rect()
    probes += [
        (hub[0], hub[1]),
        (hub[2], hub[3]),
        (hub[0] - 1.0, hub[1] - 1.0),
        (hub[2] + 1.0, hub[3] + 1.0),
        (hub[0] - drop.HOVER_MARGIN + 0.5, (hub[1] + hub[3]) / 2),
        ((hub[0] + hub[2]) / 2, hub[3] + drop.HOVER_MARGIN + 1.0),
        (-100.0, -100.0),
        (1000.0, 1000.0),
    ]
    return probes


def model_case(
    out: FixtureFile,
    name: str,
    layout: str,
    dock: str | None,
    steps: list[dict[str, Any]],
    probes: list[tuple[float, float]] | None = None,
    form_hidden: bool = False,
) -> None:
    model = drop.DropModel(layout, dock)
    model.form_hidden = form_hidden
    used = list(probes) if probes is not None else default_probes(layout, dock)
    snapshots = []
    for values in steps:
        run_step(model, values)
        snapshots.append(
            {
                "now": values["now"],
                "positions": model.positions(),
                "item_at": [model.item_at(x, y) for x, y in used],
                "bar_rect": model.bar_rect(),
                "hub_rect": model.hub_rect(),
                "near": [model.near(x, y) for x, y in used],
                "animating": model.animating(values["now"]),
                "flow": model.flow,
                "bright": model.bright,
                "lvl": model.lvl,
                "peak": model.peak,
            }
        )
    add(
        out,
        f"model/{name}",
        "DropModel.update",
        {
            "layout": layout,
            "dock": dock,
            "form_hidden": form_hidden,
            "probes": used,
            "steps": steps,
        },
        {"snapshots": snapshots},
    )


def warm(count: int = 40, **kwargs: Any) -> list[dict[str, Any]]:
    return [step(index * DT, DT, **kwargs) for index in range(count)]


def sequence_cases(out: FixtureFile) -> None:
    model_case(out, "pill-idle", "pill", None, warm(36))
    model_case(out, "pill-hover", "pill", None,
               [step(i * DT, DT, hover=True) for i in range(36)])
    model_case(
        out, "pill-hover-hold", "pill", None,
        [step(i * DT, DT, hover=i < 12) for i in range(46)],
    )
    model_case(
        out, "pill-asking", "pill", None,
        [step(i * DT, DT, hover=True) for i in range(12)]
        + [step((12 + i) * DT, DT, asking=True) for i in range(30)],
    )
    model_case(
        out, "pill-hang", "pill", None,
        [step(i * DT, DT, hover=True) for i in range(12)]
        + [step((12 + i) * DT, DT, hover=True, in_call=False) for i in range(30)],
    )
    model_case(
        out, "pill-menu", "pill", None,
        [step(i * DT, DT, menu_open=i >= 6) for i in range(30)],
    )
    model_case(
        out, "pill-muted", "pill", None,
        [step(i * DT, DT, hover=True, muted=i >= 6) for i in range(30)],
    )
    model_case(
        out, "pill-buttons-always", "pill", None,
        [step(i * DT, DT, buttons="always") for i in range(30)],
    )
    model_case(
        out, "pill-reduced", "pill", None,
        [step(i * DT, DT, hover=i < 12, reduced=True) for i in range(36)],
    )
    model_case(
        out, "pill-listen", "pill", None,
        [step(i * DT, DT, pose="listen", rms=0.42, theme="dark", hover=True)
         for i in range(24)],
    )
    model_case(
        out, "pill-speak", "pill", None,
        [step(i * DT, DT, pose="speak", rms=0.55, theme="light", hover=True)
         for i in range(24)],
    )
    poses = ["idle"] * 8 + ["listen"] * 8 + ["speak"] * 8 + ["done"] * 6
    model_case(
        out, "pill-pose-change", "pill", None,
        [step(i * DT, DT, pose=pose, rms=0.3 if pose in ("listen", "speak") else 0.0,
              theme=theme, hover=True)
         for i, (pose, theme) in enumerate(
             zip(poses, ["light"] * 8 + ["dark"] * 8 + ["dark"] * 8 + ["light"] * 6,
                 strict=False))],
    )
    model_case(
        out, "pill-hover-item", "pill", None,
        [step(i * DT, DT, hover=True, hover_item="gear" if i < 12 else None,
              pressed="gear" if 8 <= i < 16 else None) for i in range(28)],
    )
    model_case(
        out, "pill-sleep", "pill", None,
        [step(i * DT, DT, pose="sleep", in_call=False, hover=True) for i in range(24)],
    )
    model_case(
        out, "pill-form-hidden", "pill", None,
        [step(i * DT, DT, hover=True) for i in range(20)], form_hidden=True,
    )
    for layout, dock in (("circle", None), ("circle", "top"), ("circle", "left"),
                         ("circle", "right")):
        model_case(
            out, f"{layout}-{dock_label(dock)}-hover", layout, dock,
            [step(i * DT, DT, hover=True) for i in range(30)],
        )
    for dock in ("left", "right"):
        model_case(
            out, f"pill-{dock}-hover", "pill", dock,
            [step(i * DT, DT, hover=True) for i in range(30)],
        )
    # Rejilla alrededor del hub: item_at y near en cada paso.
    probes = default_probes("pill", None) + [
        (x, y)
        for x in (0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0)
        for y in (0.0, 20.0, 31.0, 45.0, 62.0)
    ]
    model_case(
        out, "pill-grid", "pill", None,
        [step(0.0, 0.0), step(0.5, 0.5, hover=True), step(1.0, 0.5, hover=True)],
        probes=probes,
    )


def dock_label(dock: str | None) -> str:
    if dock is None:
        return "default"
    return "empty" if dock == "" else dock


def geometry_cases(out: FixtureFile) -> None:
    add(
        out,
        "geometry/constants",
        "drop.constants",
        {},
        {
            "BTN": drop.BTN,
            "GAP": drop.GAP,
            "PAD": drop.PAD,
            "PILL_W": drop.PILL_W,
            "PILL_H": drop.PILL_H,
            "CIRCLE_D": drop.CIRCLE_D,
            "ORB_GAP": drop.ORB_GAP,
            "HOVER_MARGIN": drop.HOVER_MARGIN,
            "DOUBLE_CLICK": drop.DOUBLE_CLICK,
            "HOVER_HOLD": drop.HOVER_HOLD,
            "BUD_SS": drop.BUD_SS,
            "GRID_CELL": drop.GRID_CELL,
            "SHADOW": drop.SHADOW,
            "FPS_BUSY": drop.FPS_BUSY,
            "FPS_CALM": drop.FPS_CALM,
            "HYSTERESIS": drop.HYSTERESIS,
            "BUTTONS": list(drop.BUTTONS),
            "NB": drop.NB,
            "LAYOUTS": list(drop.LAYOUTS),
            "MOTION_MODES": list(drop.MOTION_MODES),
            "DEFAULT_DOCK": drop.DEFAULT_DOCK,
            "DOCKS": drop.DOCKS,
            "POSE_OF": drop.POSE_OF,
            "BRIGHT": drop.BRIGHT,
        },
    )
    docks = [None, "sides", "bottom", "top", "right", "left", "nope", ""]
    for layout in (*drop.LAYOUTS, "wide"):
        for dock in docks:
            name = f"{layout}-{dock_label(dock)}"
            add(out, f"geometry/clean-dock/{name}", "drop.clean_dock",
                {"layout": layout, "dock": dock}, {"dock": drop.clean_dock(layout, dock)})
            add(out, f"geometry/layout-size/{name}", "drop.layout_size",
                {"layout": layout, "dock": dock}, {"size": drop.layout_size(layout, dock)})
            for label, fn, call in (
                ("emerge-order", "drop.emerge_order",
                 partial(drop.emerge_order, layout, dock)),
                ("row-center", "drop.row_center_y",
                 partial(drop.row_center_y, layout, dock)),
                ("form-center", "drop.form_center",
                 partial(drop.form_center, layout, dock)),
            ):
                try:
                    value = call()
                except KeyError as error:
                    add_error(out, f"geometry/{label}/{name}", fn,
                              {"layout": layout, "dock": dock}, error)
                else:
                    key = "order" if label == "emerge-order" else (
                        "y" if label == "row-center" else "center")
                    add(out, f"geometry/{label}/{name}", fn,
                        {"layout": layout, "dock": dock}, {key: value})
    for layout in drop.LAYOUTS:
        for dock in docks:
            for mic in (0.0, 0.25, 0.5, 0.75, 1.0):
                add(
                    out,
                    f"geometry/slots/{layout}-{dock_label(dock)}-{str(mic).replace('.', '_')}",
                    "drop.slots",
                    {"layout": layout, "mic": mic, "dock": dock},
                    {"slots": drop.slots(layout, mic, dock)},
                )
    anchors = [
        (960.0, 1000.0), (960.0, 40.0), (40.0, 540.0), (1880.0, 540.0),
        (40.0, 40.0), (1880.0, 1040.0), (100.0, 900.0), (1800.0, 900.0),
        (960.0, 540.0), (0.0, 0.0), (1920.0, 1080.0),
    ]
    works = [(0, 0, 1920, 1080), (0, 0, 800, 600), (-1920, 0, 0, 1080)]
    currents = [None, "sides", "bottom", "top", "right", "left"]
    for layout in drop.LAYOUTS:
        for index, (anchor, work, bs, current) in enumerate(
            [(a, w, 1.0, currents[i % len(currents)])
             for i, (a, w) in enumerate(zip(anchors, works * 4, strict=False))], start=1
        ):
            add(
                out,
                f"geometry/choose-dock/{layout}/{index:02d}",
                "drop.choose_dock",
                {"layout": layout, "current": current, "anchor": anchor, "work": work,
                 "bs": bs},
                {"dock": drop.choose_dock(layout, current, anchor, work, bs)},
            )
        for index, (anchor, bs, current) in enumerate(
            [((100.0, 100.0), 1.0, None), ((960.0, 1000.0), 1.25, "left"),
             ((1800.0, 1000.0), 2.0, "sides"), ((40.0, 540.0), 1.5, "right")],
            start=1,
        ):
            add(
                out,
                f"geometry/choose-dock-dpi/{layout}/{index:02d}",
                "drop.choose_dock",
                {"layout": layout, "current": current, "anchor": anchor,
                 "work": (0, 0, 1920, 1080), "bs": bs},
                {"dock": drop.choose_dock(layout, current, anchor, (0, 0, 1920, 1080), bs)},
            )


def generate(out: FixtureFile) -> None:
    geometry_cases(out)
    sequence_cases(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
