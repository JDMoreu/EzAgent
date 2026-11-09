"""La barra: forma fija, fluido interior, botones, disposiciones y contrato con la app."""

from __future__ import annotations

import time

import numpy as np
import pytest

from ezagent.voice.config import VoiceConfig
from ezagent.voice.notch import drop as D


def _run(model, pose, seconds=1.0, rms=0.0, palette="dark", hover=False, asking=False,
         menu_open=False, reduced=False, buttons="always", t0=1000.0, in_call=True):
    t = t0
    for _ in range(int(seconds * 60)):
        t += 1 / 60
        model.update(t, 1 / 60, pose, rms, palette, hover, asking, menu_open, reduced, buttons,
                     in_call=in_call)
    return t


def _alpha(model, scale=1.0, theme="dark", now=1000.0):
    w, h = D.layout_size(model.layout)
    f = D.render_model(model, round(w * scale), round(h * scale), scale, theme, False, True, False, now)
    return f[..., 3]


# ------------------------------------------------------------------ geometría
def test_tamanos_de_ventana() -> None:
    wp, hp = D.layout_size("pill")
    wc, hc = D.layout_size("circle")
    assert wp > 300 and hp < 80                  # barra baja y ancha
    assert wc < 220 and hc > 100                 # círculo sobre la fila de botones
    assert wp > wc and hp < hc


def test_posiciones_de_la_capsula_ordenadas_y_fijas() -> None:
    m = D.DropModel("pill")
    _run(m, "idle", 1.0)
    pos = m.positions()
    xs = [pos[n][0] for n in ("gear", "pill", "mic", "hang")]
    assert xs == sorted(xs)                       # ☰ (forma) 🎤 ✕
    assert set(pos) == {"gear", "pill", "mic", "hang"}
    assert abs(pos["pill"][0] - D.form_center("pill")[0]) < 0.5
    assert len({round(pos[n][1], 3) for n in pos}) == 1   # todos en la misma fila


def test_disposicion_circulo_forma_arriba_y_fila_abajo() -> None:
    m = D.DropModel("circle")
    _run(m, "idle", 1.0)
    pos = m.positions()
    assert pos["pill"][1] < pos["mic"][1]
    assert pos["gear"][1] == pytest.approx(pos["mic"][1], abs=0.01) == pytest.approx(pos["hang"][1], abs=0.01)
    w, _ = D.layout_size("circle")
    assert abs(pos["pill"][0] - w / 2) < 0.5
    assert abs(pos["mic"][0] - w / 2) < 0.5       # el micrófono, bajo el círculo


def test_al_preguntar_se_oculta_el_menu_y_la_forma_no_se_mueve() -> None:
    m = D.DropModel("pill")
    t = _run(m, "idle", 1.0)
    antes = m.positions()
    _run(m, "ask", 1.5, asking=True, t0=t)
    despues = m.positions()
    assert despues["gear"][3] < 0.05                                    # ☰ desaparece
    assert despues["mic"][3] > 0.95 and despues["hang"][3] > 0.95       # quedan ✓ y ✕
    assert m.ask.x > 0.95                                               # 🎤 se convirtió en ✓
    assert despues["pill"][:2] == pytest.approx(antes["pill"][:2], abs=0.01)   # la forma no se mueve
    assert despues["hang"][:2] == pytest.approx(antes["hang"][:2], abs=0.01)

# ------------------------------------------------------- la forma NO se deforma
def test_la_forma_es_fija_en_todos_los_estados() -> None:
    """Lo central de la especificación: ni tamaño ni contorno cambian con la voz."""
    siluetas = []
    for pose, rms in (("idle", 0.0), ("listen", 0.08), ("speak", 0.12), ("think", 0.0)):
        m = D.DropModel("pill")
        _run(m, pose, 2.0, rms=rms)
        siluetas.append(_alpha(m))
    base = siluetas[0]
    for s in siluetas[1:]:
        assert np.array_equal(base, s), "el contorno cambió entre estados"


def test_el_modelo_no_tiene_deformacion_de_contorno() -> None:
    m = D.DropModel()
    for attr in ("contour", "blob", "rings", "halo", "trph", "R"):
        assert not hasattr(m, attr), attr


def test_mascara_nitida_y_estable() -> None:
    mask = D.form_mask("pill", 1.0)
    a = np.asarray(mask).astype(int)
    assert a.max() == 255 and a[a.shape[0] // 2, a.shape[1] // 2] == 255
    assert a[0, 0] == 0                                        # esquinas fuera de la cápsula
    fila = a[a.shape[0] // 2]
    assert (fila[1:-1] > 0).all()                              # sin huecos
    assert D.form_mask("pill", 1.0) is mask                    # cacheada
    circ = np.asarray(D.form_mask("circle", 1.0)).astype(int)
    assert circ[0, 0] == 0 and circ[circ.shape[0] // 2, circ.shape[1] // 2] == 255
    borde = (a > 5) & (a < 250)
    assert 0 < borde.sum() < 600                               # borde definido, ~1–2 px


# --------------------------------------------------------- nubes y aire (interior)
def test_el_interior_se_mueve_con_el_tiempo() -> None:
    m = D.DropModel("pill")
    t = _run(m, "idle", 1.0)
    a = D.render_fluid(m, t).astype(int)
    _run(m, "idle", 3.0, t0=t)
    b = D.render_fluid(m, t + 3).astype(int)
    assert np.abs(a - b).mean() > 1.0


def test_al_hablar_entra_aire_que_empuja_las_nubes_y_al_callar_se_asientan() -> None:
    m = D.DropModel()
    t = _run(m, "listen", 1.5, rms=0.0)
    calma = m.field.pos.copy()
    t = _run(m, "listen", 1.0, rms=0.12, t0=t)
    assert m.field.pressure > 0.8 and m.lvl > 0.8
    empuje = np.abs(m.field.pos - calma).max()
    assert empuje > 0.3                                    # el aire mueve las nubes
    assert m.field.pos[:, 1].mean() < calma[:, 1].mean()   # y las eleva (y crece hacia abajo)
    t = _run(m, "listen", 0.3, rms=0.0, t0=t)
    assert m.field.pressure > 0.4                          # inercia: el aire no se corta en seco
    _run(m, "listen", 4.0, rms=0.0, t0=t)
    assert m.field.pressure < 0.05 and m.lvl < 0.05
    assert np.abs(m.field.vel).max() < 0.2                 # se asientan


def test_en_calma_las_nubes_estan_en_la_mitad_inferior() -> None:
    from ezagent.voice.notch.clouds import BANK_Y, CloudField

    for layout in ("pill", "circle"):
        f = CloudField(layout)
        assert (f.home[:, 1] >= BANK_Y[0]).all()              # y crece hacia abajo
    m = D.DropModel("pill")
    _run(m, "idle", 2.0, palette="light")
    rgb = D.render_fluid(m, 1.0).astype(int)
    top = rgb[: rgb.shape[0] // 4]
    assert (top[..., 0] < 200).mean() > 0.95                  # arriba, cielo sin nubes


def test_cuando_habla_ezagent_el_banco_ondula() -> None:
    m = D.DropModel("pill")
    t = _run(m, "speak", 1.0, rms=0.0)
    calma = m.field.pos[:, 1].copy()
    ys = []
    for _ in range(6):
        t = _run(m, "speak", 0.25, rms=0.1, t0=t)
        ys.append(m.field.pos[:, 1].copy())
    mov = np.abs(np.asarray(ys) - calma).max()
    assert mov > 0.2                                          # se nota el movimiento
    sube_y_baja = np.ptp(np.asarray(ys), axis=0)
    assert (sube_y_baja > 0.05).sum() >= 3                    # ola: varias nubes suben y bajan
    assert m.field.pos[:, 1].min() > -0.6                     # el techo suave las contiene


def test_el_fluido_devuelve_rgb_uint8() -> None:
    for layout in ("pill", "circle"):
        m = D.DropModel(layout)
        _run(m, "speak", 1.0, rms=0.1)
        rgb = D.render_fluid(m, 1.0)
        assert rgb.dtype == np.uint8 and rgb.ndim == 3 and rgb.shape[2] == 3
        assert rgb.shape[:2] == D._fluid_grid(layout)[0].shape


def test_cielo_arriba_y_nubes_claras() -> None:
    m = D.DropModel("pill")
    _run(m, "idle", 1.0, palette="light")
    rgb = D.render_fluid(m, 1.0).astype(int)
    assert rgb[0].mean() < rgb.max()                       # arriba, cielo azul
    assert rgb.max() >= 240                                # hay nubes casi blancas


def test_ruido_determinista_y_acotado() -> None:
    from ezagent.voice.notch import clouds as C

    x = np.linspace(0, 40, 200, dtype=np.float32)
    xx, yy = np.meshgrid(x, x)
    n = C._fbm(xx, yy)
    assert 0.0 <= n.min() and n.max() <= 1.0001
    assert np.array_equal(n, C._fbm(xx, yy))


def test_colores_siguen_al_estado_y_al_tema() -> None:
    from ezagent.voice.notch.clouds import colors_for

    m = D.DropModel()
    t = _run(m, "think", 3.0, palette="light")
    assert np.abs(m.cols - colors_for("light", "think")).max() < 8
    t = _run(m, "error", 3.0, palette="light", t0=t)
    assert np.abs(m.cols - colors_for("light", "error")).max() < 8
    _run(m, "speak", 3.0, palette="dark", t0=t)
    assert np.abs(m.cols - colors_for("dark", "speak")).max() < 8


def test_temas_completos_y_distintos() -> None:
    from ezagent.voice.notch import clouds as C

    for theme in C.THEMES:
        assert set(C.COLORS[theme]) >= set(C.POSES)
        assert len({C.COLORS[theme][p].tobytes() for p in C.POSES}) == len(C.POSES)
        assert all(c.shape == (5, 3) for c in C.COLORS[theme].values())
    assert not np.array_equal(C.COLORS["light"]["idle"], C.COLORS["dark"]["idle"])


def test_ganancia_automatica_usa_todo_el_rango() -> None:
    weak, strong = D.DropModel(), D.DropModel()
    _run(weak, "listen", 1.0, rms=0.012)
    _run(strong, "listen", 1.0, rms=0.2)
    assert abs(weak.lvl - strong.lvl) < 0.1 and weak.lvl > 0.7


def test_movimiento_reducido_no_anima_el_flujo() -> None:
    m = D.DropModel()
    home = m.field.pos.copy()
    _run(m, "speak", 1.0, rms=0.1, reduced=True)
    assert m.flow == 0.0 and np.array_equal(m.field.pos, home)


# ------------------------------------------------------------------- botones
def test_modo_al_acercar_oculta_y_muestra_los_botones() -> None:
    m = D.DropModel()
    t = _run(m, "idle", 1.0, buttons="hover", hover=False)
    assert all(m.positions()[n][3] < 0.05 for n in D.BUTTONS)
    for n in D.BUTTONS:                                           # no se puede pulsar lo invisible
        assert m.item_at(*D.slots("pill")[n]) is None
        assert m.item_at(*m.positions()[n][:2]) != n
    t = _run(m, "idle", 0.6, buttons="hover", hover=True, t0=t)
    assert all(m.positions()[n][3] > 0.95 for n in D.BUTTONS)
    _run(m, "idle", 1.5, buttons="hover", hover=False, t0=t)
    assert all(m.positions()[n][3] < 0.05 for n in D.BUTTONS)


def test_modo_siempre_deja_los_botones_visibles() -> None:
    m = D.DropModel()
    _run(m, "idle", 1.0, buttons="always", hover=False)
    assert all(m.positions()[n][3] > 0.95 for n in D.BUTTONS)


def test_deteccion_de_elementos() -> None:
    m = D.DropModel("pill")
    _run(m, "idle", 1.0)
    pos = m.positions()
    for n in D.BUTTONS:
        assert m.item_at(*pos[n][:2]) == n
    assert m.item_at(*pos["pill"][:2]) == "pill"
    assert m.item_at(1.0, 1.0) is None
    w, h = D.layout_size("pill")
    assert m.near(w / 2, h / 2) and not m.near(w / 2, h / 2 + 400)


def test_los_botones_plegados_no_se_pulsan() -> None:
    m = D.DropModel("pill")
    t = _run(m, "idle", 1.0)
    gear = m.positions()["gear"][:2]
    _run(m, "ask", 1.5, asking=True, t0=t)
    assert m.item_at(*gear) is None


@pytest.mark.parametrize("layout", ["pill", "circle"])
@pytest.mark.parametrize("pose", ["idle", "listen", "think", "speak", "ask", "error", "mute", "sleep"])
def test_render_produce_bgra_premultiplicado(layout: str, pose: str) -> None:
    m = D.DropModel(layout)
    _run(m, pose, 0.8, rms=0.06, asking=(pose == "ask"))
    w, h = D.layout_size(layout)
    frame = D.render_model(m, round(w), round(h), 1.0, "light", muted=(pose == "mute"),
                           in_call=True, menu_open=False, now=1001.0)
    assert frame.shape == (round(h), round(w), 4) and frame.dtype == np.uint8
    assert (frame[..., :3].max(axis=2).astype(int) <= frame[..., 3].astype(int) + 1).all()
    fx, fy = D.form_center(layout)
    assert frame[int(fy), int(fx), 3] == 255                    # la forma es opaca en el centro
    assert frame[1, 1, 3] == 0                                  # esquina transparente


def test_los_botones_cambian_con_el_tema_y_se_cachean() -> None:
    claro = D.button_sprite("mic", "light", 1.0)
    oscuro = D.button_sprite("mic", "dark", 1.0)
    assert claro.size == oscuro.size
    c = np.asarray(claro)[claro.height // 2 + 12, claro.width // 2 - 14]
    o = np.asarray(oscuro)[oscuro.height // 2 + 12, oscuro.width // 2 - 14]
    assert c[:3].sum() > o[:3].sum()                            # botón claro vs oscuro
    assert D.button_sprite("mic", "light", 1.0) is claro        # cacheado


# ------------------------------------------------------------- contrato con la app
class _Fake(D.DropUI):
    """DropUI sin ventana ni Tk para probar el contrato."""

    def __init__(self, cfg=None) -> None:
        super().__init__(root=None, cfg=cfg)
        self.actions: list[str] = []
        self.on_action = self.actions.append


def _prepare(ui: _Fake, asking: bool = False) -> None:
    _run(ui.model, "ask" if asking else "idle", 1.5, asking=asking)


def test_clic_en_botones_emite_acciones() -> None:
    ui = _Fake()
    _prepare(ui)
    ui.set_in_call(True)
    pos = ui.model.positions()
    for name in ("gear", "mic", "hang"):
        assert ui.pointer_down(*pos[name][:2]) is True
        ui.pointer_up(*pos[name][:2])
    assert ui.actions == ["panel", "mute", "hangup"]


def test_al_colgar_x_se_convierte_en_llamar_y_el_micro_se_pliega() -> None:
    ui = _Fake()
    ui.set_in_call(False)
    _run(ui.model, "sleep", 1.5, in_call=False)
    pos = ui.model.positions()
    assert pos["mic"][3] < 0.05                               # sin llamada no hay micrófono
    assert ui.model.call.x > 0.95                              # ✕ es ahora «llamar»
    pill_edge = D.slots("pill")["pill"][0] + D.PILL_W / 2
    gap = pos["hang"][0] - D.BTN / 2 - pill_edge
    assert abs(gap - (D.GAP + D.CALL_GAP)) < 0.5             # con respiro, no pegado a la forma
    assert ui.model.positions()["pill"][:2] == pytest.approx(D.slots("pill")["pill"])
    ui.pointer_down(*pos["hang"][:2])
    ui.pointer_up(*pos["hang"][:2])
    assert ui.actions == ["listen"]                            # el mismo botón vuelve a llamar
    ui.set_in_call(True)
    _run(ui.model, "listen", 1.5)
    pos = ui.model.positions()
    assert pos["mic"][3] > 0.95 and ui.model.call.x < 0.05
    ui.pointer_down(*pos["hang"][:2])
    ui.pointer_up(*pos["hang"][:2])
    assert ui.actions[-1] == "hangup"


def test_en_circulo_la_fila_sin_micro_queda_centrada_y_con_respiro() -> None:
    sl = D.slots("circle", 0.0)
    w, _ = D.layout_size("circle")
    assert abs((sl["gear"][0] + sl["hang"][0]) / 2 - w / 2) < 0.5
    assert sl["hang"][0] - sl["gear"][0] - D.BTN >= D.GAP + D.CALL_GAP - 0.5


def test_el_menos_del_menu_cierra_solo_el_menu() -> None:
    ui = _Fake()
    hidden: list = []
    ui.on_hide = lambda: hidden.append(True)
    closed: list = []
    ui._close_menu = lambda: closed.append(True)
    ui._hide_from_menu()
    assert closed == [True] and hidden == []                   # la barra sigue visible


def test_clic_sobre_la_forma_inicia_arrastre() -> None:
    ui = _Fake()
    _prepare(ui)
    assert ui.pointer_down(*ui.model.positions()["pill"][:2]) is False
    assert ui.pointer_down(1.0, 1.0) is False


def test_soltar_fuera_del_boton_no_acciona() -> None:
    ui = _Fake()
    _prepare(ui)
    pos = ui.model.positions()
    ui.pointer_down(*pos["gear"][:2])
    ui.pointer_up(1.0, 1.0)
    assert ui.actions == []


def test_confirmacion_con_los_botones_x_y_v() -> None:
    ui = _Fake()
    calls: list[str] = []
    ui.show_confirmation("¿Detengo?", False, lambda: calls.append("si"), lambda: calls.append("no"))
    assert ui._asking
    _prepare(ui, asking=True)
    pos = ui.model.positions()
    ui.pointer_down(*pos["mic"][:2])
    ui.pointer_up(*pos["mic"][:2])
    ui.pointer_down(*pos["hang"][:2])             # al preguntar, ✕ es «no», no cuelga
    ui.pointer_up(*pos["hang"][:2])
    assert calls == ["si", "no"] and ui.actions == []
    ui.hide_confirmation()
    assert not ui._asking and ui._on_yes is None


def test_contrato_con_la_app_sin_ventana() -> None:
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
    assert ui._theme_name() == "light"
    ui.set_mic_level(0.3)
    ui.toggle_menu()                       # sin ajustes ni ventana: no falla


def test_pose_dormida_sin_llamada_y_ask_manda() -> None:
    ui = _Fake()
    ui._ui.set_visible(True)
    ui.set_in_call(False)
    now = time.monotonic() + 1
    assert ui._pose(now) == "sleep"
    ui.set_in_call(True)
    assert ui._pose(now + 1) in ("idle", "listen")
    ui._asking = True
    assert ui._pose(now + 2) == "ask"


def test_clamp_mantiene_la_barra_en_pantalla() -> None:
    ui = _Fake()
    wa = D._shell.work_area()
    w, h = ui._size()
    x, y = ui.clamp_position(-99999, -99999)
    assert x >= wa[0] - round(D.PAD * ui.s) and y >= wa[1] - round(D.PAD * ui.s)
    x2, y2 = ui.clamp_position(99999, 99999)
    assert x2 <= wa[2] - w + round(D.PAD * ui.s) and y2 <= wa[3] - h + round(D.PAD * ui.s)


def test_posicion_por_defecto_abajo_y_centrada() -> None:
    ui = _Fake()
    wa = D._shell.work_area()
    _, h = ui._size()
    x, y = ui._default_position()
    fx, _ = D.form_center(ui.layout)
    assert abs((x + fx * ui.s) - (wa[0] + wa[2]) / 2) <= 1      # la forma, centrada
    assert y + h <= wa[3]


# ------------------------------------------------------------------ configuración
def test_config_por_defecto_y_validacion() -> None:
    cfg = VoiceConfig()
    assert cfg.ui_layout == "pill"
    assert not hasattr(cfg, "ui_palette") and not hasattr(cfg, "ui_buttons")
    cfg.ui_layout = "circle"
    ui = D.DropUI(root=None, cfg=cfg)
    assert (ui.layout, ui.buttons) == ("circle", "hover")     # botones solo al acercar
    cfg.ui_layout = "raro"
    assert D.DropUI(root=None, cfg=cfg).layout == "pill"


def test_un_voice_json_antiguo_con_paleta_sigue_cargando(tmp_path) -> None:
    path = tmp_path / "voice.json"
    path.write_text('{"ui_palette": "ezagent", "ui_buttons": "always", "ui_layout": "circle"}',
                    encoding="utf-8")
    assert VoiceConfig.load(path).ui_layout == "circle"


def test_create_ui_crea_la_barra() -> None:
    assert isinstance(D.create_ui(None, VoiceConfig()), D.DropUI)


def test_arrastrar_guarda_posicion_propia() -> None:
    cfg = VoiceConfig()
    saved: dict = {}
    cfg.save_key = lambda k, v, path=None: saved.__setitem__(k, v)  # type: ignore[method-assign]
    ui = D.DropUI(root=None, cfg=cfg)
    got: list = []
    ui.on_position_change = lambda x, y: got.append((x, y))
    ui.drag_end(123, 456)
    assert saved == {"drop_x": 123, "drop_y": 456, "drop_layout": "pill"} and got == [(123, 456)]


def test_la_posicion_guardada_solo_vale_con_la_misma_disposicion() -> None:
    cfg = VoiceConfig()
    cfg.drop_x, cfg.drop_y = 500, 600
    cfg.drop_layout = "circle"                    # se guardó con otra disposición
    assert D.create_ui(None, cfg)._position is None
    cfg.drop_layout = "pill"
    assert D.create_ui(None, cfg)._position == (500, 600)
    cfg.drop_layout = ""                          # posición antigua del orbe
    assert D.create_ui(None, cfg)._position is None


# ------------------------------------------------------ cambio de forma al vuelo
def test_set_layout_cambia_modelo_y_guarda_la_preferencia() -> None:
    cfg = VoiceConfig()
    saved: dict = {}
    cfg.save_key = lambda k, v, path=None: saved.__setitem__(k, v)  # type: ignore[method-assign]
    ui = D.DropUI(root=None, cfg=cfg)
    assert ui.layout == "pill"
    ui.set_layout("circle")
    assert (ui.layout, ui.model.layout) == ("circle", "circle")
    assert saved["ui_layout"] == "circle"
    ui.set_layout("circle")                       # sin cambios: no hace nada
    ui.toggle_layout()
    assert (ui.layout, ui.model.layout, saved["ui_layout"]) == ("pill", "pill", "pill")
    ui.set_layout("raro")                         # valor inválido: cae a la píldora
    assert ui.layout == "pill"


def test_doble_clic_en_la_forma_cambia_de_disposicion() -> None:
    ui = D.DropUI(root=None, cfg=VoiceConfig())
    _run(ui.model, "idle", 1.0)
    fx, fy = D.form_center("pill")
    assert ui.pointer_down(fx, fy) is False       # primer clic: arrastra la ventana
    assert ui.layout == "pill"
    assert ui.pointer_down(fx, fy) is True        # segundo clic rápido: cambia la forma
    assert ui.layout == "circle"


def test_el_boton_de_ajustes_es_una_hamburguesa_no_un_sol() -> None:
    """Tres rayas horizontales (el engranaje de 8 dientes se leía como un sol)."""
    sp = D.button_sprite("gear", "light", 2.0)
    a = np.asarray(sp)[..., 3]
    h, w = a.shape
    icon = np.asarray(sp.convert("L")).astype(int)
    core = icon[h // 2 - 10 : h // 2 + 10, w // 2 - 10 : w // 2 + 10]
    rows = (core < core.max() - 60).any(axis=1)
    # alterna raya / hueco / raya / hueco / raya
    cambios = int(np.count_nonzero(np.diff(rows.astype(int)) != 0))
    assert cambios == 6
