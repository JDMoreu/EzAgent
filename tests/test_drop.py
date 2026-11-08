"""La gota: animación pura, geometría de las gotas pequeñas, dibujo y contrato."""

from __future__ import annotations

import time

import numpy as np
import pytest

from ezagent.voice.config import VoiceConfig
from ezagent.voice.notch import drop as D


def _run(model, pose, seconds=1.0, rms=0.0, palette=D.DARK, hover=False, asking=False,
         menu_open=False, reduced=False, t0=1000.0):
    t = t0
    for _ in range(int(seconds * 60)):
        t += 1 / 60
        model.update(t, 1 / 60, pose, rms, palette, hover, asking, menu_open, reduced)
    return t


def test_nivel_perceptual_en_rango() -> None:
    assert D.level_from_rms(0) == 0
    assert 0 < D.level_from_rms(0.01) < D.level_from_rms(0.05) < D.level_from_rms(0.3) <= 1
    assert D.level_from_rms(None) == 0 and D.level_from_rms("x") == 0


def test_la_gota_nunca_queda_quieta_en_reposo() -> None:
    m = D.DropModel()
    t = _run(m, "idle", 1.0)
    a = m.blob()
    _run(m, "idle", 0.5, t0=t)
    b = m.blob()
    assert max(abs(p[0] - q[0]) + abs(p[1] - q[1]) for p, q in zip(a, b, strict=True)) > 0.05


def test_transiciones_suaves_sin_saltos() -> None:
    """Al cambiar de estado, ningún punto salta más de unos píxeles por fotograma."""
    m = D.DropModel()
    t = _run(m, "idle", 1.0)
    prev = m.blob()
    worst = 0.0
    for pose in ("listen", "think", "speak", "ask", "idle"):
        for _ in range(30):
            t += 1 / 60
            m.update(t, 1 / 60, pose, 0.05, D.DARK, False, pose == "ask", False, False)
            cur = m.blob()
            worst = max(worst, max(abs(a[0] - b[0]) + abs(a[1] - b[1]) for a, b in zip(prev, cur, strict=True)))
            prev = cur
    assert worst < 8.0, worst


def test_habla_se_deforma_con_la_voz() -> None:
    quiet, loud = D.DropModel(), D.DropModel()
    _run(quiet, "speak", 1.0, rms=0.0)
    _run(loud, "speak", 1.0, rms=0.12)
    assert loud.R > quiet.R and loud.m["a2"] > quiet.m["a2"]


def test_color_sigue_al_estado() -> None:
    m = D.DropModel()
    _run(m, "think", 1.5)
    # al pensar el color oscila entre ámbar y violeta: nunca sale de ese tramo
    a, b = D.DARK["attention"], D.DARK["agent"]
    def dist_to_segment(c):
        ab = [b[i] - a[i] for i in range(3)]
        t = max(0.0, min(1.0, sum((c[i] - a[i]) * ab[i] for i in range(3)) / sum(v * v for v in ab)))
        return max(abs(c[i] - (a[i] + ab[i] * t)) for i in range(3))
    assert dist_to_segment(m.color) < 12
    _run(m, "error", 1.5)
    assert all(abs(c - v) < 6 for c, v in zip(m.color, D.DARK["fault"], strict=True))
    _run(m, "mute", 1.5)
    assert all(abs(c - v) < 6 for c, v in zip(m.color, D.DARK["text3"], strict=True))


def test_movimiento_reducido_aplica_al_instante_y_sin_deriva() -> None:
    m = D.DropModel()
    _run(m, "mute", 0.1, reduced=True)
    assert abs(m.R - 9) < 1e-6 and m.cy_offset == 0.0 and m.shift == 0.0
    assert all(s.x in (0.0, 1.0) for s in m.bud.values())


def test_gotas_pequenas_brotan_con_hover_y_se_ocultan() -> None:
    m = D.DropModel()
    t = _run(m, "idle", 0.6, hover=True)
    assert m.bud["mic"].x > 0.9 and m.bud["gear"].x > 0.9
    assert m.bud["no"].x < 0.05 and m.bud["yes"].x < 0.05
    _run(m, "idle", 1.0, hover=False, t0=t)
    assert abs(m.bud["mic"].x) < 0.05


def test_confirmacion_cambia_las_gotas_pequenas() -> None:
    m = D.DropModel()
    _run(m, "ask", 0.8, hover=True, asking=True)
    assert m.bud["no"].x > 0.9 and m.bud["yes"].x > 0.9
    assert m.bud["mic"].x < 0.05 and m.bud["gear"].x < 0.05


def test_deteccion_de_gotas_pequenas() -> None:
    m = D.DropModel()
    assert m.bud_at(D.CX - D.BUD_X, D.CY) is None  # aún no han brotado
    _run(m, "idle", 0.8, hover=True)
    assert m.bud_at(D.CX - D.BUD_X, D.CY) == "mic"
    assert m.bud_at(D.CX + D.BUD_X, D.CY) == "gear"
    assert m.bud_at(D.CX, D.CY) is None
    assert D.DropModel.near(D.CX + 30, D.CY) and not D.DropModel.near(5, 5)


@pytest.mark.parametrize("pose", ["idle", "listen", "think", "speak", "done", "mute", "error", "ask", "sleep"])
def test_render_produce_bgra_premultiplicado(pose: str) -> None:
    m = D.DropModel()
    _run(m, pose, 0.5, rms=0.05, hover=True, asking=(pose == "ask"))
    w, h = D.LW, D.LH
    frame = D.render_model(m, w, h, 1.0, D.DARK, muted=(pose == "mute"), in_call=True, menu_open=False)
    assert frame.shape == (h, w, 4) and frame.dtype == np.uint8
    # premultiplicado: ningún canal de color supera el alfa
    assert (frame[..., :3].max(axis=2).astype(int) <= frame[..., 3].astype(int) + 1).all()
    # la gota está dibujada en el centro
    assert frame[h // 2, w // 2, 3] > 200


def test_recorte_de_clics_por_geometria_sin_zona_pintada() -> None:
    """No hay disco «casi invisible»: lo transparente es transparente de verdad."""
    m = D.DropModel()
    t = _run(m, "idle", 1.0)
    frame = D.render_model(m, D.LW, D.LH, 1.0, D.DARK, False, True, False, t)
    assert frame[2, 2, 3] == 0                                   # esquina
    assert frame[int(D.CY), int(D.CX + 100), 3] == 0              # lejos del cuerpo: nada pintado
    assert m.on_body(D.CX, D.CY) and not m.on_body(D.CX + 60, D.CY)
    assert m.on_body(D.CX + m.R, D.CY)


def test_pose_de_escucha_pinta_anillos_entrantes_y_habla_salientes() -> None:
    m = D.DropModel()
    t = _run(m, "listen", 1.0, rms=0.06)
    assert m.rings and all(kind == "in" for _, kind, _ in m.rings)
    m2 = D.DropModel()
    # una sílaba: de silencio a voz
    t2 = _run(m2, "speak", 0.3, rms=0.0)
    for _ in range(30):
        t2 += 1 / 60
        m2.update(t2, 1 / 60, "speak", 0.1, D.DARK, False, False, False, False)
    assert any(kind == "out" for _, kind, _ in m2.rings)
    assert t > 0


def test_ganancia_automatica_usa_todo_el_rango() -> None:
    """Un micrófono flojo y uno fuerte producen el mismo nivel relativo."""
    weak, strong = D.DropModel(), D.DropModel()
    _run(weak, "listen", 1.0, rms=0.012)
    _run(strong, "listen", 1.0, rms=0.2)
    assert abs(weak.lvl - strong.lvl) < 0.1 and weak.lvl > 0.7


class _Fake(D.DropUI):
    """DropUI sin ventana ni Tk para probar el contrato."""

    def __init__(self) -> None:
        super().__init__(root=None)
        self.actions: list[str] = []
        self.on_action = self.actions.append


def test_clic_en_gotas_pequenas_emite_acciones() -> None:
    ui = _Fake()
    _run(ui.model, "idle", 0.8, hover=True)
    ui.set_in_call(True)
    assert ui.pointer_down(D.CX - D.BUD_X, D.CY) is True
    ui.pointer_up(D.CX - D.BUD_X, D.CY)
    assert ui.pointer_down(D.CX + D.BUD_X, D.CY) is True
    ui.pointer_up(D.CX + D.BUD_X, D.CY)
    assert ui.actions == ["mute", "panel"]
    ui.set_in_call(False)
    ui.pointer_down(D.CX - D.BUD_X, D.CY)
    ui.pointer_up(D.CX - D.BUD_X, D.CY)
    assert ui.actions[-1] == "listen"


def test_clic_sobre_el_cuerpo_inicia_arrastre() -> None:
    ui = _Fake()
    assert ui.pointer_down(D.CX, D.CY) is False


def test_soltar_fuera_de_la_gota_no_acciona() -> None:
    ui = _Fake()
    _run(ui.model, "idle", 0.8, hover=True)
    ui.pointer_down(D.CX - D.BUD_X, D.CY)
    ui.pointer_up(5, 5)
    assert ui.actions == []


def test_confirmacion_llama_a_si_y_no() -> None:
    ui = _Fake()
    calls: list[str] = []
    ui.show_confirmation("¿Detengo?", False, lambda: calls.append("si"), lambda: calls.append("no"))
    assert ui._asking
    _run(ui.model, "ask", 0.8, hover=True, asking=True)
    ui.pointer_down(D.CX + D.BUD_X, D.CY)
    ui.pointer_up(D.CX + D.BUD_X, D.CY)
    ui.pointer_down(D.CX - D.BUD_X, D.CY)
    ui.pointer_up(D.CX - D.BUD_X, D.CY)
    assert calls == ["si", "no"]
    ui.hide_confirmation()
    assert not ui._asking and ui._on_yes is None


def test_contrato_con_la_app_sin_ventana() -> None:
    """Todos los métodos que usa app.py existen y no fallan sin ventana."""
    ui = _Fake()
    for name in ("show", "hide", "close", "draw", "set_state", "set_in_call", "set_mic_level",
                 "set_testing", "hide_confirmation", "toggle_menu", "show_confirmation",
                 "set_theme", "set_save_result", "set_muted", "refresh_menu_settings",
                 "open_menu", "notice"):
        assert callable(getattr(ui, name)), name
    for name in ("on_action", "on_menu_action", "menu_settings_provider", "on_live_setting",
                 "on_save_settings", "on_test_voice", "on_hide", "on_position_change"):
        assert hasattr(ui, name), name
    ui.set_muted(True)
    ui.set_in_call(True)
    ui.draw(0.05, "speaking")
    ui.notice()
    ui.set_theme("light")
    ui.set_mic_level(0.3)
    ui.menu_settings_provider = lambda: "x"
    assert ui.menu_settings_provider() == "x"


def test_pose_dormida_sin_llamada_y_ask_manda() -> None:
    ui = _Fake()
    ui.show = lambda state="idle": None  # sin ventana
    ui._ui.set_visible(True)
    ui.set_in_call(False)
    now = time.monotonic() + 1
    assert ui._pose(now) == "sleep"
    ui.set_in_call(True)
    assert ui._pose(now + 1) in ("idle", "listen")
    ui._asking = True
    assert ui._pose(now + 2) == "ask"


def test_clamp_mantiene_la_gota_en_pantalla() -> None:
    ui = _Fake()
    x, y = ui.clamp_position(-99999, -99999)
    wa = D._shell.work_area()
    assert x >= wa[0] - round((D.CX - 36) * ui.s) and y >= wa[1] - round((D.CY - 36) * ui.s)
    x2, y2 = ui.clamp_position(99999, 99999)
    assert x2 <= wa[2] and y2 <= wa[3]


def test_create_ui_respeta_ui_style(monkeypatch: pytest.MonkeyPatch) -> None:
    cfg = VoiceConfig()
    assert cfg.ui_style == "drop"
    assert isinstance(D.create_ui(None, cfg), D.DropUI)
    cfg.ui_style = "notch"
    ui = D.create_ui(None, cfg)
    assert not isinstance(ui, D.DropUI)


def test_arrastrar_guarda_posicion_propia(tmp_path) -> None:
    cfg = VoiceConfig()
    saved: dict = {}
    cfg.save_key = lambda k, v, path=None: saved.__setitem__(k, v)  # type: ignore[method-assign]
    ui = D.DropUI(root=None, cfg=cfg)
    got: list = []
    ui.on_position_change = lambda x, y: got.append((x, y))
    ui.drag_end(123, 456)
    assert saved == {"drop_x": 123, "drop_y": 456} and got == [(123, 456)]
