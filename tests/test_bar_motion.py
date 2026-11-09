"""Coreografía de la barra (bar_motion) y cadencia de fotogramas (layered)."""

from __future__ import annotations

import numpy as np

from ezagent.voice.notch import bar_motion as BM
from ezagent.voice.notch import drop as D
from ezagent.voice.notch.layered import FRAME_SLACK, frame_due, paint_mark


def _steps(model: D.DropModel, seconds: float, t0: float, **kw) -> tuple[float, list[dict]]:
    t, trace = t0, []
    for _ in range(int(seconds * 120)):
        t += 1 / 120
        model.update(
            t,
            1 / 120,
            "idle",
            0.0,
            "sky",
            kw.get("hover", False),
            kw.get("asking", False),
            False,
            kw.get("reduced", False),
            "hover",
            muted=kw.get("muted", False),
            hover_item=kw.get("hover_item"),
            pressed=kw.get("pressed"),
        )
        trace.append({n: model.choreo.appear[n].x for n in D.BUTTONS})
    return t, trace


def _hidden(layout: str = "pill", reduced: bool = False) -> tuple[D.DropModel, float]:
    m = D.DropModel(layout)
    t, _ = _steps(m, 1.0, 1000.0, reduced=reduced)
    assert all(m.choreo.appear[n].x < 0.01 for n in D.BUTTONS)
    return m, t


def _first_move(trace: list[dict], name: str, threshold: float = 0.02) -> int:
    return next(i for i, row in enumerate(trace) if row[name] > threshold)


def test_los_botones_salen_escalonados_del_mas_cercano_al_mas_lejano() -> None:
    m, t = _hidden("pill")
    _, trace = _steps(m, 0.6, t, hover=True)
    gear, mic, hang = (_first_move(trace, n) for n in ("gear", "mic", "hang"))
    assert gear == mic  # los dos pegados a la forma salen a la vez
    assert hang - mic >= 4  # ✕ sale ~40 ms después (120 Hz → 4–5 pasos)


def test_en_circulo_sale_primero_el_microfono() -> None:
    m, t = _hidden("circle")
    _, trace = _steps(m, 0.6, t, hover=True)
    assert _first_move(trace, "mic") < _first_move(trace, "gear")


def test_entrada_con_rebote_suave_y_salida_sin_rebote() -> None:
    m, t = _hidden()
    t, trace = _steps(m, 0.8, t, hover=True)
    peak = max(row["mic"] for row in trace)
    assert 1.03 < peak < 1.15  # rebote de entrada ~8 %
    _, trace = _steps(m, 1.6, t, hover=False)
    low = min(row["mic"] for row in trace)
    assert low > -0.01  # al volver no atraviesa la forma


def test_los_botones_nacen_desde_detras_de_la_forma() -> None:
    m, t = _hidden()
    _steps(m, 1 / 60, t, hover=True)
    fx, fy = D.form_center("pill")
    cx, cy, sc, al = m.positions()["hang"]
    assert abs(cx - fx) < 1 and abs(cy - fy) < 1
    assert sc < 0.65 and al < 0.05
    slot = D.slots("pill")["hang"]
    _steps(m, 1.0, t + 1 / 60, hover=True)
    cx, cy, sc, al = m.positions()["hang"]
    assert abs(cx - slot[0]) < 0.5 and sc > 0.99 and al > 0.99


def test_la_forma_crece_un_poco_al_acercar_y_vuelve() -> None:
    m, t = _hidden()
    t, _ = _steps(m, 0.8, t, hover=True)
    assert abs(m.positions()["pill"][2] - BM.FORM_GROW) < 0.005
    _steps(m, 1.6, t, hover=False)
    assert abs(m.positions()["pill"][2] - 1.0) < 0.005


def test_puntero_encima_y_presionado() -> None:
    m = D.DropModel("pill")
    t, _ = _steps(m, 1.0, 1000.0, hover=True, hover_item="mic")
    assert abs(m.positions()["mic"][2] - BM.HOVER_SCALE) < 0.01
    _steps(m, 0.5, t, hover=True, hover_item="mic", pressed="mic")
    assert abs(m.positions()["mic"][2] - BM.PRESS_SCALE) < 0.01


def test_movimiento_reducido_solo_funde_en_120_ms() -> None:
    m, t = _hidden(reduced=True)
    _, trace = _steps(m, 0.3, t, hover=True, reduced=True)
    assert max(row["mic"] for row in trace) <= 1.0  # sin rebote
    idx = next(i for i, row in enumerate(trace) if row["mic"] >= 1.0)
    assert 12 <= idx <= 16  # ~120 ms a 120 Hz
    cx, cy, sc, _ = m.positions()["mic"]
    assert (cx, cy) == D.slots("pill")["mic"] and sc == 1.0  # sin deslizar ni escalar
    assert m.positions()["pill"][2] == 1.0  # la forma no crece


def test_la_raya_de_pausa_se_dibuja_progresivamente() -> None:
    m = D.DropModel("pill")
    t, _ = _steps(m, 0.5, 1000.0)
    _steps(m, 0.05, t, muted=True)
    assert 0.05 < m.mute_p.x < 0.95
    medio = np.asarray(D.button_sprite("mic", "light", 1.0, muted=0.5))
    lleno = np.asarray(D.button_sprite("mic", "light", 1.0, muted=1.0))
    assert not np.array_equal(medio, lleno)
    assert D.button_sprite("mic", "light", 1.0, muted=0.51) is D.button_sprite(
        "mic", "light", 1.0, muted=0.5
    )


def test_fps_altos_mientras_algo_se_mueve() -> None:
    ui = D.DropUI(root=None)
    m = ui.model
    _steps(m, 1.0, 1000.0)
    assert ui.target_fps("idle", False, 1001.0) == D.FPS_CALM
    assert ui.target_fps("listen", False, 1001.0) == D.FPS_BUSY
    _steps(m, 0.05, 1001.0, hover=True)  # a medio salir
    assert ui.target_fps("idle", False, 1001.05) == D.FPS_BUSY  # aunque el puntero ya se fue


def test_cadencia_no_salta_cuadros_por_despertar_temprano() -> None:
    interval = 1 / 60
    # Windows despierta a los 16 ms enteros: antes eso saltaba el fotograma.
    assert frame_due(0.016, 0.0, interval)
    assert not frame_due(0.010, 0.0, interval)
    assert FRAME_SLACK < interval / 4


def test_la_marca_avanza_en_pasos_fijos_sin_deriva() -> None:
    interval = 1 / 60
    last, painted = 0.0, 0
    now = 0.0
    while now < 1.0:
        now += 0.016  # despertares de 16 ms
        if frame_due(now, last, interval):
            last = paint_mark(now, last, interval)
            painted += 1
    assert 58 <= painted <= 61
    assert paint_mark(0.5, 0.0, interval) == 0.5  # muy tarde: resincroniza
