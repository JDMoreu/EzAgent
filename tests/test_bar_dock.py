"""Barra flotante: los botones cambian de lado cerca de los bordes y la píldora es compacta."""

from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.notch import drop as D
from ezagent.voice.notch.drop import (
    BUTTONS,
    DEFAULT_DOCK,
    DOCKS,
    DropModel,
    DropUI,
    choose_dock,
    clean_dock,
    emerge_order,
    form_center,
    layout_size,
    render_model,
    slots,
)

WORK = (0, 0, 1920, 1040)
CASES = [(layout, dock) for layout, docks in DOCKS.items() for dock in docks]


# --------------------------------------------------------------- la píldora
def test_the_pill_is_narrower_than_before() -> None:
    assert D.PILL_W <= 112  # antes 148
    assert D.PILL_H == 38
    assert layout_size("pill")[0] < D.NB * D.BTN + 148 + D.NB * D.GAP + 2 * D.PAD


def test_the_fluid_keeps_the_real_proportion_of_the_pill() -> None:
    assert D._flow_for("pill").aspect == pytest.approx(D.PILL_W / D.PILL_H)
    assert D._flow_for("circle").aspect == 1.0


# ---------------------------------------------------------------- geometría
@pytest.mark.parametrize(("layout", "dock"), CASES)
def test_every_element_fits_inside_the_window(layout: str, dock: str) -> None:
    w, h = layout_size(layout, dock)
    for mic in (0.0, 0.5, 1.0):
        for name, (x, y) in slots(layout, mic, dock).items():
            assert 0 <= x <= w and 0 <= y <= h, (name, mic)
    assert set(slots(layout, 1.0, dock)) == {*BUTTONS, "pill"}


@pytest.mark.parametrize(("layout", "dock"), CASES)
def test_the_form_never_moves_when_the_mic_folds(layout: str, dock: str) -> None:
    assert slots(layout, 0.0, dock)["pill"] == slots(layout, 1.0, dock)["pill"]
    assert form_center(layout, dock) == slots(layout, 1.0, dock)["pill"]


@pytest.mark.parametrize(("layout", "dock"), CASES)
def test_buttons_do_not_overlap_each_other_or_the_form(layout: str, dock: str) -> None:
    sl = slots(layout, 1.0, dock)
    form_r = (D.CIRCLE_D if layout == "circle" else D.PILL_H) / 2
    names = [*BUTTONS, "pill"]
    for i, a in enumerate(names):
        for b in names[i + 1 :]:
            (ax, ay), (bx, by) = sl[a], sl[b]
            ra = form_r if a == "pill" else D.BTN / 2
            rb = form_r if b == "pill" else D.BTN / 2
            if "pill" in (a, b) and layout == "pill":  # cápsula: se compara con su ancho
                dx = max(abs(ax - bx) - D.PILL_W / 2 - D.BTN / 2, 0.0)
                assert dx > 0 or abs(ay - by) > D.PILL_H / 2 + D.BTN / 2 - 1
            else:
                assert np.hypot(ax - bx, ay - by) >= ra + rb, (layout, dock, a, b)


@pytest.mark.parametrize(("layout", "dock"), CASES)
def test_emerge_order_lists_every_button_once_nearest_first(layout: str, dock: str) -> None:
    flat = [name for group in emerge_order(layout, dock) for name in group]
    assert sorted(flat) == sorted(BUTTONS)
    sl = slots(layout, 1.0, dock)
    fx, fy = sl["pill"]
    dist = [np.hypot(sl[n][0] - fx, sl[n][1] - fy) for n in flat]
    assert dist == sorted(dist)


def test_default_docks_keep_the_original_arrangement() -> None:
    assert DEFAULT_DOCK == {"pill": "sides", "circle": "bottom"}
    assert emerge_order("pill") == (("chat", "mic"), ("gear", "hang"))
    assert emerge_order("circle") == (("chat", "mic"), ("gear", "hang"))
    sl = slots("pill")  # ☰ 💬 [forma] 🎤 ✕
    assert sl["gear"][0] < sl["chat"][0] < sl["pill"][0] < sl["mic"][0] < sl["hang"][0]
    sl = slots("circle")  # la forma arriba y los cuatro botones en fila debajo
    assert sl["pill"][1] < sl["gear"][1] == sl["chat"][1] == sl["mic"][1] == sl["hang"][1]
    assert sl["gear"][0] < sl["chat"][0] < sl["mic"][0] < sl["hang"][0]


def test_circle_docks_put_the_buttons_on_the_chosen_side() -> None:
    center = slots("circle", 1.0, "top")
    assert center["gear"][1] < center["pill"][1]
    right, left = slots("circle", 1.0, "right"), slots("circle", 1.0, "left")
    assert all(right[n][0] > right["pill"][0] for n in BUTTONS)
    assert all(left[n][0] < left["pill"][0] for n in BUTTONS)
    # ☰ 💬 🎤 ✕ se leen siempre de izquierda a derecha.
    for sl in (right, left):
        assert sl["gear"][0] < sl["chat"][0] < sl["mic"][0] < sl["hang"][0]


def test_unknown_docks_fall_back_to_the_default() -> None:
    assert clean_dock("circle", "diagonal") == "bottom"
    assert clean_dock("pill", "bottom") == "sides"  # «bottom» no existe para la píldora
    assert clean_dock("pill", None) == "sides"
    assert layout_size("circle", "raro") == layout_size("circle", "bottom")


# --------------------------------------------------------------- elegir el dock
def test_in_the_middle_of_the_screen_the_default_dock_is_kept() -> None:
    assert choose_dock("circle", None, (960, 500), WORK, 1.0) == "bottom"
    assert choose_dock("pill", None, (960, 500), WORK, 1.0) == "sides"


def test_near_the_bottom_edge_the_buttons_go_above() -> None:
    assert choose_dock("circle", "bottom", (960, 1011), WORK, 1.0) == "top"


@pytest.mark.parametrize(("anchor", "dock"), [((45, 500), "right"), ((1875, 500), "left")])
def test_near_a_side_edge_the_circle_buttons_go_to_the_free_side(anchor, dock) -> None:
    assert choose_dock("circle", "bottom", anchor, WORK, 1.0) == dock


@pytest.mark.parametrize(("anchor", "dock"), [((60, 500), "right"), ((1860, 500), "left")])
def test_near_a_side_edge_the_pill_buttons_go_to_the_free_side(anchor, dock) -> None:
    assert choose_dock("pill", "sides", anchor, WORK, 1.0) == dock


def test_the_form_can_reach_the_edge_thanks_to_the_dock() -> None:
    """Con los botones al otro lado, la ventana cabe aunque la forma esté casi en el borde."""
    for layout, anchor in (("circle", (45, 500)), ("pill", (60, 500))):
        dock = choose_dock(layout, None, anchor, WORK, 1.0)
        assert D._window_overflow(layout, dock, anchor, WORK, 1.0) == 0.0
        assert D._window_overflow(layout, DEFAULT_DOCK[layout], anchor, WORK, 1.0) > 0.0


def test_returning_to_the_default_dock_needs_some_slack() -> None:
    # Justo en el límite: sigue arriba si ya estaba arriba, y abajo si ya estaba abajo.
    anchor = (960, 960)
    assert choose_dock("circle", "top", anchor, WORK, 1.0) == "top"
    assert choose_dock("circle", "bottom", anchor, WORK, 1.0) == "bottom"
    # Con holgura de sobra vuelve al de por defecto.
    assert choose_dock("circle", "top", (960, 700), WORK, 1.0) == "bottom"


def test_a_valid_non_default_dock_is_not_changed_for_no_reason() -> None:
    anchor = (45, 1011)  # esquina inferior izquierda
    first = choose_dock("circle", "bottom", anchor, WORK, 1.0)
    assert first in DOCKS["circle"] and first != "bottom"
    assert choose_dock("circle", first, anchor, WORK, 1.0) == first


def test_when_nothing_fits_the_least_bad_dock_is_returned() -> None:
    tiny = (0, 0, 120, 90)
    dock = choose_dock("circle", None, (60, 45), tiny, 1.0)
    assert dock in DOCKS["circle"]
    best = min(D._window_overflow("circle", d, (60, 45), tiny, 1.0) for d in DOCKS["circle"])
    assert D._window_overflow("circle", dock, (60, 45), tiny, 1.0) == best


@pytest.mark.parametrize("bs", [0.6, 1.0, 1.5, 2.0])
def test_the_choice_scales_with_the_bar_size(bs: float) -> None:
    for layout in DOCKS:
        dock = choose_dock(layout, None, (960, 500), WORK, bs)
        assert dock == DEFAULT_DOCK[layout]


# --------------------------------------------------------------- DropUI
class _FakeWindow:
    def __init__(self, x: int, y: int) -> None:
        self.x, self.y, self.w, self.h = x, y, 0, 0
        self._drag_off = (30, 20)

    def resize(self, w: int, h: int) -> None:
        self.w, self.h = w, h

    def set_pos(self, x: int, y: int) -> None:
        self.x, self.y = x, y


@pytest.fixture
def ui(monkeypatch) -> DropUI:
    monkeypatch.setattr(D._shell, "work_area", lambda: WORK)
    bar = DropUI(None, cfg=SimpleNamespace(ui_layout="circle", ui_scale=1.0))
    bar._win = _FakeWindow(900, 400)
    return bar


def test_dragging_to_the_bottom_edge_flips_the_buttons_up_and_keeps_the_form_under_the_cursor(
    ui: DropUI,
) -> None:
    fx, fy = form_center("circle", "bottom")
    want_anchor = (960.0, 1011.0)  # la forma tocando el borde inferior (radio 29)
    x, y = round(want_anchor[0] - fx), round(want_anchor[1] - fy)
    old_off = ui._win._drag_off
    nx, ny = ui.drag_target(x, y)
    assert ui.dock == "top" and ui.model.dock == "top"
    nfx, nfy = form_center("circle", "top")
    assert (nx + nfx, ny + nfy) == pytest.approx(want_anchor, abs=1.0)
    assert ui._win._drag_off == (old_off[0] + round(nfx - fx), old_off[1] + round(nfy - fy))
    assert (ui._win.w, ui._win.h) == ui._size()


def test_dragging_back_to_the_middle_returns_to_the_default_dock(ui: DropUI) -> None:
    ui.dock = ui.model.dock = "top"
    fx, fy = form_center("circle", "top")
    ui.drag_target(round(960 - fx), round(500 - fy))
    assert ui.dock == "bottom"


def test_a_drag_that_needs_no_change_just_clamps(ui: DropUI) -> None:
    before = ui._win._drag_off
    x, y = ui.drag_target(900, 400)
    assert ui.dock == "bottom" and ui._win._drag_off == before
    assert (x, y) == ui.clamp_position(900, 400)


def test_the_window_reaches_the_screen_edge_with_the_form_beside_it(ui: DropUI) -> None:
    fx, fy = form_center("circle", "bottom")
    # El cursor pide la forma a 45 px del borde izquierdo: ya no queda frenada por la ventana.
    x, y = ui.drag_target(round(45 - fx), 300)
    assert ui.dock == "right"
    nfx, _ = form_center("circle", "right")
    assert x + nfx == pytest.approx(45, abs=1.0)


def test_switching_shape_resets_the_dock(ui: DropUI, monkeypatch) -> None:
    monkeypatch.setattr(D._shell, "work_area", lambda: WORK)
    ui.dock = ui.model.dock = "top"
    ui.set_layout("pill")
    assert ui.layout == "pill" and ui.dock == "sides" and ui.model.dock == "sides"
    assert (ui._win.w, ui._win.h) == ui._size()


def test_changing_the_bar_size_rechecks_the_dock(ui: DropUI) -> None:
    ui._win.x, ui._win.y = 900, 900  # muy abajo: a 200 % ya no cabe debajo
    fx, fy = form_center("circle", ui.dock)
    ui._win.x, ui._win.y = round(960 - fx), round(1000 - fy)
    ui.set_bar_scale(2.0)
    assert ui.dock != "bottom"
    assert D._window_overflow("circle", ui.dock, ui._form_anchor(), WORK, ui.bs) <= 24.0


def test_the_saved_dock_only_counts_for_the_layout_it_was_saved_with() -> None:
    same = DropUI(
        None, cfg=SimpleNamespace(ui_layout="circle", drop_layout="circle", drop_dock="top")
    )
    other = DropUI(
        None, cfg=SimpleNamespace(ui_layout="circle", drop_layout="pill", drop_dock="top")
    )
    junk = DropUI(None, cfg=SimpleNamespace(ui_layout="pill", drop_layout="pill", drop_dock="top"))
    assert same.dock == "top"
    assert other.dock == "bottom"
    assert junk.dock == "sides"  # «top» no existe para la píldora


def test_the_dock_is_saved_with_the_position() -> None:
    saved: dict = {}
    cfg = SimpleNamespace(
        ui_layout="circle",
        save_key=lambda key, value: saved.__setitem__(key, value),
    )
    bar = DropUI(None, cfg=cfg)
    bar.dock = "left"
    bar.drag_end(100, 200)
    assert saved["drop_dock"] == "left" and saved["drop_layout"] == "circle"


def test_the_config_has_the_new_field() -> None:
    from ezagent.voice.config import VoiceConfig

    assert VoiceConfig().drop_dock == ""


# --------------------------------------------------------------------- dibujo
@pytest.mark.parametrize(("layout", "dock"), CASES)
def test_every_dock_renders_with_the_buttons_inside_the_frame(layout: str, dock: str) -> None:
    model = DropModel(layout, dock)
    for step in range(60):  # los botones salen al acercar el puntero
        model.update(
            step / 60, 1 / 60, "idle", 0.0, "dark", True, False, False, False, in_call=True
        )
    w, h = layout_size(layout, dock)
    frame = render_model(model, round(w), round(h), 1.0, "dark", False, True, False, 1.0)
    assert frame.shape == (round(h), round(w), 4)
    alpha = frame[..., 3]
    assert alpha.max() > 0
    # Nada se pinta pegado al borde de la ventana: los botones caben con su margen.
    assert alpha[:2].max() == 0 and alpha[-2:].max() == 0
    assert alpha[:, :2].max() == 0 and alpha[:, -2:].max() == 0


def test_there_are_four_buttons_and_the_chat_one_is_a_real_button() -> None:
    assert BUTTONS == ("gear", "chat", "mic", "hang") and D.NB == 4
    model = DropModel("circle")
    for step in range(60):
        model.update(
            step / 60, 1 / 60, "idle", 0.0, "dark", True, False, False, False, in_call=True
        )
    cx, cy, scale, alpha = model.positions()["chat"]
    assert scale > 0.9 and alpha > 0.9
    assert model.item_at(cx, cy) == "chat"


def test_the_chat_button_hides_with_the_menu_one_while_a_confirmation_is_pending() -> None:
    model = DropModel("pill")
    for step in range(90):
        model.update(step / 60, 1 / 60, "idle", 0.0, "dark", True, True, False, False, in_call=True)
    pos = model.positions()
    assert pos["gear"][3] < 0.1 and pos["chat"][3] < 0.1  # ☰ y 💬 se retiran al preguntar
    assert pos["mic"][3] > 0.9 and pos["hang"][3] > 0.9  # ✓ y «no» se quedan


def test_the_chat_glyph_is_drawn_and_lights_up_when_open() -> None:
    import numpy as np

    closed = np.asarray(D.button_sprite("chat", "dark", 1.0)).astype(int)
    opened = np.asarray(D.button_sprite("chat", "dark", 1.0, signal=True)).astype(int)
    assert closed[..., 3].max() > 200 and not np.array_equal(closed, opened)
    plain = np.asarray(D.button_sprite("gear", "dark", 1.0)).astype(int)
    assert not np.array_equal(closed, plain)  # no es el mismo icono


def test_clicking_the_chat_button_asks_the_app_to_toggle_the_chat() -> None:
    actions: list[str] = []
    bar = DropUI(None, cfg=SimpleNamespace(ui_layout="circle"))
    bar.on_action = actions.append
    bar._activate("chat")
    assert actions == ["chat"]
    bar._asking = True
    bar._activate("chat")
    assert actions == ["chat"]  # con una confirmación pendiente no abre el chat
