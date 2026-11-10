"""Confirmaciones identificadas, métricas con metadatos, auditoría y redacción."""

from __future__ import annotations

import json
import threading
import time
from pathlib import Path

import pytest

from ezagent.agent.audit import AuditLog
from ezagent.agent.confirm import ConfirmationManager
from ezagent.voice.metrics import LatencyStats, TurnTimings
from ezagent.voice.redact import redact

ACCEPT = ["sí", "si", "confirmo", "adelante", "dale"]
REJECT = ["no", "cancela", "para"]


def make(timeout: float = 5.0, on_ask=None) -> ConfirmationManager:
    return ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=timeout, on_ask=on_ask)


class Asker:
    """Lanza ``ask`` en otro hilo, como hace el hilo de herramientas."""

    def __init__(self, manager: ConfirmationManager, **kwargs) -> None:
        self.manager = manager
        self.result: bool | None = None
        self.thread = threading.Thread(
            target=lambda: setattr(self, "result", manager.ask("¿Borro?", **kwargs)),
            daemon=True,
        )
        self.thread.start()
        deadline = time.monotonic() + 2
        while not manager.pending and time.monotonic() < deadline:
            time.sleep(0.001)
        assert manager.pending, "la confirmación nunca quedó pendiente"

    def join(self, timeout: float = 5.0) -> bool:
        self.thread.join(timeout)
        assert not self.thread.is_alive(), "ask() no devolvió"
        assert self.result is not None
        return self.result


def test_una_frase_ajena_o_vacia_no_resuelve() -> None:
    manager = make()
    asker = Asker(manager)
    assert manager.submit_text("quizás") is None
    assert manager.submit_text("") is None
    assert manager.pending
    assert manager.submit_text("sí") is True
    assert asker.join() is True


@pytest.mark.parametrize(("text", "expected"), [("sí", True), ("dale", True), ("no", False)])
def test_aceptacion_o_rechazo_claro_resuelve(text: str, expected: bool) -> None:
    manager = make()
    asker = Asker(manager)
    assert manager.submit_text(text) is expected
    assert asker.join() is expected
    assert manager.last_outcome == ("yes" if expected else "no")


def test_el_callback_ve_el_token_que_luego_ata_la_respuesta() -> None:
    seen: list[str | None] = []
    manager = make(on_ask=lambda summary, strong: seen.append(manager.pending_token))
    asker = Asker(manager)
    assert seen and seen[0] == manager.pending_token
    assert manager.submit_text("no", token=seen[0]) is False
    assert asker.join() is False


def test_un_token_incorrecto_no_resuelve() -> None:
    manager = make()
    asker = Asker(manager)
    token = manager.pending_token
    assert token and len(token) == 32
    assert manager.submit_text("sí", token="0" * 32) is None
    assert manager.pending
    assert manager.submit_text("sí", token=token) is True
    assert asker.join() is True


def test_el_timeout_se_audita_como_timeout() -> None:
    manager = make(timeout=0.05)
    assert manager.ask("¿Borro?") is False
    assert manager.last_outcome == "timeout"
    assert manager.pending_token is None


def test_arm_retrasa_el_plazo_hasta_que_se_lee_la_pregunta() -> None:
    manager = make(timeout=0.05, on_ask=lambda summary, strong: None)
    asker = Asker(manager)
    time.sleep(0.15)  # el plazo habría vencido sin arm
    assert manager.pending and asker.thread.is_alive()
    manager.arm()
    assert asker.join() is False
    assert manager.last_outcome == "timeout"


def test_un_arm_con_otro_token_no_arma_la_pregunta_vigente() -> None:
    manager = make(timeout=0.05, on_ask=lambda summary, strong: None)
    asker = Asker(manager)
    manager.arm("0" * 32)
    time.sleep(0.1)
    assert manager.pending and asker.thread.is_alive()
    manager.arm(manager.pending_token)
    assert asker.join() is False


def test_clic_y_cancelacion_actualizan_el_resultado() -> None:
    clicked = make()
    asker = Asker(clicked)
    clicked.submit_click(True)
    assert asker.join() is True and clicked.last_outcome == "yes"

    cancelled = make()
    asker = Asker(cancelled)
    cancelled.cancel("el usuario cambió de tema")
    assert asker.join() is False and cancelled.last_outcome == "cancelled"


@pytest.mark.parametrize(
    "text",
    [
        "usa sk-proj_ABCdef123 para entrar",
        "Authorization: Bearer abc.DEF-123",
        "api_key=abc123",
        "x-api-key: abc123",
        "https://usuario:clave-real@example.com/x",
        "hash 0123456789abcdef0123456789abcdef",
        "token QWxhZGRpbjpvcGVuIHNlc2FtZQ1234567890",
    ],
)
def test_redact_oculta_secretos(text: str) -> None:
    cleaned = redact(text)
    assert "[REDACTED]" in cleaned
    for secret in ("sk-proj_ABCdef123", "abc.DEF-123", "abc123", "clave-real", "QWxhZGRp"):
        assert secret not in cleaned


def test_redact_oculta_valores_entre_comillas() -> None:
    assert redact('api_key="dos palabras"') == 'api_key="[REDACTED]"'


def test_redact_no_toca_texto_normal() -> None:
    assert redact("Hola, revisa el informe de mañana.") == "Hola, revisa el informe de mañana."


def test_metrics_record_guarda_metadatos(tmp_path: Path) -> None:
    timings = TurnTimings(speech_end=0.0, vad_end=0.1)
    timings.mark("first_token", 0.4)
    timings.mark("first_sound", 0.9)
    path = tmp_path / "latency.jsonl"
    stats = LatencyStats(maxlen=5, jsonl_path=path)
    stats.record(
        timings,
        {
            "provider": "openai",
            "model": "modelo-prueba",
            "rounds": 2,
            "outcome": "ok",
            "cancelled": False,
            "first_ack_ms": 123.4,
            "transcript": "no debe guardarse",
            "api_key": "no debe guardarse",
        },
    )
    record = json.loads(path.read_text("utf-8").splitlines()[-1])
    assert record["provider"] == "openai"
    assert record["model"] == "modelo-prueba"
    assert record["rounds"] == 2 and record["outcome"] == "ok"
    assert record["cancelled"] is False and record["first_ack_ms"] == 123.4
    assert "transcript" not in record and "api_key" not in record
    assert "ttfa_ms" in record
    # La memoria conserva tiempos y metadatos para el resumen periódico.
    report, meta = stats._turns[-1]
    assert meta["model"] == "modelo-prueba" and "ttfa_ms" in report


def test_metrics_record_sin_meta_mantiene_el_registro(tmp_path: Path) -> None:
    path = tmp_path / "latency.jsonl"
    stats = LatencyStats(jsonl_path=path)
    report = stats.record(TurnTimings(speech_end=0.0, vad_end=0.1))
    record = json.loads(path.read_text("utf-8").splitlines()[-1])
    assert record["outcome"] == "unknown"
    assert report["vad_wait_ms"] == pytest.approx(100.0)


def test_audit_guarda_error_kind_solo_cuando_existe(tmp_path: Path) -> None:
    log = AuditLog(tmp_path / "audit.jsonl")
    fields = dict(
        turn_id="t1",
        tool="web_search",
        level=0,
        args={},
        decision="allow",
        taint=False,
        result_ok=False,
        ms=3.0,
    )
    log.log(**fields, error_kind="timeout")
    log.log(**fields)
    first, second = [json.loads(line) for line in log.path.read_text("utf-8").splitlines()]
    assert first["error_kind"] == "timeout"
    assert "error_kind" not in second


def test_audit_reutiliza_la_redaccion_comun(tmp_path: Path) -> None:
    log = AuditLog(tmp_path / "audit.jsonl")
    out = log.redact({"nota": "api_key=abc123 y https://u:p@example.com"})
    assert "abc123" not in out["nota"] and "u:p@" not in out["nota"]
