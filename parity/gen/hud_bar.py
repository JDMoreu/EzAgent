"""Golden HUD frames: bar, Marea, orbit and buttons (domain `hud_bar`, F1-10a).

Every frame comes from the real drawing functions of ``ezagent.voice.hud``, in
memory: Pillow/numpy only, no Win32 window, no Tk, no network. Each frame lands
in ``parity/golden/hud/<view>/<state>@<t_ms>.png`` next to ``<state>.json``
(its serializable input), and every frame is also a case in
``parity/fixtures/hud_bar.jsonl`` whose ``output`` names the PNG (path relative
to ``parity/golden``).

The same fixture file carries the numeric animation curves of ``anim`` and
``bar_motion`` (plus the ``marks`` geometry) as floats.

Coverage: themes light/dark/custom, DPI 1.0/1.25/1.5/2.0, pill and circle, every
display state of ``hud/state.py``, every dock, muted and with/without activity in
the orbit. Animations are sampled at 0, 16, 33, 50, 100, 200, 400 ms and settle.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

import numpy as np
from _common import GOLDEN, FixtureFile, dumps, main_guard
from PIL import Image

from ezagent.voice.activity import DONE, FAILED, NEEDS_YOU, STALE, WAITING, WORKING
from ezagent.voice.hud import anim, bar_motion, drop, flow, marks, state, tokens
from ezagent.voice.hud import text as hud_text
from ezagent.voice.hud.theme_accent import derive as derive_themed

DOMAIN = "hud_bar"
OUT = GOLDEN / "hud"
SAMPLES_MS = (0, 16, 33, 50, 100, 200, 400)
DT = 1.0 / 60.0
WARM_S = 1.2
DPIS = (1.0, 1.25, 1.5, 2.0)
THEMES = (("light", "light"), ("dark", "dark"))
CUSTOM_PRESET = "lavanda"

# Codomain of ``state.display_state``: the eight things the bar can show.
DISPLAY_STATES = ("off", *state.BASE_STATES, "mute", "error")
# Fixed voice levels per pose so the fluid has shape without a microphone.
RMS = {"listen": 0.42, "speak": 0.55, "notice": 0.34}
ORBITS = (
    ("work", (WORKING,)),
    ("attention", (NEEDS_YOU, FAILED)),
    ("full", (NEEDS_YOU, FAILED, WORKING, WAITING, STALE)),
)


@dataclass(frozen=True)
class Look:
    """A drawable bar state built from the real HUD inputs (no invented dicts)."""

    mode: str = "light"  # light | dark | custom
    base: str = "light"  # palette branch used by Marea, marks and sprites
    preset: str | None = None
    dpi: float = 1.0
    layout: str = "pill"
    dock: str | None = None
    pose: str = "idle"
    muted: bool = False
    in_call: bool = True
    orbit: tuple[str, ...] = ()
    pulse: float = 1.0
    hover: bool = True
    asking: bool = False
    menu_open: bool = False
    rms: float = 0.0
    hover_item: str | None = None
    warm_s: float = WARM_S


def _custom_colors(look: Look) -> np.ndarray | None:
    """The four user colours of the preset as a Marea palette (or None)."""
    if look.mode != "custom" or look.preset is None:
        return None
    primary, secondary, accent, background = flow.PRESETS[look.preset]
    return flow.custom_palette(primary, secondary, accent, background)


def _activate(look: Look) -> None:
    """Applies the custom palette (or clears it) before a draw."""
    themed = None
    if look.mode == "custom" and look.preset is not None:
        primary, _secondary, _accent, background = flow.PRESETS[look.preset]
        themed = derive_themed(primary, background)
    tokens.set_themed(themed)


def _from_premultiplied(rgba: np.ndarray) -> Image.Image:
    """float RGBA premultiplied 0..1 -> uint8 straight-alpha image."""
    a = rgba[..., 3]
    safe = np.maximum(a, 1e-6)[..., None]
    straight = np.clip(np.where(a[..., None] > 0.0, rgba[..., :3] / safe, 0.0), 0.0, 1.0)
    data = np.empty((*a.shape, 4), np.uint8)
    data[..., :3] = np.round(straight * 255.0).astype(np.uint8)
    data[..., 3] = np.round(np.clip(a, 0.0, 1.0) * 255.0).astype(np.uint8)
    return Image.fromarray(data, "RGBA")


def _straight(pixels: np.ndarray) -> Image.Image:
    """BGRA premultiplied (the render contract) -> straight-alpha RGBA."""
    floats = pixels.astype(np.float32) / 255.0
    return _from_premultiplied(floats[..., [2, 1, 0, 3]])


def _name(look: Look, extra: str = "") -> str:
    theme = look.mode if look.mode != "custom" else f"custom-{look.preset}"
    parts = [
        theme,
        f"{int(round(look.dpi * 100))}",
        look.layout,
        drop.clean_dock(look.layout, look.dock),
        look.pose,
        f"m{int(look.muted)}",
        f"o{len(look.orbit)}",
    ]
    if extra:
        parts.append(extra)
    return "-".join(parts)


def _desc(look: Look) -> dict[str, Any]:
    logical_w, logical_h = drop.layout_size(look.layout, look.dock)
    return {
        "theme": look.mode,
        "base": look.base,
        "preset": look.preset,
        "dpi": look.dpi,
        "layout": look.layout,
        "dock": drop.clean_dock(look.layout, look.dock),
        "state": look.pose,
        "bar_pose": drop.POSE_OF.get(look.pose),
        "form_hidden": look.pose == "off",
        "muted": look.muted,
        "in_call": look.in_call,
        "orbit": list(look.orbit),
        "pulse": look.pulse,
        "hover": look.hover,
        "asking": look.asking,
        "menu_open": look.menu_open,
        "rms": look.rms,
        "window_px": [max(1, round(logical_w * look.dpi)), max(1, round(logical_h * look.dpi))],
    }


class Book:
    """Writes frames, one state JSON per state, and one fixture case per frame."""

    def __init__(self, out: FixtureFile) -> None:
        self.out = out
        self.states: dict[tuple[str, str], dict[str, Any]] = {}
        self.frames: dict[tuple[str, str], list[dict[str, Any]]] = {}

    def frame(
        self, view: str, name: str, desc: dict[str, Any], t_ms: int, img: Image.Image, fn: str
    ) -> None:
        folder = OUT / view
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"{name}@{t_ms}.png"
        img.save(path, format="PNG")
        rel = f"hud/{view}/{path.name}"
        self.frames.setdefault((view, name), []).append({"t_ms": t_ms, "png": rel})
        self.states.setdefault((view, name), {"view": view, "state": name, "input": desc})
        self.out.add(f"hud_bar/{view}/{name}@{t_ms:04d}", fn, desc, {"png": rel})

    def flush(self) -> None:
        total = sum(len(items) for items in self.frames.values())
        if total < 60:  # self-check: the catalogue must stay above the agreed floor
            raise AssertionError(f"hud_bar: solo {total} PNG, se esperaban >= 60")
        for view, name in sorted(self.states):
            entry = {**self.states[(view, name)], "frames": self.frames[(view, name)]}
            path = OUT / view / f"{name}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(dumps(entry) + "\n", encoding="utf-8", newline="\n")


# ----------------------------------------------------------------------- bar
def _bar_step(model: drop.DropModel, look: Look, now: float, dt: float) -> None:
    model.update(
        now,
        dt,
        drop.POSE_OF[look.pose],
        look.rms,
        look.base,
        look.hover,
        look.asking,
        look.menu_open,
        False,
        muted=look.muted,
        hover_item=look.hover_item,
        in_call=look.in_call,
        custom=_custom_colors(look),
    )


def _paper(model: drop.DropModel, look: Look) -> Image.Image:
    """The bar as the real renderer paints it, at this DPI and window size."""
    logical_w, logical_h = drop.layout_size(look.layout, look.dock)
    pixels = drop.render_model(
        model,
        max(1, round(logical_w * look.dpi)),
        max(1, round(logical_h * look.dpi)),
        look.dpi,
        look.base,
        look.muted,
        look.in_call,
        look.menu_open,
        now=0.0,
        orbit=look.orbit,
        pulse=look.pulse,
    )
    return _straight(pixels)


def _bar_still(book: Book, look: Look, extra: str = "") -> None:
    _activate(look)
    model = drop.DropModel(look.layout, look.dock)
    model.form_hidden = look.pose == "off"  # hidden window: the frame is empty
    t = 0.0
    _bar_step(model, look, t, 0.0)
    for _ in range(max(1, round(look.warm_s / DT))):
        t += DT
        _bar_step(model, look, t, DT)
    t_ms = int(round(look.warm_s * 1000))
    book.frame("bar", _name(look, extra), _desc(look), t_ms, _paper(model, look), "render_model")


def _bar_transition(book: Book, tag: str, before: Look, after: Look, final_ms: int) -> None:
    """Warms ``before``, switches to ``after`` at t=0 and samples the settle."""
    _activate(after)
    model = drop.DropModel(after.layout, after.dock)
    for i in range(max(1, round(before.warm_s / DT))):
        _bar_step(model, before, -before.warm_s + i * DT, DT)
    _bar_step(model, after, 0.0, 0.0)
    desc = {**_desc(after), "animation": tag, "samples_ms": [*SAMPLES_MS, final_ms]}
    name = f"anim-{tag}-{_name(after)}"
    previous = 0.0
    for index, t_ms in enumerate((*SAMPLES_MS, final_ms)):
        if index:
            _bar_step(model, after, t_ms / 1000.0, t_ms / 1000.0 - previous)
            previous = t_ms / 1000.0
        book.frame("bar", name, desc, t_ms, _paper(model, after), "render_model")


def _bar_states(book: Book) -> None:
    """Display state x shape x theme, the custom theme and the resting bar."""
    for mode, base in THEMES:
        for layout in drop.LAYOUTS:
            for pose in DISPLAY_STATES:
                _bar_still(
                    book,
                    Look(mode=mode, base=base, layout=layout, pose=pose,
                         muted=pose == "mute", rms=RMS.get(pose, 0.0)),
                )
            for pose in ("idle", "listen", "speak"):
                _bar_still(
                    book,
                    Look(mode=mode, base=base, layout=layout, pose=pose, hover=False,
                         rms=RMS.get(pose, 0.0)),
                    "rest",
                )
    for pose in DISPLAY_STATES:
        _bar_still(
            book,
            Look(mode="custom", base="dark", preset=CUSTOM_PRESET, pose=pose,
                 muted=pose == "mute", rms=RMS.get(pose, 0.0)),
        )


def _bar_variants(book: Book) -> None:
    """Every DPI, every dock, the muted bar and the orbit with activity."""
    for mode, base in THEMES:
        for dpi in DPIS:
            for layout in drop.LAYOUTS:
                _bar_still(book, Look(mode=mode, base=base, dpi=dpi, layout=layout), "dpi")
    for layout, docks in drop.DOCKS.items():
        for dock in docks:
            for mode, base in THEMES:
                _bar_still(book, Look(mode=mode, base=base, layout=layout, dock=dock), "dock")
    for mode, base in THEMES:
        for layout in drop.LAYOUTS:
            _bar_still(book, Look(mode=mode, base=base, layout=layout, muted=True))
            for tag, combo in ORBITS:
                _bar_still(book, Look(mode=mode, base=base, layout=layout, orbit=combo),
                           f"orbit-{tag}")


def bar_view(book: Book) -> None:
    """Full bar frames: states, themes, DPI, docks, mute, orbit and animations."""
    _bar_states(book)
    _bar_variants(book)
    for mode, base in THEMES:  # buttons born, mic muted and hang -> call
        idle = Look(mode=mode, base=base, pose="idle", hover=False)
        _bar_transition(book, "emerge", idle, Look(mode=mode, base=base), 1200)
        mute = Look(mode=mode, base=base, pose="mute", muted=True)
        _bar_transition(book, "mute", idle, mute, 1000)
        _bar_transition(book, "call", idle, Look(mode=mode, base=base, in_call=False), 1000)


# --------------------------------------------------------------------- Marea
def _marea_image(field: flow.FlowField, look: Look) -> Image.Image:
    """One FlowField frame, as the bar's interior paints it, at this DPI."""
    if look.layout == "circle":
        width, height = drop.CIRCLE_D, drop.CIRCLE_D
    else:
        width, height = drop.PILL_W, drop.PILL_H
    nx, ny = max(2, round(width / 2)), max(2, round(height / 2))
    unit = height / 2.0
    xs = (((np.arange(nx) + 0.5) * (width / nx) - width / 2.0) / unit).astype(np.float32)
    ys = (((np.arange(ny) + 0.5) * (height / ny) - height / 2.0) / unit).astype(np.float32)
    gx, gy = np.meshgrid(xs, ys)
    custom = _custom_colors(look)
    colors = (
        custom
        if custom is not None and look.pose in flow.CUSTOM_POSES
        else flow.colors_for(look.base, look.pose)
    )
    arr = np.clip(field.render(gx, gy, look.base, colors), 0.0, 255.0).astype(np.uint8)
    size = (max(1, round(width * look.dpi)), max(1, round(height * look.dpi)))
    return Image.fromarray(arr, "RGB").resize(size, Image.BICUBIC)


def _marea_field(look: Look, t_ms: int = int(WARM_S * 1000)) -> flow.FlowField:
    """A settled Marea field (or a cold one when ``t_ms`` is zero)."""
    _activate(look)
    aspect = 1.0 if look.layout == "circle" else drop.PILL_W / drop.PILL_H
    field = flow.FlowField(look.layout, aspect=aspect)
    for _ in range(round(t_ms / 1000.0 * 30)):
        field.step(1 / 30.0, look.rms, look.pose, False)
    return field


def fluid_view(book: Book) -> None:
    """Marea alone: every pose, both palettes, custom presets and its animation."""
    for mode, base in THEMES:
        for pose in flow.POSES:
            look = Look(mode=mode, base=base, pose=pose, rms=RMS.get(pose, 0.35))
            field = _marea_field(look)
            book.frame("fluid", f"marea-{_name(look, 'pose')}", _desc(look),
                       int(WARM_S * 1000), _marea_image(field, look), "FlowField.render")
    for preset in flow.PRESETS:
        for pose in ("idle", "speak"):
            look = Look(mode="custom", base="dark", preset=preset, pose=pose,
                        rms=RMS.get(pose, 0.35))
            field = _marea_field(look)
            book.frame("fluid", f"marea-custom-{preset}-{pose}", _desc(look),
                       int(WARM_S * 1000), _marea_image(field, look), "FlowField.render")
    # Animation: the fluid starts cold and is driven by a voice (speak).
    for pose in ("idle", "speak"):
        look = Look(mode="dark", base="dark", pose=pose, rms=RMS.get(pose, 0.35))
        _activate(look)
        field = _marea_field(look, 0)
        desc = {**_desc(look), "animation": "flow", "samples_ms": [*SAMPLES_MS, 1000]}
        name = f"anim-flow-{pose}-{_name(look)}"
        previous = 0.0
        for index, t_ms in enumerate((*SAMPLES_MS, 1000)):
            if index:
                field.step(t_ms / 1000.0 - previous, look.rms, pose, False)
                previous = t_ms / 1000.0
            book.frame("fluid", name, desc, t_ms, _marea_image(field, look), "FlowField.render")


# --------------------------------------------------------------------- orbit
def _orbit_frame(book: Book, look: Look, tag: str, states: tuple[str, ...], t_ms: int) -> None:
    _activate(look)
    width, height = ((drop.CIRCLE_D, drop.CIRCLE_D) if look.layout == "circle"
                     else (drop.PILL_W, drop.PILL_H))
    tile = marks.orbit_tile(look.layout, states, width, height, look.dpi, look.base, look.pulse)
    if tile is None:  # no activity: nothing to draw, as the renderer does
        return
    desc = {"theme": look.mode, "base": look.base, "preset": look.preset, "dpi": look.dpi,
            "layout": look.layout, "states": list(states), "pulse": look.pulse,
            "form_px": [round(width), round(height)]}
    book.frame("orbit", f"orbit-{tag}-{_name(look)}", desc, t_ms, tile, "orbit_tile")


def orbit_view(book: Book) -> None:
    """The activity orbit: one mark per state, the full row, custom ink and breath."""
    singles = [(s, (s,)) for s in (WORKING, NEEDS_YOU, FAILED, WAITING, STALE, DONE)]
    for mode, base in THEMES:
        for layout in drop.LAYOUTS:
            for tag, states in (*singles, *ORBITS[2:]):
                _orbit_frame(book, Look(mode=mode, base=base, layout=layout), tag, states, 0)
    for tag, states in singles:
        _orbit_frame(book, Look(mode="custom", base="dark", preset=CUSTOM_PRESET),
                     f"custom-{tag}", states, 0)
    # Breathing of a working mark, quantized the way the renderer does it.
    for mode, base in THEMES:
        for t_ms in (*SAMPLES_MS, 1600):
            look = Look(mode=mode, base=base, pulse=marks.breath(t_ms / 1000.0, False))
            _orbit_frame(book, look, "breath", (WORKING,), t_ms)


# ------------------------------------------------------------------ buttons
def _button_frame(book: Book, glyph: str, look: Look, tag: str, **sprite: Any) -> None:
    _activate(look)
    img = drop.button_sprite(glyph, look.base, look.dpi, **sprite)
    desc = {"glyph": glyph, "theme": look.mode, "base": look.base, "preset": look.preset,
            "dpi": look.dpi, "hang": bool(sprite.get("hang", False)),
            "hover": bool(sprite.get("hover", False)),
            "muted": float(sprite.get("muted", 0.0)),
            "signal": bool(sprite.get("signal", False)),
            "call": bool(sprite.get("call", False))}
    book.frame("buttons", f"btn-{tag}-{_name(look)}", desc, 0, img, "button_sprite")


def buttons_view(book: Book) -> None:
    """Every bar glyph, both themes, two DPIs, and the interactive variants."""
    glyphs = ("gear", "chat", "mic", "hang", "call", "yes")
    for glyph in glyphs:
        for mode, base in THEMES:
            for dpi in (1.0, 2.0):
                _button_frame(book, glyph, Look(mode=mode, base=base, dpi=dpi), glyph)
    for mode, base in THEMES:
        look = Look(mode=mode, base=base)
        for glyph in glyphs:
            _button_frame(book, glyph, look, f"{glyph}-hover", hover=True)
        _button_frame(book, "mic", look, "mic-muted", muted=0.5)
        _button_frame(book, "hang", look, "hang-solid", hang=True)
        _button_frame(book, "call", look, "call-solid", call=True)
        _button_frame(book, "gear", look, "gear-signal", signal=True)
    for glyph in glyphs:
        _button_frame(book, glyph, Look(mode="custom", base="dark", preset=CUSTOM_PRESET),
                      f"{glyph}-custom")


# --------------------------------------------------------------------- text
def _text_frame(book: Book, style_key: str, label: str, base: str, dpi: float) -> None:
    style = tokens.FONT_STYLES[style_key]
    tokens.set_themed(None)
    logical_w, logical_h = hud_text.text_size(label, style, dpi)
    pad = 4
    buf = np.zeros(
        (max(1, math.ceil(logical_h * dpi) + 2 * pad),
         max(1, math.ceil(logical_w * dpi) + 2 * pad), 4),
        np.float32,
    )
    color = tokens.palette(base)["TEXT"]
    hud_text.draw_text(buf, label, style, color, pad, pad, dpi)
    desc = {"text": label, "style": style_key, "family": style.family, "size": style.size,
            "weight": style.weight, "tracking_em": style.tracking_em,
            "uppercase": style.uppercase, "color": list(color), "theme": base, "dpi": dpi}
    name = f"text-{base}-{style_key.lower()}-{int(round(dpi * 100))}"
    book.frame("text", name, desc, 0, _from_premultiplied(buf), "draw_text")


def text_view(book: Book) -> None:
    """The five closed text styles on both themes, plus two extra DPIs of the mono."""
    labels = {
        "FS_TITLE": "EzAgent",
        "FS_ROW": "Escuchando",
        "FS_BODY": "El plan cuesta 19,99 €/mes.",
        "FS_SECONDARY": "Ajustes",
        "FS_MICRO": state.STATE_WORD["listen"],
    }
    for base in ("light", "dark"):
        for style_key, label in labels.items():
            _text_frame(book, style_key, label, base, 1.5)
    for dpi in (1.0, 2.0):
        _text_frame(book, "FS_MICRO", labels["FS_MICRO"], "light", dpi)


# ------------------------------------------------------------------- curvas
CURVES = {
    "out": (anim.EASE_OUT, tokens.BEZIER_OUT),
    "in": (anim.EASE_IN, tokens.BEZIER_IN),
    "inout": (anim.EASE_INOUT, tokens.BEZIER_INOUT),
    "spring": (anim.EASE_SPRING, tokens.BEZIER_SPRING),
}
XS = (0.0, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 1.0)


def _case(out: FixtureFile, case: str, fn: str, data: Any, value: Any) -> None:
    out.add(f"hud_bar/curve/{case}", fn, data, value)


def _ease_cases(out: FixtureFile) -> None:
    for name, (ease, control) in CURVES.items():
        for i, x in enumerate(XS):
            _case(out, f"anim/ease-{name}/{i:02d}", "cubic_bezier",
                  {"control": list(control), "x": x}, {"y": ease(x)})
        tween = anim.Tween(0.0)
        tween.to(1.0, 0.0, 240.0, ease)
        samples = [*SAMPLES_MS, 240]
        _case(out, f"anim/tween-{name}", "Tween.update",
              {"start": 0.0, "target": 1.0, "duration_ms": 240.0, "curve": name,
               "samples_ms": samples},
              {"values": [tween.update(ms / 1000.0) for ms in samples]})
    for i, k in enumerate((0.0, 0.25, 0.5, 0.75, 1.0)):
        _case(out, f"anim/lerp-rgb/{i:02d}", "lerp_rgb",
              {"a": [12, 112, 117], "b": [94, 226, 196], "k": k},
              {"rgb": list(anim.lerp_rgb((12, 112, 117), (94, 226, 196), k))})
    targets = [0.0] * 4 + [0.8] * 8 + [0.2] * 8 + [0.0] * 8
    follower = anim.LevelFollower(tokens.VOICE_ATTACK, tokens.VOICE_RELEASE)
    _case(out, "anim/level-follower", "LevelFollower.step",
          {"attack_ms": tokens.VOICE_ATTACK, "release_ms": tokens.VOICE_RELEASE,
           "dt_ms": 16.0, "targets": targets},
          {"values": [follower.step(target, 16.0) for target in targets]})


def _tween_invert_case(out: FixtureFile) -> None:
    tween = anim.Tween(0.0)
    tween.to(1.0, 0.0, 240.0, anim.EASE_OUT)
    values = [tween.update(ms / 1000.0) for ms in (0, 100, 200)]
    tween.to(0.0, 0.2, 240.0, anim.EASE_OUT)  # inverted mid-flight: no jump
    values += [tween.update(ms / 1000.0) for ms in (200, 216, 250, 320, 440)]
    _case(out, "anim/tween-reverse", "Tween.update",
          {"start": 0.0, "target": 1.0, "invert_ms": 200, "duration_ms": 240.0,
           "samples_ms": [0, 100, 200, 200, 216, 250, 320, 440]}, {"values": values})


def _spring_cases(out: FixtureFile) -> None:
    samples = [*SAMPLES_MS, 1200]
    for name, spring in (("in", bar_motion.SPRING_IN), ("out", bar_motion.SPRING_OUT),
                         ("pointer", bar_motion.SPRING_POINTER)):
        body = bar_motion.Spring(0.0, *spring)
        body.t = 1.0
        previous, values = 0.0, []
        for t_ms in samples:
            values.append(body.step(t_ms / 1000.0 - previous))
            previous = t_ms / 1000.0
        _case(out, f"bar-motion/spring-{name}", "Spring.step",
              {"start": 0.0, "target": 1.0, "k": spring[0], "c": spring[1],
               "samples_ms": samples}, {"values": values})
    fade = bar_motion.Spring(0.0, *bar_motion.SPRING_IN)
    fade.t = 1.0
    fade_ms = (0, 16, 33, 50, 100, 120)
    _case(out, "bar-motion/spring-fade", "Spring.fade",
          {"start": 0.0, "target": 1.0, "samples_ms": list(fade_ms)},
          {"values": [fade.fade(ms / 1000.0) for ms in fade_ms]})
    for i, a in enumerate((0.0, 0.25, 0.5, 0.75, 1.0, 1.04)):
        _case(out, f"bar-motion/emerge/{i:02d}", "emerge",
              {"slot": [220.0, 31.0], "origin": [154.0, 31.0], "a": a, "reduced": False},
              {"value": list(bar_motion.emerge((220.0, 31.0), (154.0, 31.0), a, False))})


def _choreo_case(out: FixtureFile) -> None:
    samples = [*SAMPLES_MS, 1200]
    choreo = bar_motion.Choreo(drop.BUTTONS, shown=False)
    choreo.set_targets({name: True for name in drop.BUTTONS}, drop.emerge_order("pill"), 0.0)
    previous, history = 0.0, []
    for t_ms in samples:
        choreo.step(t_ms / 1000.0, t_ms / 1000.0 - previous, False, grow=True)
        previous = t_ms / 1000.0
        history.append({"t_ms": t_ms, "appear": {n: s.x for n, s in choreo.appear.items()},
                        "grow": choreo.grow.x})
    _case(out, "bar-motion/choreo", "Choreo.step",
          {"buttons": list(drop.BUTTONS), "order": [list(g) for g in drop.emerge_order("pill")],
           "reduced": False, "grow": True}, {"samples": history})


def _marks_cases(out: FixtureFile) -> None:
    for layout, count in (("pill", 1), ("pill", 5), ("circle", 1), ("circle", 5)):
        width, height = ((drop.PILL_W, drop.PILL_H) if layout == "pill"
                         else (drop.CIRCLE_D, drop.CIRCLE_D))
        paths = marks.mark_paths(layout, count, width, height)
        _case(out, f"marks/paths-{layout}-{count}", "mark_paths",
              {"layout": layout, "count": count, "w": width, "h": height},
              {"paths": [[[x, y] for x, y in path] for path in paths]})
    breath_ms = (*SAMPLES_MS, 1600)
    _case(out, "marks/breath", "breath",
          {"samples_ms": list(breath_ms), "reduced": False},
          {"values": [marks.breath(ms / 1000.0, False) for ms in breath_ms]})


def curves(out: FixtureFile) -> None:
    """Numeric tracks of ``anim`` and ``bar_motion`` (floats for the Rust tests)."""
    _ease_cases(out)
    _tween_invert_case(out)
    _spring_cases(out)
    _choreo_case(out)
    _marks_cases(out)


def generate(out: FixtureFile) -> None:
    book = Book(out)
    bar_view(book)
    fluid_view(book)
    orbit_view(book)
    buttons_view(book)
    text_view(book)
    curves(out)
    book.flush()
    tokens.set_themed(None)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
