"""AuditLog: JSONL append-only, sin contenidos ni secretos."""

from __future__ import annotations

import json
import threading
from pathlib import Path

import pytest

from ezagent.agent.audit import AuditLog


def entry(log: AuditLog, **overrides) -> None:
    fields = dict(
        turn_id="t1",
        tool="write_file",
        level=1,
        args={},
        decision="allow",
        taint=False,
        result_ok=True,
        ms=1.234,
    )
    fields.update(overrides)
    log.log(**fields)


def lines(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text("utf-8").splitlines()]


@pytest.fixture
def log(tmp_path: Path) -> AuditLog:
    return AuditLog(tmp_path / "logs" / "audit.jsonl")


def test_cada_llamada_es_una_linea_json_con_los_campos_del_contrato(log: AuditLog) -> None:
    entry(log, args={"path": "a.txt"})
    entry(log, tool="delete_path", level=2, decision="confirm_yes", taint=True, result_ok=False)
    first, second = lines(log.path)
    assert set(first) == {
        "ts",
        "turn_id",
        "tool",
        "level",
        "args",
        "decision",
        "taint",
        "result_ok",
        "ms",
    }
    assert first["args"] == {"path": "a.txt"} and first["ms"] == 1.2
    assert second["tool"] == "delete_path" and second["taint"] is True
    assert second["result_ok"] is False and second["decision"] == "confirm_yes"
    assert first["ts"].endswith("+00:00")


def test_se_crean_las_carpetas_intermedias(tmp_path: Path) -> None:
    target = tmp_path / "a" / "b" / "c.jsonl"
    entry(AuditLog(target))
    assert target.exists()


@pytest.mark.parametrize("key", ["content", "text", "data", "output", "CONTENT", "Output"])
def test_los_contenidos_se_sustituyen_por_su_longitud(log: AuditLog, key: str) -> None:
    redacted = log.redact({key: "hola mundo", "path": "x"})
    assert key.lower() + "_chars" in redacted
    assert redacted[key.lower() + "_chars"] == 10
    assert "hola" not in json.dumps(redacted)
    assert redacted["path"] == "x"


def test_tamano_de_contenedores_y_none(log: AuditLog) -> None:
    assert log.redact({"content": None}) == {"content_chars": 0}
    assert log.redact({"data": b"abc"}) == {"data_chars": 3}
    assert log.redact({"data": [1, 2]})["data_chars"] == len("[1, 2]")


@pytest.mark.parametrize(
    "key",
    [
        "api_key",
        "API_KEY",
        "apikey",
        "token",
        "access-token",
        "password",
        "db_passwd",
        "client_secret",
    ],
)
def test_las_claves_secretas_se_ocultan(log: AuditLog, key: str) -> None:
    assert log.redact({key: "valor-real"}) == {key: "[REDACTED]"}


def test_tokens_dentro_de_textos_libres_se_ocultan(log: AuditLog) -> None:
    out = log.redact(
        {
            "cmd": "curl -H 'Authorization: Bearer abc.DEF-123' https://x",
            "nota": "usa sk-proj_ABCdef123 para entrar",
            "lista": ["sk-live-999", "normal"],
        }
    )
    assert "abc.DEF" not in out["cmd"] and "Bearer [REDACTED]" in out["cmd"]
    assert "sk-proj" not in out["nota"] and "[REDACTED]" in out["nota"]
    assert out["lista"] == ["[REDACTED]", "normal"]


def test_la_redaccion_llega_a_estructuras_anidadas(log: AuditLog) -> None:
    out = log.redact({"a": {"b": [{"password": "x", "content": "yyyy"}]}})
    assert out == {"a": {"b": [{"password": "[REDACTED]", "content_chars": 4}]}}


def test_redact_no_modifica_el_original(log: AuditLog) -> None:
    original = {"api_key": "abc", "nested": {"content": "zz"}}
    log.redact(original)
    assert original == {"api_key": "abc", "nested": {"content": "zz"}}


def test_un_secreto_nunca_llega_al_archivo(log: AuditLog) -> None:
    entry(
        log,
        args={
            "api_key": "SUPERSECRETO",
            "note": "Bearer SUPERSECRETO2",
            "content": "SUPERSECRETO3",
        },
    )
    text = log.path.read_text("utf-8")
    assert "SUPERSECRETO" not in text


def test_los_valores_no_texto_pasan_intactos(log: AuditLog) -> None:
    assert log.redact({"n": 3, "ok": True, "x": None, "f": 1.5}) == {
        "n": 3,
        "ok": True,
        "x": None,
        "f": 1.5,
    }


def test_rotacion_por_tamano_conserva_los_backups_indicados(tmp_path: Path) -> None:
    log = AuditLog(tmp_path / "audit.jsonl", max_bytes=600, backups=2)
    for i in range(40):
        entry(log, turn_id=f"turno-{i:03d}")
    names = sorted(p.name for p in tmp_path.iterdir())
    assert names == ["audit.jsonl", "audit.jsonl.1", "audit.jsonl.2"]
    for p in tmp_path.iterdir():
        assert p.stat().st_size <= 600 + 400  # un registro de margen
    # Lo más reciente está en el archivo vivo; lo más viejo se descartó.
    assert lines(tmp_path / "audit.jsonl")[-1]["turn_id"] == "turno-039"
    every = [r["turn_id"] for p in tmp_path.iterdir() for r in lines(p)]
    assert "turno-000" not in every and "turno-039" in every


def test_sin_backups_no_rota(tmp_path: Path) -> None:
    log = AuditLog(tmp_path / "audit.jsonl", max_bytes=10, backups=0)
    for _ in range(5):
        entry(log)
    assert len(lines(tmp_path / "audit.jsonl")) == 5
    assert [p.name for p in tmp_path.iterdir()] == ["audit.jsonl"]


def test_escrituras_concurrentes_no_entrelazan_lineas(log: AuditLog) -> None:
    def work(worker: int) -> None:
        for i in range(60):
            entry(log, turn_id=f"{worker}-{i}", args={"p": "ñ" * 50})

    threads = [threading.Thread(target=work, args=(w,)) for w in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(20)
    records = lines(log.path)  # cada línea debe ser JSON válido
    assert len(records) == 8 * 60
    assert len({r["turn_id"] for r in records}) == 8 * 60


def test_un_fallo_de_disco_no_lanza(tmp_path: Path) -> None:
    blocker = tmp_path / "es-carpeta.jsonl"
    blocker.mkdir()  # abrir una carpeta como archivo falla
    entry(AuditLog(blocker))  # no debe propagar la excepción
