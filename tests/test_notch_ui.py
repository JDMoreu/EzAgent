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
    assert (layout["label_appearance"].y0, layout["label_appearance"].y1) == (80, 100)
    assert (layout["label_audio"].y0, layout["label_audio"].y1) == (368, 388)
    assert (layout["group_audio"].y0, layout["group_audio"].y1) == (388, 512)
    assert (layout["group_vad"].y0, layout["group_vad"].y1) == (544, 800)
    assert (layout["group_tts"].y0, layout["group_tts"].y1) == (1112, 1316)
    assert (layout["footer"].y0, layout["footer"].y1) == (2368, 2412)
    assert layout["total"].y1 == 2424
    assert ml.MENU_HEIGHT == 560

    # Selector de micrófono con 3 elementos: 96 px extra y el resto se desplaza.
    opened = ml.layout_menu({"input_device_index": 96})
    assert opened["group_audio"].y1 == 608
    assert opened["footer"].y0 == 2464
    assert opened["total"].y1 == 2520

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
    # Con la paleta nueva MINT/WARM cumplen sobre BODY; solo quedan
    # orientativos los pares de texto terciario sobre superficies elevadas.
    assert ("MINT", "BODY") not in flagged
    assert ("WARM", "BODY") not in flagged


def test_T08_tokens_exactos():
    """Valores visuales cerrados de las secciones 3, 4.1 y 6.1."""
    assert T.W == 320
    assert T.H_ISLAND == 36
    assert T.FLARE == 7
    assert T.R_MENU == 18
    assert T.R_GROUP == 12
    assert T.BODY_X0 == 7
    assert T.BODY_X1 == 313

    assert (T.SP_1, T.SP_2, T.SP_3, T.SP_4) == (2, 4, 6, 8)
    assert (T.SP_5, T.SP_6, T.SP_7, T.SP_8) == (12, 16, 20, 24)

    assert (T.T_MICRO, T.T_FAST, T.T_BASE, T.T_SLOW) == (120, 180, 240, 320)
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
    assert T.DARK["BODY"] == (20, 18, 16)
    assert T.DARK["MINT"] == (212, 242, 90)
    assert T.DARK["EYE_IDLE"] == (140, 160, 63)
    assert T.LIGHT["BODY"] == (250, 248, 245)
    assert T.LIGHT["MINT"] == (77, 107, 0)
    assert T.LIGHT["EYE_IDLE"] == (111, 138, 31)
    assert T.LIGHT["EYE_MUTE"] == (163, 95, 0)
    assert T.LIGHT["WORD_MINT"] == (77, 107, 0)
    assert T.DARK["LINE"] == (244, 239, 232, 23)
    assert T.LIGHT["LINE"] == (26, 23, 20, 26)
    assert T.DARK["LINE_2"] == (244, 239, 232, 15)
    assert T.LIGHT["LINE_2"] == (26, 23, 20, 15)

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
