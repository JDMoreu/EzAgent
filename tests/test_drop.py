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


def test_colores_siguen_al_estado() -> None:
    """Las manchas de color migran a la paleta del estado."""
    m = D.DropModel()

    def near(pose: str, key: str) -> bool:
        target = D.orb_palette(pose, D.DARK)[0]
        return all(abs(c - v) < 8 for c, v in zip(m.cols[0], target, strict=True)) and key

    t = _run(m, "think", 3.0)
    assert near("think", "think")
    t = _run(m, "error", 3.0, t0=t)
    assert near("error", "error")
    _run(m, "mute", 3.0, t0=t)
    assert near("mute", "mute")


def test_movimiento_reducido_aplica_al_instante_y_sin_deriva() -> None:
    m = D.DropModel()
    _run(m, "mute", 0.1, reduced=True)
    assert abs(m.R - D.BASE_R["mute"]) < 1e-6 and m.cy_offset == 0.0 and m.shift == 0.0
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


def test_respira_en_calma_con_cambios_pequenos() -> None:
    """En reposo el radio oscila poco (respira) y el contorno se deforma poco."""
    m = D.DropModel()
    t = _run(m, "idle", 1.0)
    radios = []
    for _ in range(600):
        t += 1 / 60
        m.update(t, 1 / 60, "idle", 0.0, D.DARK, False, False, False, False)
        radios.append(m.R)
    assert 0.8 < max(radios) - min(radios) < 3.0           # respira, sin saltos grandes
    assert max(abs(v) for v in (m.m["a2"], m.m["a3"], m.m["a4"])) < 0.05
    assert m.m["tr"] < 0.01                                  # sin sonido no hay temblor


def test_se_expande_con_la_voz_tiembla_y_vuelve_a_la_base_al_callar() -> None:
    """El ciclo completo que se pidió: expandirse, temblar sutilmente y volver."""
    m = D.DropModel()
    t = _run(m, "speak", 1.0, rms=0.0)
    base_r, base_tr = m.R, m.m["tr"]
    assert base_tr < 0.05
    t = _run(m, "speak", 1.0, rms=0.1, t0=t)                 # con voz
    assert m.R > base_r + 6                                   # se expande
    assert m.m["tr"] > 0.5                                    # tiembla
    contour = [m.contour(i / 36 * 6.2831853) for i in range(36)]
    assert 0.0 < max(contour) - min(contour) < 0.5            # deformación contenida
    t = _run(m, "speak", 2.5, rms=0.0, t0=t)                 # calla
    assert abs(m.R - base_r) < 1.5                            # vuelve a su forma base
    assert m.m["tr"] < 0.05                                   # el temblor se apaga


def test_el_temblor_es_rapido_y_pequeno() -> None:
    m = D.DropModel()
    _run(m, "listen", 1.0, rms=0.06)
    f1 = [m.contour(i / 90 * 6.2831853) for i in range(90)]
    t = 1000.0 + 2.0
    m.update(t, 1 / 60, "listen", 0.06, D.DARK, False, False, False, False)
    f2 = [m.contour(i / 90 * 6.2831853) for i in range(90)]
    cambio = max(abs(a - b) for a, b in zip(f1, f2, strict=True))
    assert 0.0 < cambio < 0.25      # cambia entre fotogramas, pero poco


def test_borde_suave_sin_contorno_duro() -> None:
    """La opacidad cae de forma gradual: ningún salto brusco entre píxeles vecinos."""
    m = D.DropModel()
    _run(m, "idle", 1.0)
    patch = D.render_orb(m)
    n = patch.shape[0]
    fila = patch[n // 2, :, 3].astype(int)
    assert fila.max() >= 250 and fila[0] == 0 and fila[-1] == 0
    assert np.abs(np.diff(fila)).max() < 90                   # sin escalón duro
    assert (fila > 10).sum() > 8                              # banda de transición ancha


def test_el_orbe_no_dibuja_halo_ni_anillos() -> None:
    m = D.DropModel()
    t = _run(m, "speak", 1.0, rms=0.1)
    frame = D.render_model(m, D.LW, D.LH, 1.0, D.DARK, False, True, False, t)
    assert frame[int(D.CY), int(D.CX + 2 * D.RMAX), 3] == 0   # fuera del parche: nada
    assert not hasattr(m, "rings") and not hasattr(m, "halo")


def test_colores_distintos_por_estado() -> None:
    pals = {
        p: tuple(tuple(round(c) for c in col) for col in D.orb_palette(p, D.DARK))
        for p in ("idle", "listen", "think", "speak", "ask", "error", "mute")
    }
    assert len(set(pals.values())) == 7



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
