"""Actividad de los agentes: modelo, órbita y vista rápida (sin ventana real)."""

from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice import activity as act
from ezagent.voice.activity import ActivityItem, ActivityLog
from ezagent.voice.hud import peek as pk


def _item(key: str, state: str, since: float = 0.0, **kw) -> ActivityItem:
    return ActivityItem(key, f"Tarea {key}", "codex", state, since, **kw)


# ------------------------------------------------------------------ modelo
def test_el_orden_pone_primero_lo_que_te_necesita_y_al_final_lo_terminado() -> None:
    items = [
        _item("a", act.DONE, 50, unseen=True),
        _item("b", act.WORKING, 10, live=True),
        _item("c", act.NEEDS_YOU, 5, unseen=True),
        _item("d", act.WAITING, 1, live=True),
        _item("e", act.FAILED, 2, unseen=True),
    ]
    assert [i.key for i in act.order(items)] == ["c", "e", "b", "d", "a"]


def test_lo_vivo_gana_a_un_desenlace_con_la_misma_clave() -> None:
    merged = act.merge([_item("t1", act.WORKING, live=True)], [_item("t1", act.DONE)])
    assert [(i.key, i.state) for i in merged] == [("t1", act.WORKING)]


def test_la_orbita_solo_marca_lo_vivo_y_lo_no_visto_con_tope() -> None:
    items = [_item(str(i), act.WORKING, live=True) for i in range(7)]
    items.append(_item("seen", act.DONE))
    assert act.orbit(items) == (act.WORKING,) * 5
    assert act.orbit([_item("x", act.DONE)]) == ()
    assert act.orbit([_item("x", act.DONE, unseen=True)]) == (act.DONE,)


def test_el_registro_traduce_motivos_y_caduca_lo_visto() -> None:
    now = [1000.0]
    log = ActivityLog(clock=lambda: now[0])
    assert log.record("t1", "codex", "  Revisar\n permisos ", "lost").state == act.STALE
    assert log.record("t2", "claude", "", "error").title == "Tarea"
    assert {i.state for i in log.items()} == {act.STALE, act.FAILED}
    now[0] += act.KEEP_SECS + 1
    assert len(log.items()) == 2  # sin ver: no caduca
    assert log.seen_all() and not log.seen_all()
    assert log.items() == ()  # visto y viejo: se va


def test_el_registro_tiene_tope_y_olvida_por_clave() -> None:
    log = ActivityLog()
    for i in range(act.LOG_LIMIT + 3):
        log.record(f"t{i}", "", "x", "done")
    assert len(log.items()) == act.LOG_LIMIT
    log.forget("t5")
    assert "t5" not in {i.key for i in log.items()}


def test_las_cadenas_quedan_en_espera() -> None:
    tasks = SimpleNamespace(
        get=lambda a: None,
        chains=lambda: [{"source": "t1", "agent": "opencode", "name": "QA", "created_at": 3.0}],
    )
    items = act.live_items(SimpleNamespace(watched=set(), tasks=tasks), now=10.0)
    assert [(i.title, i.state) for i in items] == [("QA", act.WAITING)]


def test_los_titulos_largos_se_recortan() -> None:
    title = act.clean_title("x" * 500)
    assert len(title) == act.TITLE_LIMIT and title.endswith("…")


# ------------------------------------------------------------ vista rápida
def test_la_linea_de_estado_dice_estado_agente_y_tiempo() -> None:
    live = _item("a", act.WORKING, since=0.0, live=True)
    done = _item("b", act.DONE, since=0.0)
    assert pk.meta_line(live, 12 * 60) == "En curso · Codex · 12 min"
    assert pk.meta_line(done, 3 * 3600) == "Completada · Codex · hace 3 h"
    assert pk.meta_line(done, 20) == "Completada · Codex · ahora"


def test_fit_recorta_con_puntos_suspensivos() -> None:
    text = "Una tarea con un título larguísimo que no cabe en la tarjeta de actividad"
    short = pk.fit(text, pk.FS_TASK, 120, 1.0)
    assert short.endswith("…") and len(short) < len(text)
    assert pk.fit("Corto", pk.FS_TASK, 120, 1.0) == "Corto"


def _peek(monkeypatch) -> tuple[pk.Peek, list]:
    placed: list = []
    ui = SimpleNamespace(s=1.0, on_activity_open=placed.append)
    peek = pk.Peek(ui)
    monkeypatch.setattr(peek, "_place", lambda size: placed.append(size))
    monkeypatch.setattr(peek, "_pointer_in_card", lambda: False)
    monkeypatch.setattr(peek, "_pointer_row", lambda: None)
    return peek, placed


def test_el_puntero_quieto_sobre_la_forma_abre_y_al_salir_cierra_con_margen(monkeypatch) -> None:
    peek, placed = _peek(monkeypatch)
    peek.refresh((_item("a", act.WORKING, live=True),))
    peek.tick(0.0, True, False)
    peek.tick(pk.HOVER_DELAY - 0.05, True, False)
    assert not peek.is_open  # pasar por encima no basta
    peek.tick(pk.HOVER_DELAY + 0.01, True, False)
    assert peek.is_open and placed
    peek.tick(1.0, False, False)
    peek.tick(1.0 + pk.LEAVE_GRACE - 0.05, False, False)
    assert peek.is_open  # aún se puede alcanzar la tarjeta
    peek.tick(1.0 + pk.LEAVE_GRACE + 0.05, False, False)
    assert not peek.is_open


def test_sin_actividad_no_se_abre(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    peek.tick(0.0, True, False)
    peek.tick(1.0, True, False)
    assert not peek.is_open


def test_un_aviso_se_muestra_solo_y_luego_se_va(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    done = _item("b", act.DONE, unseen=True)
    peek.refresh((_item("a", act.WORKING, live=True), done))
    peek.toast(done, 0.0)
    peek.tick(0.0, False, False)
    assert peek.is_open and peek._rows == ((done,), 0)
    peek.tick(pk.TOAST_SECS[act.DONE] + 0.1, False, False)
    assert not peek.is_open


def test_el_menu_o_el_chat_cierran_la_vista_rapida(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    peek.refresh((_item("a", act.WORKING, live=True),))
    peek.tick(0.0, True, False)
    peek.tick(1.0, True, False)
    peek.tick(1.1, True, True)
    assert not peek.is_open


def test_clic_en_una_fila_abre_esa_tarea(monkeypatch) -> None:
    peek, opened = _peek(monkeypatch)
    rows = (_item("a", act.NEEDS_YOU, unseen=True), _item("b", act.WORKING, live=True))
    peek._rows = (rows, 0)
    y = pk.MARGIN + pk.PAD_Y + pk.ROW_H * 1.5
    peek.click(int(pk.MARGIN + 40), int(y))
    assert opened == ["b"]
    assert peek.row_at(int(pk.MARGIN + 40), int(pk.MARGIN + pk.PAD_Y + pk.ROW_H * 3)) is None


def test_la_tarjeta_se_dibuja_con_filas_y_aparece_sin_reescalar() -> None:
    rows = (_item("a", act.NEEDS_YOU, unseen=True), _item("b", act.WORKING, live=True))
    w = round(pk.PEEK_W + 2 * pk.MARGIN)
    h = round(pk.card_height(2, False) + 2 * pk.MARGIN)
    layer = pk.paint_rows(rows, 0, w, h, 1.0, "dark", None, 60.0)
    pk.paint_marks(layer, rows, 1.0, "dark", 0.0, False)
    full = pk.compose(layer, 1.0, True, 1.0, False)
    half = pk.compose(layer, 0.5, True, 1.0, False)
    assert full.shape == (h, w, 4) and full[..., 3].max() == 255
    assert half[..., 3].max() < 200  # se funde...
    assert pk.compose(layer, 0.5, True, 1.0, True)[..., 3].max() < 200  # ...también reducido
    # el contenido baja hacia la forma (que está debajo) mientras aparece
    top_full = np.argmax(full[..., 3].max(axis=1) > 0)
    top_half = np.argmax(half[..., 3].max(axis=1) > 0)
    assert top_half > top_full


# ----------------------------------------------------------- tiempos de cuadro
def test_las_estadisticas_de_cuadros_informan_cada_tanto() -> None:
    from ezagent.voice.hud.frame_stats import BUDGET_MS, REPORT_SECS, FrameStats

    now = [0.0]
    stats = FrameStats("drop", clock=lambda: now[0])
    for ms in (4.0, 5.0, 30.0):
        assert stats.add(ms) is None
    now[0] = REPORT_SECS + 1
    line = stats.add(6.0)
    assert line is not None and "Cuadros drop: 4" in line and "1 sobre" in line
    assert f"{BUDGET_MS:.1f}" in line
    assert stats.summary() == ""


def test_una_ventana_oculta_espera_en_vez_de_girar_sin_dormir() -> None:
    from ezagent.voice.hud.layered import wait_delay

    assert wait_delay(False, 0.0, 1 / 30, 100.0) > 0.01  # oculta y sin pintar desde hace rato
    assert wait_delay(True, 0.0, 1 / 30, 100.0) < 0  # visible y atrasada: pinta ya
    assert 0 < wait_delay(True, 100.0, 1 / 30, 100.0) <= 1 / 60


def test_pedirlo_por_voz_muestra_la_lista_completa_un_rato(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    items = tuple(_item(str(i), act.WORKING, live=True) for i in range(6))
    peek.refresh(items)
    peek.reveal(0.0)
    peek.tick(0.0, False, False)
    assert peek.is_open and peek._rows == (items[: pk.MAX_ROWS], 2)
    peek.tick(pk.REVEAL_SECS + 0.1, False, False)
    assert not peek.is_open


class _LockCheckingWindow:
    """Ventana falsa: cada llamada de Win32 anota si el lock del Peek estaba libre.

    En la ventana real, ShowWindow/SetWindowPos esperan al hilo de la tarjeta, que
    puede estar en ``render()`` esperando ese lock: llamarlas con el lock tomado
    congelaba la barra («la app no responde»).
    """

    def __init__(self, peek: pk.Peek) -> None:
        self.peek = peek
        self._visible = True
        self.w = self.h = 10
        self.x = self.y = 0
        self.calls: list[tuple[str, bool]] = []

    def _check(self, name: str) -> None:
        free = self.peek._lock.acquire(timeout=0.2)
        if free:
            self.peek._lock.release()
        self.calls.append((name, free))

    def hide(self) -> None:
        self._check("hide")
        self._visible = False

    def show(self) -> None:
        self._check("show")
        self._visible = True

    def set_pos(self, x: int, y: int) -> None:
        self._check("set_pos")

    def resize(self, w: int, h: int) -> None:
        self._check("resize")
        self.w, self.h = w, h

    def _screen_cursor(self) -> tuple[int, int]:
        return (-9999, -9999)


def test_abrir_y_cerrar_muchas_veces_nunca_toca_la_ventana_con_el_lock_tomado(
    monkeypatch,
) -> None:
    monkeypatch.setattr(pk._shell, "work_area", lambda: (0, 0, 1920, 1080))
    ui = SimpleNamespace(s=1.0, form_extent=lambda: (960.0, 1000.0, 19.0))
    peek = pk.Peek(ui)
    win = _LockCheckingWindow(peek)
    peek._win = win
    peek.refresh((_item("a", act.WORKING, live=True),))
    t = 0.0
    for _ in range(20):  # hover → abre; el chat se abre → cierra con animación
        for _ in range(30):
            t += 1 / 60
            peek.tick(t, True, False)
        for _ in range(30):
            t += 1 / 60
            peek.tick(t, False, True)
    names = {name for name, _ in win.calls}
    assert {"hide", "set_pos", "resize"} <= names
    assert all(free for _, free in win.calls), win.calls[:5]


def test_las_llamadas_de_win32_desde_otro_hilo_las_hace_el_hilo_de_la_ventana(
    monkeypatch,
) -> None:
    import threading

    from ezagent.voice.hud import layered as lw

    posted: list[tuple] = []
    native: list[str] = []
    fake = SimpleNamespace(
        PostMessageW=lambda *a: posted.append(a),
        ShowWindow=lambda *a: native.append("ShowWindow"),
        SetWindowPos=lambda *a: native.append("SetWindowPos"),
    )
    monkeypatch.setattr(lw, "_u32", fake)
    win = lw.LayeredWindow(10, 10)
    win.hwnd, win._owner = 1234, -1  # la ventana es de otro hilo
    win.show()
    win.set_pos(5, 6)
    assert native == [] and len(posted) == 2  # quien llama no espera a nadie
    assert (win.x, win.y, win._visible) == (5, 6, True)  # el estado sí cambia ya
    win._run_owned_calls()  # lo que hace el bucle del hilo de la ventana
    assert native == ["ShowWindow", "SetWindowPos", "SetWindowPos"]
    win._owner = threading.get_ident()  # desde su propio hilo: directo
    win.hide()
    assert native[-1] == "ShowWindow" and not win._owned_calls


# ------------------------------------------------------------- quitar filas
def test_la_x_de_una_fila_la_quita_al_instante_y_avisa_a_la_app(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    gone: list[str] = []
    peek.ui.on_activity_dismiss = gone.append
    rows = (_item("a", act.DONE, unseen=True), _item("b", act.WORKING, live=True))
    peek.refresh(rows)
    peek._rows = (rows, 0)
    y = int(pk.MARGIN + pk.PAD_Y + pk.ROW_H / 2)
    x_close = int(pk.MARGIN + pk.PEEK_W - pk.PAD_X - 8)
    assert peek.dismiss_at(int(pk.MARGIN + 60), y) is None  # el texto abre, no quita
    peek.click(x_close, y)
    assert gone == ["a"]
    assert [i.key for i in peek._items] == ["b"] and [i.key for i in peek._rows[0]] == ["b"]


def test_un_enjambre_en_marcha_no_se_puede_quitar(monkeypatch) -> None:
    peek, _ = _peek(monkeypatch)
    rows = (ActivityItem("swarm:s1", "Migrar", "enjambre", act.WORKING, 0.0, live=True),)
    peek._rows = (rows, 0)
    y = int(pk.MARGIN + pk.PAD_Y + pk.ROW_H / 2)
    assert peek.dismiss_at(int(pk.MARGIN + pk.PEEK_W - pk.PAD_X - 8), y) is None
    assert act.dismissible(ActivityItem("swarm:ask", "¿Sigo?", "", act.NEEDS_YOU, 0.0))


def test_la_x_se_dibuja_solo_en_la_fila_bajo_el_puntero() -> None:
    rows = (_item("a", act.DONE), _item("b", act.DONE))
    w = round(pk.PEEK_W + 2 * pk.MARGIN)
    h = round(pk.card_height(2, False) + 2 * pk.MARGIN)
    plain = pk.paint_rows(rows, 0, w, h, 1.0, "dark", None, 0.0)
    hovered = pk.paint_rows(rows, 0, w, h, 1.0, "dark", 0, 0.0)
    x = round(pk.MARGIN + pk.PEEK_W - pk.PAD_X - 8)
    y = round(pk.MARGIN + pk.PAD_Y + pk.ROW_H / 2)
    zone = (slice(y - 6, y + 6), slice(x - 6, x + 6))
    assert np.abs(hovered[zone] - plain[zone]).max() > 0.2


def test_la_app_olvida_vigila_menos_y_cancela_etapas_al_quitar(tmp_path) -> None:
    from ezagent.voice import app as voice_app
    from ezagent.voice.tasks import TaskRegistry

    tasks = TaskRegistry(tmp_path / "tasks.json")
    tasks.add("t1", "codex", "Refactor")
    tasks.add_chain({"source": "t1", "agent": "opencode", "task": "QA", "created_at": 1.0})
    tasks.add_chain({"source": "t9", "agent": "claude", "task": "Docs", "created_at": 2.0})
    snapshots: list[tuple] = []
    app = SimpleNamespace(
        tasks=tasks, watched={"t1"}, hud=SimpleNamespace(set_activity=snapshots.append)
    )
    voice_app._news_arrived(app, "t5", "claude", "Listo", "done")
    voice_app._dismiss_activity(app, "t5")  # un desenlace: se olvida
    assert "t5" not in {i.key for i in snapshots[-1]}
    docs = next(s for s in tasks.chains() if s["source"] == "t9")
    voice_app._dismiss_activity(app, act.chain_key(docs))  # una etapa: se cancela sola
    assert [s["source"] for s in tasks.chains()] == ["t1"]
    voice_app._dismiss_activity(app, "t1")  # vigilada: deja de vigilarse con sus etapas
    assert app.watched == set() and tasks.get("t1") is None and tasks.chains() == []
    assert snapshots[-1] == ()


# ---------------------------------------------------- tamaño coherente con la barra
def test_a_notice_is_a_capsule_sized_like_the_pill() -> None:
    from ezagent.voice.hud import drop as D
    from ezagent.voice.hud import peek_paint as pp

    assert pp.card_height(1, False) == D.PILL_H
    assert pp.RADIUS == D.PILL_H / 2
    assert D.PILL_W < pp.PEEK_W < D.PILL_W + 4 * (D.BTN + D.GAP)  # entre la píldora y la fila


def test_the_card_follows_the_bar_size_not_only_the_screen(monkeypatch) -> None:
    ui = SimpleNamespace(s=1.5, bs=1.5 * 0.85)
    peek = pk.Peek(ui)
    w, h = peek._window_size(1, 0)
    assert w == round((pk.PEEK_W + 2 * pk.MARGIN) * 1.5 * 0.85)
    assert h == round((pk.ROW_H + 2 * pk.MARGIN) * 1.5 * 0.85)


@pytest.mark.parametrize("scale", [1.0, 1.275, 2.0])
def test_the_two_lines_sit_centred_in_the_row(scale: float) -> None:
    from ezagent.voice.hud import peek_paint as pp
    from ezagent.voice.hud.text import cap_height

    first, second = pp.baselines(scale)
    above = first - cap_height(pp.FS_TASK, scale)
    below = pp.ROW_H - second
    assert above == pytest.approx(below, abs=0.01)


# ------------------------------------------------------ colocación junto al hub
WORK = (0, 0, 1920, 1040)
CARD = (round(pk.PEEK_W + 2 * pk.MARGIN), round(pk.ROW_H + 2 * pk.MARGIN))


def _card_edges(hub) -> tuple[float, float, float, float, bool]:
    from ezagent.voice.hud.peek import place_card

    x, y, above = place_card(hub, CARD, WORK, 1.0)
    m = pk.MARGIN
    return (x + m, y + m, x + CARD[0] - m, y + CARD[1] - m, above)


def test_the_card_is_centred_on_the_whole_hub() -> None:
    hub = (800, 980, 1100, 1030)  # forma + botones, abajo en el centro
    x0, _, x1, y1, above = _card_edges(hub)
    assert (x0 + x1) / 2 == pytest.approx((800 + 1100) / 2, abs=1)
    assert above and y1 <= 980


def test_near_the_right_edge_it_lines_up_with_the_hub_not_the_screen_corner() -> None:
    hub = (1800, 980, 1905, 1030)  # centrada en el hub, la tarjeta se saldría por la derecha
    assert (1800 + 1905) / 2 + pk.PEEK_W / 2 > 1920
    _, _, x1, _, _ = _card_edges(hub)
    assert x1 == pytest.approx(1905, abs=1)  # borde derecho del hub, no el de la pantalla


def test_near_the_left_edge_it_lines_up_with_the_hub() -> None:
    hub = (20, 980, 200, 1030)
    x0, _, _, _, _ = _card_edges(hub)
    assert x0 == pytest.approx(20, abs=1)


def test_at_the_top_of_the_screen_it_opens_below() -> None:
    hub = (800, 10, 1100, 60)
    _, y0, _, _, above = _card_edges(hub)
    assert not above and y0 >= 60


def test_the_hub_includes_the_buttons_not_only_the_sphere() -> None:
    from ezagent.voice.hud.drop import DropModel

    model = DropModel("circle", "left")
    x0, _, x1, _ = model.hub_rect()
    from ezagent.voice.hud import drop as D

    assert x1 - x0 > D.CIRCLE_D + 3 * D.BTN  # esfera + botones a la izquierda
    folded = model.hub_rect(0.0)
    assert folded[2] - folded[0] < x1 - x0  # sin llamada, el micro plegado no cuenta
