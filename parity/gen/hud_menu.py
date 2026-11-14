"""Golden menu frames and input sequences (domain `hud_menu`, F1-10b).

Frames come from the real ``Menu.draw`` in memory (numpy/Pillow, no window), and
are saved to ``parity/golden/hud/menu/<state>@<t_ms>.png`` with a ``<state>.json``
holding their input. Settings-window frames (``Menu`` + sidebar + card body) go
to ``parity/golden/hud/settings``.

The same file carries the pointer, wheel, slider and text-edit sequences: each
case replays a list of concrete actions (coordinates and times in the input) and
records the resulting state, hit rectangles and callbacks after every step.
"""

from __future__ import annotations

from typing import Any

import numpy as np
from _common import GOLDEN, FixtureFile, dumps, main_guard
from PIL import Image

from ezagent.voice.hud import (
    bar_theme,
    drop,
    flow,
    menu,
    menu_layout,
    settings_window,
    tokens,
)
from ezagent.voice.hud.theme_accent import derive

from hud_settings import WORLD, build as build_settings, ready

DOMAIN = "hud_menu"
OUT = GOLDEN / "hud" / "menu"
OUT_SETTINGS = GOLDEN / "hud" / "settings"
SAMPLES_MS = (0, 16, 33, 50, 100, 200, 400)
CUSTOM_PRESET = "lavanda"
DARK = ("dark", "dark", None)
LIGHT = ("light", "light", None)
CUSTOM_DARK = ("custom", "dark", CUSTOM_PRESET)
CUSTOM_LIGHT = ("custom", "light", CUSTOM_PRESET)
# DECISION: las ventanas de ajustes se pintan desde este generador porque
# reutilizan `Menu.draw` y el mismo mundo; sus PNG van a golden/hud/settings.
# DECISION: los cuadros de animación usan un búfer fijo del viewport completo,
# aunque la ventana real se encoja; así todos los fotogramas comparan igual.
# DECISION: las animaciones se dibujan a DPI 1.0 y el tema personalizado solo a
# 1.0/2.0 para no pasar de ~6 MB; los DPI 1.25/1.5 quedan en los estados fijos.


# --------------------------------------------------------------------- menú
class Look:
    """Un tema dibujable: paleta base, preset y DPI."""

    def __init__(self, theme: str, base: str, preset: str | None = None, dpi: float = 1.0) -> None:
        self.theme = theme
        self.base = base
        self.preset = preset
        self.dpi = dpi

    def tag(self) -> str:
        # DECISION: el DPI y la base entran en el nombre; dos temas custom con
        # distinta base comparten preset y no deben pisarse el PNG.
        name = self.theme if self.preset is None else f"custom-{self.preset}-{self.base}"
        return f"{name}-{int(round(self.dpi * 100))}"


def _look(values: tuple[str, str, str | None], dpi: float = 1.0) -> Look:
    return Look(values[0], values[1], values[2], dpi)


def _activate(look: Look) -> None:
    if look.theme != "custom" or look.preset is None:
        tokens.set_themed(None)
        return
    background, primary, _secondary, _accent = flow.PRESETS[look.preset]
    tokens.set_themed(derive(primary, background))


def _custom_values(look: Look) -> dict[str, Any]:
    if look.theme != "custom" or look.preset is None:
        return {}
    background, primary, secondary, accent = flow.PRESETS[look.preset]
    return {
        "theme": "custom",
        "theme_primary": primary,
        "theme_secondary": secondary,
        "theme_accent": accent,
        "theme_background": background,
    }


class Rec:
    """Callbacks del menú que apuntan lo que ocurre en cada paso."""

    def __init__(self, on_save: bool | None = True, on_copy: bool = True) -> None:
        self.calls: list[list[Any]] = []
        self.save = on_save
        self.copy = on_copy
        self.modifiers: set[str] = set()
        self.saved: dict[str, Any] = {}

    def _diff(self, values: dict[str, Any]) -> dict[str, Any]:
        return {key: value for key, value in values.items() if value != self.saved.get(key)}

    def callbacks(self) -> dict[str, Any]:
        def save(values: dict[str, Any]) -> bool | None:
            self.calls.append(["save", self.save, ready(self._diff(values))])
            return self.save

        return {
            "on_live_change": lambda key, value: self.calls.append(["live", key, ready(value)]),
            "on_save": save,
            "on_test": lambda values: self.calls.append(["test", ready(self._diff(values))]),
            "on_hide": lambda: self.calls.append(["hide"]),
            "on_close": lambda: self.calls.append(["close"]),
            "on_text_edit": lambda active: self.calls.append(["text_edit", active]),
            "on_action": lambda key: self.calls.append(["action", key]),
            "on_copy": lambda text: self._copy(text),
            "on_hotkey_capture": lambda active: self.calls.append(["hotkey_capture", active]),
        }

    def _copy(self, text: str) -> bool:
        self.calls.append(["copy", text])
        return self.copy

    def take(self) -> list[list[Any]]:
        calls, self.calls = self.calls, []
        return calls


def _new(
    look: Look,
    rec: Rec,
    *,
    width: float | None = None,
    screen_h: float = 1080.0,
    values: dict[str, Any] | None = None,
    world: dict[str, Any] | None = None,
    section: str | None = None,
    max_h: float | None = None,
    available_h: float | None = None,
) -> menu.Menu:
    _activate(look)
    data = dict(_custom_values(look))
    data.update(values or {})
    model = build_settings(data, world)
    widget = menu.Menu(
        model,
        scale=look.dpi,
        theme=look.base if look.theme == "custom" else look.theme,
        screen_h=screen_h,
        **rec.callbacks(),
    )
    rec.saved = model.saved_snapshot()
    widget.modifier_source = lambda: frozenset(rec.modifiers)
    widget.set_palette_overrides(bar_theme.card_overrides)
    if width is not None:
        widget.set_width(width)
    if max_h is not None:
        widget.set_max_height(max_h)
    if available_h is not None:
        widget.set_available_height(available_h)
    if section is not None:
        widget.set_groups(
            settings_window.section_groups(section),
            settings_window.section_title(section),
            0.0,
        )
    return widget


# --------------------------------------------------------------- geometría
def _center(rect: Any) -> tuple[float, float]:
    return ((rect.x0 + rect.x1) / 2.0, (rect.y0 + rect.y1) / 2.0)


def _row_kind(widget: menu.Menu, key: str) -> str:
    for _name, rows in widget._shown_groups():
        for row_key, kind in rows:
            if row_key == key:
                return kind
    raise KeyError(key)


def _row_point(widget: menu.Menu, key: str, fraction: float = 0.5) -> tuple[float, float]:
    layout = widget._layout()
    row = layout[key]
    kind = _row_kind(widget, key)
    x0, x1 = menu.ROW_X0, menu.ROW_X1 + widget._dx
    x = x0 + (x1 - x0) * fraction
    y = row.y0 - widget.scroll_offset + menu_layout.ROW[kind] / 2.0
    return (x, y)


def _slider_point(widget: menu.Menu, key: str, fraction: float) -> tuple[float, float]:
    layout = widget._layout()
    x0, x1 = menu.SLIDER_TRACK_X0, menu.SLIDER_TRACK_X1 + widget._dx
    y = layout[key].y0 - widget.scroll_offset + menu.SLIDER_LINE2_CY
    return (x0 + (x1 - x0) * fraction, y)


def _meter_point(widget: menu.Menu, fraction: float) -> tuple[float, float]:
    layout = widget._layout()
    x0, x1 = menu.SLIDER_TRACK_X0, menu.SLIDER_TRACK_X1 + widget._dx
    y = (layout["input_meter"].y0 - widget.scroll_offset
         + menu.METER_TRACK_Y + menu.METER_TRACK_H / 2.0)
    return (x0 + (x1 - x0) * fraction, y)


def _point(widget: menu.Menu, at: list[Any]) -> tuple[float, float]:
    name = at[0]
    layout = widget._layout()
    scroll = widget.scroll_offset
    if name == "hide":
        return _center(widget._hide_rect(layout))
    if name == "save":
        return _center(widget._save_rect(layout))
    if name == "test":
        return _center(widget._test_rect(layout, scroll))
    if name == "action":
        return _center(widget._action_rect(layout, at[1], scroll))
    if name == "copy":
        return _center(widget._copy_rect(layout[at[1]].y0 - scroll))
    if name == "row":
        return _row_point(widget, at[1], at[2] if len(at) > 2 else 0.5)
    if name == "slider":
        return _slider_point(widget, at[1], at[2])
    if name == "meter":
        return _meter_point(widget, at[1])
    if name == "select":
        return _row_point(widget, at[1])
    if name == "theme_preview":
        return _row_point(widget, "theme_preview", at[1])
    if name == "palette":
        pills = widget._palette_pills(layout["theme_preview"].y0 - scroll)
        return _center(pills[at[1]][2])
    geometry = widget._selector_geometry(layout, scroll)
    if name == "option" and geometry is not None:
        top, _visible, _options, list_scroll = geometry
        x = (menu.GROUP_X0 + menu.GROUP_X1 + widget._dx) / 2.0
        y = top + at[1] * menu.LIST_ITEM_H + menu.LIST_ITEM_H / 2.0 - list_scroll
        return (x, y)
    raise ValueError(f"punto desconocido: {at!r}")


def _rect(rect: Any) -> list[float]:
    return [round(float(rect.x0), 3), round(float(rect.y0), 3),
            round(float(rect.x1), 3), round(float(rect.y1), 3)]


def _hit_out(hit: menu.MenuHit | None) -> dict[str, Any] | None:
    if hit is None:
        return None
    return {"kind": hit.kind, "key": hit.key, "rect": _rect(hit.rect),
            "value": ready(hit.value), "index": hit.index, "disabled": hit.disabled}


def _geometry(widget: menu.Menu) -> dict[str, Any]:
    layout = widget._layout()
    scroll = widget.scroll_offset
    rows: dict[str, list[float]] = {}
    for _name, group_rows in widget._shown_groups():
        for key, _kind in group_rows:
            row = layout[key]
            rows[key] = [menu.GROUP_X0, round(row.y0 - scroll, 3),
                         round(menu.GROUP_X1 + widget._dx, 3), round(row.y1 - scroll, 3)]
    controls: dict[str, Any] = {
        "hide": _rect(widget._hide_rect(layout)),
        "save": _rect(widget._save_rect(layout)),
        "test": _rect(widget._test_rect(layout, scroll)),
        "actions": {key: _rect(widget._action_rect(layout, key, scroll))
                    for key in menu.ACTION_KEYS if key in layout},
        "copy": {key: _rect(widget._copy_rect(layout[key].y0 - scroll))
                 for key in menu.SECRET_KEYS if key in layout},
        "palette": [[value, label, _rect(pill)]
                    for value, label, pill in widget._palette_pills(
                        layout["theme_preview"].y0 - scroll)]
        if "theme_preview" in layout else [],
    }
    return {
        "viewport": round(widget.viewport_height, 3),
        "width": round(widget.width, 3),
        "scroll": round(scroll, 3),
        "scroll_max": round(widget.scroll_max, 3),
        "layout": {key: [round(rect.y0, 3), round(rect.y1, 3)] for key, rect in layout.items()},
        "rows": rows,
        "controls": controls,
        "title": widget._title,
        "groups": list(widget.groups),
    }


# --------------------------------------------------------------- acciones
def _resolve(widget: menu.Menu, raw: dict[str, Any]) -> dict[str, Any]:
    step = dict(raw)
    if "at" in step:
        x, y = _point(widget, step.pop("at"))
        step["x"], step["y"] = round(x, 3), round(y, 3)
    step.setdefault("t", round(widget._now, 3))
    return step


def _apply(widget: menu.Menu, rec: Rec, step: dict[str, Any]) -> menu.MenuHit | None:
    op = step["op"]
    t = step["t"]
    hit: menu.MenuHit | None = None
    before = widget.settings.snapshot()
    if op == "open":
        widget.open(t)
    elif op == "close":
        widget.close(t)
    elif op == "wait":
        widget.update(t)
    elif op == "set":
        widget.settings.set(step["key"], step["value"])
    elif op == "pointer":
        getattr(widget, "pointer_" + step["kind"])(step["x"], step["y"], t)
        hit = widget.hit(step["x"], step["y"], t)
    elif op == "wheel":
        widget.wheel(step["x"], step["y"], step["delta"], t)
        hit = widget.hit(step["x"], step["y"], t)
    elif op == "hit":
        hit = widget.hit(step["x"], step["y"], t, include_disabled=step.get("include_disabled", False))
    elif op == "begin_edit":
        widget.begin_text_edit(step["key"], t)
    elif op == "finish_edit":
        widget.finish_text_edit(step["commit"])
    elif op == "text":
        widget.text_input(step["value"])
    elif op == "key":
        widget.text_key(step["vk"], step.get("ctrl", False))
    elif op == "paste":
        widget.paste_text(step["value"])
    elif op == "set_testing":
        widget.set_testing(step["value"], t)
    elif op == "set_input_level":
        widget.set_input_level(step["value"], muted=step.get("muted", False),
                               active=step.get("active", False))
    elif op == "set_reduced_motion":
        widget.set_reduced_motion(step["value"])
    elif op == "on_applied":
        widget.on_applied(step["value"])
    elif op == "set_theme":
        widget.set_theme(step["value"])
    elif op == "set_scale":
        widget.set_scale(step["value"])
    elif op == "toggle":
        widget.toggle(t)
    elif op == "resize":
        widget.resize(step["width"], step["height"], t)
    elif op == "set_width":
        widget.set_width(step["value"])
    elif op == "set_max_height":
        widget.set_max_height(step["value"])
    elif op == "set_available_height":
        widget.set_available_height(step["value"])
    elif op == "set_groups":
        widget.set_groups(settings_window.section_groups(step["section"]),
                          settings_window.section_title(step["section"]), t)
    elif op == "set_settings":
        widget.set_settings(build_settings(step["values"]), t)
    elif op == "modifiers":
        rec.modifiers = set(step["value"])
    elif op == "probe":
        pass
    else:
        raise ValueError(f"acción desconocida: {op!r}")
    after = widget.settings.snapshot()
    changed = {key: after[key] for key in after if before.get(key) != after[key]}
    extra: dict[str, Any] = {}
    if op == "probe":
        extra = {"is_animating": widget.is_animating(t),
                 "frame_key": widget.frame_key(t) is not None}
    step["_changed"] = changed
    step["_extra"] = extra
    return hit


def _result(widget: menu.Menu, step: dict[str, Any], hit: menu.MenuHit | None,
            rec: Rec) -> dict[str, Any]:
    # DECISION: el oráculo lee campos privados (`_armed`, `_capturing`…) para
    # fijar el estado completo; el port de Rust los expone con sus nombres.
    t = step["t"]
    calls = rec.take()
    result = {
        "t": round(t, 3),
        "open": widget.is_open(),
        "height": round(float(widget._height.value), 3),
        "scroll": round(widget.scroll_offset, 3),
        "hover": widget._hover_id,
        "hover_option": widget._hover_option,
        "selector": widget._selector_key,
        "selecting": not widget._selector_closing,
        "editing": widget.editing_key,
        "buffer": widget.text_buffer,
        "cursor": widget._edit_cursor,
        "select_all": widget._edit_select_all,
        "edit_error": None if widget._edit_error is None else list(widget._edit_error),
        "capturing": widget._capturing,
        "capture_mods": sorted(widget._capture_mods),
        "capture_msg": None if widget._capture_msg is None else
        [widget._capture_msg[0], round(widget._capture_msg[1], 3)],
        "copy_hover": widget._copy_hover,
        "copied": sorted(key for key, until in widget._copied.items() if until > t),
        "saving_pending": widget._saving_pending,
        "saved_flash": t < widget._saved_flash_until,
        "save_check": t < widget._check_until,
        "save_error": t < widget._error_until,
        "armed": sorted(key for key, until in widget._armed.items() if until > t),
        "dirty": widget.settings.dirty(),
        "hit": _hit_out(hit),
        "changed": ready(step["_changed"]),
        "calls": calls,
    }
    result.update(step["_extra"])
    return result


def _run(widget: menu.Menu, rec: Rec, steps: list[dict[str, Any]]) -> tuple[list[dict], list[dict]]:
    resolved: list[dict[str, Any]] = []
    results: list[dict[str, Any]] = []
    for raw in steps:
        step = _resolve(widget, raw)
        hit = _apply(widget, rec, step)
        resolved.append({key: value for key, value in step.items() if not key.startswith("_")})
        results.append(_result(widget, step, hit, rec))
    return resolved, results


def _desc(widget: menu.Menu, look: Look, state: str, actions: list[dict[str, Any]],
          world: dict[str, Any] | None = None, **extra: Any) -> dict[str, Any]:
    data: dict[str, Any] = {
        "theme": look.theme,
        "base": look.base,
        "preset": look.preset,
        "dpi": look.dpi,
        "width": round(widget.width, 3),
        "window_px": [max(1, round(widget.width * look.dpi)),
                      max(1, round((tokens.H_ISLAND + widget.viewport_height) * look.dpi))],
        "viewport": round(widget.viewport_height, 3),
        "scroll_max": round(widget.scroll_max, 3),
        "title": widget._title,
        "groups": list(widget.groups),
        "state": state,
        "world": ready(world or WORLD),
        "actions": actions,
    }
    data.update(extra)
    return data


def _straight(rgba: np.ndarray) -> Image.Image:
    alpha = rgba[..., 3]
    safe = np.maximum(alpha, 1e-6)[..., None]
    straight = np.clip(np.where(alpha[..., None] > 0.0, rgba[..., :3] / safe, 0.0), 0.0, 1.0)
    data = np.empty((*alpha.shape, 4), np.uint8)
    data[..., :3] = np.round(straight * 255.0).astype(np.uint8)
    data[..., 3] = np.round(np.clip(alpha, 0.0, 1.0) * 255.0).astype(np.uint8)
    return Image.fromarray(data, "RGBA")


def _paper(widget: menu.Menu, look: Look, t: float) -> Image.Image:
    width = max(1, int(round(widget.width * look.dpi)))
    height = max(1, int(round((tokens.H_ISLAND + widget.viewport_height) * look.dpi)))
    buf = np.zeros((height, width, 4), np.float32)
    widget.draw(buf, t)
    return _straight(buf)


class Book:
    def __init__(self, out: FixtureFile) -> None:
        self.out = out
        self.states: dict[str, dict[str, Any]] = {}
        self.frames: dict[str, list[dict[str, Any]]] = {}
        self.total = 0

    def frame(self, folder: Any, view: str, name: str, desc: dict[str, Any], t_ms: int,
              img: Image.Image, fn: str) -> None:
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"{name}@{t_ms}.png"
        img.save(path, format="PNG")
        rel = f"hud/{view}/{path.name}"
        self.frames.setdefault(name, []).append({"t_ms": t_ms, "png": rel})
        self.states.setdefault(name, {"view": view, "state": name, "input": desc})
        self.out.add(f"hud_menu/{view}/{name}@{t_ms:04d}", fn, desc, {"png": rel})
        self.total += 1

    def flush(self) -> None:
        if self.total < 120:  # self-check: el catálogo no debe encogerse
            raise AssertionError(f"hud_menu: solo {self.total} PNG, se esperaban >= 120")
        for name in sorted(self.states):
            entry = {**self.states[name], "frames": self.frames[name]}
            path = (OUT_SETTINGS if entry["view"] == "settings" else OUT) / f"{name}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(dumps(entry) + "\n", encoding="utf-8", newline="\n")


def _still(book: Book, look: Look, name: str, state: str, steps: list[dict[str, Any]] = (),
           t: float = 1.2, rec_options: dict[str, Any] | None = None,
           world: dict[str, Any] | None = None, **options: Any) -> menu.Menu:
    rec = Rec(**(rec_options or {}))
    widget = _new(look, rec, world=world, **options)
    actions, _results = _run(
        widget, rec,
        [{"op": "open", "t": 0.0}, *steps, {"op": "wait", "t": t}],
    )
    desc = _desc(widget, look, state, actions, world=world)
    img = _paper(widget, look, t)
    book.frame(OUT, "menu", name, desc, int(round(t * 1000)), img, "Menu.draw")
    return widget


def _animation(book: Book, look: Look, name: str, state: str, animation: str,
               steps: list[dict[str, Any]], samples: tuple[int, ...], t0: float,
               rec_options: dict[str, Any] | None = None, **options: Any) -> None:
    rec = Rec(**(rec_options or {}))
    widget = _new(look, rec, **options)
    todo = sorted(steps, key=lambda step: step.get("t", 0.0))
    clock = 0.0
    resolved: list[dict[str, Any]] = []
    for ms in samples:
        target = t0 + ms / 1000.0
        # DECISION: pasos de 1/60 s antes de cada muestra para que el medidor
        # (que suaviza por dt) vea la misma cadencia que la aplicación.
        while clock < target - 1e-9:
            clock = min(target, clock + 1.0 / 60.0)
            while todo and todo[0].get("t", 0.0) <= clock + 1e-9:
                step = _resolve(widget, todo.pop(0))
                _apply(widget, rec, step)
                resolved.append({k: v for k, v in step.items() if not k.startswith("_")})
            widget.update(clock)
        while todo and todo[0].get("t", 0.0) <= target + 1e-9:
            step = _resolve(widget, todo.pop(0))
            _apply(widget, rec, step)
            resolved.append({k: v for k, v in step.items() if not k.startswith("_")})
        rec.take()
        desc = _desc(widget, look, state, list(resolved),
                     animation={"name": animation, "trigger_s": t0,
                                "samples_ms": list(samples)})
        book.frame(OUT, "menu", name, desc, ms, _paper(widget, look, target), "Menu.draw")


# ------------------------------------------------------------------- vistas
def full_view(book: Book) -> None:
    for theme in (DARK, LIGHT):
        for dpi in (1.0, 1.25, 1.5, 2.0):
            look = _look(theme, dpi)
            _still(book, look, f"full-{look.tag()}", "full")
    for dpi in (1.0, 2.0):
        look = _look(CUSTOM_DARK, dpi)
        _still(book, look, f"full-{look.tag()}", "custom")


def sections_view(book: Book) -> None:
    width, _height = settings_window.content_size(settings_window.DEFAULT_W,
                                                  settings_window.DEFAULT_H)
    for theme in (DARK, LIGHT):
        look = _look(theme)
        for section in settings_window.SIDEBAR_ORDER:
            _still(book, look, f"section-{section}-{look.tag()}", "section",
                   width=width, section=section)
    advanced = _look(DARK)
    _still(book, advanced, f"section-system-advanced-{advanced.tag()}", "advanced",
           width=width, section="system", values={"show_advanced": True})


def advanced_view(book: Book) -> None:
    for theme in (DARK, LIGHT):
        look = _look(theme)
        _still(book, look, f"advanced-{look.tag()}", "advanced", values={"show_advanced": True})
    custom = _look(CUSTOM_DARK)
    _still(book, custom, f"advanced-{custom.tag()}", "advanced", values={"show_advanced": True})
    _still(book, custom, f"advanced-bottom-{custom.tag()}", "advanced",
           values={"show_advanced": True},
           steps=[{"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120 * 20, "t": 0.6}],
           t=1.0)


def scroll_view(book: Book) -> None:
    for theme in (DARK, LIGHT):
        look = _look(theme)
        for tag, delta in (("middle", -120 * 4), ("bottom", -120 * 40)):
            _still(book, look, f"scroll-{tag}-{look.tag()}", "scroll",
                   steps=[{"op": "wheel", "x": 160.0, "y": 300.0, "delta": delta, "t": 0.6}],
                   t=1.0)
    for tag, delta in (("top", 0), ("middle", -120 * 3)):
        look = _look(DARK)
        _still(book, look, f"small-{tag}-{look.tag()}", "scroll", screen_h=480.0,
               steps=[] if not delta else [{"op": "wheel", "x": 160.0, "y": 250.0,
                                            "delta": delta, "t": 0.6}], t=1.0)


def selector_view(book: Book) -> None:
    width, _height = settings_window.content_size(settings_window.DEFAULT_W,
                                                  settings_window.DEFAULT_H)
    sections = {
        "theme": "appearance", "voice": "voice", "stt_model": "models",
        "theme_preset": "appearance", "llm_saved_profile": "models",
        "llm_model_pick": "models",
    }
    cases = [
        ("theme", DARK, None), ("theme", LIGHT, None), ("voice", DARK, None),
        ("stt_model", LIGHT, None), ("theme_preset", LIGHT, None),
        ("llm_saved_profile", DARK, None),
    ]
    for key, theme, values in cases:
        look = _look(theme)
        if key == "theme_preset":
            values = {"theme": "custom"}
        _still(book, look, f"select-{key}-{look.tag()}", "selector", t=1.0,
               width=width, section=sections[key], values=values,
               steps=[{"op": "pointer", "kind": "down", "at": ["select", key], "t": 0.6},
                      {"op": "pointer", "kind": "up", "at": ["select", key], "t": 0.62}])
    look = _look(DARK)
    _still(book, look, f"select-llm_model_pick-{look.tag()}", "selector", t=1.0,
           width=width, section="models",
           steps=[{"op": "pointer", "kind": "down", "at": ["select", "llm_model_pick"], "t": 0.6},
                  {"op": "pointer", "kind": "up", "at": ["select", "llm_model_pick"], "t": 0.62},
                  {"op": "wheel", "at": ["option", 2], "delta": -120, "t": 0.9}])
    _still(book, look, f"select-closing-{look.tag()}", "selector", t=0.72,
           width=width, section="appearance",
           steps=[{"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 0.6},
                  {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 0.62},
                  {"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 0.66},
                  {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 0.68}])


def editing_view(book: Book) -> None:
    width, _height = settings_window.content_size(settings_window.DEFAULT_W,
                                                  settings_window.DEFAULT_H)
    for theme, tag in ((DARK, "end"), (LIGHT, "end")):
        look = _look(theme)
        _still(book, look, f"edit-wake-{tag}-{look.tag()}", "editing", t=0.9,
               width=width, section="listening",
               steps=[{"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
                      {"op": "text", "value": " y escucha", "t": 0.7}])
    look = _look(DARK)
    _still(book, look, f"edit-wake-all-{look.tag()}", "editing", t=0.9,
           width=width, section="listening",
           steps=[{"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
                  {"op": "key", "vk": 65, "ctrl": True, "t": 0.7}])
    _still(book, look, f"edit-dictionary-{look.tag()}", "editing", t=0.9,
           width=width, section="models",
           steps=[{"op": "begin_edit", "key": "stt_dictionary", "t": 0.6},
                  {"op": "text", "value": "open code → OpenCode; cloud → Claude", "t": 0.7}])
    _still(book, look, f"secret-{look.tag()}", "secret", width=width, section="models",
           values={"api_key": "sk-or-v1-1234567890abcdef"},
           steps=[{"op": "pointer", "kind": "move", "at": ["row", "api_key"], "t": 0.6}])
    _still(book, look, f"secret-copied-{look.tag()}", "secret", t=0.8,
           width=width, section="models",
           values={"api_key": "sk-or-v1-1234567890abcdef"},
           steps=[{"op": "pointer", "kind": "move", "at": ["copy", "api_key"], "t": 0.6},
                  {"op": "pointer", "kind": "down", "at": ["copy", "api_key"], "t": 0.62},
                  {"op": "pointer", "kind": "up", "at": ["copy", "api_key"], "t": 0.64}])
    _still(book, _look(LIGHT), f"secret-edit-{_look(LIGHT).tag()}", "secret", t=0.9,
           width=width, section="models",
           steps=[{"op": "begin_edit", "key": "api_key", "t": 0.6},
                  {"op": "text", "value": "nueva-clave-1234", "t": 0.7}])
    _still(book, look, f"hotkey-{look.tag()}", "hotkey", t=1.0,
           width=width, section="system",
           steps=[{"op": "begin_edit", "key": "hotkey", "t": 0.6}])
    _still(book, look, f"hotkey-mods-{look.tag()}", "hotkey", t=1.0,
           width=width, section="system",
           steps=[{"op": "begin_edit", "key": "hotkey", "t": 0.6},
                  {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7},
                  {"op": "wait", "t": 0.8}])
    _still(book, look, f"hotkey-conflict-{look.tag()}", "hotkey", t=1.0,
           width=width, section="system",
           steps=[{"op": "begin_edit", "key": "mute_hotkey", "t": 0.6},
                  {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7},
                  {"op": "key", "vk": 32, "ctrl": False, "t": 0.8}])
    _still(book, _look(LIGHT), f"hotkey-reject-{_look(LIGHT).tag()}", "hotkey", t=1.0,
           width=width, section="system",
           steps=[{"op": "begin_edit", "key": "hotkey", "t": 0.6},
                  {"op": "key", "vk": 65, "ctrl": False, "t": 0.8}])


def hover_view(book: Book) -> None:
    width, _height = settings_window.content_size(settings_window.DEFAULT_W,
                                                  settings_window.DEFAULT_H)
    cases = ((DARK, "speed", "voice"), (LIGHT, "smart_turn", "listening"),
             (DARK, "hotkey", "system"), (DARK, "test_voice", "voice"),
             (LIGHT, "repeat_last", "voice"))
    for theme, key, section in cases:
        look = _look(theme)
        _still(book, look, f"hover-{key}-{look.tag()}", "hover", t=0.8,
               width=width, section=section,
               steps=[{"op": "pointer", "kind": "move", "at": ["row", key], "t": 0.6}])
    look = _look(DARK)
    _still(book, look, f"hover-hide-{look.tag()}", "hover", t=0.8,
           steps=[{"op": "pointer", "kind": "move", "at": ["hide"], "t": 0.6}])
    _still(book, look, f"press-save-{look.tag()}", "hover", t=0.9,
           values={"speed": 1.1},
           steps=[{"op": "pointer", "kind": "move", "at": ["save"], "t": 0.6},
                  {"op": "pointer", "kind": "down", "at": ["save"], "t": 0.7}])
    _still(book, look, f"hover-option-{look.tag()}", "hover", t=0.9,
           width=width, section="appearance",
           steps=[{"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 0.6},
                  {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 0.62},
                  {"op": "pointer", "kind": "move", "at": ["option", 2], "t": 0.8}])


def footer_view(book: Book) -> None:
    width, _height = settings_window.content_size(settings_window.DEFAULT_W,
                                                  settings_window.DEFAULT_H)
    for theme in (DARK, LIGHT):
        look = _look(theme)
        _still(book, look, f"footer-dirty-{look.tag()}", "footer", t=0.9,
               steps=[{"op": "set", "key": "speed", "value": 1.1, "t": 0.6}])
        _still(book, look, f"footer-test-{look.tag()}", "footer", t=1.0,
               width=width, section="voice",
               steps=[{"op": "pointer", "kind": "down", "at": ["test"], "t": 0.6},
                      {"op": "pointer", "kind": "up", "at": ["test"], "t": 0.62}])
    look = _look(DARK)
    _still(book, look, f"footer-saved-{look.tag()}", "footer", t=0.9,
           steps=[{"op": "set", "key": "speed", "value": 1.1, "t": 0.6},
                  {"op": "pointer", "kind": "down", "at": ["save"], "t": 0.7},
                  {"op": "pointer", "kind": "up", "at": ["save"], "t": 0.72}])
    _still(book, look, f"footer-error-{look.tag()}", "footer", t=0.9,
           rec_options={"on_save": False},
           steps=[{"op": "set", "key": "speed", "value": 1.1, "t": 0.6},
                  {"op": "pointer", "kind": "down", "at": ["save"], "t": 0.7},
                  {"op": "pointer", "kind": "up", "at": ["save"], "t": 0.72}])
    _still(book, look, f"footer-armed-{look.tag()}", "footer", t=0.9,
           width=width, section="voice",
           steps=[{"op": "pointer", "kind": "down", "at": ["action", "new_conversation"],
                   "t": 0.6},
                  {"op": "pointer", "kind": "up", "at": ["action", "new_conversation"],
                   "t": 0.62}])


# ---------------------------------------------------------------- animaciones
def animations_view(book: Book) -> None:
    samples = (*SAMPLES_MS, 600)
    for theme in (DARK, LIGHT):
        look = _look(theme)
        _animation(book, look, f"anim-open-{look.tag()}", "animation", "open",
                   [{"op": "open", "t": 0.0}], samples, 0.0)
    look = _look(DARK)
    _animation(book, look, f"anim-close-{look.tag()}", "animation", "close",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "close", "t": 1.2}], (*SAMPLES_MS, 500), 1.2)
    _animation(book, look, f"anim-selector-{look.tag()}", "animation", "selector",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 1.2},
                {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 1.22}],
               (*SAMPLES_MS, 320), 1.2)
    _animation(book, look, f"anim-toggle-{look.tag()}", "animation", "toggle",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "pointer", "kind": "down", "at": ["row", "smart_turn"], "t": 1.2},
                {"op": "pointer", "kind": "up", "at": ["row", "smart_turn"], "t": 1.22}],
               (*SAMPLES_MS, 300), 1.2)
    drag = [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
            {"op": "pointer", "kind": "down", "at": ["slider", "speed", 0.1], "t": 1.2}]
    for ms in (33, 50, 100, 200, 300):
        drag.append({"op": "pointer", "kind": "move",
                     "at": ["slider", "speed", min(1.0, 0.1 + ms / 1000.0)],
                     "t": 1.2 + ms / 1000.0})
    _animation(book, look, f"anim-slider-{look.tag()}", "animation", "slider",
               drag, (*SAMPLES_MS, 300), 1.2)
    _animation(book, look, f"anim-scroll-{look.tag()}", "animation", "scroll",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120 * 3, "t": 1.2}],
               (*SAMPLES_MS, 600), 1.2)
    _animation(book, look, f"anim-save-{look.tag()}", "animation", "save",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "set", "key": "speed", "value": 1.1, "t": 1.18},
                {"op": "pointer", "kind": "down", "at": ["save"], "t": 1.2},
                {"op": "pointer", "kind": "up", "at": ["save"], "t": 1.22}],
               (*SAMPLES_MS, 1200), 1.2)
    _animation(book, look, f"anim-test-{look.tag()}", "animation", "test",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "pointer", "kind": "down", "at": ["test"], "t": 1.2},
                {"op": "pointer", "kind": "up", "at": ["test"], "t": 1.22}],
               (*SAMPLES_MS, 600), 1.2)
    _animation(book, look, f"anim-meter-{look.tag()}", "animation", "meter",
               [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                {"op": "set_input_level", "value": 0.65, "active": True, "t": 1.2},
                {"op": "set_input_level", "value": 0.1, "active": True, "t": 1.5}],
               (*SAMPLES_MS, 600), 1.2)


# --------------------------------------------------------- ventana de ajustes
def _card_paper(widget: menu.Menu, look: Look, width: float, height: float,
                section: str, hover: str | None, t: float, amount: float = 1.0) -> Image.Image:
    scale = look.dpi
    w, h = int(round(width * scale)), int(round(height * scale))
    frame = np.array(
        drop.render_card_body(w, h, scale, look.base, fresh=True,
                              version=tokens.palette_version()),
        dtype=np.float32,
    )
    sidebar = settings_window.Sidebar()
    sidebar.draw(frame, scale, bar_theme.card_palette(look.base), look.base, section, hover, t,
                 False, with_mark=True)
    cw = int(round(widget.width * scale))
    y0 = int(round(tokens.H_ISLAND * scale))
    viewport_h = settings_window.content_size(width, height)[1]
    content = np.zeros((y0 + int(round((viewport_h + 4) * scale)), cw, 4), np.float32)
    widget.draw(content, t)
    settings_window.composite_over(frame, np.clip(content[y0:y0 + h], 0.0, 1.0),
                                   int(round(settings_window.SIDEBAR_W * scale)), 0)
    settings_window.draw_grip(frame, scale, bar_theme.card_palette(look.base))
    settings_window.fade(frame, amount)
    return _straight(frame)


def _card(book: Book, look: Look, name: str, section: str, steps: list[dict[str, Any]] = (),
          t: float = 1.2, width: float = settings_window.DEFAULT_W,
          height: float = settings_window.DEFAULT_H, hover: str | None = None,
          amount: float = 1.0, values: dict[str, Any] | None = None,
          rec_options: dict[str, Any] | None = None) -> None:
    rec = Rec(**(rec_options or {}))
    viewport_h = settings_window.content_size(width, height)[1]
    widget = _new(look, rec, width=width - settings_window.SIDEBAR_W,
                  values=values, section=section, max_h=viewport_h)
    actions, _results = _run(widget, rec, [{"op": "open", "t": 0.0}, *steps,
                                           {"op": "wait", "t": t}])
    desc = _desc(widget, look, f"settings-{section}", actions,
                 section=section, window=[round(width, 3), round(height, 3)],
                 fade=amount, sidebar_hover=hover)
    desc["window_px"] = [int(round(width * look.dpi)), int(round(height * look.dpi))]
    img = _card_paper(widget, look, width, height, section, hover, t, amount)
    book.frame(OUT_SETTINGS, "settings", name, desc, int(round(t * 1000)), img,
               "settings_window")


def settings_frames(book: Book) -> None:
    for theme in (DARK, LIGHT):
        look = _look(theme)
        for section in settings_window.SIDEBAR_ORDER:
            _card(book, look, f"window-{section}-{look.tag()}", section)
    for dpi in (1.25, 1.5, 2.0):
        look = _look(DARK, dpi)
        _card(book, look, f"window-appearance-{look.tag()}", "appearance")
    for theme in (CUSTOM_DARK, CUSTOM_LIGHT):
        look = _look(theme)
        _card(book, look, f"window-appearance-custom-{look.tag()}", "appearance")
        _card(book, look, f"window-hotkeys-custom-{look.tag()}", "system",
              steps=[], values={"show_advanced": True})
    for section in ("appearance", "system"):
        _card(book, _look(LIGHT), f"window-min-{section}-{_look(LIGHT).tag()}", section,
              width=settings_window.MIN_W, height=settings_window.MIN_H,
              values={"show_advanced": True})
    _card(book, _look(DARK), f"window-hover-{_look(DARK).tag()}", "appearance",
          hover="voice")
    _card(book, _look(DARK), f"window-fade-{_look(DARK).tag()}", "appearance", amount=0.55)
    for theme in (DARK, LIGHT):
        look = _look(theme)
        _card(book, look, f"window-capture-{look.tag()}", "system", t=1.0,
              values={"show_advanced": True},
              steps=[{"op": "begin_edit", "key": "hotkey", "t": 0.6},
                     {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7}])
    _card(book, _look(DARK), f"window-conflict-{_look(DARK).tag()}", "system", t=1.1,
          values={"show_advanced": True},
          steps=[{"op": "begin_edit", "key": "mute_hotkey", "t": 0.6},
                 {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7},
                 {"op": "key", "vk": 32, "ctrl": False, "t": 0.8}])
    _card(book, _look(DARK), f"window-dirty-{_look(DARK).tag()}", "system", t=1.0,
          values={"show_advanced": True},
          steps=[{"op": "set", "key": "speed", "value": 1.1, "t": 0.6}])


# ------------------------------------------------------------------- casos
def layout_cases(out: FixtureFile) -> None:
    """`menu_layout.layout_menu` y `visible_menu_height` sin pasar por el menú."""
    groups = menu_layout.GROUPS
    cases: list[tuple[str, dict[str, float], float | None, Any]] = [
        ("base", {}, None, None),
        ("extra-select", {"theme": 128.0}, None, None),
        ("extra-partial", {"theme": 40.0}, None, None),
        ("extra-other", {"voice": 96.0}, None, None),
        ("viewport-560", {}, 560.0, None),
        ("viewport-400", {}, 400.0, None),
        ("viewport-300", {}, 300.0, None),
        ("viewport-160", {}, 160.0, None),
        ("viewport-tall", {}, 900.0, None),
        ("groups-look", {}, None, (groups[0],)),
        ("groups-two", {}, 560.0, (groups[0], groups[2])),
        ("groups-one-row", {}, 400.0, (("custom", (("speed", "slider"),)),)),
    ]
    for tag, extra, viewport, chosen in cases:
        layout = menu_layout.layout_menu(extra, viewport, chosen)
        out.add(f"hud_menu/layout/{tag}", "layout_menu",
                {"extra": extra, "viewport_h": viewport,
                 "groups": None if chosen is None else [[name, [list(row) for row in rows]]
                                                        for name, rows in chosen]},
                {"rects": {key: [round(rect.y0, 3), round(rect.y1, 3)]
                           for key, rect in layout.items()},
                 "header": {"width": layout["header"].width,
                            "height": layout["header"].height,
                            "center_y": layout["header"].center_y},
                 "total": [round(layout["total"].y0, 3), round(layout["total"].y1, 3)]})
    for screen in (0.0, 320.0, 480.0, 600.0, 720.0, 1080.0, 1440.0, 2160.0):
        out.add(f"hud_menu/layout/visible/{int(screen)}", "visible_menu_height",
                {"screen_h": screen}, {"value": menu_layout.visible_menu_height(screen)})
    out.add("hud_menu/layout/tables", "menu_layout",
            {},
            {"rows": dict(menu_layout.ROW),
             "groups": [[name, [list(row) for row in rows]] for name, rows in groups],
             "sections": [[name, title, list(names)]
                          for name, title, names in menu_layout.SECTIONS],
             "labels": dict(menu_layout.GROUP_LABELS),
             "advanced": sorted(menu_layout.ADVANCED_ROWS),
             "custom": sorted(menu_layout.CUSTOM_THEME_ROWS)})


def geometry_cases(out: FixtureFile) -> None:
    cases: list[tuple[str, Look, dict[str, Any], list[dict[str, Any]]]] = [
        ("full", _look(DARK), {}, []),
        ("advanced", _look(DARK), {"values": {"show_advanced": True}}, []),
        ("custom", _look(CUSTOM_DARK), {}, []),
        ("width-400", _look(DARK), {"width": 400.0}, []),
        ("width-680", _look(DARK), {"width": 680.0}, []),
        ("small", _look(DARK), {"screen_h": 480.0}, []),
        ("scroll", _look(DARK), {},
         [{"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120 * 6, "t": 0.6}]),
        ("select-open", _look(DARK), {},
         [{"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 0.6},
          {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 0.62}]),
        ("no-devices", _look(DARK), {"world": {"devices": []}}, []),
        ("reduced", _look(DARK), {},
         [{"op": "set_reduced_motion", "value": True, "t": 0.1}]),
    ]
    for section in settings_window.SIDEBAR_ORDER:
        cases.append((f"section-{section}", _look(DARK),
                      {"width": 576.0, "section": section}, []))
    for tag, look, options, steps in cases:
        rec = Rec()
        widget = _new(look, rec, **options)
        _run(widget, rec, [{"op": "open", "t": 0.0}, {"op": "wait", "t": 0.8}, *steps])
        out.add(f"hud_menu/geometry/{tag}", "Menu.layout",
                {"theme": look.theme, "base": look.base, "dpi": look.dpi,
                 "actions": [{"op": "open", "t": 0.0}, {"op": "wait", "t": 0.8}, *steps],
                 "world": ready(options.get("world") or WORLD),
                 **{k: v for k, v in options.items() if k != "world"}},
                _geometry(widget))
    rec = Rec()
    widget = _new(_look(DARK), rec, available_h=300.0)
    _run(widget, rec, [{"op": "open", "t": 0.0}, {"op": "wait", "t": 0.8}])
    out.add("hud_menu/geometry/available-height", "Menu.layout",
            {"available_h": 300.0}, _geometry(widget))


def probe_cases(out: FixtureFile) -> None:
    probes = [
        ("open-settled", {}, 1.2),
        ("opening", {}, 0.1),
        ("editing", {"steps": [{"op": "begin_edit", "key": "wake_phrase", "t": 0.6}]}, 0.7),
        ("testing", {"steps": [{"op": "pointer", "kind": "down", "at": ["test"], "t": 0.6},
                               {"op": "pointer", "kind": "up", "at": ["test"], "t": 0.62}]}, 0.7),
        ("save-flash", {"rec": {"on_save": True},
                        "steps": [{"op": "set", "key": "speed", "value": 1.1, "t": 0.6},
                                  {"op": "pointer", "kind": "down", "at": ["save"], "t": 0.7},
                                  {"op": "pointer", "kind": "up", "at": ["save"], "t": 0.72}]},
         0.8),
        ("save-error", {"rec": {"on_save": False},
                        "steps": [{"op": "set", "key": "speed", "value": 1.1, "t": 0.6},
                                  {"op": "pointer", "kind": "down", "at": ["save"], "t": 0.7},
                                  {"op": "pointer", "kind": "up", "at": ["save"], "t": 0.72}]},
         0.8),
        ("copied", {"steps": [{"op": "set", "key": "api_key", "value": "clave-12345678", "t": 0.5},
                              {"op": "pointer", "kind": "down", "at": ["copy", "api_key"], "t": 0.6},
                              {"op": "pointer", "kind": "up", "at": ["copy", "api_key"], "t": 0.62}]},
         0.7),
        ("scrolled", {"steps": [{"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120, "t": 0.6}]},
         0.7),
        ("closed", {"steps": [{"op": "close", "t": 0.6}], "t": 1.2}, 1.2),
        ("dirty-settled", {"steps": [{"op": "set", "key": "speed", "value": 1.1, "t": 0.6}]},
         1.2),
    ]
    for tag, options, t in probes:
        rec_options = options.pop("rec", None)
        steps = options.pop("steps", [])
        final = options.pop("t", t)
        rec = Rec(**(rec_options or {}))
        widget = _new(_look(DARK), rec, max_h=4000.0)
        _run(widget, rec, [{"op": "open", "t": 0.0}, *steps, {"op": "wait", "t": final},
                           {"op": "probe", "t": final}])
        out.add(f"hud_menu/probe/{tag}", "Menu.is_animating",
                {"actions": [{"op": "open", "t": 0.0}, *steps, {"op": "wait", "t": final}],
                 "t": final},
                {"is_animating": widget.is_animating(final),
                 "frame_key": widget.frame_key(final) is not None})


def _sequence(out: FixtureFile, tag: str, look: Look, steps: list[dict[str, Any]],
              values: dict[str, Any] | None = None, prefix: bool = True,
              width: float | None = None, max_h: float | None = None,
              screen_h: float = 1080.0, rec_options: dict[str, Any] | None = None,
              world: dict[str, Any] | None = None, section: str | None = None) -> None:
    rec = Rec(**(rec_options or {}))
    widget = _new(look, rec, values=values, width=width, max_h=max_h, screen_h=screen_h,
                  world=world, section=section)
    head = [{"op": "open", "t": 0.0}, {"op": "wait", "t": 0.4}] if prefix else []
    actions, results = _run(widget, rec, [*head, *steps])
    out.add(f"hud_menu/sequence/{tag}", "Menu.input",
            {"theme": look.theme, "base": look.base, "dpi": look.dpi,
             "width": width, "screen_h": screen_h, "max_h": max_h, "section": section,
             "values": values or {}, "world": ready(world or WORLD), "actions": actions},
            {"steps": results, "rects": _geometry(widget)})


def sequences(out: FixtureFile) -> None:
    dark = _look(DARK)
    click = lambda at, t: [{"op": "pointer", "kind": "down", "at": at, "t": t},
                           {"op": "pointer", "kind": "up", "at": at, "t": t + 0.02}]
    _sequence(out, "open-close", dark, prefix=False, steps=[
        {"op": "wait", "t": 0.1}, {"op": "wait", "t": 0.2}, {"op": "wait", "t": 0.4},
        {"op": "close", "t": 1.0}, {"op": "wait", "t": 1.1}, {"op": "wait", "t": 1.3},
        {"op": "open", "t": 2.0}, {"op": "wait", "t": 2.4}])
    _sequence(out, "open-reverse", dark, prefix=False, steps=[
        {"op": "wait", "t": 0.2}, {"op": "close", "t": 0.25}, {"op": "wait", "t": 0.3},
        {"op": "open", "t": 0.35}, {"op": "wait", "t": 0.5}])
    _sequence(out, "hide", dark, steps=[*click(["hide"], 0.6), {"op": "wait", "t": 1.2}])
    _sequence(out, "click-outside", dark, steps=[
        {"op": "pointer", "kind": "down", "x": 5.0, "y": 5.0, "t": 0.6},
        {"op": "pointer", "kind": "up", "x": 5.0, "y": 5.0, "t": 0.62},
        {"op": "hit", "x": 5.0, "y": 5.0, "t": 0.7}])
    _sequence(out, "save-clean", dark, steps=[*click(["save"], 0.6)])
    _sequence(out, "save-now", dark, width=576.0, section="listening", steps=[
        *click(["row", "smart_turn"], 0.6), *click(["save"], 0.8), {"op": "wait", "t": 1.0}])
    _sequence(out, "save-pending-ok", dark, width=576.0, section="listening",
              rec_options={"on_save": None}, steps=[
        *click(["row", "smart_turn"], 0.6), *click(["save"], 0.8),
        {"op": "wait", "t": 0.9}, {"op": "on_applied", "value": True, "t": 0.9}])
    _sequence(out, "save-pending-fail", dark, width=576.0, section="listening",
              rec_options={"on_save": None}, steps=[
        *click(["row", "smart_turn"], 0.6), *click(["save"], 0.8),
        {"op": "wait", "t": 0.9}, {"op": "on_applied", "value": False, "t": 0.9}])
    _sequence(out, "save-fail", dark, width=576.0, section="listening",
              rec_options={"on_save": False}, steps=[
        *click(["row", "smart_turn"], 0.6), *click(["save"], 0.8), {"op": "wait", "t": 1.0}])
    toggles = ["smart_turn", "barge_in", "confirm_actions", "auto_start_whisper",
               "voice_wake", "show_advanced", "chat_voice"]
    steps: list[dict[str, Any]] = []
    for index, key in enumerate(toggles):
        steps += click(["row", key], 0.6 + index * 0.1)
        steps += click(["row", key], 0.65 + index * 0.1)
    _sequence(out, "toggles", dark, max_h=4000.0, steps=steps)
    _sequence(out, "select-theme", dark, width=576.0, section="appearance", steps=[
        *click(["select", "theme"], 0.6), {"op": "wait", "t": 0.6},
        *click(["option", 2], 0.9)])
    _sequence(out, "select-same", dark, width=576.0, section="appearance", steps=[
        *click(["select", "theme"], 0.6), *click(["select", "theme"], 0.9)])
    _sequence(out, "select-switch", dark, width=576.0, section="appearance",
              values={"theme": "custom"}, steps=[
        *click(["select", "theme"], 0.6), *click(["select", "theme_base"], 0.9)])
    _sequence(out, "select-wheel", dark, width=576.0, section="models", steps=[
        *click(["select", "llm_model_pick"], 0.6),
        {"op": "wheel", "at": ["option", 2], "delta": -120, "t": 0.9},
        {"op": "wheel", "at": ["option", 2], "delta": -120, "t": 1.0},
        *click(["option", 4], 1.1)])
    _sequence(out, "select-disabled", dark, width=576.0, section="listening",
              world={"devices": []}, steps=[
        {"op": "hit", "at": ["row", "input_device_index"], "t": 0.6},
        {"op": "hit", "at": ["row", "input_device_index"], "include_disabled": True, "t": 0.6},
        *click(["select", "input_device_index"], 0.7)])
    _sequence(out, "select-no-options", dark, width=576.0, section="models",
              world={"llm_models": {}},
              steps=[*click(["select", "llm_model_pick"], 0.6)])
    _sequence(out, "slider-click", dark, max_h=4000.0, steps=[
        *click(["slider", "speed", 0.0], 0.6), *click(["slider", "speed", 0.25], 0.7),
        *click(["slider", "speed", 0.5], 0.8), *click(["slider", "speed", 1.0], 0.9)])
    drag = [{"op": "pointer", "kind": "down", "at": ["slider", "speed", 0.1], "t": 0.6}]
    for index, fraction in enumerate((0.2, 0.35, 0.5, 0.8, 1.0)):
        drag.append({"op": "pointer", "kind": "move", "at": ["slider", "speed", fraction],
                     "t": 0.62 + index * 0.06})
    drag.append({"op": "pointer", "kind": "up", "at": ["slider", "speed", 1.0], "t": 1.0})
    _sequence(out, "slider-drag", dark, max_h=4000.0, steps=drag)
    _sequence(out, "slider-wheel", dark, max_h=4000.0, steps=[
        {"op": "wheel", "at": ["slider", "speed", 0.5], "delta": 120, "t": 0.6},
        {"op": "wheel", "at": ["slider", "speed", 0.5], "delta": -120, "t": 0.7},
        {"op": "wheel", "at": ["slider", "speed", 0.5], "delta": 240, "t": 0.8},
        {"op": "wheel", "at": ["row", "smart_turn"], "delta": 120, "t": 0.9}])
    meter = [{"op": "pointer", "kind": "down", "at": ["meter", 0.0], "t": 0.6}]
    for index, fraction in enumerate((0.25, 0.5, 0.75, 1.0)):
        meter.append({"op": "pointer", "kind": "move", "at": ["meter", fraction],
                      "t": 0.62 + index * 0.06})
    meter.append({"op": "pointer", "kind": "up", "at": ["meter", 1.0], "t": 1.0})
    _sequence(out, "meter", dark, max_h=4000.0, steps=meter)
    _sequence(out, "text-edit", dark, width=576.0, section="listening", steps=[
        {"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
        {"op": "text", "value": " y escucha", "t": 0.7},
        {"op": "key", "vk": 37, "t": 0.72}, {"op": "key", "vk": 37, "t": 0.74},
        {"op": "key", "vk": 8, "t": 0.76}, {"op": "key", "vk": 46, "t": 0.78},
        {"op": "key", "vk": 36, "t": 0.8}, {"op": "key", "vk": 35, "t": 0.82},
        {"op": "key", "vk": 65, "ctrl": True, "t": 0.84},
        {"op": "text", "value": "nuevo", "t": 0.86}, {"op": "key", "vk": 13, "t": 0.9}])
    _sequence(out, "text-cancel", dark, width=576.0, section="listening", steps=[
        {"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
        {"op": "text", "value": "borrado", "t": 0.7},
        {"op": "key", "vk": 27, "t": 0.8}])
    _sequence(out, "text-paste", dark, width=576.0, section="models", steps=[
        {"op": "begin_edit", "key": "stt_dictionary", "t": 0.6},
        {"op": "paste", "value": "open code → OpenCode\ncloud → Claude", "t": 0.7},
        {"op": "key", "vk": 13, "t": 0.8}])
    _sequence(out, "text-invalid", dark, width=576.0, section="models", steps=[
        {"op": "begin_edit", "key": "stt_dictionary", "t": 0.6},
        {"op": "text", "value": "sin flecha", "t": 0.7},
        {"op": "key", "vk": 13, "t": 0.8}, {"op": "key", "vk": 27, "t": 0.9}])
    _sequence(out, "text-outside", dark, width=576.0, section="listening", steps=[
        {"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
        {"op": "text", "value": " fuera", "t": 0.7},
        {"op": "pointer", "kind": "down", "x": 160.0, "y": 200.0, "t": 0.8},
        {"op": "pointer", "kind": "up", "x": 160.0, "y": 200.0, "t": 0.82}])
    _sequence(out, "text-switch", dark, max_h=4000.0, steps=[
        {"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
        {"op": "text", "value": " uno", "t": 0.65},
        {"op": "begin_edit", "key": "llm_model", "t": 0.7},
        {"op": "finish_edit", "commit": False, "t": 0.8}])
    _sequence(out, "secret-copy", dark, width=576.0, section="models",
              values={"api_key": "sk-or-v1-abcdef123456"},
              steps=[*click(["copy", "api_key"], 0.6), {"op": "wait", "t": 2.3},
                     *click(["copy", "api_key"], 2.4), {"op": "wait", "t": 2.5}])
    _sequence(out, "secret-empty", dark, width=576.0, section="models",
              steps=[*click(["copy", "api_key"], 0.6)])
    _sequence(out, "hotkey", dark, width=576.0, section="system", steps=[
        {"op": "begin_edit", "key": "hotkey", "t": 0.6},
        {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7},
        {"op": "key", "vk": 65, "t": 0.8}])
    _sequence(out, "hotkey-cancel", dark, width=576.0, section="system", steps=[
        {"op": "begin_edit", "key": "hotkey", "t": 0.6},
        {"op": "modifiers", "value": [], "t": 0.7}, {"op": "key", "vk": 27, "t": 0.8}])
    _sequence(out, "hotkey-pending", dark, width=576.0, section="system", steps=[
        {"op": "begin_edit", "key": "hotkey", "t": 0.6},
        {"op": "modifiers", "value": ["shift"], "t": 0.7}, {"op": "key", "vk": 16, "t": 0.72},
        {"op": "modifiers", "value": [], "t": 0.8}, {"op": "key", "vk": 116, "t": 0.9}])
    _sequence(out, "hotkey-reject", dark, width=576.0, section="system", steps=[
        {"op": "begin_edit", "key": "hotkey", "t": 0.6},
        {"op": "modifiers", "value": [], "t": 0.7}, {"op": "key", "vk": 65, "t": 0.8},
        {"op": "modifiers", "value": ["ctrl"], "t": 0.9}, {"op": "key", "vk": 27, "t": 1.0}])
    _sequence(out, "hotkey-conflict", dark, width=576.0, section="system", steps=[
        {"op": "begin_edit", "key": "mute_hotkey", "t": 0.6},
        {"op": "modifiers", "value": ["ctrl", "alt"], "t": 0.7},
        {"op": "key", "vk": 32, "t": 0.8}])
    _sequence(out, "action-test", dark, width=576.0, section="voice", steps=[
        *click(["action", "test_voice"], 0.6), *click(["action", "test_voice"], 0.9)])
    _sequence(out, "action-repeat", dark, width=576.0, section="voice",
              steps=[*click(["action", "repeat_last"], 0.6)])
    _sequence(out, "action-armed", dark, width=576.0, section="voice", steps=[
        *click(["action", "new_conversation"], 0.6),
        {"op": "wait", "t": 3.7},
        *click(["action", "new_conversation"], 3.8),
        *click(["action", "new_conversation"], 3.9)])
    _sequence(out, "wheel-scroll", dark, steps=[
        {"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120, "t": 0.6},
        {"op": "wait", "t": 0.8},
        {"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120 * 60, "t": 1.0},
        {"op": "wait", "t": 1.3},
        {"op": "wheel", "x": 160.0, "y": 300.0, "delta": 120 * 60, "t": 1.5}])
    _sequence(out, "wheel-scaled", dark, steps=[
        {"op": "wheel", "x": 160.0, "y": 300.0, "delta": 60, "t": 0.6},
        {"op": "wheel", "x": 160.0, "y": 300.0, "delta": -30, "t": 0.7}])
    _sequence(out, "close-editing", dark, width=576.0, section="listening", steps=[
        {"op": "begin_edit", "key": "wake_phrase", "t": 0.6},
        {"op": "text", "value": " commit", "t": 0.7},
        {"op": "close", "t": 0.8}])
    _sequence(out, "close-slider", dark, max_h=4000.0, steps=[
        {"op": "pointer", "kind": "down", "at": ["slider", "speed", 0.2], "t": 0.6},
        {"op": "pointer", "kind": "move", "at": ["slider", "speed", 0.7], "t": 0.7},
        {"op": "close", "t": 0.8}])
    _sequence(out, "resize", dark, steps=[
        {"op": "resize", "width": 400.0, "height": 300.0, "t": 0.6},
        {"op": "hit", "at": ["save"], "t": 0.7}])
    _sequence(out, "set-settings", dark, steps=[
        {"op": "set_settings", "values": {"speed": 1.2, "theme": "light"}, "t": 0.6}])
    _sequence(out, "testing-fade", dark, steps=[
        {"op": "set_testing", "value": True, "t": 0.6},
        {"op": "set_testing", "value": False, "t": 0.9}])
    _sequence(out, "reduced", dark, prefix=False, steps=[
        {"op": "set_reduced_motion", "value": True, "t": 0.0},
        {"op": "open", "t": 0.1}, {"op": "wait", "t": 0.2}])
    _sequence(out, "available", dark, steps=[
        {"op": "set_available_height", "value": 300.0, "t": 0.6}])
    _sequence(out, "input-level", dark, steps=[
        {"op": "set_input_level", "value": 0.5, "muted": False, "active": True, "t": 0.6},
        {"op": "wait", "t": 0.8},
        {"op": "set_input_level", "value": 0.5, "muted": True, "active": True, "t": 0.9}])
    _sequence(out, "on-applied-no-save", dark, steps=[
        {"op": "on_applied", "value": True, "t": 0.6}])
    _sequence(out, "theme", dark, steps=[
        {"op": "set_theme", "value": "light", "t": 0.6},
        {"op": "set_theme", "value": "auto", "t": 0.7},
        {"op": "set_theme", "value": "fucsia", "t": 0.8}])
    _sequence(out, "palette", dark, width=576.0, section="appearance",
              steps=[*click(["palette", 1], 0.6)])
    _sequence(out, "hit-grid", dark, max_h=4000.0, steps=[
        {"op": "hit", "at": ["row", "speed"], "t": 0.6},
        {"op": "hit", "at": ["slider", "speed", 0.5], "t": 0.6},
        {"op": "hit", "at": ["meter", 0.5], "t": 0.6},
        {"op": "hit", "at": ["hide"], "t": 0.6}, {"op": "hit", "at": ["save"], "t": 0.6},
        {"op": "hit", "at": ["test"], "t": 0.6},
        {"op": "hit", "at": ["action", "repeat_last"], "t": 0.6},
        {"op": "hit", "at": ["theme_preview", 0.9], "t": 0.6},
        {"op": "hit", "at": ["row", "hotkey"], "t": 0.6}])


def detail_cases(out: FixtureFile) -> None:
    """Funciones privadas con valor numérico: texto recortado, alfas y bloques."""
    texts = ("", "Corto", "Escuchando el plan de 19,99 €/mes", "áéíóú ✨ " * 6,
             "Una sola palabra muy larga sin espacios", "  bordes  ")
    for index, text in enumerate(texts):
        for width in (0.0, 40.0, 120.0, 400.0):
            value = menu._fit_text(text, tokens.FS_ROW, width, 1.0)  # noqa: SLF001
            out.add(f"hud_menu/fit/{index:02d}/{int(width)}", "Menu._fit_text",
                    {"text": text, "style": "FS_ROW", "max_width": width, "scale": 1.0},
                    {"value": value})
    rec = Rec()
    widget = _new(_look(DARK), rec)
    _run(widget, rec, [
        {"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
        {"op": "pointer", "kind": "down", "at": ["select", "theme"], "t": 1.2},
        {"op": "pointer", "kind": "up", "at": ["select", "theme"], "t": 1.22},
    ])
    options = widget._selector_options("theme")  # noqa: SLF001
    for ms in (0, 16, 33, 50, 100, 140, 200, 260):
        out.add(f"hud_menu/selector-alpha/{ms:03d}", "Menu._selector_item_alpha",
                {"selector": "theme", "t_ms": ms, "options": [option.value for option in options]},
                {"values": [widget._selector_item_alpha(index, options, 1.22 + ms / 1000.0)
                            for index in range(len(options))]})
    rec = Rec()
    widget = _new(_look(DARK), rec)
    _run(widget, rec, [{"op": "open", "t": 0.0}, {"op": "wait", "t": 1.2},
                       {"op": "wheel", "x": 160.0, "y": 300.0, "delta": -120, "t": 1.2}])
    for ms in (0, 50, 119, 120, 200, 399, 400, 500, 800, 900):
        out.add(f"hud_menu/indicator-alpha/{ms:03d}", "Menu._indicator_alpha",
                {"t_ms_after_wheel": ms, "wheel_t": 1.2},
                {"value": widget._indicator_alpha(1.2 + ms / 1000.0)})
    rec = Rec()
    widget = _new(_look(DARK), rec)
    _run(widget, rec, [{"op": "open", "t": 0.0}])
    for ms in (0, 16, 33, 60, 100, 140, 200, 300, 400):
        widget.update(ms / 1000.0)
        out.add(f"hud_menu/block-values/{ms:03d}", "Menu._block_values",
                {"t_ms_after_open": ms},
                {"blocks": {name: list(widget._block_values(name, ms / 1000.0))
                            for name in ("header", "look", "footer")}})
    _sequence(out, "toggle", _look(DARK), prefix=False,
              steps=[{"op": "toggle", "t": 0.0}, {"op": "wait", "t": 0.4},
                     {"op": "toggle", "t": 0.6}, {"op": "wait", "t": 1.2}])
    _sequence(out, "set-scale", _look(DARK), steps=[
        {"op": "set_scale", "value": 1.5, "t": 0.6},
        {"op": "hit", "at": ["save"], "t": 0.7}])
    try:
        menu.Menu(build_settings({}), scale=0.0)
    except Exception as error:
        out.add_error("hud_menu/error/scale", "Menu.__init__", {"scale": 0.0}, str(error))
    try:
        menu.Menu(None)
    except Exception as error:
        out.add_error("hud_menu/error/settings", "Menu.__init__", {"settings": None}, str(error))
    rec = Rec()
    widget = _new(_look(DARK), rec)
    _run(widget, rec, [{"op": "open", "t": 0.0}])
    try:
        widget.set_scale(0.0)
    except Exception as error:
        out.add_error("hud_menu/error/set-scale", "Menu.set_scale", {"scale": 0.0}, str(error))


def generate(out: FixtureFile) -> None:
    book = Book(out)
    full_view(book)
    sections_view(book)
    advanced_view(book)
    scroll_view(book)
    selector_view(book)
    editing_view(book)
    hover_view(book)
    footer_view(book)
    animations_view(book)
    settings_frames(book)
    book.flush()
    layout_cases(out)
    geometry_cases(out)
    probe_cases(out)
    detail_cases(out)
    sequences(out)
    tokens.set_themed(None)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
