"""Panel del chat: la forma se transforma en el panel, se cierra de vuelta y reparte las entradas."""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore
from ezagent.voice.notch import chat_ui as cu
from ezagent.voice.notch import drop as D
from ezagent.voice.notch.chat_ui import ChatPanel, panel_rect, panel_size, rounded_coverage

WORK = (0, 0, 1920, 1040)
FORM = (930.0, 970.0, 988.0, 1028.0)  # esfera de 58 px en la mitad de abajo


# ---------------------------------------------------------------- dobles
class FakeWindow:
    def __init__(self, panel, x: int, y: int, w: int, h: int) -> None:
        self.panel, self.x, self.y, self.w, self.h = panel, x, y, w, h
        self._interval = 0.0
        self.log: list[str] = []
        self.copied: list[str] = []

    def start(self) -> bool:
        self.log.append("start")
        return True

    def show(self) -> None:
        self.log.append("show")

    def stop(self) -> None:
        self.log.append("stop")

    def set_keyboard_enabled(self, enabled: bool) -> None:
        self.log.append(f"kb={enabled}")

    def copy_text(self, text: str) -> bool:
        self.copied.append(text)
        return True

    def resize(self, w: int, h: int) -> None:
        self.w, self.h = int(w), int(h)

    def _screen_cursor(self) -> tuple[int, int]:
        return CURSOR[0]


CURSOR: list[tuple[int, int]] = [(0, 0)]  # dónde está el puntero del sistema en las pruebas


class FakeUI:
    def __init__(self, model: ChatModel | None, anchor=(959.0, 999.0)) -> None:
        self.s = self.bs = 1.0
        self.layout = "circle"
        self._reduced = False
        self.chat_model = model
        self.chat_roots = None
        self.chat_base_dirs = ()
        self.chat_hotkey = "ctrl+alt+t"
        self._anchor = anchor
        self.calls: list[tuple] = []
        self.hidden: list[bool] = []
        self.open_flags: list[bool] = []
        self.pose = "idle"
        self.chat_size: tuple[float, float] | None = None
        self.remembered: list[tuple[float, float]] = []

    def chat_pose(self) -> str:
        return self.pose

    def remember_chat_size(self, width: float, height: float) -> None:
        self.remembered.append((round(width), round(height)))

    def _form_anchor(self):
        return self._anchor

    def form_size(self):
        return (58.0, 58.0)

    def _theme_name(self) -> str:
        return "dark"

    def chat_body_rgb(self):
        return (13, 22, 26)

    def chat_fluid(self, w, h, now):
        out = np.zeros((h, w, 3), np.float32)
        out[..., 1], out[..., 2] = 0.7, 0.6
        return out

    def set_chat_open(self, on):
        self.open_flags.append(on)

    def set_form_hidden(self, hidden):
        self.hidden.append(hidden)

    def set_scale(self, scale):
        self.calls.append(("scale", scale))

    def on_chat_send(self, text):
        self.calls.append(("send", text))

    def on_chat_voice(self, enabled):
        self.calls.append(("voice", enabled))

    def on_chat_mic(self, muted):
        self.calls.append(("mic", muted))

    def on_chat_menu(self):
        self.calls.append(("menu",))

    def on_chat_new(self):
        self.calls.append(("new",))

    def on_chat_stop(self):
        self.calls.append(("stop",))

    def on_chat_switch(self, session):
        self.calls.append(("switch", session))

    def on_chat_delete(self, session):
        self.calls.append(("delete", session))

    def on_chat_confirm(self, yes):
        self.calls.append(("confirm", yes))


@pytest.fixture
def clock(monkeypatch):
    now = [100.0]
    monkeypatch.setattr(cu.time, "monotonic", lambda: now[0])
    monkeypatch.setattr(cu, "ChatWindow", FakeWindow)
    monkeypatch.setattr(cu._shell, "work_area", lambda: WORK)
    return now


@pytest.fixture
def panel(tmp_path: Path, clock) -> ChatPanel:
    ui = FakeUI(ChatModel(ChatStore(tmp_path / "chat.jsonl")))
    return ChatPanel(ui)


def settle(panel: ChatPanel, clock) -> None:
    clock[0] += 1.0
    panel.render()


def alpha_box(frame: np.ndarray, threshold: int = 40):
    mask = frame[..., 3] > threshold
    rows, cols = np.where(mask)
    return (cols.min(), rows.min(), cols.max() + 1, rows.max() + 1) if rows.size else None


# --------------------------------------------------------------- geometría
def test_a_bar_in_the_lower_half_opens_the_panel_upwards() -> None:
    x0, y0, x1, y1 = panel_rect(FORM, WORK, 1.0)
    assert y1 == pytest.approx(FORM[3]) and y0 < FORM[1]
    assert (x0 + x1) / 2 == pytest.approx((FORM[0] + FORM[2]) / 2)
    assert (x1 - x0, y1 - y0) == panel_size(WORK, 1.0)


def test_a_bar_in_the_upper_half_opens_the_panel_downwards() -> None:
    top_form = (930.0, 20.0, 988.0, 78.0)
    x0, y0, x1, y1 = panel_rect(top_form, WORK, 1.0)
    assert y0 == pytest.approx(top_form[1]) and y1 > top_form[3]


@pytest.mark.parametrize("cx", [10.0, 400.0, 960.0, 1500.0, 1915.0])
@pytest.mark.parametrize("cy", [30.0, 400.0, 1010.0])
def test_the_panel_always_stays_on_screen(cx: float, cy: float) -> None:
    form = (cx - 29, cy - 29, cx + 29, cy + 29)
    x0, y0, x1, y1 = panel_rect(form, WORK, 1.0)
    assert x0 >= WORK[0] + cu.SCREEN_PAD - 0.5 and x1 <= WORK[2] - cu.SCREEN_PAD + 0.5
    assert y0 >= WORK[1] + cu.SCREEN_PAD - 0.5 and y1 <= WORK[3] - cu.SCREEN_PAD + 0.5


def test_small_screens_shrink_the_panel_and_scaling_grows_it() -> None:
    small = (0, 0, 900, 560)
    w, h = panel_size(small, 1.0)
    assert w <= 900 and h <= 560 - 2 * cu.SCREEN_PAD and w >= 320 and h >= 360
    assert panel_size(WORK, 1.0) == (cu.PANEL_W, cu.PANEL_H)
    x0, y0, x1, y1 = panel_rect(FORM, WORK, 1.5)
    assert x1 - x0 == pytest.approx(cu.PANEL_W * 1.5)


def test_rounded_coverage_is_smooth_and_round() -> None:
    cov = rounded_coverage(120, 80, 20)
    assert cov.shape == (80, 120) and cov.min() >= 0 and cov.max() <= 1
    assert cov[0, 0] == 0 and cov[40, 60] == 1  # esquina vacía, centro lleno
    assert ((cov > 0) & (cov < 1)).any()  # el borde del arco está suavizado
    circle = rounded_coverage(60, 60, 30)
    assert circle.sum() == pytest.approx(np.pi * 30**2, rel=0.03)
    assert rounded_coverage(60, 60, 30, inset=2).sum() < circle.sum()


def test_easing_is_monotonic_and_bounded() -> None:
    values = [cu.ease(i / 20) for i in range(21)]
    assert values[0] == 0 and values[-1] == 1 and values == sorted(values)
    assert cu.ease(-1) == 0 and cu.ease(9) == 1
    assert cu.lerp_rect((0, 0, 10, 10), (10, 20, 30, 40), 0.5) == (5, 10, 20, 25)


# -------------------------------------------------------------- ciclo de vida
def test_opening_creates_the_window_around_the_panel_and_takes_the_keyboard(panel, clock) -> None:
    assert panel.open() is True
    window = panel.window
    x0, y0, x1, y1 = panel._panel
    m = round(cu.MARGIN)
    assert (window.x, window.y) == (round(x0) - m, round(y0) - m)
    assert (window.w, window.h) == (round(x1 - x0) + 2 * m, round(y1 - y0) + 2 * m)
    assert window.log == ["start", "show", "kb=True"]
    assert panel.ui.open_flags == [True] and panel.is_open and panel.anim.state == "opening"


def test_without_a_model_or_a_bar_the_chat_does_not_open(clock, tmp_path: Path) -> None:
    assert ChatPanel(FakeUI(None)).open() is False
    ui = FakeUI(ChatModel(ChatStore(tmp_path / "c.jsonl")), anchor=None)
    assert ChatPanel(ui).toggle() is False and not ChatPanel(ui).is_open


def test_the_form_hides_only_after_the_first_painted_frame(panel, clock) -> None:
    panel.open()
    assert panel.ui.hidden == []  # todavía no: la barra sigue dibujando su forma
    panel.render()
    assert panel.ui.hidden == [True]


def test_the_messages_are_drawn_before_the_morph_starts(tmp_path: Path, clock) -> None:
    model = ChatModel(ChatStore(tmp_path / "chat.jsonl"))
    model.add_user("hola")
    msg = model.begin_assistant()
    model.append(msg.id, "respuesta")
    model.finish(msg.id)
    panel = ChatPanel(FakeUI(model))
    panel.open()
    assert len(panel.view._cache) == 2  # sin congelar la forma a mitad de la animación


def test_it_reaches_the_open_state_after_the_morph(panel, clock) -> None:
    panel.open()
    clock[0] += cu.MORPH_SECS / 2
    panel.render()
    assert panel.anim.state == "opening" and 0 < panel.anim.p < 1
    clock[0] += cu.MORPH_SECS
    panel.render()
    assert panel.anim.state == "open" and panel.anim.p == 1.0


def test_closing_shrinks_back_and_then_removes_the_window(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    window = panel.window
    panel.close()
    assert panel.anim.state == "closing" and panel.ui.hidden[-1] is False  # la forma vuelve
    assert "kb=False" in window.log
    clock[0] += cu.MORPH_SECS / 2
    panel.render()
    assert 0 < panel.anim.p < 1 and panel.window is window
    clock[0] += cu.MORPH_SECS
    panel.render()
    panel.poll(clock[0])
    assert panel.window is None and window.log[-1] == "stop"
    assert not panel.is_open and panel.ui.open_flags[-1] is False


def test_toggle_opens_and_closes(panel, clock) -> None:
    assert panel.toggle() is True and panel.is_open
    settle(panel, clock)
    assert panel.toggle() is True and panel.anim.state == "closing"


def test_reopening_mid_close_continues_without_a_jump(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    panel.close()
    clock[0] += cu.MORPH_SECS * 0.4
    panel.render()
    before = panel.anim.p
    panel.open()
    clock[0] += 0.001
    panel.render()
    assert panel.anim.state == "opening" and abs(panel.anim.p - before) < 0.08


def test_close_now_cleans_up_immediately(panel, clock) -> None:
    panel.open()
    window = panel.window
    panel.close_now()
    assert panel.window is None and window.log[-1] == "stop" and not panel.is_open
    assert panel.ui.hidden[-1] is False


# ------------------------------------------------------------- transformación
def test_the_first_frame_is_the_form_and_the_last_is_the_panel(panel, clock) -> None:
    panel.open()
    window = panel.window
    first = panel.render()
    ox, oy = window.x, window.y
    fx0, fy0, fx1, fy1 = (
        round(v) for v in (FORM[0] - ox, FORM[1] - oy, FORM[2] - ox, FORM[3] - oy)
    )
    box = alpha_box(first)
    assert box is not None
    assert abs(box[0] - fx0) <= 2 and abs(box[2] - fx1) <= 2 and abs(box[1] - fy0) <= 2
    settle(panel, clock)
    last = panel.render()
    px0, py0, px1, py1 = (
        round(v)
        for v in (
            panel._panel[0] - ox,
            panel._panel[1] - oy,
            panel._panel[2] - ox,
            panel._panel[3] - oy,
        )
    )
    big = alpha_box(last, threshold=250)
    assert (
        big is not None
        and abs(big[0] - px0) <= 3
        and abs(big[2] - px1) <= 3
        and abs(big[3] - py1) <= 3
    )


def test_the_shape_grows_continuously_during_the_morph(panel, clock) -> None:
    panel.open()
    widths = []
    for _ in range(8):
        frame = panel.render()
        box = alpha_box(frame)
        widths.append(box[2] - box[0])
        clock[0] += cu.MORPH_SECS / 8
    # crece sin saltos y, al final, se pasa un poco del tamaño y se asienta (no más de un 6 %)
    assert widths[:5] == sorted(widths[:5]) and widths[-1] > widths[0] * 3
    assert max(widths) <= panel_size(WORK, 1.0)[0] * 1.06


def test_the_fluid_fades_into_the_panel_background(panel, clock) -> None:
    panel.open()
    start = panel.render()
    ox, oy = panel.window.x, panel.window.y
    cx, cy = round(959 - ox), round(999 - oy)
    green_start = start[cy, cx, 1]  # canal G (el orden de la ventana es BGRA)
    settle(panel, clock)
    end = panel.render()
    inside = end[round(panel._panel[1] - oy) + 200, round(panel._panel[0] - ox) + 200]
    assert green_start > 100 and inside[3] == 255
    assert inside[1] < 60  # el fondo del panel (oscuro), ya no el fluido verdoso


def test_the_content_only_appears_at_the_end(panel, clock) -> None:
    panel.open()
    early = panel.render()
    settle(panel, clock)
    late = panel.render()
    assert late[..., 3].sum() > early[..., 3].sum()
    # el título «Chat» y los botones solo existen cuando el panel ya está abierto
    top = late[
        round(panel._panel[1] - panel.window.y) + 8 : round(panel._panel[1] - panel.window.y) + 50
    ]
    assert (top[..., :3].max(axis=2) > 120).any()


def test_reduced_motion_only_fades(panel, clock) -> None:
    panel.ui._reduced = True
    panel.open()
    clock[0] += cu.FADE_SECS / 2
    frame = panel.render()
    ox, _oy = panel.window.x, panel.window.y
    box = alpha_box(frame, threshold=100)  # sin contar la sombra, que es muy tenue
    px0, px1 = round(panel._panel[0] - ox), round(panel._panel[2] - ox)
    assert abs(box[0] - px0) <= 3 and abs(box[2] - px1) <= 3  # ya tiene el tamaño del panel
    assert frame[..., 3].max() < 255  # pero aún está a medio fundir
    clock[0] += 1
    assert panel.render()[..., 3].max() == 255


def test_the_render_asks_for_60_fps_only_while_something_moves(panel, clock) -> None:
    panel.open()
    panel.render()
    assert panel.window._interval == pytest.approx(1 / 60)
    settle(panel, clock)
    panel.render()
    assert panel.window._interval == pytest.approx(1 / 30)
    panel.view.model.add_user("hola")
    panel.view.model.begin_assistant()
    panel.render()
    assert panel.window._interval == pytest.approx(1 / 60)  # responde: hay puntos que animar


def test_the_content_is_cached_while_nothing_changes(panel, clock, monkeypatch) -> None:
    panel.open()
    settle(panel, clock)
    calls = []
    original = panel.view.render
    monkeypatch.setattr(panel.view, "render", lambda now: (calls.append(1), original(now))[1])
    for _ in range(5):
        clock[0] += 0.01
        panel.render()
    assert len(calls) <= 1  # quieto: se reutiliza el contenido
    before = len(calls)
    panel.view.paste("x")
    panel.render()
    assert len(calls) == before + 1  # al escribir se rehace una vez


# ------------------------------------------------------------------ entradas
def local_to_window(panel: ChatPanel, lx: float, ly: float) -> tuple[int, int]:
    return (
        round(lx + panel._panel[0] - panel.window.x),
        round(ly + panel._panel[1] - panel.window.y),
    )


def spot(panel: ChatPanel, kind: str, data=None) -> tuple[int, int]:
    panel.view.render(1.0)
    for (x0, y0, x1, y1), k, d in panel.view._hits:
        if k == kind and (data is None or d == data):
            return local_to_window(panel, (x0 + x1) / 2, (y0 + y1) / 2)
    raise AssertionError(kind)


def test_clicks_are_swallowed_while_animating(panel, clock) -> None:
    panel.open()
    assert panel.pointer_down(300, 300) is True and panel.ui.calls == []
    assert panel.key(13, False) is True and panel.char("a") and panel.paste("b")
    assert panel.view.input.text == ""  # nada se escribe mientras se anima


def test_clicking_a_control_acts_and_clicking_empty_space_drags_the_window(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    panel.view.paste("resume el informe")
    assert panel.pointer_down(*spot(panel, "send")) is True
    assert panel.ui.calls == [("send", "resume el informe")]
    panel.view.render(1.0)
    empty = local_to_window(panel, 200, 300)
    assert panel.pointer_down(*empty) is False  # sin controles: arrastra la ventana
    assert panel.pointer_down(*spot(panel, "voice")) is True
    assert panel.ui.calls[-1] == ("voice", True)


def test_header_buttons_reach_the_app(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    panel.pointer_down(*spot(panel, "settings"))
    assert panel.ui.calls[-1] == ("menu",)  # ajustes sin cerrar el chat
    panel.pointer_down(*spot(panel, "mic"))
    assert panel.ui.calls[-1] == ("mic", True)  # silencia el mismo micro de la barra
    panel.pointer_down(*spot(panel, "new"))
    assert panel.ui.calls[-1] == ("new",)
    panel.pointer_down(*spot(panel, "close"))
    assert panel.anim.state == "closing"


def test_confirmation_buttons_and_stop(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    panel.view.model.ask_confirm("Mover 3 archivos", True)
    panel.pointer_down(*spot(panel, "confirm_yes"))
    panel.pointer_down(*spot(panel, "confirm_no"))
    assert panel.ui.calls[-2:] == [("confirm", True), ("confirm", False)]
    panel.view.model.clear_confirm()
    panel.view.model.add_user("x")
    panel.view.model.begin_assistant()
    panel.pointer_down(*spot(panel, "stop"))
    assert panel.ui.calls[-1] == ("stop",)


def test_keyboard_goes_to_the_view(panel, clock, monkeypatch) -> None:
    monkeypatch.setattr(cu.hk, "os_modifiers", lambda: frozenset())
    panel.open()
    settle(panel, clock)
    for ch in "hola":
        assert panel.char(ch)
    assert panel.key(13, False) is True
    assert panel.ui.calls == [("send", "hola")]
    panel.paste("otra")
    assert panel.view.input.text == "otra"
    assert panel.key(27, False) is True and panel.view.input.text == ""  # Esc vacía
    assert panel.key(27, False) is True and panel.anim.state == "closing"  # y luego cierra


def test_shift_enter_adds_a_line_instead_of_sending(panel, clock, monkeypatch) -> None:
    monkeypatch.setattr(cu.hk, "os_modifiers", lambda: frozenset({"shift"}))
    panel.open()
    settle(panel, clock)
    panel.paste("a")
    panel.key(13, False)
    assert panel.view.input.text == "a\n" and panel.ui.calls == []


def test_copy_goes_to_the_window_clipboard(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    model = panel.view.model
    model.add_user("hola")
    reply = model.begin_assistant()
    model.append(reply.id, "Un **resumen**")
    model.finish(reply.id)
    panel.pointer_down(*spot(panel, "copy_msg", "Un resumen"))
    assert panel.window.copied == ["Un resumen"]
    panel.pointer_down(*spot(panel, "copy_msg", "hola"))  # también lo que escribió el usuario
    assert panel.window.copied == ["Un resumen", "hola"]


def test_open_actions_use_the_safe_media_helpers(panel, clock, monkeypatch) -> None:
    opened, revealed = [], []
    monkeypatch.setattr(cu.media, "open_file", lambda info: opened.append(info))
    monkeypatch.setattr(cu.media, "reveal_file", lambda info: revealed.append(info))
    panel._perform(cu.Action("open", "A"))
    panel._perform(cu.Action("reveal", "B"))
    panel._perform(None)
    assert opened == ["A"] and revealed == ["B"]


def test_dropped_files_land_in_the_input(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    panel.files([r"C:\Users\Ana Pérez\Informe Q3.docx"])
    assert "Informe Q3.docx" in panel.view.input.text


def test_the_wheel_scrolls_only_when_open(panel, clock) -> None:
    panel.open()
    assert panel.wheel(120) is False
    settle(panel, clock)
    assert panel.wheel(120) is True


def test_hover_reports_the_hand_cursor_over_controls(panel, clock) -> None:
    panel.open()
    settle(panel, clock)
    x, y = spot(panel, "close")
    assert panel.hover(x, y) is True and panel.view.hover == "close"
    assert panel.hover(*local_to_window(panel, 200, 300)) is False


def test_the_window_stays_inside_the_work_area_when_dragged(panel, clock) -> None:
    panel.open()
    w, h = panel.window.w, panel.window.h
    m = round(cu.MARGIN)  # solo la sombra puede salirse; el panel no
    assert panel.clamp(-500, -500) == (-m, -m)
    assert panel.clamp(5000, 5000) == (WORK[2] - w + m, WORK[3] - h + m)
    assert panel.clamp(300, 100) == (300, 100)


def test_the_hotkey_label_is_human(panel, clock) -> None:
    assert cu._hotkey_label("ctrl+alt+t") == "Ctrl+Alt+T"
    assert cu._hotkey_label("") == "Ctrl+Alt+T"
    panel.ui.chat_hotkey = "ctrl+shift+f2"
    panel.open()
    assert panel.view.hotkey_label == "Ctrl+Mayús+F2"


def test_a_window_that_cannot_start_leaves_everything_clean(panel, clock, monkeypatch) -> None:
    monkeypatch.setattr(FakeWindow, "start", lambda self: False)
    assert panel.open() is False
    assert panel.window is None and not panel.is_open
    assert panel.ui.open_flags[-1] is False and panel.ui.hidden[-1] is False


# ----------------------------------------------------- integración con la barra
def test_the_bar_stops_drawing_its_form_while_the_chat_replaces_it() -> None:
    model = D.DropModel("circle")
    for step in range(90):
        model.update(
            step / 60, 1 / 60, "idle", 0.0, "dark", True, False, False, False, in_call=True
        )
    shown = D.render_model(model, 148, 128, 1.0, "dark", False, True, False, 1.0)
    slot = D.slots("circle", 1.0, model.dock)["chat"]
    assert model.item_at(*slot) == "chat"  # con la barra a la vista, el hub responde
    model.form_hidden = True
    hidden = D.render_model(model, 148, 128, 1.0, "dark", False, True, False, 1.0)
    fx, fy = (round(v) for v in D.form_center("circle"))
    assert shown[fy, fx, 3] > 200 and hidden[fy, fx, 3] == 0
    # Con el chat abierto el hub no se ve ni se pulsa: el clic llega al chat.
    assert model.item_at(*slot) is None and not model.near(*slot)
    assert hidden[round(slot[1]), round(slot[0]), 3] == 0


def test_the_bar_exposes_the_chat_api(tmp_path: Path) -> None:
    bar = D.DropUI(None, cfg=SimpleNamespace(ui_layout="circle"))
    assert bar.chat_model is None and bar.chat_is_open is False
    assert bar.form_size() == (D.CIRCLE_D, D.CIRCLE_D)
    bar.set_chat_open(True)
    bar.set_form_hidden(True)
    assert bar._chat_open and bar.model.form_hidden
    assert bar.chat_body_rgb() == tuple(
        __import__("ezagent.voice.notch.bar_theme", fromlist=["x"]).card_palette(bar._theme_name())[
            "BODY"
        ][:3]
    )
    bar.toggle_chat()  # sin modelo no abre, y no revienta
    assert not bar.chat_is_open


def test_a_pending_confirmation_blocks_opening_but_not_closing(tmp_path: Path, clock) -> None:
    bar = D.DropUI(None, cfg=SimpleNamespace(ui_layout="circle"))
    opened: list[str] = []
    bar._chat = SimpleNamespace(is_open=False, toggle=lambda: opened.append("toggle"))
    bar._asking = True
    bar.toggle_chat()
    assert opened == []
    bar._chat.is_open = True
    bar.toggle_chat()
    assert opened == ["toggle"]


def test_hiding_or_closing_the_bar_closes_the_chat() -> None:
    bar = D.DropUI(None, cfg=SimpleNamespace(ui_layout="circle"))
    closed: list[str] = []
    bar._chat = SimpleNamespace(close_now=lambda: closed.append("x"), poll=lambda now: None)
    bar.hide()
    bar.close()
    assert closed == ["x", "x"]
