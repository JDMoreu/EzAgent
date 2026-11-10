"""Regresiones de la auditoría de rendimiento y robustez de audio.

Cubre F08 (síntesis fuera del hilo lector), F12 (cliente STT persistente y
presupuesto único), F13/F46 (re-sondeo de Smart Turn) y los cierres de
``SpeechQueue``.
"""

from __future__ import annotations

import threading
import time
from concurrent.futures import Future
from types import SimpleNamespace

import httpx
import pytest

from ezagent.voice import stt
from ezagent.voice.audio import CHUNK_SECS, Segmenter
from ezagent.voice.speech_queue import SpeechQueue

RATE = 16000
CHUNK = b"\x00\x00" * int(RATE * CHUNK_SECS)
QUIET = [0.0003] * 30


# --------------------------------------------------------------- SpeechQueue
def play_ok(samples, rate, on_start) -> bool:
    on_start()
    return True


def test_say_no_bloquea_con_una_sintesis_lenta() -> None:
    stop = threading.Event()
    release = threading.Event()
    started = threading.Event()

    def synth(text: str):
        started.set()
        assert release.wait(5)
        return text, RATE

    speech = SpeechQueue(synth, play_ok, stop)
    began = time.monotonic()
    assert speech.say("uno") is True
    assert speech.say("dos") is True
    assert time.monotonic() - began < 0.2  # el llamador solo encola
    assert started.wait(2)
    release.set()
    assert speech.finish(timeout=5)


def test_las_frases_suenan_en_orden() -> None:
    played: list[str] = []

    def play(samples, rate, on_start) -> bool:
        played.append(samples)
        on_start()
        return True

    speech = SpeechQueue(lambda text: (text, RATE), play, threading.Event())
    for text in ("uno", "dos", "tres"):
        assert speech.say(text) is True
    assert speech.finish(timeout=5) is True
    assert played == ["uno", "dos", "tres"]


def test_abort_descarta_lo_pendiente() -> None:
    started = threading.Event()
    release = threading.Event()
    played: list[str] = []

    def play(samples, rate, on_start) -> bool:
        played.append(samples)
        on_start()
        if samples == "uno":
            started.set()
            release.wait(5)
        return True

    speech = SpeechQueue(lambda text: (text, RATE), play, threading.Event())
    for text in ("uno", "dos", "tres"):
        assert speech.say(text) is True
    assert started.wait(2)
    speech.abort()
    release.set()
    assert speech.finish(timeout=5)
    assert played == ["uno"]


def test_las_colas_de_texto_y_audio_estan_acotadas() -> None:
    speech = SpeechQueue(lambda text: (text, RATE), play_ok, threading.Event())
    try:
        assert speech._text_q.maxsize == 64
        assert speech._audio_q.maxsize == 8
    finally:
        speech.abort()
        speech.finish(timeout=5)


def test_say_espera_hueco_en_vez_de_sintetizar_en_el_llamador() -> None:
    release = threading.Event()

    def play(samples, rate, on_start) -> bool:
        release.wait(5)
        return True

    speech = SpeechQueue(lambda text: (text, RATE), play, threading.Event())
    done = threading.Event()
    results: list[bool] = []

    def fill() -> None:
        for index in range(80):
            results.append(speech.say(f"frase {index}"))
        done.set()

    filler = threading.Thread(target=fill)
    filler.start()
    assert not done.wait(0.5)  # la cola llena hace backpressure
    speech.abort()
    release.set()
    assert done.wait(2)
    filler.join(2)
    assert speech.finish(timeout=5)
    assert False in results  # lo que no cupo se descarta al abortar


def test_say_tras_finish_y_finish_repetido() -> None:
    speech = SpeechQueue(lambda text: (text, RATE), play_ok, threading.Event())
    assert speech.say("hola") is True
    assert speech.finish(timeout=5) is True
    assert speech.finish(timeout=5) is True
    assert speech.say("tarde") is False


def test_un_error_de_sintesis_se_propaga_y_no_deja_hilos() -> None:
    def synth(text: str):
        raise ValueError("kokoro roto")

    speech = SpeechQueue(synth, play_ok, threading.Event())
    assert speech.say("hola") is True
    with pytest.raises(ValueError, match="kokoro roto"):
        speech.finish(timeout=5)
    assert isinstance(speech.error, ValueError)
    assert not speech._synth_thread.is_alive()
    assert not speech._play_thread.is_alive()


# ------------------------------------------------------------ Smart Turn (F46)
def seg(**kwargs) -> Segmenter:
    defaults = dict(min_rms=0.0006, silence_secs=1.3, pre_roll_secs=0.0)
    defaults.update(kwargs)
    return Segmenter(**defaults)


def run(segmenter: Segmenter, levels: list[float]) -> list[tuple[int, bytes]]:
    closes = []
    for index, level in enumerate(levels):
        pcm, _ = segmenter.feed(CHUNK, level)
        if pcm is not None:
            closes.append((index, pcm))
    return closes


def test_smart_turn_se_vuelve_a_sondear_en_la_misma_pausa() -> None:
    marks: list[float] = []

    def probe(pcm: bytes) -> Future[bool]:
        marks.append(segmenter._silence_run)
        done: Future[bool] = Future()
        done.set_result(len(marks) >= 2)  # solo acierta en la segunda sonda
        return done

    segmenter = seg(end_probe=probe, quick_silence_secs=0.3)
    closes = run(segmenter, QUIET + [0.03] * 20 + [0.0003] * int(1.5 / CHUNK_SECS))
    assert len(marks) >= 2
    assert all(b - a >= 0.29 for a, b in zip(marks, marks[1:], strict=False))
    assert len(closes) == 1 and segmenter.last_close_reason == "turn"
    assert closes[0][0] < len(QUIET) + 20 + int(1.2 / CHUNK_SECS)


def test_smart_turn_no_sonda_en_cada_chunk() -> None:
    marks: list[float] = []

    def probe(pcm: bytes) -> Future[bool]:
        marks.append(segmenter._silence_run)
        done: Future[bool] = Future()
        done.set_result(False)
        return done

    segmenter = seg(end_probe=probe, quick_silence_secs=0.3)
    closes = run(segmenter, QUIET + [0.03] * 20 + [0.0003] * int(1.5 / CHUNK_SECS))
    assert len(closes) == 1 and segmenter.last_close_reason == "silence"
    assert 2 <= len(marks) <= 5
    assert all(b - a >= 0.29 for a, b in zip(marks, marks[1:], strict=False))


# ------------------------------------------------------------------- STT (F12)
class FakeClient:
    def __init__(self, *outcomes) -> None:
        self.outcomes = list(outcomes)
        self.calls: list[tuple[str, float]] = []

    def post(self, url, data=None, files=None, timeout=None):
        self.calls.append((url, timeout))
        outcome = self.outcomes.pop(0)
        if isinstance(outcome, BaseException):
            raise outcome
        return outcome


def reply(status: int = 200, payload: dict | None = None) -> httpx.Response:
    request = httpx.Request("POST", "http://audit")
    return httpx.Response(status, json=payload or {}, request=request)


def cfg(**overrides) -> SimpleNamespace:
    values = dict(
        whisper_url="http://audit-stt.local:8080/inference",
        stt_model="",
        stt_prompt="",
        stt_timeout_secs=5.0,
        stt_dictionary="",
        stt_vocabulary_hints=False,
    )
    values.update(overrides)
    return SimpleNamespace(**values)


@pytest.fixture(autouse=True)
def _clean_endpoint_cache():
    stt._WORKING_ENDPOINT.clear()
    yield
    stt._WORKING_ENDPOINT.clear()


def test_un_timeout_de_lectura_no_repite_la_ruta_alternativa() -> None:
    client = FakeClient(httpx.ReadTimeout("tarde"))
    with pytest.raises(stt.STTError, match="no respondió a tiempo"):
        stt.transcribe(cfg(), b"wav", client=client)
    assert len(client.calls) == 1


def test_un_connect_timeout_prueba_la_alternativa_con_el_presupuesto_restante() -> None:
    client = FakeClient(httpx.ConnectTimeout("rechazada"), reply(payload={"text": "ok"}))
    assert stt.transcribe(cfg(), b"wav", client=client) == "ok"
    assert [url for url, _ in client.calls] == [
        "http://audit-stt.local:8080/inference",
        "http://audit-stt.local:8080/v1/audio/transcriptions",
    ]
    first, second = (timeout for _, timeout in client.calls)
    assert first == pytest.approx(5.0)
    assert 0 < second <= first


def test_se_recuerda_el_endpoint_que_funciono() -> None:
    first = FakeClient(reply(404), reply(payload={"text": "uno"}))
    assert stt.transcribe(cfg(), b"wav", client=first) == "uno"
    second = FakeClient(reply(payload={"text": "dos"}))
    assert stt.transcribe(cfg(), b"wav", client=second) == "dos"
    assert [url for url, _ in second.calls] == [
        "http://audit-stt.local:8080/v1/audio/transcriptions"
    ]
