"""Menú con el nuevo diseño: Apariencia, Escucha, acciones y colores de la barra."""

from __future__ import annotations

import queue
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice import app as app_module
from ezagent.voice.app import VoiceApp
from ezagent.voice.config import VoiceConfig
from ezagent.voice.notch import bar_theme as BT
from ezagent.voice.notch import drop as D
from ezagent.voice.notch import menu as M
from ezagent.voice.notch import menu_layout as ML
from ezagent.voice.notch import settings as S


def _menu(**values):
    settings = S.MenuSettings.from_app(values)
    calls: dict = {"live": [], "actions": []}
    menu = M.Menu(
        settings,
        on_live_change=lambda k, v: calls["live"].append((k, v)),
        on_action=calls["actions"].append,
    )
    menu.open(10.0)
    menu.update(11.0)
    return menu, settings, calls


def _row_center(menu: M.Menu, key: str) -> tuple[float, float]:
    row = menu._layout()[key]
    return 253.0, (row.y0 + row.y1) / 2.0 - menu.scroll_offset


def _scroll_to(menu: M.Menu, key: str) -> None:
    row = ML.layout_menu()[key]
    target = max(0.0, min(menu.scroll_max, row.y0 - 200.0))
    menu._scroll.value = target
    menu._scroll_target = target


# ------------------------------------------------------------ catálogo
def test_apariencia_y_escucha_son_ajustes_instantaneos():
    assert set(S.INSTANT_KEYS) == set(app_module.INSTANT_SETTINGS)
    assert not set(S.INSTANT_KEYS) & set(S.EDITABLE_KEYS)  # no cuentan para «Guardar»
    assert set(S.INSTANT_KEYS) <= set(S.ALL_KEYS)
    assert set(S.INSTANT_KEYS) <= set(S.LABELS)
    fields = set(VoiceConfig.__dataclass_fields__)
    assert set(S.INSTANT_KEYS) <= fields


def test_valores_por_defecto_coinciden_con_la_configuracion():
    cfg = VoiceConfig()
    for key in S.INSTANT_KEYS:
        assert S.DEFAULT_VALUES[key] == getattr(cfg, key), key


@pytest.mark.parametrize(
    ("key", "raw", "expected"),
    [
        ("ui_layout", "CIRCLE", "circle"),
        ("ui_layout", "raro", "pill"),
        ("theme", "light", "light"),
        ("ui_motion", "x", "system"),
        ("close_action", "hangup", "hangup"),
        ("voice_wake", "no", False),
        ("wake_phrase", "   ", "EzAgent, escúchame"),
    ],
)
def test_normalizacion(key, raw, expected):
    assert S.normalize_setting(key, raw) == expected


def test_cambiar_apariencia_no_ensucia_el_guardado():
    settings = S.MenuSettings.from_app({})
    settings.set("ui_layout", "circle")
    settings.set("theme", "dark")
    assert not settings.dirty()
    assert [o.label for o in settings.select_options("ui_layout")] == ["Píldora", "Círculo"]
    assert settings.selector_enabled("ui_motion")


def test_apariencia_y_escucha_van_primero_en_el_menu():
    names = [name for name, _rows in ML.GROUPS]
    assert names[:3] == ["appearance", "listening", "audio"]
    assert ML.GROUP_LABELS["appearance"] == "APARIENCIA"
    conversation = dict(ML.GROUPS)["conversation"]
    assert ("repeat_last", "action") in conversation
    assert ("new_conversation", "action") in conversation
    assert set(M.BLOCK_DELAYS_MS) >= {"header", "footer", *names}


# ------------------------------------------------------------ menú
def test_elegir_forma_emite_el_cambio_en_vivo():
    menu, settings, calls = _menu()
    x, y = _row_center(menu, "ui_layout")
    menu.pointer_down(150, y, 11.0)
    menu.pointer_up(150, y, 11.0)
    menu.update(11.4)
    hits = [menu.hit(150, y + dy, 11.4) for dy in range(20, 80, 4)]
    hit = next(h for h in hits if h is not None and h.kind == "option" and h.value == "circle")
    menu.pointer_down(150, hit.rect.center[1], 11.4)
    menu.pointer_up(150, hit.rect.center[1], 11.4)
    assert calls["live"][-1] == ("ui_layout", "circle")
    assert settings.get("ui_layout") == "circle"


def test_repetir_actua_al_primer_clic():
    menu, _, calls = _menu()
    _scroll_to(menu, "repeat_last")
    x, y = _row_center(menu, "repeat_last")
    menu.pointer_down(x, y, 11.0)
    menu.pointer_up(x, y, 11.0)
    assert calls["actions"] == ["repeat_last"]


def test_nueva_conversacion_pide_un_segundo_clic():
    menu, _, calls = _menu()
    _scroll_to(menu, "new_conversation")
    x, y = _row_center(menu, "new_conversation")
    menu.pointer_down(x, y, 11.0)
    menu.pointer_up(x, y, 11.0)
    assert calls["actions"] == [] and menu.is_armed("new_conversation", 11.0)
    menu.pointer_down(x, y, 12.0)
    menu.pointer_up(x, y, 12.0)
    assert calls["actions"] == ["new_conversation"]
    assert not menu.is_armed("new_conversation", 12.0)


def test_la_confirmacion_caduca():
    menu, _, calls = _menu()
    _scroll_to(menu, "new_conversation")
    x, y = _row_center(menu, "new_conversation")
    menu.pointer_down(x, y, 11.0)
    menu.pointer_up(x, y, 11.0)
    later = 11.0 + M.ARM_SECS + 0.1
    menu.pointer_down(x, y, later)
    menu.pointer_up(x, y, later)
    assert calls["actions"] == []  # caducó: este clic solo vuelve a armar
    assert menu.is_armed("new_conversation", later)


def test_el_menu_usa_los_colores_de_la_barra():
    menu, _, _ = _menu()
    menu.set_theme("light")
    menu.set_palette_overrides(BT.card_overrides)
    pal = menu._palette()
    assert pal["BODY"] == BT.card_overrides("light")["BODY"]
    assert pal["MINT_BTN"] == BT.card_overrides("light")["MINT_BTN"]
    menu.set_theme("dark")
    assert menu._palette()["BODY"] == (17, 22, 38)  # azul noche, no el marrón antiguo
    buf = np.zeros((round(ML.ISLAND_H + menu.viewport_height) + 4, 320, 4), np.float32)
    menu.draw(buf, 11.0)  # dibuja sin fallar con la paleta nueva


# ------------------------------------------------------------ colores
@pytest.mark.parametrize("theme", ["light", "dark"])
def test_contraste_de_la_tarjeta(theme):
    pal = BT.card_palette(theme)
    assert BT.contrast(pal["TEXT"], pal["BODY"]) >= 4.5
    assert BT.contrast(pal["TEXT"], BT.body_top(theme)) >= 4.5
    assert BT.contrast(pal["TEXT_3"], pal["GROUP"]) >= 4.5
    assert BT.contrast(pal["TEXT"], pal["GROUP"]) >= 4.5
    assert BT.contrast(pal["MINT"], pal["GROUP"]) >= 4.5
    assert BT.contrast(pal["MINT_INK"], pal["MINT_BTN"]) >= 4.5


def test_cuerpo_de_la_tarjeta_fresco_con_degradado_suave():
    body = D.render_card_body(60, 400, 1.0, "light", fresh=True)
    top, bottom = body[10, 30, :3] * 255, body[390, 30, :3] * 255
    assert np.allclose(bottom, BT.card_overrides("light")["BODY"], atol=1.5)
    assert top[2] >= top[0] + 10  # arriba, un toque de cielo
    assert np.abs(top - bottom).max() < 14  # degradado muy suave
    old = D.render_card_body(60, 40, 1.0, "light")
    assert not np.allclose(old[20, 30, :3] * 255, bottom, atol=1.5)  # sin fresh: tokens antiguos


# ------------------------------------------------------------ app
class _Notch:
    def __init__(self) -> None:
        self.calls: list = []

    def set_layout(self, v):
        self.calls.append(("layout", v))

    def set_theme(self, v):
        self.calls.append(("theme", v))

    def set_motion(self, v):
        self.calls.append(("motion", v))


def _app():
    app = VoiceApp.__new__(VoiceApp)
    saved: dict = {}
    cfg = VoiceConfig()
    cfg.save_key = lambda k, v, path=None: saved.__setitem__(k, v)  # type: ignore[method-assign]
    app.cfg = cfg
    app.notch = _Notch()
    return app, saved


@pytest.mark.parametrize(
    ("key", "value", "call"),
    [
        ("ui_layout", "circle", ("layout", "circle")),
        ("theme", "light", ("theme", "light")),
        ("ui_motion", "reduced", ("motion", "reduced")),
    ],
)
def test_ajuste_instantaneo_aplica_y_guarda(key, value, call):
    app, saved = _app()
    VoiceApp._apply_live_setting(app, key, value)
    assert getattr(app.cfg, key) == value and saved[key] == value
    assert app.notch.calls == [call]


def test_ajustes_de_escucha_se_guardan_sin_tocar_la_barra():
    app, saved = _app()
    VoiceApp._apply_live_setting(app, "close_action", "hangup")
    VoiceApp._apply_live_setting(app, "voice_wake", False)
    VoiceApp._apply_live_setting(app, "wake_phrase", "EzAgent, vuelve")
    assert (app.cfg.close_action, app.cfg.voice_wake, app.cfg.wake_phrase) == (
        "hangup",
        False,
        "EzAgent, vuelve",
    )
    assert saved == {
        "close_action": "hangup",
        "voice_wake": False,
        "wake_phrase": "EzAgent, vuelve",
    }
    assert app.notch.calls == []


def test_nueva_conversacion_borra_el_historial():
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app._turn_gen = 3
    app._tts_stop = None
    app._speaking_kind = "answer"
    app.last_summary = "algo"
    reset: list = []
    app.messenger = SimpleNamespace(reset_history=lambda: reset.append(True))
    spoken: list = []
    app._speak = lambda text, kind="answer": spoken.append(text)
    VoiceApp._menu_action_ui(app, "new_conversation")
    assert reset == [True] and app.last_summary == "" and app._turn_gen == 4
    assert spoken == ["Listo, empezamos de cero."]


def test_repetir_desde_el_menu_usa_el_atajo():
    app = VoiceApp.__new__(VoiceApp)
    calls: list = []
    app._handle_hotkey = calls.append
    VoiceApp._menu_action_ui(app, "repeat_last")
    assert calls == ["repeat"]


def test_reset_history_conserva_el_sistema():
    from ezagent.voice.llm import Messenger

    m = Messenger.__new__(Messenger)
    m.messages = [{"role": "system", "content": "s"}, {"role": "user", "content": "u"}]
    m._last_user_text = "u"
    Messenger.reset_history(m)
    assert m.messages == [{"role": "system", "content": "s"}] and m._last_user_text == ""


def test_icono_orbe_de_cielo_con_nubes():
    from ezagent.voice.brand import SVG, mark_image

    im = np.asarray(mark_image(64)).astype(int)
    assert im[0, 0, 3] == 0  # esquinas transparentes: es un orbe
    top, bottom = im[14, 32], im[50, 32]
    assert top[2] > top[0] + 60  # arriba, cielo azul
    assert bottom[:3].min() > 200  # abajo, nubes blancas
    assert "#D4F25A" not in SVG  # sin la lima antigua
