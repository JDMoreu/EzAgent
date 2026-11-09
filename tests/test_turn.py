"""Fin de turno: características de Smart Turn, Segmenter con sondeo y unión."""

from __future__ import annotations

import queue
from concurrent.futures import Future
from types import SimpleNamespace

import numpy as np

from ezagent.voice import turn
from ezagent.voice.app import THINKING, VoiceApp
from ezagent.voice.audio import Segmenter

CHUNK = b"\x00\x00" * 480  # 30 ms a 16 kHz


def _done(value: bool) -> Future[bool]:
    future: Future[bool] = Future()
    future.set_result(value)
    return future


def _speak(segmenter: Segmenter, seconds: float) -> None:
    for _ in range(10):  # calibra el ruido
        segmenter.feed(CHUNK, 0.002)
    for _ in range(int(seconds / 0.03)):
        pcm, _ = segmenter.feed(CHUNK, 0.02)
        assert pcm is None


def _silence_until_cut(segmenter: Segmenter, limit: int = 100) -> tuple[int, bytes]:
    for index in range(1, limit + 1):
        pcm, _ = segmenter.feed(CHUNK, 0.002)
        if pcm:
            return index, pcm
    raise AssertionError("el turno no se cerró")


# ------------------------------------------------------------ características
def test_prepare_da_la_forma_del_modelo_y_alinea_la_voz_al_final() -> None:
    rate = 16000
    tone = (np.sin(np.arange(rate) * 2 * np.pi * 440 / rate) * 8000).astype(np.int16)
    features = turn.prepare(tone.tobytes(), rate)
    assert features.shape == (1, turn.N_MELS, 800)
    assert features.dtype == np.float32
    energy = features[0].mean(axis=0)
    # Ceros al principio: la voz (1 s) ocupa el final de la ventana de 8 s.
    assert energy[-50:].mean() > energy[:50].mean()


def test_prepare_remuestrea_y_recorta_a_ocho_segundos() -> None:
    long_audio = np.zeros(44100 * 12, dtype=np.int16).tobytes()
    assert turn.prepare(long_audio, 44100).shape == (1, turn.N_MELS, 800)
    assert turn.prepare(b"", 44100).shape == (1, turn.N_MELS, 800)


def test_filtros_mel_cubren_todo_el_espectro() -> None:
    filters = turn._mel_filters()
    assert filters.shape == (1 + turn.N_FFT // 2, turn.N_MELS)
    assert (filters.sum(axis=0) > 0).all()


def test_sin_modelo_no_evalua_y_no_rompe(tmp_path) -> None:
    detector = turn.SmartTurn(model_path=str(tmp_path / "no-existe.onnx"))
    detector._pool.submit(lambda: None).result(timeout=10)  # espera la carga
    assert not detector.ready
    assert detector.submit(b"\0\0" * 100, 16000) is None
    detector.close()


# ------------------------------------------------------------- Segmenter
def test_frase_completa_cierra_tras_el_silencio_corto() -> None:
    calls: list[int] = []

    def probe(pcm: bytes) -> Future[bool]:
        calls.append(len(pcm))
        return _done(True)

    segmenter = Segmenter(min_rms=0.003, silence_secs=1.3, end_probe=probe, quick_silence_secs=0.3)
    _speak(segmenter, 0.6)
    chunks, pcm = _silence_until_cut(segmenter)
    assert chunks * 0.03 < 0.5
    assert segmenter.last_close_reason == "turn"
    assert len(calls) == 1
    assert pcm


def test_frase_incompleta_espera_el_silencio_largo() -> None:
    segmenter = Segmenter(
        min_rms=0.003,
        silence_secs=1.3,
        end_probe=lambda pcm: _done(False),
        quick_silence_secs=0.3,
    )
    _speak(segmenter, 0.6)
    chunks, _ = _silence_until_cut(segmenter)
    assert chunks * 0.03 >= 1.29
    assert segmenter.last_close_reason == "silence"


def test_sin_resultado_o_con_error_manda_el_silencio() -> None:
    for probe in (lambda pcm: None, lambda pcm: Future()):
        segmenter = Segmenter(
            min_rms=0.003, silence_secs=1.3, end_probe=probe, quick_silence_secs=0.3
        )
        _speak(segmenter, 0.6)
        chunks, _ = _silence_until_cut(segmenter)
        assert chunks * 0.03 >= 1.29

    def broken(pcm: bytes) -> Future[bool]:
        raise RuntimeError("sin modelo")

    segmenter = Segmenter(min_rms=0.003, silence_secs=1.3, end_probe=broken, quick_silence_secs=0.3)
    _speak(segmenter, 0.6)
    chunks, _ = _silence_until_cut(segmenter)
    assert chunks * 0.03 >= 1.29


def test_cada_pausa_se_evalua_una_vez_y_la_voz_la_invalida() -> None:
    answers = iter([False, True])
    calls: list[int] = []

    def probe(pcm: bytes) -> Future[bool]:
        calls.append(1)
        return _done(next(answers))

    segmenter = Segmenter(min_rms=0.003, silence_secs=1.3, end_probe=probe, quick_silence_secs=0.3)
    _speak(segmenter, 0.6)
    for _ in range(20):  # pausa de 0,6 s: incompleta, no corta
        pcm, _ = segmenter.feed(CHUNK, 0.002)
        assert pcm is None
    assert len(calls) == 1
    for _ in range(10):  # retoma la voz
        segmenter.feed(CHUNK, 0.02)
    chunks, _ = _silence_until_cut(segmenter)
    assert len(calls) == 2
    assert chunks * 0.03 < 0.5


# ---------------------------------------------------------- unión de frases
def _app() -> VoiceApp:
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app.in_call = True
    app.muted = False
    app.agent = None
    app._turn_gen = 1
    app._tts_stop = None
    app._ignore_utterance = False
    app._post_tts_ignore = False
    app._post_tts_until = 0.0
    app._user_speaking = False
    app._last_turn_audio = None
    app._resume_pcm = None
    app.cfg = SimpleNamespace(barge_in=True, barge_in_rms=0.008)
    app.mic = SimpleNamespace(last_speech_end=None)
    app.state = "listening"
    app._set_state = lambda state: setattr(app, "state", state)
    app._signal_tts_stop = lambda: None
    app.stt_audio: list[bytes] = []
    app._stt_worker = lambda pcm, rate, gen, timing: app.stt_audio.append(pcm)
    return app


def _wait_stt(app: VoiceApp, count: int) -> None:
    import time

    deadline = time.monotonic() + 5
    while len(app.stt_audio) < count and time.monotonic() < deadline:
        time.sleep(0.01)


def test_si_sigue_hablando_mientras_piensa_se_une_con_la_frase_anterior() -> None:
    app = _app()
    app._on_utterance(b"AA", 16000)
    _wait_stt(app, 1)
    assert app.state == THINKING
    app._on_speech_start(0.02)
    app._on_utterance(b"BB", 16000)
    _wait_stt(app, 2)
    assert app.stt_audio == [b"AA", b"AABB"]


def test_un_ruido_descartado_no_pega_audio_viejo_a_la_siguiente_frase() -> None:
    app = _app()
    app._on_utterance(b"AA", 16000)
    _wait_stt(app, 1)
    app._on_speech_start(0.02)  # arranque en THINKING que no llega a frase
    app.state = "listening"
    app._on_speech_start(0.02)  # frase nueva, mucho después
    app._on_utterance(b"CC", 16000)
    _wait_stt(app, 2)
    assert app.stt_audio[-1] == b"CC"
