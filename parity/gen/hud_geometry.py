"""Fixture numérica del oráculo del HUD: tokens, paletas, movimiento y geometría (F1-10c).

Cubre, con los valores exactos que produce el Python (``py-baseline``):

* ``tokens`` (paletas, estilos de fuente, constantes y ``Rect``);
* ``bar_theme`` y ``theme_accent`` (paletas derivadas y contraste WCAG);
* ``a11y`` (luminancia, contraste, informes y validación);
* ``anim`` y ``bar_motion`` (curvas, ``Tween``, ``LevelFollower`` y muelles);
* ``chat_geometry`` (panel, cobertura redondeada, sombra, bordes y resize);
* ``menu_layout`` (filas, grupos, secciones y alto visible);
* el ritmo de ``layered`` (``frame_due``, ``paint_due``, ``wait_delay``, ``paint_mark``).

Nada de dibujo: son números para que las pruebas de Rust comparen exacto.
"""

from __future__ import annotations

from typing import Any

import numpy as np
from _common import FixtureFile, main_guard

from ezagent.voice.hud import (
    a11y,
    anim,
    bar_motion,
    chat_geometry,
    menu_layout,
    theme_accent,
    tokens,
)
from ezagent.voice.hud.bar_theme import body_top, card_overrides, card_palette
from ezagent.voice.hud.bar_theme import contrast as card_contrast
from ezagent.voice.hud.layered import frame_due, paint_due, paint_mark, wait_delay
from ezagent.voice.hud.theme_accent import derive, parse_hex

DOMAIN = "hud_geometry"


def plain(value: Any) -> Any:
    """Convierte numpy y tuplas a algo que ``json.dumps`` serializa igual."""
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
    out.add(f"hud_geometry/{name}", fn, plain(data), plain(value))


def add_error(out: FixtureFile, name: str, fn: str, data: Any, error: Exception) -> None:
    out.add_error(f"hud_geometry/{name}", fn, plain(data), str(error))


# --------------------------------------------------------------------- tokens
def token_cases(out: FixtureFile) -> None:
    tokens.set_themed(None)
    for theme in ("dark", "light", "auto"):
        add(out, f"tokens/palette/{theme}", "tokens.palette", {"theme": theme}, tokens.palette(theme))
    for bad in ("unknown", "", "DARK", "custom"):
        try:
            tokens.palette(bad)
        except ValueError as error:
            add_error(out, f"tokens/palette-error/{bad or 'empty'}", "tokens.palette", {"theme": bad}, error)

    for key, style in tokens.FONT_STYLES.items():
        add(
            out,
            f"tokens/font-style/{key.lower()}",
            "tokens.FontStyle",
            {"token": style.token, "family": style.family, "size": style.size,
             "weight": style.weight, "tracking_em": style.tracking_em,
             "uppercase": style.uppercase},
            {"token": style.token, "family": style.family, "size": style.size,
             "weight": style.weight, "tracking_em": style.tracking_em,
             "uppercase": style.uppercase, "mono": style.mono},
        )
    for theme in ("dark", "light", "auto", "other"):
        add(out, f"tokens/glow/{theme}", "tokens.glow_max", {"theme": theme}, tokens.glow_max(theme))

    constants = {
        "COLOR_KEYS": tokens.COLOR_KEYS,
        "GLOW_MAX_DARK": tokens.GLOW_MAX_DARK,
        "GLOW_MAX_LIGHT": tokens.GLOW_MAX_LIGHT,
        "SP_1": tokens.SP_1,
        "SP_2": tokens.SP_2,
        "SP_3": tokens.SP_3,
        "SP_4": tokens.SP_4,
        "SP_5": tokens.SP_5,
        "SP_6": tokens.SP_6,
        "SP_7": tokens.SP_7,
        "SP_8": tokens.SP_8,
        "R_MENU": tokens.R_MENU,
        "R_GROUP": tokens.R_GROUP,
        "FLARE": tokens.FLARE,
        "W": tokens.W,
        "H_ISLAND": tokens.H_ISLAND,
        "BODY_X0": tokens.BODY_X0,
        "BODY_X1": tokens.BODY_X1,
        "H_MENU": tokens.H_MENU,
        "AA_RAMP": tokens.AA_RAMP,
        "T_MICRO": tokens.T_MICRO,
        "T_FAST": tokens.T_FAST,
        "T_BASE": tokens.T_BASE,
        "T_SLOW": tokens.T_SLOW,
        "T_CLOSE_RETRACT": tokens.T_CLOSE_RETRACT,
        "VOICE_ATTACK": tokens.VOICE_ATTACK,
        "VOICE_RELEASE": tokens.VOICE_RELEASE,
        "FPS_ACTIVE": tokens.FPS_ACTIVE,
        "FPS_IDLE": tokens.FPS_IDLE,
        "BEZIER_OUT": tokens.BEZIER_OUT,
        "BEZIER_IN": tokens.BEZIER_IN,
        "BEZIER_INOUT": tokens.BEZIER_INOUT,
        "BEZIER_SPRING": tokens.BEZIER_SPRING,
        "ICON_GRID": tokens.ICON_GRID,
        "ICON_STROKE": tokens.ICON_STROKE,
        "ICON_STROKE_SMALL": tokens.ICON_STROKE_SMALL,
        "ICON_SMALL_THRESHOLD": tokens.ICON_SMALL_THRESHOLD,
        "ICON_SSAA": tokens.ICON_SSAA,
    }
    add(out, "tokens/constants", "tokens.constants", {}, constants)

    # Tema personalizado: sobrescrituras por tema y su versión.
    version = tokens.palette_version()
    overrides = {
        "light": {"MINT": (1, 2, 3), "LINE": (4, 5, 6, 7)},
        "dark": {"MINT": (8, 9, 10)},
    }
    tokens.set_themed(overrides)
    light = tokens.palette("light")
    dark = tokens.palette("dark")
    after = tokens.palette_version()
    tokens.set_themed(overrides)
    same = tokens.palette_version()
    tokens.set_themed(None)
    reset = tokens.palette_version()
    add(
        out,
        "tokens/themed",
        "tokens.set_themed",
        {"overrides": overrides},
        {
            "version_before": version,
            "version_after": after,
            "version_same": same,
            "version_reset": reset,
            "light": {"MINT": light["MINT"], "LINE": light["LINE"]},
            "dark": {"MINT": dark["MINT"], "LINE": dark["LINE"]},
        },
    )

    rects = [
        (0.0, 0.0, 10.0, 4.0),
        (-2.5, 1.25, 8.75, 9.5),
        (3.0, 3.0, 3.0, 3.0),
        (10.0, 0.0, 0.0, 4.0),
        (0.125, 0.5, 100.875, 31.25),
    ]
    for index, values in enumerate(rects, start=1):
        rect = tokens.Rect(*values)
        add(
            out,
            f"tokens/rect/{index:02d}",
            "tokens.Rect",
            {"rect": values},
            {"width": rect.width, "height": rect.height, "center": rect.center},
        )
    for index, (a, b) in enumerate(
        [
            ((0.0, 0.0, 10.0, 10.0), (5.0, 5.0, 15.0, 15.0)),
            ((0.0, 0.0, 10.0, 10.0), (10.0, 0.0, 20.0, 10.0)),
            ((0.0, 0.0, 10.0, 10.0), (1.0, 1.0, 2.0, 2.0)),
            ((0.0, 0.0, 10.0, 10.0), (-5.0, -5.0, 0.0, 0.0)),
            ((0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 8.0, 8.0)),
        ],
        start=1,
    ):
        add(
            out,
            f"tokens/rect-overlaps/{index:02d}",
            "tokens.Rect.overlaps",
            {"a": a, "b": b},
            {"overlaps": tokens.Rect(*a).overlaps(tokens.Rect(*b))},
        )


# ----------------------------------------------------------------- bar_theme
def theme_cases(out: FixtureFile) -> None:
    for index, theme in enumerate(("light", "dark", "auto", "custom", ""), start=1):
        add(
            out,
            f"bar-theme/card/{index:02d}",
            "bar_theme.card_palette",
            {"theme": theme},
            {"overrides": card_overrides(theme), "palette": card_palette(theme),
             "body_top": body_top(theme)},
        )
    pairs = [
        ((228, 242, 240), (13, 22, 26)),
        ((16, 34, 38), (250, 253, 252)),
        ((12, 112, 117), (250, 253, 252)),
        ((94, 226, 196), (13, 22, 26)),
        ((0, 0, 0), (255, 255, 255)),
        ((255, 255, 255), (255, 255, 255)),
        ((1, 1, 1), (254, 254, 254)),
        ((200, 36, 26, 128), (237, 246, 244)),
        ((94, 226, 196), (20, 33, 38)),
    ]
    for index, (a, b) in enumerate(pairs, start=1):
        add(
            out,
            f"bar-theme/contrast/{index:02d}",
            "bar_theme.contrast",
            {"a": a, "b": b},
            {"ratio": card_contrast(a, b)},
        )


# -------------------------------------------------------------- theme_accent
def accent_cases(out: FixtureFile) -> None:
    values = [
        "#0E6F7C", "0E6F7C", "  #ff8a65  ", "#FFF", "FFFFFFF", "#GG0000",
        "", None, 123, "#0e6f7ç", "  ", "#000000", "#ffffff",
    ]
    for index, value in enumerate(values, start=1):
        add(
            out,
            f"theme-accent/parse-hex/{index:02d}",
            "theme_accent.parse_hex",
            {"value": value},
            {"rgb": parse_hex(value)},  # type: ignore[arg-type]
        )
    presets = [
        ("marea", "#0E6F7C", "#1FAE9F"),
        ("atardecer", "#5B1A3A", "#E0564F"),
        ("bosque", "#13361F", "#2E7D4F"),
        ("lavanda", "#2E2A6B", "#7A6FF0"),
        ("medianoche", "#0B1026", "#2D3A8C"),
        ("custom-1", "#123456", "#abcdef"),
        ("custom-2", "#FF0000", "#00FF00"),
        ("custom-3", "#777777", "#EEEEEE"),
        ("white", "#FFFFFF", "#FFFFFF"),
        ("black", "#000000", "#000000"),
    ]
    for index, (name, primary, background) in enumerate(presets, start=1):
        add(
            out,
            f"theme-accent/derive/{index:02d}-{name}",
            "theme_accent.derive",
            {"primary": primary, "background": background},
            derive(primary, background),
        )
    for index, (primary, background) in enumerate(
        [("", "#000000"), ("#12345", "#000000"), ("#123456", ""), (None, "#000000")],
        start=1,
    ):
        add(
            out,
            f"theme-accent/derive-error/{index:02d}",
            "theme_accent.derive",
            {"primary": primary, "background": background},
            derive(primary, background),  # type: ignore[arg-type]
        )
    add(
        out,
        "theme-accent/constants",
        "theme_accent.constants",
        {},
        {
            "SURFACE_KEYS": list(theme_accent.SURFACE_KEYS),
            "LINE_KEYS": list(theme_accent.LINE_KEYS),
            "ACCENT_KEYS": list(theme_accent.ACCENT_KEYS),
            "MIN_CONTRAST": theme_accent.MIN_CONTRAST,
            "DARK_ACCENT_CONTRAST": theme_accent.DARK_ACCENT_CONTRAST,
            "ACCENT_START": theme_accent.ACCENT_START,
            "BODY_TOP": theme_accent.BODY_TOP,
        },
    )


# ---------------------------------------------------------------------- a11y
def check_dict(check: a11y.ContrastCheck) -> dict[str, Any]:
    return {
        "theme": check.theme,
        "foreground": check.foreground,
        "background": check.background,
        "kind": check.kind,
        "minimum": check.minimum,
        "ratio": check.ratio,
        "advisory": check.advisory,
        "passed": check.passed,
    }


def a11y_cases(out: FixtureFile) -> None:
    colors = [
        (0, 0, 0), (255, 255, 255), (13, 22, 26), (228, 242, 240),
        (12, 112, 117), (94, 226, 196), (128, 128, 128), (255, 0, 0),
        (0, 0, 255), (250, 253, 252), (20, 33, 38), (1, 2, 3, 4),
    ]
    for index, color in enumerate(colors, start=1):
        add(
            out,
            f"a11y/luminance/{index:02d}",
            "a11y.luminance",
            {"rgb": color},
            {"value": a11y.luminance(color)},
        )
    pairs = [
        ((228, 242, 240), (13, 22, 26)),
        ((13, 22, 26), (228, 242, 240)),
        ((12, 112, 117), (250, 253, 252)),
        ((94, 226, 196), (13, 22, 26)),
        ((0, 0, 0), (255, 255, 255)),
        ((255, 255, 255), (255, 255, 255)),
        ((16, 34, 38), (250, 253, 252)),
        ((78, 102, 106), (199, 224, 219)),
        ((255, 93, 82), (13, 22, 26)),
        ((163, 95, 0), (250, 253, 252)),
    ]
    for index, (a, b) in enumerate(pairs, start=1):
        add(
            out,
            f"a11y/contrast/{index:02d}",
            "a11y.contrast",
            {"a": a, "b": b},
            {"ratio": a11y.contrast(a, b)},
        )
    for theme in ("dark", "light"):
        for advisory in (False, True):
            report = a11y.contrast_report(theme, include_advisory=advisory)
            add(
                out,
                f"a11y/report/{theme}-{'yes' if advisory else 'no'}",
                "a11y.contrast_report",
                {"theme": theme, "include_advisory": advisory},
                [check_dict(check) for check in report],
            )
            add(
                out,
                f"a11y/failures/{theme}-{'yes' if advisory else 'no'}",
                "a11y.failures",
                {"theme": theme, "include_advisory": advisory},
                [check_dict(check) for check in a11y.failures(theme, include_advisory=advisory)],
            )
        add(
            out,
            f"a11y/validate/{theme}",
            "a11y.validate_palette",
            {"theme": theme},
            {"valid": a11y.validate_palette(theme)},
        )
    add(
        out,
        "a11y/validate/all",
        "a11y.validate_all_palettes",
        {},
        a11y.validate_all_palettes(),
    )
    add(
        out,
        "a11y/constants",
        "a11y.constants",
        {},
        {"AA_TEXT": a11y.AA_TEXT, "AA_GRAPHIC": a11y.AA_GRAPHIC},
    )


# ----------------------------------------------------------------------- anim
CURVES = {
    "out": (anim.EASE_OUT, tokens.BEZIER_OUT),
    "in": (anim.EASE_IN, tokens.BEZIER_IN),
    "inout": (anim.EASE_INOUT, tokens.BEZIER_INOUT),
    "spring": (anim.EASE_SPRING, tokens.BEZIER_SPRING),
}
XS = (-0.5, 0.0, 0.05, 0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 0.9, 0.95, 1.0, 1.5)


def anim_cases(out: FixtureFile) -> None:
    for name, (ease, control) in CURVES.items():
        for index, x in enumerate(XS):
            add(
                out,
                f"anim/cubic-bezier/{name}/{index:02d}",
                "anim.cubic_bezier",
                {"control": list(control), "x": x},
                {"y": ease(x)},
            )
    samples = (0.0, 0.05, 0.1, 0.16, 0.2, 0.24, 0.3, 0.5, 1.0)
    for name, (ease, _control) in CURVES.items():
        tween = anim.Tween(0.0)
        tween.to(1.0, 0.0, 240.0, ease)
        add(
            out,
            f"anim/tween/{name}",
            "anim.Tween.update",
            {"start": 0.0, "target": 1.0, "duration_ms": 240.0, "samples_s": list(samples),
             "curve": name},
            {"values": [tween.update(s) for s in samples],
             "active_after": tween.active, "value_after": tween.value},
        )
    cases = [
        ("invert", [0.0, 0.1, 0.2, 0.216, 0.25, 0.32, 0.44], 0.2),
        ("zero-duration", [0.0, 0.1], 0.0),
        ("negative", [-1.0, 0.0, 0.1], 0.1),
        ("long", [0.0, 1.0, 2.0, 3.0], 1.5),
    ]
    for name, times, at in cases:
        tween = anim.Tween(0.0)
        tween.to(1.0, at, 240.0, anim.EASE_OUT)
        values = [tween.update(s) for s in times]
        if name == "invert":
            tween.to(0.0, 0.2, 240.0, anim.EASE_OUT)
            values += [tween.update(s) for s in (0.2, 0.216, 0.25, 0.32, 0.44)]
        add(
            out,
            f"anim/tween-sequence/{name}",
            "anim.Tween.update",
            {"invert_at": at if name == "invert" else None, "times_s": times},
            {"values": values, "active": tween.active},
        )
    targets = [0.0] * 3 + [0.8] * 6 + [0.2] * 6 + [0.0] * 3
    follower = anim.LevelFollower(tokens.VOICE_ATTACK, tokens.VOICE_RELEASE)
    add(
        out,
        "anim/level-follower/normal",
        "anim.LevelFollower.step",
        {"attack_ms": tokens.VOICE_ATTACK, "release_ms": tokens.VOICE_RELEASE,
         "dt_ms": 16.0, "targets": targets},
        {"values": [follower.step(target, 16.0) for target in targets]},
    )
    for name, attack, release, dts in (
        ("fast", 10.0, 20.0, [1.0, 2.0, 4.0, 8.0]),
        ("zero-dt", 45.0, 200.0, [0.0, 0.0, 1.0]),
        ("negative-dt", 45.0, 200.0, [-5.0, 0.0, 5.0]),
    ):
        follower = anim.LevelFollower(attack, release)
        values = [follower.step(0.6, dt) for dt in dts]
        add(
            out,
            f"anim/level-follower/{name}",
            "anim.LevelFollower.step",
            {"attack_ms": attack, "release_ms": release, "dt_ms": dts},
            {"values": values},
        )
    pairs = [
        ((12, 112, 117), (94, 226, 196)),
        ((0, 0, 0), (255, 255, 255)),
        ((200, 36, 26), (13, 22, 26)),
    ]
    for index, (a, b) in enumerate(pairs):
        for k in (0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0, 1.2, -0.2):
            label = f"{k:g}".replace("-", "neg").replace(".", "_")
            add(
                out,
                f"anim/lerp-rgb/{index + 1:02d}-{label}",
                "anim.lerp_rgb",
                {"a": a, "b": b, "k": k},
                {"rgb": anim.lerp_rgb(a, b, k)},
            )
    add(
        out,
        "anim/constants",
        "anim.constants",
        {},
        {"EASE_CONTROLS": {name: list(control) for name, (_ease, control) in CURVES.items()}},
    )


# ----------------------------------------------------------------- bar_motion
def motion_cases(out: FixtureFile) -> None:
    add(
        out,
        "bar-motion/constants",
        "bar_motion.constants",
        {},
        {
            "STAGGER_IN": bar_motion.STAGGER_IN,
            "STAGGER_OUT": bar_motion.STAGGER_OUT,
            "SPRING_IN": bar_motion.SPRING_IN,
            "SPRING_OUT": bar_motion.SPRING_OUT,
            "SPRING_POINTER": bar_motion.SPRING_POINTER,
            "SPRING_GROW": bar_motion.SPRING_GROW,
            "FORM_GROW": bar_motion.FORM_GROW,
            "HOVER_SCALE": bar_motion.HOVER_SCALE,
            "PRESS_SCALE": bar_motion.PRESS_SCALE,
            "BORN_SCALE": bar_motion.BORN_SCALE,
            "REDUCED_FADE": bar_motion.REDUCED_FADE,
            "SETTLE_EPS": bar_motion.SETTLE_EPS,
        },
    )
    spring_settings = [
        ("in", bar_motion.SPRING_IN),
        ("out", bar_motion.SPRING_OUT),
        ("pointer", bar_motion.SPRING_POINTER),
        ("grow", bar_motion.SPRING_GROW),
        ("custom", (100.0, 4.0)),
        ("critical", (400.0, 40.0)),
        ("overdamped", (100.0, 40.0)),
        ("underdamped", (900.0, 10.0)),
    ]
    dts = (0.0, 0.016, 0.033, 0.05, 0.1, 0.2, 0.4)
    for name, (k, c) in spring_settings:
        spring = bar_motion.Spring(0.0, k, c)
        spring.t = 1.0
        values = [spring.step(dt) for dt in dts]
        add(
            out,
            f"bar-motion/spring/{name}",
            "bar_motion.Spring.step",
            {"start": 0.0, "target": 1.0, "k": k, "c": c, "dts": list(dts)},
            {"values": values, "x": spring.x, "v": spring.v, "settled": spring.settled()},
        )
        nspring = bar_motion.Spring(0.0, k, c)
        nspring.t = 1.0
        add(
            out,
            f"bar-motion/spring-negative/{name}",
            "bar_motion.Spring.step",
            {"start": 0.0, "target": 1.0, "k": k, "c": c, "dts": [-0.1, 0.0, 0.016]},
            {"values": [nspring.step(dt) for dt in (-0.1, 0.0, 0.016)]},
        )
    spring = bar_motion.Spring(1.0, *bar_motion.SPRING_IN)
    spring.t = 0.0
    instant = spring.step(0.5, instant=True)
    add(
        out,
        "bar-motion/spring/instant",
        "bar_motion.Spring.step",
        {"start": 1.0, "target": 0.0, "instant": True},
        {"value": instant, "x": spring.x, "v": spring.v},
    )
    for index, (x, target, seconds) in enumerate(
        [(0.0, 1.0, 0.12), (1.0, 0.0, 0.12), (0.5, 0.5, 0.12), (0.0, 1.0, 0.0),
         (0.0, 1.0, -1.0)]
    ):
        spring = bar_motion.Spring(x, *bar_motion.SPRING_IN)
        spring.t = target
        values = [spring.fade(dt, seconds) for dt in (0.0, 0.016, 0.05, 0.11, 0.2)]
        add(
            out,
            f"bar-motion/spring-fade/{index + 1:02d}",
            "bar_motion.Spring.fade",
            {"x": x, "target": target, "seconds": seconds,
             "dts": [0.0, 0.016, 0.05, 0.11, 0.2]},
            {"values": values, "x": spring.x, "v": spring.v, "settled": spring.settled()},
        )
    for index, a in enumerate((0.0, 0.1, 0.25, 0.5, 0.75, 1.0, 1.04), start=1):
        add(
            out,
            f"bar-motion/emerge/{index:02d}",
            "bar_motion.emerge",
            {"slot": (220.0, 31.0), "origin": (154.0, 31.0), "a": a, "reduced": False},
            {"value": bar_motion.emerge((220.0, 31.0), (154.0, 31.0), a, False)},
        )
        add(
            out,
            f"bar-motion/emerge-reduced/{index:02d}",
            "bar_motion.emerge",
            {"slot": (220.0, 31.0), "origin": (154.0, 31.0), "a": a, "reduced": True},
            {"value": bar_motion.emerge((220.0, 31.0), (154.0, 31.0), a, True)},
        )
    choreo_scenarios = [
        ("show", False, {name: True for name in ("gear", "chat", "mic", "hang")}, False),
        ("hide", True, {name: False for name in ("gear", "chat", "mic", "hang")}, False),
        ("partial", True, {"gear": True, "chat": True, "mic": False, "hang": True}, False),
        ("reduced", False, {name: True for name in ("gear", "chat", "mic", "hang")}, True),
    ]
    order = (("chat", "mic"), ("gear", "hang"))
    for name, shown, want, reduced in choreo_scenarios:
        choreo = bar_motion.Choreo(("gear", "chat", "mic", "hang"), shown=shown)
        choreo.set_targets(want, order, 0.0, reduced)
        history = []
        for t_ms in (0, 16, 33, 50, 80, 120, 200, 400, 800):
            choreo.step(t_ms / 1000.0, t_ms / 1000.0 - (history[-1]["t_ms"] / 1000.0 if history else 0.0),
                        reduced, grow=True)
            history.append({
                "t_ms": t_ms,
                "appear": {key: spring.x for key, spring in choreo.appear.items()},
                "pointer": {key: spring.x for key, spring in choreo.pointer.items()},
                "grow": choreo.grow.x,
                "pending": choreo.pending(t_ms / 1000.0),
                "animating": choreo.animating(t_ms / 1000.0),
            })
        add(
            out,
            f"bar-motion/choreo/{name}",
            "bar_motion.Choreo.step",
            {"buttons": ["gear", "chat", "mic", "hang"], "shown": shown, "want": want,
             "order": [list(group) for group in order], "reduced": reduced},
            {"samples": history},
        )


# --------------------------------------------------------------- chat_geometry
def chat_geometry_cases(out: FixtureFile) -> None:
    add(
        out,
        "chat-geometry/constants",
        "chat_geometry.constants",
        {},
        {
            "PANEL_W": chat_geometry.PANEL_W,
            "PANEL_H": chat_geometry.PANEL_H,
            "MIN_W": chat_geometry.MIN_W,
            "MIN_H": chat_geometry.MIN_H,
            "MARGIN": chat_geometry.MARGIN,
            "SCREEN_PAD": chat_geometry.SCREEN_PAD,
            "PANEL_RADIUS": chat_geometry.PANEL_RADIUS,
            "GRIP": chat_geometry.GRIP,
            "MORPH_SECS": chat_geometry.MORPH_SECS,
            "FADE_SECS": chat_geometry.FADE_SECS,
            "IDC_SIZEWE": chat_geometry.IDC_SIZEWE,
            "IDC_SIZENS": chat_geometry.IDC_SIZENS,
            "IDC_SIZENWSE": chat_geometry.IDC_SIZENWSE,
            "IDC_SIZENESW": chat_geometry.IDC_SIZENESW,
        },
    )
    for index, x in enumerate((-1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0), start=1):
        add(out, f"chat-geometry/smoothstep/{index:02d}", "chat_geometry.smoothstep",
            {"x": x}, {"y": chat_geometry.smoothstep(x)})
        add(out, f"chat-geometry/ease/{index:02d}", "chat_geometry.ease",
            {"x": x}, {"y": chat_geometry.ease(x)})
        for overshoot in (0.0, 0.5, 0.75, 1.2):
            add(
                out,
                f"chat-geometry/ease-out-back/{index:02d}-{str(overshoot).replace('.', '_')}",
                "chat_geometry.ease_out_back",
                {"x": x, "overshoot": overshoot},
                {"y": chat_geometry.ease_out_back(x, overshoot)},
            )
    for index, (a, b, k) in enumerate(
        [(0.0, 1.0, 0.0), (0.0, 1.0, 0.35), (10.0, -10.0, 0.5), (440.0, 340.0, 1.0),
         (1.0, 2.0, -0.5)],
        start=1,
    ):
        add(out, f"chat-geometry/lerp/{index:02d}", "chat_geometry.lerp",
            {"a": a, "b": b, "k": k}, {"value": chat_geometry.lerp(a, b, k)})
    rect_cases = [
        ((0.0, 0.0, 440.0, 640.0), (16.0, 40.0, 456.0, 680.0), 0.0),
        ((0.0, 0.0, 440.0, 640.0), (16.0, 40.0, 456.0, 680.0), 0.5),
        ((0.0, 0.0, 440.0, 640.0), (16.0, 40.0, 456.0, 680.0), 1.0),
        ((100.0, 200.0, 208.0, 238.0), (736.0, 236.0, 1176.0, 876.0), 0.28),
        ((-10.5, 2.25, 10.5, 30.75), (0.0, 0.0, 0.0, 0.0), 0.75),
    ]
    for index, (a, b, k) in enumerate(rect_cases, start=1):
        add(
            out,
            f"chat-geometry/lerp-rect/{index:02d}",
            "chat_geometry.lerp_rect",
            {"a": a, "b": b, "k": k},
            {"rect": chat_geometry.lerp_rect(a, b, k)},
        )
    works = [
        ((0, 0, 1920, 1080), 1.0),
        ((0, 0, 1920, 1080), 1.25),
        ((0, 0, 1366, 768), 1.0),
        ((0, 0, 800, 600), 2.0),
        ((-1920, 0, 0, 1080), 1.5),
        ((0, 0, 300, 300), 1.0),
    ]
    for index, (work, scale) in enumerate(works, start=1):
        add(out, f"chat-geometry/panel-limits/{index:02d}", "chat_geometry.panel_limits",
            {"work": work, "scale": scale}, {"size": chat_geometry.panel_limits(work, scale)})
    saved = [None, (440.0, 640.0), (340.0, 380.0), (200.0, 200.0), (5000.0, 5000.0),
             (800.0, 900.0), (440.5, 640.5)]
    for index, value in enumerate(saved, start=1):
        for scale in (1.0, 1.5):
            add(
                out,
                f"chat-geometry/panel-size/{index:02d}-{str(scale).replace('.', '_')}",
                "chat_geometry.panel_size",
                {"work": (0, 0, 1920, 1080), "scale": scale, "saved": value},
                {"size": chat_geometry.panel_size((0, 0, 1920, 1080), scale, value)},
            )
    for index, (form, work, scale, value) in enumerate(
        [
            ((906.0, 981.0, 1014.0, 1019.0), (0, 0, 1920, 1080), 1.0, None),
            ((906.0, 981.0, 1014.0, 1019.0), (0, 0, 1920, 1080), 1.25, None),
            ((906.0, 981.0, 1014.0, 1019.0), (0, 0, 1920, 1080), 2.0, (400.0, 500.0)),
            ((906.0, 20.0, 1014.0, 58.0), (0, 0, 1920, 1080), 1.0, None),
            ((-10.0, 500.0, 98.0, 538.0), (0, 0, 1920, 1080), 1.0, None),
            ((1800.0, 500.0, 1908.0, 538.0), (0, 0, 1920, 1080), 1.0, None),
            ((400.0, 400.0, 508.0, 438.0), (0, 0, 800, 600), 2.0, (340.0, 380.0)),
            ((0.0, 0.0, 108.0, 38.0), (0, 0, 300, 300), 1.0, (5000.0, 5000.0)),
        ],
        start=1,
    ):
        add(
            out,
            f"chat-geometry/panel-rect/{index:02d}",
            "chat_geometry.panel_rect",
            {"form": form, "work": work, "scale": scale, "saved": value},
            {"rect": chat_geometry.panel_rect(form, work, scale, value)},
        )
    coverage_specs = [
        (8, 6, 3.0, 0.0), (8, 6, 0.0, 0.0), (6, 8, 10.0, 0.0), (10, 10, 2.5, 1.0),
        (2, 2, 1.0, 0.0), (1, 1, 0.0, 0.0),
    ]
    for index, (w, h, radius, inset) in enumerate(coverage_specs, start=1):
        coverage = chat_geometry.rounded_coverage(w, h, radius, inset)
        add(
            out,
            f"chat-geometry/rounded-coverage/small-{index:02d}",
            "chat_geometry.rounded_coverage",
            {"w": w, "h": h, "radius": radius, "inset": inset},
            {"shape": list(coverage.shape), "values": coverage},
        )
    for index, (w, h, radius, inset) in enumerate(
        [(440, 640, 22.0, 0.0), (440, 640, 22.0, 1.0), (108, 38, 19.0, 0.0),
         (58, 58, 29.0, 0.0)], start=1
    ):
        coverage = chat_geometry.rounded_coverage(w, h, radius, inset)
        samples = []
        for x, y in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (w // 2, 0),
                     (w // 2, h // 2), (w - 1, h // 2), (0, h // 2)):
            samples.append([x, y, float(coverage[y, x])])
        add(
            out,
            f"chat-geometry/rounded-coverage/large-{index:02d}",
            "chat_geometry.rounded_coverage",
            {"w": w, "h": h, "radius": radius, "inset": inset},
            {"shape": list(coverage.shape), "samples": samples,
             "center": float(coverage[h // 2, w // 2])},
        )
    for index, (w, h, radius, reach) in enumerate(
        [(20, 16, 4.0, 1), (20, 16, 4.0, 5), (10, 10, 2.0, 3), (2, 2, 0.0, 1)],
        start=1,
    ):
        shadow = chat_geometry.rounded_shadow(w, h, radius, reach)
        samples = []
        hh, ww = shadow.shape
        for x, y in ((0, 0), (ww - 1, 0), (0, hh - 1), (ww - 1, hh - 1),
                     (ww // 2, hh // 2), (ww // 2, 0)):
            samples.append([x, y, float(shadow[y, x])])
        add(
            out,
            f"chat-geometry/rounded-shadow/{index:02d}",
            "chat_geometry.rounded_shadow",
            {"w": w, "h": h, "radius": radius, "reach": reach},
            {"shape": list(shadow.shape), "samples": samples},
        )
    for index, (x, y, w, h, grip) in enumerate(
        [(0.0, 0.0, 440.0, 640.0, 8.0), (4.0, 4.0, 440.0, 640.0, 8.0),
         (436.0, 4.0, 440.0, 640.0, 8.0), (436.0, 636.0, 440.0, 640.0, 8.0),
         (220.0, 320.0, 440.0, 640.0, 8.0), (-1.0, 0.0, 440.0, 640.0, 8.0),
         (441.0, 0.0, 440.0, 640.0, 8.0), (0.0, -1.0, 440.0, 640.0, 8.0),
         (0.0, 641.0, 440.0, 640.0, 8.0), (20.0, 20.0, 440.0, 640.0, 20.0),
         (10.0, 10.0, 440.0, 640.0, 20.0), (430.0, 630.0, 440.0, 640.0, 20.0)],
        start=1,
    ):
        add(
            out,
            f"chat-geometry/edges-at/{index:02d}",
            "chat_geometry.edges_at",
            {"x": x, "y": y, "w": w, "h": h, "grip": grip},
            {"edges": chat_geometry.edges_at(x, y, w, h, grip)},
        )
    for index in range(16):
        edges = tuple(bool(index & (1 << bit)) for bit in range(4))
        add(
            out,
            f"chat-geometry/edge-cursor/{index:02d}",
            "chat_geometry.edge_cursor",
            {"edges": list(edges)},
            {"cursor": chat_geometry.edge_cursor(edges)},  # type: ignore[arg-type]
        )
    resize_cases = [
        ((True, False, False, False), (100.0, 100.0, 540.0, 740.0), 20.0, 0.0),
        ((True, False, False, False), (100.0, 100.0, 540.0, 740.0), -100.0, 0.0),
        ((False, False, True, False), (100.0, 100.0, 540.0, 740.0), 100.0, 0.0),
        ((False, True, False, False), (100.0, 100.0, 540.0, 740.0), 0.0, -80.0),
        ((False, False, False, True), (100.0, 100.0, 540.0, 740.0), 0.0, 80.0),
        ((True, True, False, False), (100.0, 100.0, 540.0, 740.0), 25.0, 25.0),
        ((False, False, True, True), (100.0, 100.0, 540.0, 740.0), 30.0, 30.0),
        ((True, True, True, True), (100.0, 100.0, 540.0, 740.0), 10.0, 10.0),
        ((False, False, True, False), (0.0, 0.0, 440.0, 640.0), 5000.0, 0.0),
        ((True, False, False, False), (0.0, 0.0, 440.0, 640.0), -5000.0, 0.0),
    ]
    for index, (edges, rect, dx, dy) in enumerate(resize_cases, start=1):
        add(
            out,
            f"chat-geometry/resize-rect/{index:02d}",
            "chat_geometry.resize_rect",
            {"edges": list(edges), "rect": rect, "dx": dx, "dy": dy,
             "work": (0, 0, 1920, 1080), "scale": 1.0},
            {"rect": chat_geometry.resize_rect(edges, rect, dx, dy, (0, 0, 1920, 1080), 1.0)},
        )
        add(
            out,
            f"chat-geometry/resize-rect-hidpi/{index:02d}",
            "chat_geometry.resize_rect",
            {"edges": list(edges), "rect": rect, "dx": dx, "dy": dy,
             "work": (0, 0, 1920, 1080), "scale": 2.0},
            {"rect": chat_geometry.resize_rect(edges, rect, dx, dy, (0, 0, 1920, 1080), 2.0)},
        )


# ---------------------------------------------------------------- menu_layout
def menu_cases(out: FixtureFile) -> None:
    add(
        out,
        "menu-layout/constants",
        "menu_layout.constants",
        {},
        {
            "ISLAND_H": menu_layout.ISLAND_H,
            "PAD_TOP": menu_layout.PAD_TOP,
            "HEADER": menu_layout.HEADER,
            "GAP_HEADER": menu_layout.GAP_HEADER,
            "LABEL": menu_layout.LABEL,
            "GROUP_GAP": menu_layout.GROUP_GAP,
            "FOOTER": menu_layout.FOOTER,
            "PAD_BOTTOM": menu_layout.PAD_BOTTOM,
            "ROW": menu_layout.ROW,
            "GROUPS": menu_layout.GROUPS,
            "CUSTOM_THEME_ROWS": sorted(menu_layout.CUSTOM_THEME_ROWS),
            "ADVANCED_ROWS": sorted(menu_layout.ADVANCED_ROWS),
            "SECTIONS": menu_layout.SECTIONS,
            "SIDEBAR_ORDER": menu_layout.SIDEBAR_ORDER,
            "SECTION_TITLES": menu_layout.SECTION_TITLES,
            "SECTION_GROUPS": menu_layout.SECTION_GROUPS,
            "GROUP_LABELS": menu_layout.GROUP_LABELS,
            "MENU_HEIGHT": menu_layout.MENU_HEIGHT,
        },
    )
    layouts = [
        ("default", {}, None, None),
        ("extra", {"llm_model_pick": 96.0, "input_device_index": 64.0}, None, None),
        ("viewport-560", {}, 560.0, None),
        ("viewport-400", {}, 400.0, None),
        ("viewport-700", {}, 700.0, None),
        ("viewport-200", {}, 200.0, None),
        ("subset", {}, None, (menu_layout.GROUPS[0],)),
        ("empty", {}, None, (("empty", ()),)),
        ("extra-unknown", {"does_not_exist": 40.0}, None, None),
    ]
    for name, extra, viewport, groups in layouts:
        add(
            out,
            f"menu-layout/layout/{name}",
            "menu_layout.layout_menu",
            {"extra": extra, "viewport_h": viewport,
             "groups": None if groups is None else [[g, [list(row) for row in rows]]
                                                     for g, rows in groups]},
            {key: [rect.x0, rect.y0, rect.x1, rect.y1]
             for key, rect in menu_layout.layout_menu(extra, viewport, groups).items()},
        )
    for index, screen in enumerate((1080, 768, 1440, 2160, 600, 0, -100), start=1):
        add(
            out,
            f"menu-layout/visible-height/{index:02d}",
            "menu_layout.visible_menu_height",
            {"screen_h": screen},
            {"height": menu_layout.visible_menu_height(screen)},
        )


# -------------------------------------------------------------------- layered
def pacing_cases(out: FixtureFile) -> None:
    for index, (now, last, interval) in enumerate(
        [(1.0, 1.0, 1 / 60), (1.016, 1.0, 1 / 60), (1.014, 1.0, 1 / 60),
         (1.017, 1.0, 1 / 60), (1.033, 1.0, 1 / 60), (0.999, 1.0, 1 / 60),
         (5.0, 0.0, 1 / 30), (0.5, 0.0, 0.1), (10.0, 9.9, 0.1),
         (0.0, 0.0, 1 / 60), (0.002, 0.0, 1 / 60), (0.001, 0.0, 1 / 60)],
        start=1,
    ):
        add(out, f"layered/frame-due/{index:02d}", "layered.frame_due",
            {"now": now, "last_paint": last, "interval": interval},
            {"due": frame_due(now, last, interval)})
    combos = [
        (True, False, False, 1.016, 1.0, 1 / 60),
        (True, False, True, 1.0, 1.0, 1 / 60),
        (False, False, True, 1.0, 1.0, 1 / 60),
        (True, True, True, 1.0, 1.0, 1 / 60),
        (True, False, False, 1.0, 1.0, 1 / 60),
        (False, False, False, 1.0, 1.0, 1 / 60),
        (True, False, False, 1.014, 1.0, 1 / 60),
        (True, True, False, 1.016, 1.0, 1 / 60),
    ]
    for index, (visible, dragging, dirty, now, last, interval) in enumerate(combos, start=1):
        add(out, f"layered/paint-due/{index:02d}", "layered.paint_due",
            {"visible": visible, "dragging": dragging, "dirty": dirty, "now": now,
             "last_paint": last, "interval": interval},
            {"due": paint_due(visible, dragging, dirty, now, last, interval)})
    waits = [
        (True, 1.0, 1 / 60, 1.0), (True, 1.0, 1 / 60, 1.01), (True, 1.0, 1 / 60, 1.02),
        (False, 1.0, 1 / 60, 1.0), (False, 1.0, 0.1, 5.0), (True, 0.0, 0.1, 1.0),
        (True, 1.0, 1 / 30, 1.01), (True, 1.0, 1 / 60, 0.999),
    ]
    for index, (visible, last, interval, now) in enumerate(waits, start=1):
        add(out, f"layered/wait-delay/{index:02d}", "layered.wait_delay",
            {"visible": visible, "last_paint": last, "interval": interval, "now": now},
            {"delay": wait_delay(visible, last, interval, now)})
    marks = [
        (1.016, 1.0, 1 / 60), (1.018, 1.0, 1 / 60), (1.0, 1.0, 1 / 60),
        (1.017, 1.0, 1 / 60), (1.033, 1.0, 1 / 60), (1.050, 1.0, 1 / 60),
        (0.999, 1.0, 1 / 60), (5.0, 0.0, 1 / 30), (0.5, 0.0, 0.1),
        (1.0, 0.0, 0.1), (1.1, 0.0, 0.1), (1.099, 0.0, 0.1),
    ]
    for index, (now, last, interval) in enumerate(marks, start=1):
        add(out, f"layered/paint-mark/{index:02d}", "layered.paint_mark",
            {"now": now, "last_paint": last, "interval": interval},
            {"mark": paint_mark(now, last, interval)})
    add(out, "layered/constants", "layered.constants", {}, {"FRAME_SLACK": 0.002})


def generate(out: FixtureFile) -> None:
    token_cases(out)
    theme_cases(out)
    accent_cases(out)
    a11y_cases(out)
    anim_cases(out)
    motion_cases(out)
    chat_geometry_cases(out)
    menu_cases(out)
    pacing_cases(out)
    tokens.set_themed(None)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
