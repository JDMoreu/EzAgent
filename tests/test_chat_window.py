"""Chat: mover y redimensionar la ventana, animaciones y orbe vivo, y la geometría pura."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from test_chat_ui import CURSOR, WORK, FakeUI, FakeWindow, alpha_box, local_to_window, settle

from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore
from ezagent.voice.notch import chat_geometry as geo
from ezagent.voice.notch import chat_ui as cu
from ezagent.voice.notch.chat_ui import ChatPanel


@pytest.fixture
def clock(monkeypatch):
    now = [100.0]
    monkeypatch.setattr(cu.time, "monotonic", lambda: now[0])
    monkeypatch.setattr(cu, "ChatWindow", FakeWindow)
    monkeypatch.setattr(cu._shell, "work_area", lambda: WORK)
    return now


@pytest.fixture
def panel(tmp_path: Path, clock) -> ChatPanel:
    return ChatPanel(FakeUI(ChatModel(ChatStore(tmp_path / "chat.jsonl"))))


M = round(geo.MARGIN)


def opened(panel, clock):
    panel.open()
    settle(panel, clock)
    return panel.window


# ------------------------------------------------------------- mover la ventana
def test_moving_the_window_keeps_the_panel_in_view(panel, clock) -> None:
    window = opened(panel, clock)
    before = alpha_box(panel.render())
    window.x -= 700  # el usuario la arrastra a otra parte de la pantalla
    window.y -= 350
    after = alpha_box(panel.render())
    assert before is not None and after == before  # antes se salía del lienzo y desaparecía


def test_the_panel_rect_follows_the_window_wherever_it_goes(panel, clock) -> None:
    window = opened(panel, clock)
    window.x, window.y = 123, 45
    x0, y0, x1, y1 = panel._panel
    assert (x0, y0) == (123 + M, 45 + M)
    assert (x1 - x0, y1 - y0) == (window.w - 2 * M, window.h - 2 * M)


def test_clicks_still_land_after_moving_the_window(panel, clock) -> None:
    window = opened(panel, clock)
    window.x, window.y = 40, 10
    panel.view.render(1.0)
    (x0, y0, x1, y1), _kind, _ = next(h for h in panel.view._hits if h[1] == "close")
    assert panel.pointer_down(round((x0 + x1) / 2) + M, round((y0 + y1) / 2) + M)
    assert panel.anim.state == "closing"


def test_closing_after_moving_far_away_fades_in_place(panel, clock) -> None:
    window = opened(panel, clock)
    window.x, window.y = 50, 50  # lejos de la barra (abajo, en el centro)
    assert panel._form_local() is None
    panel.close()
    clock[0] += cu.CLOSE_SECS * 0.5
    frame = panel.render()
    box = alpha_box(frame, 8)
    assert box is not None and 0 <= box[0] and box[2] <= window.w  # visible, dentro del lienzo
    clock[0] += 1.0
    panel.render()
    panel.poll(clock[0])
    assert panel.window is None


def test_closing_next_to_the_bar_still_flies_back_to_it(panel, clock) -> None:
    opened(panel, clock)
    assert panel._form_local() is not None


# ---------------------------------------------------------------- redimensionar
def right_edge(panel):
    pw, _ = panel._logical_size()
    return local_to_window(panel, pw - 2, 300)


def test_dragging_the_right_edge_widens_the_panel(panel, clock) -> None:
    window = opened(panel, clock)
    w0, x0 = window.w, window.x
    CURSOR[0] = (1000, 500)
    assert panel.pointer_down(*right_edge(panel)) is True
    CURSOR[0] = (1060, 500)
    panel.pointer_move(0, 0)
    assert window.w == w0 + 60 and window.x == x0  # el borde izquierdo no se mueve
    assert panel.view.g.width == pytest.approx((window.w - 2 * M))
    panel.pointer_up()
    assert panel.ui.remembered == [(window.w - 2 * M, window.h - 2 * M)]


def test_dragging_the_left_edge_moves_the_window_origin(panel, clock) -> None:
    window = opened(panel, clock)
    w0, x0 = window.w, window.x
    CURSOR[0] = (800, 400)
    assert panel.pointer_down(*local_to_window(panel, 2, 300)) is True
    CURSOR[0] = (770, 400)
    panel.pointer_move(0, 0)
    assert window.w == w0 + 30 and window.x == x0 - 30  # crece hacia la izquierda
    panel.pointer_up()


def test_a_corner_resizes_both_ways(panel, clock) -> None:
    window = opened(panel, clock)
    w0, h0, y0 = window.w, window.h, window.y
    pw, _ = panel._logical_size()
    CURSOR[0] = (900, 300)
    assert panel.pointer_down(*local_to_window(panel, pw - 2, 2)) is True  # arriba a la derecha
    CURSOR[0] = (940, 270)
    panel.pointer_move(0, 0)
    assert window.w == w0 + 40 and window.h == h0 + 30 and window.y == y0 - 30
    panel.pointer_up()


def test_the_panel_cannot_shrink_below_its_minimum(panel, clock) -> None:
    window = opened(panel, clock)
    CURSOR[0] = (1000, 500)
    panel.pointer_down(*right_edge(panel))
    CURSOR[0] = (-4000, 500)
    panel.pointer_move(0, 0)
    assert window.w - 2 * M == round(geo.MIN_W)
    panel.pointer_up()


def test_the_panel_cannot_grow_past_the_screen(panel, clock) -> None:
    window = opened(panel, clock)
    CURSOR[0] = (1000, 500)
    panel.pointer_down(*right_edge(panel))
    CURSOR[0] = (9000, 500)
    panel.pointer_move(0, 0)
    assert window.x + window.w - M <= WORK[2] - geo.SCREEN_PAD
    panel.pointer_up()


def test_resizing_keeps_the_messages_cached_when_only_the_height_changes(panel, clock) -> None:
    window = opened(panel, clock)
    panel.view.model.add_user("hola")
    panel.view.render(1.0)
    assert panel.view._cache
    _, ph = panel._logical_size()
    CURSOR[0] = (1000, 400)
    panel.pointer_down(*local_to_window(panel, 200, ph - 2))  # borde de abajo
    CURSOR[0] = (1000, 380)
    panel.pointer_move(0, 0)
    assert window.h < window.h + 20 and panel.view._cache  # solo cambió el alto
    panel.pointer_up()


def test_the_cursor_changes_over_the_edges_only(panel, clock) -> None:
    opened(panel, clock)
    pw, ph = panel._logical_size()
    assert panel.cursor_ident(*local_to_window(panel, pw - 2, 300)) == geo.IDC_SIZEWE
    assert panel.cursor_ident(*local_to_window(panel, 200, ph - 2)) == geo.IDC_SIZENS
    assert panel.cursor_ident(*local_to_window(panel, 2, 2)) == geo.IDC_SIZENWSE
    assert panel.cursor_ident(*local_to_window(panel, pw - 2, 2)) == geo.IDC_SIZENESW
    assert panel.cursor_ident(*local_to_window(panel, 200, 300)) == 0


def test_the_edges_do_nothing_while_the_panel_is_animating(panel, clock) -> None:
    panel.open()
    assert panel.cursor_ident(M + 2, 300) == 0
    assert panel._resize is None


def test_hovering_an_edge_does_not_light_up_the_controls(panel, clock) -> None:
    opened(panel, clock)
    pw, _ = panel._logical_size()
    assert panel.hover(*local_to_window(panel, pw - 2, 10)) is False


def test_the_remembered_size_is_used_on_the_next_open(panel, clock) -> None:
    panel.ui.chat_size = (500.0, 700.0)
    panel.open()
    assert (panel.window.w, panel.window.h) == (500 + 2 * M, 700 + 2 * M)


def test_a_tiny_screen_clamps_the_remembered_size(panel, clock) -> None:
    panel.ui.chat_size = (5000.0, 5000.0)
    panel.open()
    assert panel.window.w - 2 * M <= WORK[2] and panel.window.h - 2 * M <= WORK[3]


# --------------------------------------------------------- orbe y estado vivo
def test_the_header_reserves_a_slot_for_the_live_orb(panel, clock) -> None:
    opened(panel, clock)
    panel.view.render(1.0)
    cx, cy, d = panel.view.orb
    assert 0 < cx < 80 and 0 < cy < 60 and d > 10


def test_the_status_follows_the_bar_pose(panel, clock) -> None:
    opened(panel, clock)
    for pose, text in (("listen", "Escuchando…"), ("think", "Pensando…"), ("speak", "Hablando…")):
        panel.ui.pose = pose
        clock[0] += 0.1
        panel.render()
        assert panel.view.status == text
    panel.ui.pose = "idle"
    clock[0] += 0.1
    panel.render()
    assert panel.view.status == ""


def test_a_busy_chat_reads_as_thinking_even_if_the_bar_is_idle(panel, clock) -> None:
    opened(panel, clock)
    panel.view.model.begin_assistant()  # una respuesta en curso = ocupado
    clock[0] += 0.1
    panel.render()
    assert panel.view.status == "Pensando…"


def test_the_orb_is_calm_at_rest_and_lively_when_working(panel, clock) -> None:
    opened(panel, clock)
    clock[0] += 0.1
    rest = panel.render()
    assert panel._act == 0.0 and panel.window._interval == pytest.approx(1 / 30)
    panel.ui.pose = "think"
    for _ in range(30):
        clock[0] += 1 / 30
        working = panel.render()
    assert panel._act > 0.95 and panel.window._interval == pytest.approx(1 / 60)
    assert not np.array_equal(rest, working)  # el orbe, el halo y la línea se ven distintos


def test_the_status_line_only_shows_while_working(panel, clock) -> None:
    opened(panel, clock)
    pw = panel.window.w - 2 * M
    assert panel._status_line(pw, 1.0, 1.0, M, M) is None
    panel._act = 1.0
    x, y, arr = panel._status_line(pw, 1.0, 1.0, M, M)
    assert arr.shape[1] == pw - 2 * round(16) and arr[..., 3].max() > 0.3
    assert y > M and arr[:, 0, 3].max() < 0.05  # se funde en los extremos


def test_the_stable_frame_is_cached_and_only_the_live_part_changes(panel, clock) -> None:
    opened(panel, clock)
    panel.render()
    base = panel._base
    clock[0] += 0.05
    panel.render()
    assert panel._base is base  # no se recompone todo el panel por cada cuadro


def test_a_resize_drops_the_cached_frame(panel, clock) -> None:
    opened(panel, clock)
    panel.render()
    assert panel._base is not None
    CURSOR[0] = (1000, 500)
    panel.pointer_down(*right_edge(panel))
    CURSOR[0] = (1030, 500)
    panel.pointer_move(0, 0)
    assert panel._base is None
    panel.pointer_up()


# ----------------------------------------------------------------------- copiar
def test_copying_flashes_the_label_and_repaints(panel, clock) -> None:
    opened(panel, clock)
    sig_before = panel._sig(panel.view, clock[0])
    panel._perform(cu.Action("copy", "hola"))
    assert panel.window.copied == ["hola"]
    assert panel.view.flash_token == ("hola", True)
    assert panel._sig(panel.view, clock[0]) != sig_before  # la caché se invalida


# ------------------------------------------------------------------ animaciones
def test_the_open_shape_overshoots_a_little_and_settles() -> None:
    values = [cu.shape_progress(i / 50, True) for i in range(51)]
    assert max(values) > 1.0 and max(values) < 1.06 and values[-1] == 1.0
    assert all(cu.shape_progress(i / 50, False) == i / 50 for i in range(51))  # cerrar: directo


def test_content_appears_top_down_when_opening_and_leaves_earlier_when_closing() -> None:
    op = cu.row_reveal(100, 0.7, True)
    assert op[0] > op[-1] and list(op) == sorted(op, reverse=True)
    assert cu.row_reveal(100, 1.0, True).min() == pytest.approx(1.0)
    assert cu.row_reveal(100, 1.0, False).min() == pytest.approx(1.0)
    assert cu.row_reveal(100, 0.5, False).max() < cu.row_reveal(100, 0.5, True).max() + 1e-6
    assert cu.row_reveal(100, 0.0, True).max() == 0.0


def test_closing_is_faster_than_opening(panel, clock) -> None:
    assert cu.CLOSE_SECS < cu.MORPH_SECS
    panel.open()
    clock[0] += 0.5
    panel.render()
    panel.close()
    clock[0] += cu.CLOSE_SECS + 0.01
    panel.render()
    assert panel._finished is True


def test_content_is_invisible_at_the_start_and_full_at_the_end(panel, clock) -> None:
    panel.open()
    first = panel.render()
    # al principio solo hay la forma: nada de texto dentro del panel completo
    assert first[..., 3].max() > 0 and first.shape[:2] == (panel.window.h, panel.window.w)
    settle(panel, clock)
    assert panel.render()[..., 3].max() == 255


# ---------------------------------------------------------------- geometría pura
def test_edges_at_detects_sides_and_corners() -> None:
    w, h = 400.0, 600.0
    assert geo.edges_at(2, 300, w, h) == (True, False, False, False)
    assert geo.edges_at(398, 300, w, h) == (False, False, True, False)
    assert geo.edges_at(200, 2, w, h) == (False, True, False, False)
    assert geo.edges_at(200, 598, w, h) == (False, False, False, True)
    assert geo.edges_at(2, 2, w, h) == (True, True, False, False)
    assert geo.edges_at(398, 598, w, h) == (False, False, True, True)
    assert geo.edges_at(200, 300, w, h) is None
    assert geo.edges_at(-30, 300, w, h) is None and geo.edges_at(200, 900, w, h) is None


def test_edge_cursors_match_the_direction() -> None:
    assert geo.edge_cursor((True, False, False, False)) == geo.IDC_SIZEWE
    assert geo.edge_cursor((False, False, True, False)) == geo.IDC_SIZEWE
    assert geo.edge_cursor((False, True, False, False)) == geo.IDC_SIZENS
    assert geo.edge_cursor((True, True, False, False)) == geo.IDC_SIZENWSE
    assert geo.edge_cursor((False, False, True, True)) == geo.IDC_SIZENWSE
    assert geo.edge_cursor((True, False, False, True)) == geo.IDC_SIZENESW
    assert geo.edge_cursor((False, True, True, False)) == geo.IDC_SIZENESW


def test_resize_rect_keeps_the_opposite_edge_fixed_and_respects_limits() -> None:
    rect = (500.0, 200.0, 940.0, 840.0)
    right = geo.resize_rect((False, False, True, False), rect, 50, 99, WORK, 1.0)
    assert right == (500.0, 200.0, 990.0, 840.0)
    left = geo.resize_rect((True, False, False, False), rect, -80, 0, WORK, 1.0)
    assert left == (420.0, 200.0, 940.0, 840.0)
    tiny = geo.resize_rect((True, True, False, False), rect, 9999, 9999, WORK, 1.0)
    assert tiny[2] - tiny[0] == geo.MIN_W and tiny[3] - tiny[1] == geo.MIN_H
    assert tiny[2] == 940.0 and tiny[3] == 840.0
    huge = geo.resize_rect((True, True, True, True), rect, 9999, 9999, WORK, 1.0)
    assert huge[2] <= WORK[2] - geo.SCREEN_PAD and huge[3] <= WORK[3] - geo.SCREEN_PAD
    wide = geo.resize_rect((True, False, False, False), rect, -9999, 0, WORK, 1.0)
    assert wide[0] == WORK[0] + geo.SCREEN_PAD


def test_resize_rect_scales_limits_with_dpi() -> None:
    rect = (500.0, 200.0, 1100.0, 1000.0)
    out = geo.resize_rect((False, False, True, False), rect, -9999, 0, WORK, 1.5)
    assert out[2] - out[0] == pytest.approx(geo.MIN_W * 1.5)


def test_panel_size_honours_the_saved_size_within_limits() -> None:
    assert geo.panel_size(WORK, 1.0) == (geo.PANEL_W, geo.PANEL_H)
    assert geo.panel_size(WORK, 1.0, (500, 700)) == (500.0, 700.0)
    assert geo.panel_size(WORK, 1.0, (10, 10)) == (geo.MIN_W, geo.MIN_H)
    max_w, max_h = geo.panel_limits(WORK, 1.0)
    assert geo.panel_size(WORK, 1.0, (9999, 9999)) == (max_w, max_h)
    small = (0, 0, 800, 600)
    assert geo.panel_size(small, 1.0, (9999, 9999))[1] <= 600


def test_ease_out_back_overshoots_and_ends_exactly() -> None:
    xs = [geo.ease_out_back(i / 40) for i in range(41)]
    assert xs[0] == 0.0 and xs[-1] == pytest.approx(1.0) and max(xs) > 1.0
