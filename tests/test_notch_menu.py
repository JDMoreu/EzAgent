"""Pruebas del menú desplegable del notch (settings, layout e interacción).

No crean ventanas ni tocan audio/archivos: solo numpy y los modelos puros.
"""

from __future__ import annotations

import builtins
import os
from pathlib import Path

import numpy as np
import pytest

from ezagent.voice.notch import menu as M
from ezagent.voice.notch import settings as S
from ezagent.voice.notch import tokens as T
from ezagent.voice.notch.menu_layout import (
    layout_menu,
    visible_menu_height,
)

VOICES = (
    ("em_alex", "Álex · masculina"),
    ("ef_dora", "Dora · femenina"),
    ("em_santa", "Santa · masculina grave"),
)
STT_MODELS = (
    ("ggml-small-q5_1", "Whisper Small · equilibrada"),
    ("ggml-large-v3-turbo-q5_0", "Whisper Large v3 Turbo · recomendada"),
)
LLM_MODELS = {
    "opencode": (("deepseek-v4.1-flash", "DeepSeek Flash"),),
    "groq": (
        ("llama-3.3-70b-versatile", "Llama 70B"),
        ("llama-3.1-8b-instant", "Llama 8B"),
    ),
}


def devices(count: int = 6) -> list[dict]:
    names = [
        "Micrófono Realtek High Definition Audio",
        "Auriculares USB",
        "Webcam C920",
        "Mezcla estéreo",
        "Entrada virtual",
        "Micrófono de solapa",
        "Otra entrada",
    ]
    return [
        {
            "index": index * 2,
            "name": names[index % len(names)],
            "rate": 48000 if index % 2 == 0 else 44100,
            "channels": 2 if index % 2 == 0 else 1,
        }
        for index in range(count)
    ]


def app_values(**overrides) -> dict:
    values = {
        "input_device_index": 0,
        "min_rms": 0.0015,
        "noise_multiplier": 1.3,
        "silence_secs": 1.6,
        "voice": "em_alex",
        "speed": 0.95,
        "barge_in": True,
        "confirm_actions": True,
    }
    values.update(overrides)
    return values


def make_settings(count: int = 6, **overrides) -> S.MenuSettings:
    return S.MenuSettings.from_app(
        app_values(**overrides),
        devices(count),
        VOICES,
        stt_models=STT_MODELS,
        llm_models=LLM_MODELS,
    )


def make_menu(
    settings: S.MenuSettings,
    *,
    screen_h: float = 1080.0,
    theme: str = "dark",
    save_result: bool | None = True,
) -> tuple[M.Menu, dict]:
    calls: dict = {
        "live": [],
        "saved": [],
        "tested": [],
        "hidden": 0,
        "closed": 0,
    }

    def on_live(key, value) -> None:
        calls["live"].append((key, value))

    def on_save(values) -> bool | None:
        calls["saved"].append(dict(values))
        return save_result

    def on_test(values) -> None:
        calls["tested"].append(dict(values))

    def on_hide() -> None:
        calls["hidden"] += 1

    def on_close() -> None:
        calls["closed"] += 1

    menu = M.Menu(
        settings,
        scale=1.0,
        theme=theme,
        screen_h=screen_h,
        on_live_change=on_live,
        on_save=on_save,
        on_test=on_test,
        on_hide=on_hide,
        on_close=on_close,
    )
    return menu, calls


def opened(
    menu: M.Menu, now: float = 10.0, settle: float = 1.0
) -> float:
    menu.open(now)
    menu.update(now + settle)
    return now + settle


# --------------------------------------------------------------------------
# Layout y geometría
# --------------------------------------------------------------------------


def test_layout_tabla_7_3_exacta():
    layout = layout_menu()
    assert (layout["header"].y0, layout["header"].y1) == (44, 72)
    assert (layout["group_audio"].y0, layout["group_audio"].y1) == (100, 296)
    assert (layout["group_engine"].y0, layout["group_engine"].y1) == (328, 400)
    assert (layout["group_tts"].y0, layout["group_tts"].y1) == (432, 600)
    assert (layout["group_vad"].y0, layout["group_vad"].y1) == (632, 808)
    assert (layout["group_stt"].y0, layout["group_stt"].y1) == (840, 1000)
    assert (layout["group_llm"].y0, layout["group_llm"].y1) == (1032, 1360)
    assert (
        layout["group_conversation"].y0,
        layout["group_conversation"].y1,
    ) == (1392, 1508)
    assert (layout["footer"].y0, layout["footer"].y1) == (1520, 1564)
    assert layout["total"].y1 == 1576
    assert visible_menu_height(1080) == 560
    assert visible_menu_height(600) == pytest.approx(384)


def test_layout_bajo_el_pie_se_mueve_y_el_scroll_existe():
    menu, _ = make_menu(make_settings(), screen_h=600)
    now = opened(menu)
    layout = menu._layout()
    assert layout["footer"].y0 == pytest.approx(364)
    # El contenido completo supera con holgura el viewport recortado.
    assert menu.scroll_max == pytest.approx(1156)
    assert menu.viewport_height == pytest.approx(384)
    assert menu.height(now) == pytest.approx(384)


def test_medidas_cerradas_de_componentes():
    assert M.BUTTON_H == 28
    assert M.BUTTON_MIN_W == 72
    assert M.BUTTON_RADIUS == 14
    assert set(M.BUTTON_VARIANTS) == {
        "primary",
        "secondary",
        "amber",
        "disabled",
    }
    assert M.SLIDER_TRACK_H == 4
    assert (M.SLIDER_THUMB_REST, M.SLIDER_THUMB_HOVER, M.SLIDER_THUMB_DRAG) == (
        14,
        16,
        18,
    )
    assert (M.TOGGLE_W, M.TOGGLE_H, M.TOGGLE_THUMB) == (36, 20, 16)
    assert (M.LIST_ITEM_H, M.LIST_MAX_VISIBLE) == (32, 5)
    assert M.SELECT_VALUE_X1 == 267
    assert M.SELECT_VALUE_MAX_W == 130
    assert M.CHEVRON_CX == 281 and M.CHEVRON_SIZE == 16
    assert M.LIST_CHECK_CX == 281 and M.LIST_CHECK_SIZE == 16
    assert (M.TOGGLE_X0, M.TOGGLE_X0 + M.TOGGLE_W) == (253, 289)
    assert (M.SAVE_BUTTON_X0, M.SAVE_BUTTON_X1) == (229, 301)
    assert M.SCROLL_INDICATOR_W == 3
    assert M.SCROLL_STEP == 40
    assert M.SCROLL_GRADIENT_H == 12


def test_sin_solapes_de_botones():
    layout = layout_menu()
    header_cy = (layout["header"].y0 + layout["header"].y1) / 2
    footer_cy = (layout["footer"].y0 + layout["footer"].y1) / 2
    action_cy = (
        layout["test_voice"].y0 + layout["test_voice"].y1
    ) / 2
    hide = T.Rect(
        M.HIDE_CX - M.HIDE_SIZE / 2,
        header_cy - M.HIDE_SIZE / 2,
        M.HIDE_CX + M.HIDE_SIZE / 2,
        header_cy + M.HIDE_SIZE / 2,
    )
    test = T.Rect(
        M.TEST_BUTTON_X1 - 72,
        action_cy - M.BUTTON_H / 2,
        M.TEST_BUTTON_X1,
        action_cy + M.BUTTON_H / 2,
    )
    save = T.Rect(
        M.SAVE_BUTTON_X0,
        footer_cy - M.BUTTON_H / 2,
        M.SAVE_BUTTON_X1,
        footer_cy + M.BUTTON_H / 2,
    )
    assert not hide.overlaps(test)
    assert not hide.overlaps(save)
    assert not test.overlaps(save)
    assert hide.x0 >= M.LABEL_X
    assert test.x0 > M.ROW_X0
    assert save.x0 > M.LABEL_X

    # Los textos largos terminan antes de sus botones.
    from ezagent.voice.notch.text import text_size

    header_w = text_size(
        "AJUSTES", T.FS_MICRO, 1.0
    )[0]
    assert M.LABEL_X + header_w < hide.x0
    label_w = text_size("Prueba de voz", T.FS_ROW, 1.0)[0]
    assert M.ROW_X0 + label_w < test.x0
    footer_w = text_size("CAMBIOS SIN GUARDAR", T.FS_MICRO, 1.0)[0]
    assert (
        M.FOOTER_TEXT_X + M.FOOTER_DOT + M.FOOTER_DOT_GAP + footer_w
        < save.x0
    )


# --------------------------------------------------------------------------
# MenuSettings
# --------------------------------------------------------------------------


def test_dirty_compara_tras_redondear_al_paso():
    settings = make_settings(input_device_index=None)
    assert not settings.dirty()
    settings.set("speed", 0.9500001)
    assert not settings.dirty()
    settings.set("speed", 0.95004)
    assert not settings.dirty()
    settings.set("speed", 1.0)
    assert settings.dirty()
    settings.set("speed", 0.95)
    assert not settings.dirty()

    settings.set("min_rms", 0.00154)
    assert not settings.dirty()
    settings.set("min_rms", 0.00156)
    assert settings.dirty()
    assert settings.get("min_rms") == pytest.approx(0.0016)
    settings.mark_saved()
    assert not settings.dirty()


def test_formatos_y_rangos_reales():
    settings = make_settings(input_device_index=0)
    assert settings.format("min_rms") == "0,0015"
    assert settings.format("noise_multiplier") == "1,3×"
    assert settings.format("silence_secs") == "1,6 s"
    assert settings.format("speed") == "0,95×"
    assert settings.value_label("voice") == "Álex · masculina"
    assert settings.value_label("input_device_index").startswith("0: ")
    assert settings.slider_bounds("min_rms") == pytest.approx(
        (0.0003, 0.008, 0.0001)
    )
    assert settings.slider_bounds("speed") == pytest.approx(
        (0.75, 1.25, 0.05)
    )
    assert settings.slider_bounds("noise_multiplier") == pytest.approx(
        (1.0, 2.5, 0.1)
    )
    assert settings.slider_bounds("silence_secs") == pytest.approx(
        (0.8, 3.5, 0.1)
    )
    with pytest.raises(KeyError):
        settings.slider_bounds("barge_in")

    # Los valores se recortan y redondean al paso.
    settings.set("speed", 9.0)
    assert settings.get("speed") == pytest.approx(1.25)
    settings.set("min_rms", -1.0)
    assert settings.get("min_rms") == pytest.approx(0.0003)


def test_etiquetas_de_voces_canonicas_en_settings():
    assert S.VOICE_LABELS == {
        "em_alex": "Álex · masculina",
        "ef_dora": "Dora · femenina",
        "em_santa": "Santa · masculina grave",
    }


def test_voces_y_etiquetas_exactas():
    settings = make_settings()
    assert [option.value for option in settings.select_options("voice")] == [
        "em_alex",
        "ef_dora",
        "em_santa",
    ]
    labels = [option.label for option in settings.select_options("voice")]
    assert labels == [
        "Álex · masculina",
        "Dora · femenina",
        "Santa · masculina grave",
    ]
    settings.set("voice", "em_santa")
    assert settings.value_label("voice") == "Santa · masculina grave"
    assert settings.dirty()


def test_selector_sin_microfonos_deshabilitado():
    settings = make_settings(count=0)
    assert settings.devices == ()
    assert not settings.selector_enabled("input_device_index")
    assert settings.select_options("input_device_index") == ()
    assert settings.value_label("input_device_index") == "Sin micrófonos"
    menu, _ = make_menu(settings)
    now = opened(menu)
    assert menu.hit(150, 118, now) is None
    disabled = menu.hit(150, 118, now, include_disabled=True)
    assert disabled is not None
    assert disabled.kind == "select"
    assert disabled.disabled


def test_snapshot_solo_claves_editables_y_no_escribe_nada(monkeypatch):
    settings = make_settings()
    snapshot = settings.snapshot()
    assert tuple(snapshot) == S.EDITABLE_KEYS
    assert snapshot["voice"] == "em_alex"
    assert snapshot["speed"] == pytest.approx(0.95)

    def fail(*_args, **_kwargs):
        raise AssertionError("MenuSettings no debe escribir archivos")

    monkeypatch.setattr(builtins, "open", fail)
    monkeypatch.setattr(Path, "write_text", fail)
    monkeypatch.setattr(Path, "write_bytes", fail)
    monkeypatch.setattr(os, "replace", fail)

    other = S.MenuSettings.from_app(app_values(), devices(), VOICES)
    other.set("speed", 1.05)
    assert other.dirty()
    other.mark_saved()
    assert not other.dirty()
    other.value_label("input_device_index")
    other.slider_bounds("speed")
    other.format("silence_secs")
    other.snapshot()
    other.select_options("input_device_index")


# --------------------------------------------------------------------------
# Ajustes nuevos: perfiles, texto editable y scroll
# --------------------------------------------------------------------------


PROVIDERS = {
    "groq": {
        "label": "Groq",
        "base_url": "https://api.groq.com/openai/v1",
        "model": "llama-3.3-70b-versatile",
        "api_key": "gsk_12345678",
    },
    "anthropic": {
        "label": "Claude",
        "base_url": "https://api.anthropic.com/v1/",
        "model": "claude-sonnet-5-5",
        "api_key": "sk-ant-abcdef",
    },
}


def test_perfil_guardado_copia_proveedor_modelo_url_y_clave():
    settings = S.MenuSettings.from_app(
        {
            "llm_provider": "groq",
            "llm_model": "llama-3.3-70b-versatile",
            "llm_base_url": "https://api.groq.com/openai/v1",
            "api_key": "gsk_12345678",
        },
        [],
        VOICES,
        PROVIDERS,
    )
    assert [option.label for option in settings.select_options("llm_profile")] == [
        "Groq",
        "Claude",
    ]
    assert settings.value_label("llm_provider") == "Groq"
    assert "gsk_12345678" not in settings.format("api_key")
    settings.set("llm_profile", "anthropic")
    assert settings.get("llm_provider") == "anthropic"
    assert settings.get("llm_model") == "claude-sonnet-5-5"
    assert settings.get("llm_base_url").startswith("https://api.anthropic.com/")
    assert settings.get("api_key") == "sk-ant-abcdef"
    assert settings.dirty()
    assert settings.snapshot()["llm_provider"] == "anthropic"
    assert "llm_profile" not in settings.snapshot()


def test_fila_de_texto_editable_confirma_con_enter():
    settings = make_settings()
    menu, calls = make_menu(settings)
    now = opened(menu)
    edits: list[bool] = []
    menu.on_text_edit = edits.append

    menu._scroll.value = menu.scroll_max
    menu._scroll_target = menu.scroll_max
    layout = menu._layout()
    scroll = menu.scroll_offset
    row = layout["api_key"]
    x = 150.0
    y = (row.y0 + row.y1) / 2.0 - scroll
    hit = menu.hit(x, y, now)
    assert hit is not None and hit.kind == "text" and hit.key == "api_key"
    menu.pointer_down(x, y, now)
    menu.pointer_up(x, y, now)
    assert menu.editing_key == "api_key"
    assert edits == [True]
    menu.text_input("!")
    assert menu.text_buffer.endswith("!")
    assert menu.text_key(13) is True
    assert menu.editing_key is None
    assert edits == [True, False]
    assert settings.get("api_key").endswith("!")
    assert calls["live"][-1] == ("api_key", settings.get("api_key"))
    assert settings.dirty()


def test_dibujo_del_menu_desplazado_incluye_ajustes_nuevos():
    settings = make_settings()
    menu, _ = make_menu(settings)
    now = opened(menu)
    menu._scroll.value = menu.scroll_max
    menu._scroll_target = menu.scroll_max
    buf = np.zeros((596, 320, 4), np.float32)
    menu.draw(buf, now)
    assert float(buf[..., 3].max()) > 0.0
    menu.begin_text_edit("api_key", now)
    menu.text_input("X")
    menu.draw(buf, now + 0.3)
    assert menu.editing_key == "api_key"
    menu.finish_text_edit(commit=False)


def test_etiquetas_en_espanol_y_secciones():
    assert S.LABELS["barge_in"] == "Interrupción activa"
    assert S.LABELS["confirm_actions"] == "Confirmar acciones"
    from ezagent.voice.notch.menu_layout import GROUP_LABELS

    labels = dict(GROUP_LABELS)
    assert labels["tts"] == "TTS"
    assert labels["vad"] == "VAD"
    assert "STT" in labels["stt"]
    assert "LLM" in labels["llm"]


def test_filas_de_seleccion_para_modelos():
    settings = make_settings()
    assert [o.value for o in settings.select_options("stt_model")] == [
        "ggml-small-q5_1",
        "ggml-large-v3-turbo-q5_0",
    ]
    assert settings.value_label("stt_model") == (
        "Whisper Large v3 Turbo · recomendada"
    )
    assert [o.value for o in settings.select_options("llm_model")] == [
        "deepseek-v4.1-flash",
    ]
    settings.set("llm_provider", "groq")
    assert [o.value for o in settings.select_options("llm_model")] == [
        "llama-3.3-70b-versatile",
        "llama-3.1-8b-instant",
        "deepseek-v4.1-flash",
    ]
    settings.set("stt_model", "ggml-large-v3-turbo-q5_0")
    assert settings.get("stt_model") == "ggml-large-v3-turbo-q5_0"


def test_medidor_de_entrada_arrastra_el_umbral():
    settings = make_settings()
    menu, calls = make_menu(settings)
    now = opened(menu)
    menu.set_input_level(0.006, active=True)
    menu.update(now + 0.2)
    old = settings.get("min_rms")
    menu.pointer_down(200.0, 166.0, now)
    menu.pointer_up(200.0, 166.0, now)
    assert settings.get("min_rms") != pytest.approx(old)
    assert calls["live"] and calls["live"][-1][0] == "min_rms"
    assert settings.dirty()


# --------------------------------------------------------------------------
# Hit testing de todos los controles
# --------------------------------------------------------------------------


def test_hit_de_cada_control():
    settings = make_settings()
    settings.set("speed", 1.05)  # dirty: el botón Guardar está activo
    menu, _ = make_menu(settings)
    now = opened(menu)

    cases = [
        ((M.HIDE_CX, 58.0), "hide", "hide"),
        ((150.0, 118.0), "select", "input_device_index"),
        ((150.0, 166.0), "meter", "input_meter"),
        ((150.0, 240.0), "slider", "noise_multiplier"),
        ((150.0, 284.0), "slider", "silence_secs"),
        ((150.0, 346.0), "select", "stt_engine"),
        ((150.0, 382.0), "select", "tts_model_variant"),
        ((150.0, 450.0), "select", "voice"),
        ((150.0, 490.0), "slider", "speed"),
        ((265.0, 562.0), "save", "save"),
    ]
    for (x, y), kind, key in cases:
        hit = menu.hit(x, y, now)
        assert hit is not None, (x, y)
        assert (hit.kind, hit.key) == (kind, key), (x, y, hit)

    # Guardar deshabilitado no es interactivo por defecto.
    clean, _ = make_menu(make_settings())
    clean_now = opened(clean)
    assert clean.hit(265, 562, clean_now) is None
    disabled = clean.hit(265, 562, clean_now, include_disabled=True)
    assert disabled is not None and disabled.kind == "save"
    assert disabled.disabled

    # Fuera del menú no hay hit.
    assert menu.hit(150, 20, now) is None
    assert menu.hit(150, 595, now) is None


def test_hit_slider_solo_en_la_franja_de_la_pista():
    menu, _ = make_menu(make_settings())
    now = opened(menu)
    assert menu.hit(150, 212, now) is None  # fuera de la franja de la pista
    assert menu.hit(150, 240, now).kind == "slider"  # Umbral de ruido
    assert menu.hit(300, 240, now) is None  # fuera de x 31..289


# --------------------------------------------------------------------------
# Interacción
# --------------------------------------------------------------------------


def test_selector_abierto_desplaza_filas_y_limita_a_cinco():
    settings = make_settings(count=6)
    menu, _ = make_menu(settings)
    now = opened(menu)
    menu.pointer_down(150, 118, now)
    menu.pointer_up(150, 118, now)
    now = now + 0.3
    menu.update(now)

    assert menu._selector_key == "input_device_index"
    assert menu._selector_extra.value == pytest.approx(160)
    assert menu.scroll_max > 160
    # La fila de VOZ baja 160 px y sigue siendo visible.
    engine = menu.hit(150, 328 + 160, now)
    assert engine is not None and engine.kind == "select"
    assert engine.key == "stt_engine"
    # El primer elemento de la lista empieza en y=136.
    first = menu.hit(150, 150, now)
    assert first is not None and first.kind == "option"
    assert first.value == 0 and first.index == 0
    # El sexto elemento no se muestra (máximo 5 visibles).
    fifth = menu.hit(150, 280, now)
    assert fifth is not None and fifth.kind == "option"
    assert fifth.index == 4
    out = menu.hit(150, 300, now)
    assert out is None or out.kind != "option"

    # La rueda sobre la lista (>5) desplaza su contenido, no el menú.
    assert menu.wheel(150, 150, -120, now)
    assert menu._list_scroll == pytest.approx(32)
    assert menu.scroll_offset == pytest.approx(0)
    second = menu.hit(150, 150, now)
    assert second is not None and second.index == 1

    # Elegir cierra la lista y emite el valor real.
    menu.pointer_down(150, 150, now)
    menu.pointer_up(150, 150, now)
    assert menu._selector_key is None or menu._selector_closing
    assert settings.get("input_device_index") == 2


def test_clic_toggle_y_prueba_de_voz():
    settings = make_settings()
    menu, calls = make_menu(settings)
    now = opened(menu)
    menu._scroll.value = 960.0
    menu._scroll_target = 960.0
    menu.pointer_down(270, 450, now)
    menu.pointer_up(270, 450, now)
    assert settings.get("barge_in") is False
    assert calls["live"][-1] == ("barge_in", False)

    values_before = settings.snapshot()
    menu.pointer_down(253, 526, now + 0.1)
    menu.pointer_up(253, 526, now + 0.1)
    assert calls["tested"] and calls["tested"][-1] == values_before
    assert menu._testing_t.value == pytest.approx(0.0)  # aún no ha pasado 120 ms
    menu.update(now + 1.0)
    assert menu._testing_t.value == pytest.approx(1.0)
    disabled = menu.hit(253, 526, now + 1.0, include_disabled=True)
    assert disabled is not None and disabled.disabled
    assert menu.hit(253, 526, now + 1.0) is None
    menu.set_testing(False, now + 1.1)
    menu.update(now + 1.4)
    assert menu._testing_t.value == pytest.approx(0.0)


def test_arrastre_de_slider_redondea_y_emite_al_soltar():
    settings = make_settings()
    menu, calls = make_menu(settings)
    now = opened(menu)
    # Pista de Umbral de voz: x 31..289, centro y 168.
    menu.pointer_down(150, 168, now)
    menu.pointer_move(210, 168, now + 0.06)
    assert settings.get("min_rms") == pytest.approx(0.0056, abs=1e-4)
    menu.pointer_move(230, 168, now + 0.07)  # < 50 ms: no emite
    menu.pointer_up(230, 168, now + 0.12)
    assert settings.get("min_rms") == pytest.approx(0.0062, abs=1e-4)
    assert settings.dirty()
    # Redondeado al paso de 0,0001.
    steps = settings.get("min_rms") / 0.0001
    assert steps == pytest.approx(round(steps), abs=1e-6)
    # La última emisión coincide con el valor final.
    assert calls["live"][-1] == ("min_rms", settings.get("min_rms"))
    assert len(calls["live"]) <= 3


def test_rueda_desplaza_el_menu_largo():
    settings = make_settings()
    menu, _calls = make_menu(settings)
    now = opened(menu)
    old_rms = settings.get("min_rms")
    assert menu.wheel(150, 240, -120, now)
    menu.update(now + 0.2)
    assert menu.scroll_offset > 0
    # La rueda desplaza; no cambia un slider por accidente al pasar por él.
    assert settings.get("min_rms") == pytest.approx(old_rms)


def test_scroll_del_menu_cuando_no_cabe():
    settings = make_settings(count=1)
    menu, _ = make_menu(settings, screen_h=600)
    now = opened(menu)
    old = settings.get("min_rms")
    assert menu.scroll_max > 0
    assert menu.wheel(150, 168, -120, now)
    assert settings.get("min_rms") == old  # la rueda no toca el slider
    menu.update(now + 0.3)
    assert menu.scroll_offset > 0
    # El pie sigue fijo en su posición reducida.
    footer = menu._layout()["footer"]
    assert footer.y0 == pytest.approx(364)
    assert menu.hit(265, 386, now + 0.3, include_disabled=True).kind == "save"


# --------------------------------------------------------------------------
# Apertura, cierre y guardado
# --------------------------------------------------------------------------


def test_apertura_y_cierre_con_altura_animada_y_on_close():
    menu, calls = make_menu(make_settings())
    menu.open(0.0)
    assert menu.height(0.0) == pytest.approx(0.0)
    assert menu.is_open()
    assert menu.is_animating(0.01)
    menu.update(0.16)
    middle = menu.height(0.16)
    assert 0.0 < middle < 560.0
    menu.update(0.4)
    assert menu.height(0.4) == pytest.approx(560.0)
    # El muelle de la altura no existe: nunca supera el objetivo.
    menu.update(0.5)
    assert menu.height(0.5) == pytest.approx(560.0)

    menu.close(1.0)
    assert not menu.is_open()
    menu.update(1.1)
    assert 0.0 < menu.height(1.1) < 560.0
    menu.update(1.4)
    assert menu.height(1.4) == pytest.approx(0.0)
    assert calls["closed"] == 1
    menu.update(2.0)
    assert calls["closed"] == 1


def test_movimiento_reducido_es_instantaneo():
    menu, _ = make_menu(make_settings())
    menu.set_reduced_motion(True)
    menu.open(0.0)
    assert menu.height(0.0) == pytest.approx(560.0)
    menu.close(0.1)
    assert menu.height(0.1) == pytest.approx(0.0)
    menu.update(0.2)
    menu.open(0.3)
    assert menu.height(0.3) == pytest.approx(560.0)


def test_guardado_exitoso_y_rollback_asincrono():
    settings = make_settings()
    settings.set("speed", 1.05)
    menu, calls = make_menu(settings, save_result=True)
    now = opened(menu)
    menu.pointer_down(265, 562, now)
    menu.pointer_up(265, 562, now)
    assert calls["saved"] and calls["saved"][-1]["speed"] == pytest.approx(1.05)
    assert not settings.dirty()
    assert menu._check_until > now
    assert menu._saved_flash_until > now

    # El guardado real era asíncrono y falla: se recupera la copia anterior.
    menu.update(now + 0.1)
    menu.on_applied(False)
    assert settings.dirty()
    assert menu._error_until > now

    # Un segundo intento con éxito deja todo guardado.
    menu.pointer_down(265, 562, now + 0.2)
    menu.pointer_up(265, 562, now + 0.2)
    assert not settings.dirty()
    menu.update(now + 0.3)
    menu.on_applied(True)
    assert not settings.dirty()


def test_alto_disponible_limita_el_menu_flotante():
    menu, _ = make_menu(make_settings(), screen_h=1080.0)
    assert menu.viewport_height == pytest.approx(visible_menu_height(1080.0))
    menu.set_available_height(320.0)
    assert menu.viewport_height == pytest.approx(320.0)
    menu.set_available_height(None)
    assert menu.viewport_height == pytest.approx(visible_menu_height(1080.0))


def test_guardado_pendiente_no_confirma_hasta_resultado_real():
    """None = pendiente; el check solo aparece con on_applied(True)."""
    settings = make_settings()
    settings.set("speed", 1.05)
    menu, _ = make_menu(settings, save_result=None)
    now = opened(menu)
    menu.pointer_down(265, 562, now)
    menu.pointer_up(265, 562, now)
    assert settings.dirty()
    assert menu._saving_pending
    assert menu._check_until == 0.0
    menu.update(now + 0.05)
    menu.on_applied(True)
    assert not settings.dirty()
    assert menu._check_until > now
    assert not menu._saving_pending


def test_guardado_fallido_mantiene_dirty_y_muestra_error():
    settings = make_settings()
    settings.set("speed", 1.05)
    menu, _ = make_menu(settings, save_result=False)
    now = opened(menu)
    menu.pointer_down(265, 562, now)
    menu.pointer_up(265, 562, now)
    assert settings.dirty()
    assert menu._error_until > now
    assert menu._check_until == 0.0


def test_boton_ocultar_cierra_y_notifica():
    menu, calls = make_menu(make_settings())
    now = opened(menu)
    menu.pointer_down(M.HIDE_CX, 58, now)
    menu.pointer_up(M.HIDE_CX, 58, now)
    assert calls["hidden"] == 1
    assert not menu.is_open()
    menu.update(now + 0.4)
    assert calls["closed"] == 1


# --------------------------------------------------------------------------
# Render
# --------------------------------------------------------------------------


def _buffer(menu: M.Menu, theme: str = "dark", scale: float = 1.0) -> np.ndarray:
    from ezagent.voice.notch.silhouette import render_silhouette

    height = T.H_ISLAND + menu.height(menu._now)
    surf = render_silhouette(height, theme, scale)
    return surf[..., [2, 1, 0, 3]].astype(np.float32) / 255.0


def test_draw_oscuro_claro_selector_y_prueba():
    for theme in ("dark", "light"):
        settings = make_settings()
        settings.set("speed", 1.05)
        menu, _ = make_menu(settings, theme=theme)
        now = opened(menu)
        buf = _buffer(menu, theme)
        menu.draw(buf, now)
        assert buf.shape == (596, 320, 4)
        assert float(buf[..., 3].sum()) > 50_000
        assert np.all(
            buf[..., :3] <= buf[..., 3:4] + 1e-5
        ), "el buffer debe seguir premultiplicado"

        menu.pointer_down(150, 118, now)
        menu.pointer_up(150, 118, now)
        menu.update(now + 0.3)
        menu.pointer_move(150, 200, now + 0.3)
        menu.set_testing(True, now + 0.4)
        menu.update(now + 1.0)
        menu.draw(buf, now + 1.0)
        assert np.all(buf[..., :3] <= buf[..., 3:4] + 1e-5)


def test_draw_en_pantalla_baja_con_gradientes():
    menu, _ = make_menu(make_settings(count=1), screen_h=600)
    now = opened(menu)
    menu.pointer_move(150, 200, now)
    menu.wheel(150, 200, -120, now)
    menu.update(now + 0.3)
    buf = np.zeros((420, 320, 4), np.float32)
    menu.draw(buf, now + 0.3)
    assert float(buf[..., 3].sum()) > 20_000
    assert np.all(buf[..., :3] <= buf[..., 3:4] + 1e-5)


def test_menu_no_toca_audio_y_on_test_recibe_instantanea():
    settings = make_settings()
    menu, calls = make_menu(settings)
    now = opened(menu)
    menu._start_test(now)
    payload = calls["tested"][-1]
    assert payload["voice"] == settings.get("voice")
    assert payload["speed"] == settings.get("speed")
    menu.set_testing(False, now + 0.2)


def test_las_claves_editables_existen_en_voice_config():
    from dataclasses import fields

    from ezagent.voice.config import VoiceConfig

    field_names = {field.name for field in fields(VoiceConfig)}
    assert set(S.EDITABLE_KEYS) <= field_names
    assert set(S.EDITABLE_KEYS) <= set(S.LABELS)


def test_motor_y_razonamiento_son_listas():
    settings = make_settings()
    assert [o.value for o in settings.select_options("stt_engine")] == [
        "auto",
        "vulkan",
        "cpu",
    ]
    assert settings.format("stt_engine") == "Automático"
    settings.set("stt_engine", "cpu")
    assert settings.get("stt_engine") == "cpu"
    assert [o.value for o in settings.select_options("llm_reasoning_effort")] == [
        "none",
        "low",
        "medium",
        "high",
    ]
    settings.set("llm_reasoning_effort", "medium")
    assert settings.format("llm_reasoning_effort") == "Medio"
    assert settings.dirty()
