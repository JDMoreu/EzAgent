"""SwarmStore: tablero del enjambre persistente, breve y seguro entre hilos."""

from __future__ import annotations

import threading
from pathlib import Path

import pytest

from ezagent.swarm.store import BROADCAST, MAX_BODY, MAX_ROUNDS, SYSTEM, SwarmError, SwarmStore
from ezagent.swarm.view import board, health


class Clock:
    def __init__(self) -> None:
        self.now = 1000.0

    def __call__(self) -> float:
        return self.now


@pytest.fixture
def clock() -> Clock:
    return Clock()


@pytest.fixture
def store(tmp_path: Path, clock: Clock) -> SwarmStore:
    return SwarmStore(tmp_path / "swarm" / "board.db", clock=clock)


@pytest.fixture
def swarm(store: SwarmStore) -> str:
    swarm_id = store.create("Construir la API de login", cwd="C:/proj")
    store.join(swarm_id, "plan", role="plan", agent="claude")
    store.join(swarm_id, "dev", role="dev", agent="opencode")
    store.join(swarm_id, "qa", role="qa", agent="codex")
    return swarm_id


def _pipeline(store: SwarmStore, swarm: str) -> tuple[int, int, int]:
    plan = store.add_task(swarm, "Planificar", role="plan")
    build = store.add_task(swarm, "Desarrollar", role="dev", deps=[plan.id])
    check = store.add_task(swarm, "Validar", role="qa", deps=[build.id])
    return plan.id, build.id, check.id


def test_crear_y_persistir(store: SwarmStore, swarm: str) -> None:
    info = store.get(swarm)
    assert info is not None and info.goal == "Construir la API de login"
    assert info.status == "active" and [s.id for s in store.active()] == [swarm]
    reopened = SwarmStore(store.path)
    assert [m.name for m in reopened.members(swarm)] == ["plan", "dev", "qa"]
    assert store.get("nope") is None


def test_objetivo_vacio_y_nombres_invalidos(store: SwarmStore, swarm: str) -> None:
    with pytest.raises(SwarmError):
        store.create("   ")
    for bad in ("", "Con espacio", "-x", "a" * 40, SYSTEM, "all"):
        with pytest.raises(SwarmError):
            store.join(swarm, bad)
    with pytest.raises(SwarmError):
        store.join(swarm, "otro", role="Rol Raro")
    with pytest.raises(SwarmError):
        store.join("nope", "dev")


def test_join_es_idempotente_y_token(store: SwarmStore, swarm: str) -> None:
    store.join(swarm, "DEV", role="dev", agent="opencode", address="t2:1")
    members = store.members(swarm)
    assert [m.name for m in members] == ["plan", "dev", "qa"]
    assert members[1].address == "t2:1"
    store.set_token(swarm, "dev", "abc123")
    found = store.by_token("abc123")
    assert found is not None and found[0] == swarm and found[1].name == "dev"
    assert store.by_token("") is None and store.by_token("otro") is None


def test_cada_rol_solo_toma_lo_suyo_y_en_orden(store: SwarmStore, swarm: str) -> None:
    plan, build, check = _pipeline(store, swarm)
    assert store.claim(swarm, "dev") == [] and store.claim(swarm, "qa") == []
    assert [t.id for t in store.claim(swarm, "plan")] == [plan]
    assert store.members(swarm)[0].state == "working"
    store.finish(swarm, "plan", plan, "PLAN.md listo", ref="PLAN.md")
    assert store.members(swarm)[0].state == "idle"
    assert store.claim(swarm, "qa") == [] and [t.id for t in store.ready(swarm, "dev")] == [build]
    assert [t.id for t in store.claim(swarm, "dev")] == [build]
    assert [t.id for t in store.mine(swarm, "dev")] == [build]
    store.finish(swarm, "dev", build, "hecho")
    assert [t.id for t in store.claim(swarm, "qa")] == [check]
    with pytest.raises(SwarmError):
        store.add_task(swarm, "x", deps=[999])


def test_tarea_a_nombre_y_sin_rol(store: SwarmStore, swarm: str) -> None:
    named = store.add_task(swarm, "Solo qa", owner="qa")
    free = store.add_task(swarm, "Cualquiera")
    assert [t.id for t in store.claim(swarm, "dev")] == [free.id]
    assert [t.id for t in store.claim(swarm, "qa", named.id)] == [named.id]


def test_finish_solo_el_dueno_avisa_y_es_idempotente(store: SwarmStore, swarm: str) -> None:
    task = store.add_task(swarm, "Plan", role="plan")
    store.claim(swarm, "plan")
    with pytest.raises(SwarmError):
        store.finish(swarm, "dev", task.id, "no es mía")
    done = store.finish(swarm, "plan", task.id, "x" * 1000, ref="PLAN.md")
    assert done.state == "done" and len(done.result) <= 280
    assert store.finish(swarm, "plan", task.id, "otra vez").result == done.result
    inbox = store.inbox(swarm, "dev")
    assert [m.kind for m in inbox] == ["done"] and inbox[0].ref == "PLAN.md"
    assert inbox[0].sender == "plan" and inbox[0].body.startswith(f"T{task.id} hecha")


def test_partes_cierran_la_tarea_madre(store: SwarmStore, swarm: str) -> None:
    plan, build, check = _pipeline(store, swarm)
    store.claim(swarm, "plan")
    first = store.add_task(swarm, "API login", parent=build)
    second = store.add_task(swarm, "Pruebas", parent=build)
    assert first.role == "dev" and first.parent == build
    with pytest.raises(SwarmError):
        store.add_task(swarm, "nieta", parent=first.id)
    assert store.ready(swarm, "dev") == []  # las partes esperan al plan
    store.finish(swarm, "plan", plan, "ok")
    store.join(swarm, "dev2", role="dev")
    taken = store.claim(swarm, "dev", every=True)
    assert [t.id for t in taken] == [first.id, second.id]  # la madre no se toma
    assert store.claim(swarm, "dev2") == []
    store.finish(swarm, "dev", first.id, "ok")
    assert store.claim(swarm, "qa") == []
    store.finish(swarm, "dev", second.id, "ok")
    states = {t.id: t for t in store.tasks(swarm)}
    assert states[build].state == "done" and states[build].result == "2 partes hechas"
    assert [t.id for t in store.claim(swarm, "qa")] == [check]


def test_reopen_devuelve_el_trabajo_y_libera_al_validador(store: SwarmStore, swarm: str) -> None:
    plan, build, check = _pipeline(store, swarm)
    for name, task in (("plan", plan), ("dev", build)):
        store.claim(swarm, name)
        store.finish(swarm, name, task, "ok")
    store.claim(swarm, "qa")
    store.inbox(swarm, "dev")
    again, exhausted = store.reopen(swarm, build, by="qa", note="Falta validar el token")
    assert not exhausted and again.state == "pending" and again.owner == "dev"
    assert again.rounds == 1
    states = {t.id: t for t in store.tasks(swarm)}
    assert states[check].state == "pending" and states[check].owner == "qa"
    assert store.members(swarm)[2].state == "idle"
    fix = store.inbox(swarm, "dev")
    assert [m.kind for m in fix] == ["fix"] and "Falta validar" in fix[0].body
    assert store.ready(swarm, "qa") == []
    store.claim(swarm, "dev")
    store.finish(swarm, "dev", build, "corregido")
    assert [t.id for t in store.claim(swarm, "qa")] == [check]
    with pytest.raises(SwarmError):
        store.reopen(swarm, check, by="qa", note="aún en curso")


def test_reopen_con_partes_crea_una_correccion(store: SwarmStore, swarm: str) -> None:
    plan, build, check = _pipeline(store, swarm)
    part = store.add_task(swarm, "API", parent=build)
    store.claim(swarm, "plan")
    store.finish(swarm, "plan", plan, "ok")
    store.claim(swarm, "dev")
    store.finish(swarm, "dev", part.id, "ok")
    store.claim(swarm, "qa")
    store.reopen(swarm, build, by="qa", note="Sin pruebas")
    tasks = {t.id: t for t in store.tasks(swarm)}
    extra = [t for t in tasks.values() if t.parent == build and t.id != part.id]
    assert tasks[build].state == "pending" and tasks[part.id].state == "done"
    assert len(extra) == 1 and extra[0].title == "Corregir: Sin pruebas" and extra[0].role == "dev"
    assert [t.id for t in store.claim(swarm, "dev")] == [extra[0].id]


def test_reopen_tiene_tope_de_rondas(store: SwarmStore, swarm: str) -> None:
    task = store.add_task(swarm, "Hacer", role="dev")
    for _ in range(MAX_ROUNDS):
        store.claim(swarm, "dev")
        store.finish(swarm, "dev", task.id, "ok")
        assert store.reopen(swarm, task.id, by="qa", note="mal")[1] is False
    store.claim(swarm, "dev")
    store.finish(swarm, "dev", task.id, "ok")
    final, exhausted = store.reopen(swarm, task.id, by="qa", note="sigue mal")
    assert exhausted and final.state == "failed"
    assert store.inbox(swarm, "plan")[-1].kind == "block"


def test_mensajes_breves_y_cursor(store: SwarmStore, swarm: str) -> None:
    message, cut = store.post(swarm, "plan", "¿SQLite o JSON?", to="dev", kind="ask")
    assert not cut and message.line() == f"#{message.id} plan→dev ask: ¿SQLite o JSON?"
    long, cut = store.post(swarm, "qa", "y" * 900, ref="docs/audit.md")
    assert cut and len(long.body) == MAX_BODY and long.body.endswith("…")
    assert store.unread(swarm, "dev") == 2
    first = store.inbox(swarm, "dev", limit=1)
    assert [m.id for m in first] == [message.id]
    rest = store.inbox(swarm, "dev")
    assert [m.id for m in rest] == [long.id] and rest[0].recipient == BROADCAST
    assert store.inbox(swarm, "dev") == [] and store.unread(swarm, "dev") == 0
    assert [m.id for m in store.inbox(swarm, "plan", mark=False)] == [long.id]


def test_mensajes_invalidos(store: SwarmStore, swarm: str) -> None:
    with pytest.raises(SwarmError):
        store.post(swarm, "dev", "   ")
    with pytest.raises(SwarmError):
        store.post(swarm, "dev", "hola", kind="shell")
    with pytest.raises(SwarmError):
        store.post(swarm, "intruso", "hola")
    with pytest.raises(SwarmError):
        store.post(swarm, "dev", "hola", to="nadie")
    message, _ = store.post(swarm, SYSTEM, "línea\n\x00 rara")
    assert message.body == "línea rara"


def test_reservas_todo_o_nada(store: SwarmStore, swarm: str, clock: Clock) -> None:
    assert store.reserve(swarm, "dev", ["src/API/", "./tests/test_api.py"]) == []
    conflicts = store.reserve(swarm, "qa", ["src/api/login.py", "README.md"])
    assert [(c.path, c.member) for c in conflicts] == [("src/api", "dev")]
    assert [lease.path for lease in store.leases(swarm)] == ["src/api", "tests/test_api.py"]
    assert store.reserve(swarm, "dev", ["src/api"]) == []  # renovar la propia
    clock.now += 10_000
    assert store.reserve(swarm, "qa", ["src/api/login.py"]) == []  # caducó
    assert store.release(swarm, "qa") == 1
    for bad in ("", "../fuera", "a/../../b"):
        with pytest.raises(SwarmError):
            store.reserve(swarm, "dev", [bad])


def test_board_compacto(store: SwarmStore, swarm: str) -> None:
    plan, build, check = _pipeline(store, swarm)
    part = store.add_task(swarm, "API login", parent=build)
    store.claim(swarm, "plan")
    store.reserve(swarm, "plan", ["PLAN.md"])
    store.post(swarm, "plan", "Uso SQLite", to="dev")
    text = board(store, swarm, member="dev")
    lines = text.splitlines()
    assert lines[0] == f"Enjambre {swarm} · activo · Construir la API de login"
    assert "plan(claude) trabaja" in lines[1] and "dev(opencode) libre" in lines[1]
    assert lines[2] == "Tareas 0/3:"
    assert f"T{plan} ▶ Planificar @plan" in lines
    assert f"T{build} · Desarrollar [dev] 0/1 tras T{plan}" in lines
    assert f"  T{part.id} · API login [dev]" in lines
    assert f"T{check} · Validar [qa] tras T{build}" in lines
    assert "Reservas: plan.md @plan" in lines
    assert lines[-1] == "Sin leer para dev: 1" and len(text) < 500
    with pytest.raises(SwarmError):
        board(store, "nope")


def test_salud(store: SwarmStore, swarm: str, clock: Clock) -> None:
    assert health(store, swarm, now=clock.now).state == "empty"
    plan, build, check = _pipeline(store, swarm)
    store.claim(swarm, "plan")
    ok = health(store, swarm, now=clock.now)
    assert ok.state == "ok" and (ok.done, ok.total) == (0, 3)
    assert ok.spoken() == f"Van 0 de 3 tareas. Trabajando: plan en T{plan}. Todo en orden."
    clock.now += 1200  # plan no da señales
    stalled = health(store, swarm, now=clock.now)
    assert stalled.state == "attention" and "plan lleva 20 min sin señales" in stalled.issues[0]
    store.finish(swarm, "plan", plan, "ok")
    assert health(store, swarm, now=clock.now).state == "ok"
    clock.now += 300  # dev no toma su tarea lista
    lazy = health(store, swarm, now=clock.now)
    assert lazy.issues == (f"T{build} lleva 5 min lista y dev no la toma.",)
    store.claim(swarm, "dev")
    store.post(swarm, "dev", "¿Qué base uso?", to="plan", kind="ask")
    clock.now += 700
    store.touch(swarm, "dev")
    assert "plan no respondió a dev en 12 min." in health(store, swarm, now=clock.now).issues
    store.post(swarm, "plan", "SQLite", to="dev", kind="answer")
    store.finish(swarm, "dev", build, "no compila", ok=False)
    bad = health(store, swarm, now=clock.now)
    assert f"T{build} falló: no compila" in bad.issues
    assert f"T{check} no puede empezar: depende de T{build}, que falló." in bad.issues
    assert "dev está bloqueado." in bad.issues and bad.spoken().startswith("Van 1 de 3")


def test_salud_terminado_y_rol_sin_miembro(store: SwarmStore, swarm: str, clock: Clock) -> None:
    lonely = store.add_task(swarm, "Diseñar", role="design")
    assert health(store, swarm, now=clock.now).issues == (
        f"T{lonely.id} es para el rol «design» y nadie lo tiene.",
    )
    store.finish(swarm, SYSTEM, lonely.id, "cancelada")
    final = health(store, swarm, now=clock.now)
    assert final.state == "done" and final.spoken() == "El enjambre terminó: 1 de 1 tareas hechas."


def test_estados(store: SwarmStore, swarm: str) -> None:
    store.set_member_state(swarm, "dev", "blocked")
    assert store.members(swarm)[1].state == "blocked"
    store.set_status(swarm, "done")
    assert store.active() == []
    with pytest.raises(SwarmError):
        store.set_status(swarm, "raro")
    with pytest.raises(SwarmError):
        store.set_member_state(swarm, "dev", "raro")


def test_claim_concurrente_entrega_cada_tarea_una_vez(store: SwarmStore, swarm: str) -> None:
    for index in range(6):
        store.add_task(swarm, f"Parte {index}")
    for name in ("w1", "w2", "w3"):
        store.join(swarm, name)
    taken: list[int] = []
    lock = threading.Lock()

    def worker(name: str) -> None:
        while found := store.claim(swarm, name):
            with lock:
                taken.append(found[0].id)

    threads = [threading.Thread(target=worker, args=(n,)) for n in ("w1", "w2", "w3")]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert sorted(taken) == [task.id for task in store.tasks(swarm)]


def test_partes_cierran_en_silencio_y_sueltan_reservas(store: SwarmStore, swarm: str) -> None:
    build = store.add_task(swarm, "Desarrollar", role="dev")
    first = store.add_task(swarm, "API", parent=build.id)
    second = store.add_task(swarm, "Pruebas", parent=build.id)
    store.claim(swarm, "dev", every=True)
    store.reserve(swarm, "dev", ["src/api"])
    store.finish(swarm, "dev", first.id, "ok")
    assert store.inbox(swarm, "qa") == [] and len(store.leases(swarm)) == 1
    store.finish(swarm, "dev", second.id, "ok")
    notice = store.inbox(swarm, "qa")
    assert [m.body for m in notice] == [f"T{build.id} hecha: 2 partes hechas"]
    assert store.inbox(swarm, "dev") == []  # quien cierra no recibe su propio aviso
    assert store.leases(swarm) == []
