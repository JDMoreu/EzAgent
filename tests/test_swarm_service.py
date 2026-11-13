"""SwarmService, servidor local y cliente ``ezs``: el flujo completo del enjambre."""

from __future__ import annotations

import http.client
import json
import threading
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

from ezagent.swarm import cli
from ezagent.swarm.server import SwarmServer
from ezagent.swarm.service import Seat, SwarmService
from ezagent.swarm.store import SwarmStore

MEMBERS = [("plan", "plan", "claude"), ("dev", "dev", "opencode"), ("qa", "qa", "codex")]


class Team:
    """Enjambre de prueba: ejecuta comandos como cada miembro."""

    def __init__(self, service: SwarmService) -> None:
        self.service = service
        self.swarm, seats = service.start("API de login", MEMBERS, cwd="C:/proj")
        self.seats: dict[str, Seat] = {seat.name: seat for seat in seats}
        for name in self.seats:  # como si ya hubieran leído sus instrucciones
            service.store.mark_briefed(self.swarm, name)

    def run(self, member: str, *argv: str, ok: bool = True) -> str:
        reply = self.service.handle(self.seats[member].token, list(argv))
        assert reply.ok is ok, reply.text
        return reply.text


@pytest.fixture
def service(tmp_path: Path) -> SwarmService:
    return SwarmService(SwarmStore(tmp_path / "board.db"))


@pytest.fixture
def team(service: SwarmService) -> Team:
    return Team(service)


def test_start_reparte_roles_tareas_e_instrucciones(team: Team) -> None:
    tasks = team.service.store.tasks(team.swarm)
    assert [(t.role, len(t.deps)) for t in tasks] == [("plan", 0), ("dev", 1), ("qa", 1)]
    brief = team.seats["dev"].briefing
    assert brief.startswith(f"Eres «dev» (rol: desarrollar) en el enjambre {team.swarm}")
    assert "plan (planificar) · dev (tú) · qa (validar)" in brief
    assert "ezs lock" in brief and "termina tu turno" in brief and len(brief) < 1300
    assert "ezs fix" in team.seats["qa"].briefing and "ezs add" in team.seats["plan"].briefing
    assert team.run("dev", "me") == brief
    assert len({seat.token for seat in team.seats.values()}) == 3


def test_el_primer_next_trae_las_instrucciones_una_sola_vez(service: SwarmService) -> None:
    swarm, seats = service.start("API de login", MEMBERS)
    token = seats[0].token
    first = service.handle(token, ["next"]).text
    assert first.startswith(seats[0].briefing) and first.splitlines()[-1].startswith("Tu tarea: T")
    assert service.handle(token, ["next"]).text.startswith("En curso: T")
    fresh = service.issue_token(swarm, seats[0].name)  # terminal nueva: vuelve a hacer falta
    assert service.handle(fresh, ["next"]).text.startswith("Eres «plan»")
    assert service.join_line(swarm, "dev") == (
        f"[EzAgent] Eres «dev» (desarrollar) en el enjambre {swarm}. Ejecuta ahora: ezs next"
    )


def test_token_identifica_y_no_se_guarda(team: Team, service: SwarmService) -> None:
    token = team.seats["dev"].token
    found = service.authenticate(token)
    assert found is not None and found[1].name == "dev"
    assert token.encode() not in service.store.path.read_bytes()
    assert service.handle("falso", ["next"]).ok is False
    assert service.handle("", ["next"]).ok is False
    fresh = service.issue_token(team.swarm, "dev")
    assert service.authenticate(token) is None and service.authenticate(fresh) is not None


def test_flujo_completo_planear_desarrollar_validar(team: Team) -> None:
    events: list[tuple[str, str]] = []
    team.service.subscribe(lambda swarm, member, op: events.append((member, op)))
    assert "Sin tareas listas" in team.run("dev", "next")
    first = team.run("plan", "next")
    assert first.startswith("Tu tarea: T")
    assert "después sigue desarrollar con T2 (no la hagas tú)" in first
    build = team.service.store.tasks(team.swarm)[1].id
    assert team.run("plan", "add", "API", "login", "--in", f"T{build}").startswith("Creada: T")
    team.run("plan", "add", "Pruebas", f"--in={build}")
    done = team.run("plan", "done", "Plan en PLAN.md", "--ref", "PLAN.md")
    assert done.splitlines()[0].endswith("hecha.") and "Sin tareas listas" in done
    assert team.service.wake_line(team.swarm, "dev") == (
        "[EzAgent] Tienes 2 tareas listas en el enjambre. Ejecuta: ezs next"
    )
    assert team.service.wake_line(team.swarm, "qa").count("tarea") == 0
    work = team.run("dev", "next", "--all")
    assert "Mensajes (1):" in work and "[PLAN.md]" in work and work.count("Tu tarea:") == 2
    several = team.run("dev", "done", "listo", ok=False)
    assert "varias en curso" in several
    parts = [t.id for t in team.service.store.mine(team.swarm, "dev")]
    team.run("dev", "done", f"T{parts[0]}", "API lista")
    last = team.run("dev", "done", "12 pruebas pasan")  # «12» no es un número de tarea suya
    assert "Sin tareas listas" in last
    check = team.run("qa", "next")
    assert "Tu tarea: T" in check and "Validar" in check
    fix = team.run("qa", "fix", f"T{build}", "Falta", "limitar", "intentos", "--ref", "audit.md")
    assert "reabierta (ronda 1/3)" in fix and "volverá a ti" in fix
    again = team.run("dev", "next")
    assert (
        "fix:" in again and "Tu tarea: T" in again and "Corregir: Falta limitar intentos" in again
    )
    team.run("dev", "done", "Límite de intentos añadido")
    team.run("qa", "next")
    final = team.run("qa", "done", "Todo validado")
    assert "Todo el trabajo del enjambre está hecho" in final
    info = team.service.store.get(team.swarm)
    assert info is not None and info.status == "done" and ("qa", "finished") in events
    assert team.service.status(team.swarm).spoken().startswith("El enjambre terminó")


def test_mensajes_preguntas_y_tope_de_rafaga(team: Team) -> None:
    assert team.run("plan", "ask", "dev", "¿SQLite", "o", "JSON?").endswith("enviado a dev.")
    inbox = team.run("dev", "inbox")
    assert inbox.endswith("plan→dev ask: ¿SQLite o JSON?")
    assert team.run("dev", "inbox") == "Sin mensajes nuevos."
    long = team.run("dev", "answer", "plan", "x" * 500)
    assert "Se recortó a 280 caracteres" in long
    assert "enviado a todos" in team.run("qa", "say", "all", "Empiezo en 5 min")
    assert team.run("dev", "say", "nadie", "hola", ok=False)
    assert "Uso: ezs ask" in team.run("dev", "ask", "plan", ok=False)
    for _ in range(9):
        team.run("dev", "say", "plan", "eco")
    assert "Demasiados mensajes" in team.run("dev", "say", "plan", "eco", ok=False)


def test_errores_y_comandos_varios(team: Team) -> None:
    assert "Comando desconocido" in team.run("dev", "rm", "-rf", ok=False)
    assert "Opción desconocida: --force" in team.run("dev", "next", "--force", ok=False)
    assert "No tienes tareas en curso" in team.run("dev", "done", "nada", ok=False)
    team.run("plan", "next")
    assert "Falta el resumen" in team.run("plan", "done", ok=False)
    assert "marcada como fallida" in team.run("plan", "fail", "No hay repositorio")
    assert team.service.status(team.swarm).state == "attention"
    assert team.run("dev", "lock", "src/api", "--min", "5") == "Reservado: src/api."
    assert "Ocupado: src/api lo tiene dev" in team.run("qa", "lock", "src/api/x.py", ok=False)
    assert team.run("dev", "unlock") == "Reservas liberadas: 1."
    assert team.run("dev", "board").startswith(f"Enjambre {team.swarm} · activo")
    assert team.run("dev", "help").count("\n") == 9
    team.service.stop(team.swarm)
    assert team.run("dev", "next") == "El usuario detuvo este enjambre. Termina tu turno."


def test_wait_despierta_con_un_mensaje(team: Team) -> None:
    assert team.run("dev", "wait", "1") == "Sin novedades en 1 s."
    result: list[str] = []
    waiter = threading.Thread(target=lambda: result.append(team.run("dev", "wait", "20")))
    started = time.monotonic()
    waiter.start()
    time.sleep(0.2)
    team.run("plan", "say", "dev", "Ya está el plan")
    waiter.join(timeout=10)
    assert result and "Ya está el plan" in result[0] and time.monotonic() - started < 8


# --------------------------------------------------------- servidor y cliente
@pytest.fixture
def server(service: SwarmService, tmp_path: Path) -> Iterator[SwarmServer]:
    instance = SwarmServer(service, tmp_path / "swarm" / "endpoint.json")
    instance.start()
    yield instance
    instance.stop()


def _raw(server: SwarmServer, *, token: str = "", body: bytes = b"{}", host: str = "") -> int:
    port = int(server.url.rsplit(":", 1)[1].split("/")[0])
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if host:
        headers["Host"] = host
    try:
        connection.request("POST", "/v1", body=body, headers=headers)
        return connection.getresponse().status
    finally:
        connection.close()


def test_servidor_solo_local_y_con_token(server: SwarmServer, team: Team) -> None:
    assert server.url.startswith("http://127.0.0.1:")
    published = json.loads(server.endpoint_path.read_text(encoding="utf-8"))
    assert published["url"] == server.url
    token = team.seats["dev"].token
    assert _raw(server, body=b'{"argv": ["board"]}') == 401
    assert _raw(server, token=token, body=b"no es json") == 400
    assert _raw(server, token=token, body=b'{"argv": "next"}') == 400
    assert _raw(server, token=token, body=b'{"argv": ["' + b"x" * 20_000 + b'"]}') == 400
    assert _raw(server, token=token, body=b'{"argv": ["board"]}', host="evil.example") == 404
    assert _raw(server, token=token, body=b'{"argv": ["board"]}') == 200


def test_cliente_ezs_de_punta_a_punta(
    server: SwarmServer,
    team: Team,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.setenv("EZS_ENDPOINT", str(server.endpoint_path))
    monkeypatch.delenv("EZS_TOKEN", raising=False)
    assert cli.main(["next"]) == 2 and "falta EZS_SEAT" in capsys.readouterr().err
    monkeypatch.setenv("EZS_TOKEN", team.seats["plan"].token)
    assert cli.main(["next"]) == 0
    assert capsys.readouterr().out.startswith("Tu tarea: T")
    assert cli.main(["done"]) == 1 and "Falta el resumen" in capsys.readouterr().err
    assert cli.main(["done", "Plan listo", "--ref", "PLAN.md"]) == 0
    assert "hecha." in capsys.readouterr().out
    monkeypatch.setenv("EZS_TOKEN", "otro")
    assert cli.main(["board"]) == 1 and "no tiene un asiento válido" in capsys.readouterr().err


def test_cliente_sin_servidor(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    monkeypatch.setenv("EZS_ENDPOINT", str(tmp_path / "nada.json"))
    monkeypatch.setenv("EZS_TOKEN", "x")
    monkeypatch.setattr(cli.time, "sleep", lambda _secs: None)
    assert cli.main(["next"]) == 2
    assert "EzAgent no responde" in capsys.readouterr().err


def test_servidor_se_detiene_y_vuelve_con_otro_puerto(
    service: SwarmService, team: Team, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    endpoint = tmp_path / "endpoint.json"
    first = SwarmServer(service, endpoint)
    first.start()
    first.stop()
    assert json.loads(endpoint.read_text(encoding="utf-8"))["url"] == ""
    second = SwarmServer(service, endpoint)
    try:
        second.start()
        monkeypatch.setenv("EZS_ENDPOINT", str(endpoint))
        monkeypatch.setenv("EZS_TOKEN", team.seats["qa"].token)
        assert cli.main(["inbox"]) == 0  # el mismo token sigue sirviendo tras el reinicio
    finally:
        second.stop()


def test_enjambre_a_medio_crear_es_invisible_para_el_supervisor(service: SwarmService) -> None:
    store = service.store
    half = store.create("A medias", status="setup")
    assert store.active() == [] and service.latest() == ""
    swarm, _ = service.start("Completo", MEMBERS)
    assert [info.id for info in store.active()] == [swarm]
    with pytest.raises(ValueError):
        service.start("Con un miembro inválido", [("Nombre Malo", "dev", "codex")])
    assert [info.id for info in store.active()] == [swarm]  # el fallido no queda activo
    info = store.get(half)
    assert info is not None and info.status == "setup"
