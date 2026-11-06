"""Pruebas puras de la base visual del notch (T-01 a T-08).

No crean ventanas ni dependen del portapapeles: solo matemática, tokens,
contraste y geometría. Las pruebas de estado y layout del menú se dejan
preparadas para las fases siguientes.
"""

from __future__ import annotations

import math

import pytest

from ezagent.voice.notch import a11y
from ezagent.voice.notch import tokens as T
from ezagent.voice.notch.anim import (
    EASE_IN,
    EASE_INOUT,
    EASE_OUT,
    EASE_SPRING,
    LevelFollower,
    Tween,
)


def test_T01_curvas():
    """Curvas CSS: extremos exactos y muelle con rebase ~10 %."""
    for ease in (EASE_OUT, EASE_IN, EASE_INOUT, EASE_SPRING):
        assert ease(0.0) == pytest.approx(0.0)
        assert ease(1.0) == pytest.approx(1.0)
        assert ease(-1.0) == pytest.approx(0.0)
        assert ease(2.0) == pytest.approx(1.0)

    # EASE_OUT se acelera al principio; EASE_IN, al final. La curva in-out
    # declarada avanza más rápido en la primera mitad (y1=0, y2=1).
    assert EASE_OUT(0.5) > 0.5
    assert EASE_IN(0.5) < 0.5
    assert 0.7 < EASE_INOUT(0.5) < 0.85
    peak = max(EASE_SPRING(x / 100.0) for x in range(101))
    assert 1.05 < peak < 1.13


def test_T02_tween_reversible():
    """Un tween retargeteado parte del valor actual y no salta."""
    tween = Tween(0.0)
    tween.to(100.0, 0.0, 1000.0, lambda x: x)
    assert tween.active
    assert tween.update(0.5) == pytest.approx(50.0)
    half = tween.value
    tween.to(0.0, 0.5, 1000.0, lambda x: x)
    assert tween.update(0.5) == pytest.approx(half)
    assert tween.update(1.0) == pytest.approx(half / 2.0)
    assert tween.update(1.5) == pytest.approx(0.0)
    assert not tween.active
    # Duración cero aplica el objetivo de inmediato.
    tween.to(42.0, 2.0, 0.0, EASE_OUT)
    assert tween.value == pytest.approx(42.0)


def test_T03_level_follower_ataque_y_caida():
    """Ataque 45 ms, caída 200 ms: sube mucho más rápido de lo que baja."""
    follower = LevelFollower(T.VOICE_ATTACK, T.VOICE_RELEASE)
    up = follower.step(1.0, T.VOICE_ATTACK)
    assert up == pytest.approx(1.0 - math.exp(-1.0))
    assert up > 0.6

    before = follower.value
    down = follower.step(0.0, T.VOICE_ATTACK)
    expected = before * math.exp(-T.VOICE_ATTACK / T.VOICE_RELEASE)
    assert down == pytest.approx(expected)
    # En el mismo tiempo, el ataque avanza mucho más que la caída.
    assert up > (1.0 - down)


def test_T04_display_state_si_existe():
    """Preparada para la fase de estado; aún no existe ``state.py``."""
    state = pytest.importorskip(
        "ezagent.voice.notch.state",
        reason="display_state() se implementa en la fase de máquina de estados",
    )
    display_state = state.display_state
    assert display_state(False, True, "speak", True) == "off"
    assert display_state(True, True, "idle", False) == "error"
    assert display_state(True, False, "speak", True) == "speak"
    assert display_state(True, False, "think", True) == "think"
    assert display_state(True, False, "notice", True) == "notice"
    assert display_state(True, False, "listen", True) == "mute"
    assert display_state(True, False, "listen", False) == "listen"
    assert display_state(True, False, "idle", False) == "idle"


def test_T04b_aviso_no_lo_borra_el_nivel_del_microfono():
    """AVISO mantiene prioridad sobre listen/idle mientras dura (9.2)."""
    from ezagent.voice.notch.state import UiState

    ui = UiState(clock=lambda: 0.0)
    ui.set_visible(True, 0.0)
    ui.set_base("listen", 0.0)
    ui.update(0.01)
    ui.notice(0.02)
    ui.update(0.03)
    assert ui.base == "notice"
    ui.set_base("listen", 0.10)
    ui.update(0.11)
    assert ui.base == "notice"
    ui.set_base("idle", 0.20)
    ui.update(0.21)
    assert ui.base == "notice"
    ui.set_base("speak", 0.30)
    ui.update(0.31)
    assert ui.base == "speak"


def test_T11_aviso_cae_a_idle_a_los_1500_ms():
    """T-11: el AVISO sin TTS no vuelve a idle a los 1000 ms."""
    from ezagent.voice.notch.state import UiState

    ui = UiState(clock=lambda: 0.0)
    ui.set_visible(True, 0.0)
    ui.update(0.01)
    ui.notice(0.01)
    ui.update(1.1)
    assert ui.base == "notice"
    ui.update(1.55)
    assert ui.base == "idle"


def test_T17_isla_compacta_sin_texto():
    """La isla de 169 px empaqueta los controles y los ojos siguen centrados."""
    assert T.W_COMPACT == 169
    assert T.MIC_BUTTON_COMPACT == T.Rect(19, 4, 47, 32)
    assert T.ARROW_BUTTON_COMPACT == T.Rect(126, 6, 150, 30)
    assert not hasattr(T, "MENU_HANDLE_W")
    assert not hasattr(T, "CALL_BUTTON_COMPACT")
    assert not hasattr(T, "CALL_BUTTON")
    assert T.EYES_CX_COMPACT == pytest.approx(86.5)
    eyes = T.Rect(
        T.EYES_CX_COMPACT - T.EYES_GROUP_MAX_W / 2.0,
        0.0,
        T.EYES_CX_COMPACT + T.EYES_GROUP_MAX_W / 2.0,
        36.0,
    )
    for name, rect in (
        ("mic", T.MIC_BUTTON_COMPACT),
        ("arrow", T.ARROW_BUTTON_COMPACT),
    ):
        assert not eyes.overlaps(rect), f"los ojos se solapan con {name}"
    # Margen mínimo de 12 px dentro del cuerpo compacto (x 7..162).
    assert T.MIC_BUTTON_COMPACT.x0 - 7 >= 12
    assert (T.W_COMPACT - 7) - T.ARROW_BUTTON_COMPACT.x1 >= 12


def test_T18_acople_y_limites_de_arrastre():
    """La píldora se acopla arriba/abajo y nunca sale del monitor."""
    from ezagent.voice.notch.island import Notch

    notch = Notch.__new__(Notch)
    notch._anchor_x = 960.0
    notch.s = 1.0
    notch.W_COMPACT = T.W_COMPACT
    notch.H_ISLAND = 36
    notch._monitor_bottom = lambda x, y: ((0, 0, 1920, 1080), 1040)

    assert notch._attachment_for(0.0) == "top"
    assert notch._attachment_for(12.0) == "top"
    assert notch._attachment_for(12.5) == "float"
    assert notch._attachment_for(500.0) == "float"
    assert notch._attachment_for(992.0) == "bottom"
    assert notch._attachment_for(1004.0) == "bottom"

    assert notch.clamp_position(-30, -20) == (0, 0)
    assert notch.clamp_position(1900, 1100) == (1751, 1004)


def test_T19_superficies_de_isla_y_menu():
    """La isla cerrada mide 169×36; el menú conserva 320 y la pestaña centrada."""
    from ezagent.voice.notch.silhouette import render_surface

    centred = T.W_COMPACT // 2
    top = render_surface(T.W_COMPACT, T.H_ISLAND, attachment="top")
    assert top.shape == (T.H_ISLAND, T.W_COMPACT, 4)
    assert top[0, centred, 3] == 255
    assert top[-1, 0, 3] == 0

    pill = render_surface(T.W_COMPACT, T.H_ISLAND, attachment="float")
    for corner in ((0, 0), (0, -1), (-1, 0), (-1, -1)):
        assert pill[corner[0], corner[1], 3] == 0
    assert pill[T.H_ISLAND // 2, centred, 3] == 255

    bottom = render_surface(T.W_COMPACT, T.H_ISLAND, attachment="bottom")
    assert bottom[-1, centred, 3] == 255
    assert bottom[0, 0, 3] == 0

    menu = render_surface(
        T.W,
        T.H_ISLAND + T.H_MENU,
        attachment="bottom",
        menu_h=T.H_MENU,
        menu_above=True,
    )
    assert menu.shape == (T.H_ISLAND + T.H_MENU, T.W, 4)
    assert menu[-1, T.W // 2, 3] == 255
    assert menu[-1, 0, 3] == 0
    assert menu[0, T.W // 2, 3] == 255
    assert menu[0, 0, 3] == 0


def test_T20_menu_arriba_conserva_el_cuerpo_opaco():
    """Abrir hacia arriba compone el menú; no perfora la superficie."""
    from ezagent.voice.notch.island import Notch

    notch = Notch(None)
    notch.set_theme("dark")
    notch._visible = True
    notch._ui.set_visible(True)
    notch._ui.set_base("idle")
    notch._show_t0 = None
    notch._hide_t0 = None

    class _FakeMenu:
        class settings:  # noqa: N801 - imita la interfaz real
            @staticmethod
            def dirty() -> bool:
                return False

        def height(self, now: float) -> float:
            return 560.0

        def update(self, now: float) -> None:
            pass

        def draw(self, buf, now: float) -> None:
            pass

        def is_animating(self, now: float) -> bool:
            return False

    notch._menu = _FakeMenu()
    notch._menu_above = True
    notch._menu_open = True
    notch._tab_w = Tween(float(T.W))
    frame = notch.render_frame()
    assert frame.shape[:2] == (596, 320)
    # El cuerpo del menú es opaco y ocupa todo el ancho aunque el contenido
    # no pinte nada.
    assert frame[100, 20, 3] == 255
    assert frame[100, 160, 3] == 255
    assert frame[560, 160, 3] == 255


def test_T21_mute_rojo_y_sin_boton_de_llamada():
    """El mic muteado se pinta en ERR y la flecha sigue en la derecha."""
    from ezagent.voice.notch.island import Notch

    notch = Notch(None)
    notch.set_theme("dark")
    notch._visible = True
    notch._ui.set_visible(True)
    notch._ui.set_base("idle")
    notch._ui.set_muted(True)
    frame = notch.render_frame()
    assert frame.shape[:2] == (T.H_ISLAND, T.W_COMPACT)
    blue, green, red, alpha = frame[18, 33]
    assert alpha == 255
    assert red > 180 and green < 100 and blue < 120
    assert notch._hit_button(138, 18) == "arrow"
    assert notch._hit_button(84.5, 34.0) is None


def test_T22_mic_emite_la_intencion_mute():
    """El control se llama «mic», pero la app escucha la intención «mute»."""
    from ezagent.voice.notch.island import Notch

    notch = Notch(None)
    actions: list[str] = []
    notch.on_action = actions.append
    notch._activate("mic")
    assert actions == ["mute"]


def test_T24_expansion_continua_del_menu():
    """El menú y la pestaña comparten ancho; los controles se reparten."""
    from ezagent.voice.notch.island import Notch
    from ezagent.voice.notch.silhouette import render_surface

    full = render_surface(
        T.W,
        T.H_ISLAND + T.H_MENU,
        tab_width=T.W,
        body_width=T.W,
        menu_h=T.H_MENU,
        attachment="top",
    )
    assert full.shape == (T.H_ISLAND + T.H_MENU, T.W, 4)
    assert full[100, 20, 3] == 255
    assert full[100, 160, 3] == 255

    partial = render_surface(
        T.W,
        T.H_ISLAND + T.H_MENU,
        tab_width=220.0,
        body_width=220.0,
        menu_h=T.H_MENU,
        attachment="top",
    )
    assert partial[100, 0, 3] == 0
    assert partial[100, 160, 3] == 255

    notch = Notch.__new__(Notch)
    _mic, eyes_compact, arrow_compact = notch._control_layout(
        float(T.W_COMPACT)
    )
    assert eyes_compact == pytest.approx(T.EYES_CX_COMPACT)
    assert arrow_compact.center == (138, 18)
    _mic, eyes_wide, arrow_wide = notch._control_layout(float(T.W))
    assert eyes_wide == pytest.approx(T.EYES_CX_WIDE)
    assert arrow_wide.center == (289, 18)
    eyes_open = T.Rect(
        T.EYES_CX_WIDE - T.EYES_GROUP_MAX_W / 2.0,
        0.0,
        T.EYES_CX_WIDE + T.EYES_GROUP_MAX_W / 2.0,
        36.0,
    )
    for first, second in (
        (T.MIC_BUTTON_COMPACT, eyes_open),
        (eyes_open, arrow_wide),
        (T.MIC_BUTTON_COMPACT, arrow_wide),
    ):
        assert not first.overlaps(second)
    for rect in (T.MIC_BUTTON_COMPACT, eyes_open, arrow_wide):
        assert T.BODY_X0 + T.BODY_MARGIN_MIN <= rect.x0
        assert rect.x1 <= T.W - T.FLARE - T.BODY_MARGIN_MIN


def test_T23_arranque_en_escucha_y_muestra_isla():
    """Abrir la app inicia la llamada y recupera la isla si estaba oculta."""
    from ezagent.voice.app import VoiceApp

    calls: list[tuple] = []

    class _Notch:
        def show(self, state: str) -> None:
            calls.append(("show", state))

    class _Cfg:
        notch_hidden = True

        @staticmethod
        def save_key(key: str, value: bool) -> None:
            calls.append(("save", key, value))

    app = VoiceApp.__new__(VoiceApp)
    app.in_call = False
    app._notch_hidden = True
    app.cfg = _Cfg()
    app.notch = _Notch()
    app._start_call = lambda: calls.append(("call",))

    VoiceApp._start_listening_on_launch(app)

    assert app._notch_hidden is False
    assert app.cfg.notch_hidden is False
    assert calls == [
        ("save", "notch_hidden", False),
        ("show", "idle"),
        ("call",),
    ]


def test_T16_save_key_no_persiste_cambios_sin_guardar(tmp_path):
    """Ocultar la isla no debe guardar los ajustes del menú sin confirmar."""
    import json

    from ezagent.voice.config import VoiceConfig

    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps(
            {"speed": 0.95, "notch_hidden": False, "future_key": 7}
        ),
        encoding="utf-8",
    )
    cfg = VoiceConfig.load(path)
    cfg.speed = 1.2  # edición en vivo, todavía sin Guardar
    cfg.notch_hidden = True
    cfg.save_key("notch_hidden", True, path)
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["notch_hidden"] is True
    assert data["speed"] == 0.95
    assert data["future_key"] == 7
    assert not path.with_name(path.name + ".tmp").exists()


def test_T05_layout_menu_tabla_7_3():
    """El layout coincide con la tabla 7.3 y con el recorte por pantalla baja."""
    from ezagent.voice.notch import menu_layout as ml

    layout = ml.layout_menu()
    assert (layout["header"].y0, layout["header"].y1) == (44, 72)
    assert (layout["label_audio"].y0, layout["label_audio"].y1) == (80, 100)
    assert (layout["group_audio"].y0, layout["group_audio"].y1) == (100, 296)
    assert (layout["group_engine"].y0, layout["group_engine"].y1) == (328, 400)
    assert (layout["group_tts"].y0, layout["group_tts"].y1) == (432, 600)
    assert (layout["footer"].y0, layout["footer"].y1) == (1520, 1564)
    assert layout["total"].y1 == 1576
    assert ml.MENU_HEIGHT == 560

    # Selector de micrófono con 3 elementos: 96 px extra y el resto se desplaza.
    opened = ml.layout_menu({"input_device_index": 96})
    assert opened["group_audio"].y1 == 392
    assert opened["footer"].y0 == 1616
    assert opened["total"].y1 == 1672

    # Pantalla baja: el pie se sube y el contenido entre cabecera y pie scrollea.
    short = ml.layout_menu(viewport_h=537)
    assert (short["footer"].y0, short["footer"].y1) == (481, 525)
    assert short["total"].y1 == 537


def test_T07_contraste_aa():
    """Parejas obligatorias AA en ambos temas y excepciones documentadas."""
    assert a11y.contrast((0, 0, 0), (255, 255, 255)) == pytest.approx(21.0)
    assert a11y.contrast((10, 20, 30), (10, 20, 30)) == pytest.approx(1.0)

    assert a11y.validate_all_palettes() == {"dark": True, "light": True}
    for theme in ("dark", "light"):
        assert a11y.failures(theme) == []
        for check in a11y.contrast_report(theme):
            minimum = (
                a11y.AA_TEXT if check.kind == "text" else a11y.AA_GRAPHIC
            )
            assert check.minimum == minimum
            assert check.ratio >= minimum

    # Los ojos y la palabra de estado en claro se ajustaron un paso para
    # cumplir AA; los colores tenues de la spec quedan solo como gráfico
    # (MINT/WARM) y siguen visibles en el informe informativo.
    advisory = a11y.failures("light", include_advisory=True)
    flagged = {(item.foreground, item.background) for item in advisory}
    assert ("EYE_IDLE", "BODY") not in flagged
    assert ("EYE_MUTE", "BODY") not in flagged
    assert ("MINT", "BODY") in flagged
    assert ("WARM", "BODY") in flagged


def test_T08_tokens_exactos():
    """Valores visuales cerrados de las secciones 3, 4.1 y 6.1."""
    assert T.W == 320
    assert T.H_ISLAND == 36
    assert T.FLARE == 7
    assert T.R_ISLAND == 14
    assert T.R_MENU == 18
    assert T.R_GROUP == 12
    assert T.BODY_X0 == 7
    assert T.BODY_X1 == 313
    assert T.BODY_WIDTH == 306

    assert (T.SP_1, T.SP_2, T.SP_3, T.SP_4) == (2, 4, 6, 8)
    assert (T.SP_5, T.SP_6, T.SP_7, T.SP_8) == (12, 16, 20, 24)

    assert (T.T_MICRO, T.T_FAST, T.T_BASE, T.T_SLOW) == (120, 180, 240, 320)
    assert T.T_OPEN == 380
    assert T.T_CLOSE_EYES == 200
    assert T.T_CLOSE_RETRACT == 260
    assert T.VOICE_ATTACK == 45
    assert T.VOICE_RELEASE == 200
    assert (T.FPS_ACTIVE, T.FPS_IDLE) == (60, 30)
    assert T.BEZIER_OUT == (0.2, 0.8, 0.2, 1.0)
    assert T.BEZIER_IN == (0.5, 0.0, 0.9, 0.4)
    assert T.BEZIER_INOUT == (0.4, 0.0, 0.2, 1.0)
    assert T.BEZIER_SPRING == (0.34, 1.56, 0.64, 1.0)

    assert (T.FS_TITLE.size, T.FS_TITLE.weight) == (16, 600)
    assert (T.FS_ROW.size, T.FS_ROW.weight) == (14, 400)
    assert (T.FS_BODY.size, T.FS_BODY.weight) == (13, 400)
    assert (T.FS_SECONDARY.size, T.FS_SECONDARY.weight) == (12, 600)
    assert (T.FS_MICRO.size, T.FS_MICRO.weight) == (11, 500)
    assert T.FS_MICRO.mono
    assert T.FS_MICRO.tracking_em == pytest.approx(0.1)
    assert T.FS_MICRO.uppercase

    assert set(T.DARK) == set(T.LIGHT) == set(T.COLOR_KEYS)
    assert T.DARK["BODY"] == (14, 15, 19)
    assert T.DARK["MINT"] == (94, 234, 196)
    assert T.DARK["EYE_IDLE"] == (62, 146, 125)
    assert T.LIGHT["BODY"] == (255, 255, 255)
    assert T.LIGHT["MINT"] == (12, 145, 126)
    assert T.LIGHT["EYE_IDLE"] == (47, 127, 116)
    assert T.LIGHT["EYE_MUTE"] == (160, 100, 18)
    assert T.LIGHT["WORD_MINT"] == (10, 122, 106)
    assert T.DARK["LINE"] == (255, 255, 255, 23)
    assert T.LIGHT["LINE"] == (0, 0, 0, 26)
    assert T.DARK["LINE_2"] == (255, 255, 255, 15)
    assert T.LIGHT["LINE_2"] == (0, 0, 0, 15)

    assert T.W_COMPACT == 169
    assert T.MIC_BUTTON_COMPACT.center == (33, 18)
    assert T.ARROW_BUTTON_COMPACT.center == (138, 18)
    assert (T.EYES_CX_COMPACT, T.EYES_CY) == (86.5, 18)
    assert not hasattr(T, "STATE_WORD")
    assert not hasattr(T, "CALL_BUTTON")
    assert not hasattr(T, "MENU_HANDLE_W")

    # Las fuentes del proyecto y los respaldos declarados existen en Windows.
    for family, weights in T.FONT_FILES.items():
        for weight, path in weights.items():
            assert path.exists(), f"falta {family} {weight}: {path}"
    for family, weights in T.FONT_FALLBACKS.items():
        for weight, paths in weights.items():
            assert paths, f"sin fallback para {family} {weight}"

    # El glow es 0,10 en oscuro y 0 en claro.
    assert T.glow_max("dark") == pytest.approx(0.10)
    assert T.glow_max("light") == 0.0


def test_T20_acople_lateral_y_superficie():
    """La píldora lateral mide 36x186 y el panel crece hacia dentro."""
    from ezagent.voice.notch.island import Notch
    from ezagent.voice.notch import silhouette

    closed = silhouette.render_side_surface(
        T.W_SIDE, T.H_SIDE, attachment="left"
    )
    assert closed.shape == (T.H_SIDE, T.W_SIDE, 4)
    assert closed[T.H_SIDE // 2, 0, 3] == 255
    assert closed[0, T.W_SIDE - 1, 3] == 0

    opened = silhouette.render_side_surface(
        T.W_SIDE + T.W,
        T.H_MENU,
        attachment="left",
        panel_height=T.H_MENU,
        panel_width=T.W,
    )
    assert opened.shape == (T.H_MENU, T.W_SIDE + T.W, 4)
    assert opened[10, T.W_SIDE + 20, 3] == 255
    assert opened[T.H_MENU // 2, 5, 3] == 255

    right = silhouette.render_side_surface(
        T.W_SIDE + T.W,
        T.H_MENU,
        attachment="right",
        panel_height=T.H_MENU,
        panel_width=T.W,
    )
    assert right[10, T.W - 20, 3] == 255

    notch = Notch.__new__(Notch)
    notch.s = 1.0
    notch._anchor_x = 960.0
    notch._monitor_bottom = lambda x, y: ((0, 0, 1920, 1080), 1040)
    assert notch._attachment_for_rect(0, 500, T.W_SIDE, T.H_SIDE) == "left"
    assert (
        notch._attachment_for_rect(1920 - T.W_SIDE, 500, T.W_SIDE, T.H_SIDE)
        == "right"
    )
    assert notch._attachment_for_rect(0, 0, T.W_SIDE, T.H_SIDE) == "top"
    assert notch._attachment_for_rect(900, 500, T.W_SIDE, T.H_SIDE) == "float"


def test_T21_el_panel_lateral_se_ensancha_hacia_dentro():
    from ezagent.voice.notch.island import Notch

    class _MenuStub:
        viewport_height = 560.0

    notch = Notch.__new__(Notch)
    notch._attachment = "left"
    notch._menu = _MenuStub()
    width, height, panel_x, tab_x, tab_y, progress = notch._side_window_geometry(560.0)
    assert (width, height) == (T.W_SIDE + T.W, T.H_MENU)
    assert progress == 1.0
    assert panel_x == T.W_SIDE
    assert tab_x == 0.0
    assert tab_y == pytest.approx((T.H_MENU - T.H_SIDE) / 2.0)

    notch._attachment = "right"
    width, _height, panel_x, tab_x, _tab_y, _p = notch._side_window_geometry(560.0)
    assert width == T.W_SIDE + T.W
    assert panel_x == 0.0
    assert tab_x == T.W
