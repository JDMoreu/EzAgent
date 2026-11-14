"""MenuHit over a grid that covers the open menu (domain `hud_menu_hit`, F1-10b).

Each case replays the real Python menu to a concrete state and then calls
``Menu.hit`` (or ``hit_including``) over a grid row or a list of named edge
probes. The output carries the exact rectangle, value and disabled flag of every
hit, so the Rust port can compare kind/key/index/rect/label without drawing.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

from ezagent.voice.hud import menu

from hud_menu import (
    CUSTOM_DARK,
    DARK,
    LIGHT,
    Look,
    Rec,
    _geometry,
    _hit_out,
    _new,
    _run,
)
from hud_settings import WORLD, ready

DOMAIN = "hud_menu_hit"
GRID_STEP = 16
ROW_CHUNK = 16
KINDS: set[str] = set()
# DECISION: rejilla de 16 px (todo el ancho y alto visibles) más sondas a ±1 px
# en cada borde de control: la rejilla demuestra cobertura y las sondas fijan
# los límites exactos sin escribir decenas de miles de líneas.
# DECISION: cada caso guarda el mundo y las acciones resueltas, así se reproduce
# sin depender del orden de los casos ni de un estado compartido.


def _state_input(name: str, look: Any, options: dict[str, Any],
                 actions: list[dict[str, Any]], now: float) -> dict[str, Any]:
    return {
        "state": name,
        "theme": look.theme,
        "base": look.base,
        "dpi": look.dpi,
        "width": options.get("width"),
        "screen_h": options.get("screen_h", 1080.0),
        "section": options.get("section"),
        "values": options.get("values") or {},
        "world": ready(options.get("world") or WORLD),
        "actions": actions,
        "now": now,
    }


STATES: list[dict[str, Any]] = [
    {"name": "full-dark", "look": DARK, "options": {}, "now": 1.2, "steps": []},
    {"name": "full-light", "look": LIGHT, "options": {}, "now": 1.2, "steps": []},
    {
        "name": "full-light-125", "look": ("light", "light", None, 1.25), "now": 1.2,
        "options": {}, "steps": [],
    },
    {
        "name": "advanced-custom", "look": CUSTOM_DARK, "now": 1.2,
        "options": {"values": {"show_advanced": True}}, "steps": [],
    },
    {
        "name": "scrolled", "look": DARK, "now": 1.4, "options": {},
        "steps": [{"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120 * 4, "t": 1.2}],
    },
    {
        "name": "selector-theme", "look": DARK, "now": 1.6,
        "options": {"width": 576.0, "section": "appearance"},
        "steps": [
            {"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 1.2},
            {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 1.22},
        ],
    },
    {
        "name": "selector-llm", "look": DARK, "now": 1.8,
        "options": {"width": 576.0, "section": "models"},
        "steps": [
            {"op": "pointer", "kind": "down", "at": ["select", "llm_model_pick"], "t": 1.2},
            {"op": "pointer", "kind": "up", "at": ["select", "llm_model_pick"], "t": 1.22},
            {"op": "wheel", "at": ["option", 2], "delta": -120, "t": 1.5},
        ],
    },
    {
        "name": "section-appearance", "look": DARK, "now": 1.2,
        "options": {"width": 576.0, "section": "appearance"}, "steps": [],
    },
    {
        "name": "section-voice", "look": DARK, "now": 1.2,
        "options": {"width": 576.0, "section": "voice"}, "steps": [],
    },
    {
        "name": "section-models", "look": DARK, "now": 1.2,
        "options": {"width": 576.0, "section": "models",
                    "values": {"api_key": "sk-or-v1-1234567890abcdef"}},
        "steps": [],
    },
    {
        "name": "section-system", "look": DARK, "now": 1.2,
        "options": {"width": 576.0, "section": "system", "values": {"show_advanced": True}},
        "steps": [],
    },
    {
        "name": "closing", "look": DARK, "now": 1.35, "options": {},
        "steps": [
            {"op": "close", "t": 1.2}, {"op": "wait", "t": 1.35},
        ],
    },
    {
        "name": "no-devices", "look": DARK, "now": 1.2,
        "options": {"world": {"devices": []}}, "steps": [], "include_disabled": True,
    },
    {
        "name": "small", "look": DARK, "now": 1.4,
        "options": {"screen_h": 480.0},
        "steps": [{"op": "wheel", "x": 160.0, "y": 250.0, "delta": -120 * 20, "t": 1.2}],
    },
    {
        "name": "editing", "look": DARK, "now": 1.5,
        "options": {"width": 576.0, "section": "listening"},
        "steps": [
            {"op": "begin_edit", "key": "wake_phrase", "t": 1.2},
            {"op": "text", "value": " y escucha", "t": 1.3},
        ],
    },
    {
        "name": "dirty-save", "look": DARK, "now": 1.4, "options": {},
        "steps": [{"op": "set", "key": "speed", "value": 1.1, "t": 1.2}],
    },
    {
        "name": "all-disabled", "look": LIGHT, "now": 1.2,
        "options": {"world": {"devices": [], "voices": [], "providers": {},
                              "stt_models": {}, "llm_models": {}}},
        "steps": [], "include_disabled": True,
    },
]


def _grid(out: FixtureFile, name: str, widget: menu.Menu, data: dict[str, Any],
          now: float) -> None:
    fn = "Menu.hit_including" if data.get("include_disabled") else "Menu.hit"
    include = bool(data.get("include_disabled"))
    width = int(round(widget.width))
    height = int(round(menu_module_height(widget)))
    xs = list(range(0, width + 1, GRID_STEP))
    if xs[-1] != width:
        xs.append(width)
    for y in range(0, height + 1, GRID_STEP):
        row = {"y": y}
        if y == height:
            row["y"] = height
        hits = []
        for x in xs:
            item = _hit_out(widget.hit(x, y, now, include_disabled=include))
            if item is not None:
                KINDS.add(item["kind"])
            hits.append(item)
        out.add(f"hud_menu_hit/{name}/grid/y{y:04d}", fn,
                {**data, "y": y, "xs": xs, "include_disabled": include},
                {"y": y, "hits": hits})


def menu_module_height(widget: menu.Menu) -> float:
    """Alto lógico de la ventana del menú (barra + cuerpo actual)."""
    return 36.0 + float(widget._height.value)


def _rect_points(label: str, rect: Any) -> list[tuple[str, float, float]]:
    cx, cy = (rect.x0 + rect.x1) / 2.0, (rect.y0 + rect.y1) / 2.0
    return [
        (f"{label}/center", cx, cy),
        (f"{label}/left", rect.x0, cy), (f"{label}/left-1", rect.x0 - 1.0, cy),
        (f"{label}/right", rect.x1, cy), (f"{label}/right+1", rect.x1 + 1.0, cy),
        (f"{label}/top", cx, rect.y0), (f"{label}/top-1", cx, rect.y0 - 1.0),
        (f"{label}/bottom", cx, rect.y1), (f"{label}/bottom+1", cx, rect.y1 + 1.0),
    ]


def _control_probes(widget: menu.Menu) -> list[tuple[str, float, float]]:
    layout = widget._layout()
    scroll = widget.scroll_offset
    geometry = _geometry(widget)
    controls = geometry["controls"]
    points: list[tuple[str, float, float]] = []
    for name in ("hide", "save", "test"):
        points += _rect_points(name, _rect_from(controls[name]))
    for key, rect in controls["actions"].items():
        points += _rect_points(f"action:{key}", _rect_from(rect))
    for key, rect in controls["copy"].items():
        points += _rect_points(f"copy:{key}", _rect_from(rect))
    for value, label, rect in controls["palette"]:
        points += _rect_points(f"palette:{value}", _rect_from(rect))
        # El Python comprueba la fila entera (menu.py 1577), no la píldora: este
        # punto demuestra que la muestra responde también fuera de su rectángulo.
        row = _rect_from(geometry["rows"]["theme_preview"])
        points.append((f"palette:{value}:row-top", (rect[0] + rect[2]) / 2.0, row.y0))
    row_values = [(key, rect) for key, rect in sorted(geometry["rows"].items())
                  if key in layout]
    for index in range(0, len(row_values), ROW_CHUNK):
        points += _row_probes(row_values[index:index + ROW_CHUNK])
    selector = widget._selector_geometry(layout, scroll)
    if selector is not None:
        top, _visible, options, list_scroll = selector
        for index, _option in enumerate(options):
            y0 = top + index * menu.LIST_ITEM_H - list_scroll
            rect = (menu.GROUP_X0, y0, menu.GROUP_X1 + widget._dx,
                    y0 + menu.LIST_ITEM_H)
            points += _rect_points(f"option:{index}", _rect_from(rect))
    return points


def _row_probes(rows: list[tuple[str, list[float]]]) -> list[tuple[str, float, float]]:
    points: list[tuple[str, float, float]] = []
    for key, rect in rows:
        points += _rect_points(f"row:{key}", _rect_from(rect))
    return points


def _rect_from(values: list[float]) -> Any:
    class Box:
        pass

    box = Box()
    box.x0, box.y0, box.x1, box.y1 = values
    return box


def _probes(out: FixtureFile, name: str, widget: menu.Menu, data: dict[str, Any],
            now: float) -> None:
    include = bool(data.get("include_disabled"))
    fn = "Menu.hit_including" if include else "Menu.hit"
    points = _control_probes(widget)
    chunks = [points[index:index + 40] for index in range(0, len(points), 40)]
    for index, chunk in enumerate(chunks):
        hits = []
        for label, x, y in chunk:
            hit = widget.hit(x, y, now, include_disabled=include)
            item = _hit_out(hit)
            if item is not None:
                KINDS.add(item["kind"])
            hits.append({"point": label, "x": round(x, 3), "y": round(y, 3),
                         "hit": item})
        out.add(f"hud_menu_hit/{name}/probes/{index:02d}", fn,
                {**data, "include_disabled": include,
                 "points": [[label, round(x, 3), round(y, 3)] for label, x, y in chunk]},
                {"hits": hits})


def generate(out: FixtureFile) -> None:
    for state in STATES:
        name = state["name"]
        look = Look(*state["look"]) if len(state["look"]) == 4 else Look(*state["look"], 1.0)
        now = state["now"]
        options = dict(state.get("options") or {})
        rec = Rec()
        widget = _new(look, rec, **options)
        steps = [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2}, *state["steps"]]
        actions, _results = _run(widget, rec, steps)
        data = _state_input(name, look, options, actions, now)
        data["include_disabled"] = bool(state.get("include_disabled"))
        _grid(out, name, widget, data, now)
        _probes(out, name, widget, data, now)
        if name == "full-dark":  # la misma rejilla con los deshabilitados a la vista
            other = {**data, "state": "full-dark-including", "include_disabled": True}
            _grid(out, "full-dark-including", widget, other, now)
            _probes(out, "full-dark-including", widget, other, now)
    closed_rec = Rec()
    closed = _new(Look("dark", "dark"), closed_rec)
    hits = [_hit_out(closed.hit(x, y, 1.0)) for x, y in
            ((0.0, 0.0), (160.0, 18.0), (160.0, 46.0), (160.0, 300.0))]
    out.add("hud_menu_hit/closed", "Menu.hit", {
        "state": "closed", "actions": [], "now": 1.0,
        "xs": [0.0, 160.0, 160.0, 160.0], "ys": [0.0, 18.0, 46.0, 300.0],
    }, {"hits": hits})
    tokens_reset()
    if len(KINDS) < 11:  # self-check: la rejilla debe tocar todos los tipos de control
        raise AssertionError(f"hud_menu_hit: solo {sorted(KINDS)}")


def tokens_reset() -> None:
    from ezagent.voice.hud import tokens

    tokens.set_themed(None)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
