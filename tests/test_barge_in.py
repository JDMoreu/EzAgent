"""Interrupción del asistente: un pico de ruido no corta al modelo, la voz sí."""

from __future__ import annotations

import queue
import threading
from types import SimpleNamespace

import pytest

from ezagent.voice import audio as A
from ezagent.voice.app import SPEAKING, VoiceApp

LOUD, QUIET = 0.02, 0.001  # sobre y bajo el umbral de 0.008 (−42 dB)
THRESHOLD = 0.008


def _feed(det: A.BargeInDetector, levels: list[float]):
    """Alimenta el detector; devuelve (índice del chunk que confirmó, racha)."""
    for i, level in enumerate(levels):
        run = det.feed(bytes([i % 256]) * 4, level)
        if run is not None:
            return i, run
    return None, None


def _det(secs: float = 0.25) -> A.BargeInDetector:
    return A.BargeInDetector(threshold=THRESHOLD, secs=secs)


def test_el_umbral_por_defecto_son_menos_42_db():
    # Es justo el pico que molestaba: 20·log10(0,008) ≈ −42 dB.
    import math

    assert round(20 * math.log10(THRESHOLD)) == -42


def test_un_clic_de_un_chunk_no_interrumpe():
    assert _feed(_det(), [QUIET] * 5 + [LOUD] + [QUIET] * 20)[0] is None


@pytest.mark.parametrize("chunks", [2, 3, 5])
def test_un_golpe_de_hasta_150_ms_no_interrumpe(chunks):
    assert _feed(_det(), [LOUD] * chunks + [QUIET] * 30)[0] is None


def test_golpes_de_teclado_repetidos_no_suman():
    # Un clic cada 6 chunks (180 ms): las pausas rompen la racha cada vez.
    levels = ([LOUD] + [QUIET] * 5) * 20
    assert _feed(_det(), levels)[0] is None


def test_voz_sostenida_interrumpe_a_los_250_ms():
    idx, run = _feed(_det(), [QUIET] * 4 + [LOUD] * 30)
    need = _det().need
    assert idx == 4 + need - 1  # confirma justo al cumplirse el tiempo
    assert need == 9  # 0,25 s / 0,03 s
    assert run is not None and len(run) == need


def test_la_voz_con_pausas_entre_silabas_sigue_contando():
    syllables = ([LOUD] * 3 + [QUIET] * 2) * 6  # pausas de 60 ms < 90 ms
    idx, _ = _feed(_det(), syllables)
    assert idx is not None


def test_una_pausa_larga_rompe_la_racha():
    levels = [LOUD] * 5 + [QUIET] * 4 + [LOUD] * 5  # 120 ms de silencio en medio
    assert _feed(_det(), levels)[0] is None


def test_la_racha_devuelta_conserva_el_principio_de_la_frase():
    levels = [QUIET] * 6 + [LOUD] * 12
    _, run = _feed(_det(), levels)
    assert run is not None
    assert all(level >= THRESHOLD for _, level in run)  # empieza en el primer chunk fuerte
    assert run[0][0] == bytes([6]) * 4  # el chunk 6: el primero de la voz


def test_se_puede_ajustar_la_duracion_y_el_umbral_en_vivo():
    det = _det(secs=0.12)
    assert det.need == 4
    assert _feed(det, [LOUD] * 6)[0] == 3
    det.threshold = 0.05  # sube el umbral: LOUD ya no basta
    det.secs = 0.25
    assert _feed(det, [LOUD] * 30)[0] is None


def test_el_detector_se_reinicia_tras_confirmar():
    det = _det()
    assert _feed(det, [LOUD] * 20)[0] is not None
    assert _feed(det, [LOUD] * 3 + [QUIET] * 10)[0] is None


# ------------------------------------------------------------ MicCapture.stream
class _FakeStream:
    """Entrega chunks fijos y para la captura al terminar."""

    def __init__(self, chunks: list[bytes], stop: threading.Event) -> None:
        self._chunks = list(chunks)
        self._stop = stop

    def read(self, frames, exception_on_overflow=False):
        if not self._chunks:
            self._stop.set()
            return b"\x00\x00" * frames
        return self._chunks.pop(0)

    def stop_stream(self):
        pass

    def close(self):
        pass


def _chunk(level: float, frames: int = 480) -> bytes:
    import numpy as np

    return (np.ones(frames, np.int16) * int(level * 32768)).tobytes()


def _run_stream(levels: list[float], rms: float = THRESHOLD, secs: float = 0.25):
    stop = threading.Event()
    mic = A.MicCapture.__new__(A.MicCapture)
    mic.device_index = 0
    mic.rate = 16000
    mic.min_rms = 0.0006
    mic.silence_secs = 1.3
    mic.pre_roll_secs = 0.6
    mic.min_speech_secs = 0.2
    mic.max_utterance_secs = 30.0
    mic.noise_multiplier = 1.3
    mic.barge_in_rms = rms
    mic.barge_in_secs = secs
    mic.last_speech_end = None
    mic._pa = SimpleNamespace(open=lambda **kw: _FakeStream([_chunk(lv) for lv in levels], stop))
    events: list[str] = []
    mic.stream(
        stop,
        on_speech_start=lambda lv: events.append("start"),
        is_speaking=lambda: True,
        on_barge_in=lambda lv: events.append("barge"),
    )
    return events


def test_stream_un_pico_en_pleno_tts_no_interrumpe():
    assert "barge" not in _run_stream([QUIET] * 10 + [LOUD] * 2 + [QUIET] * 30)


def test_stream_la_voz_sostenida_en_pleno_tts_interrumpe_y_arranca_el_turno():
    events = _run_stream([QUIET] * 10 + [LOUD] * 40)
    assert "barge" in events
    assert "start" in events[events.index("barge") :]  # la frase reinyectada abre turno


def test_stream_el_umbral_se_lee_en_cada_chunk():
    # Con el umbral por encima del nivel, ni la voz sostenida interrumpe.
    assert "barge" not in _run_stream([LOUD] * 60, rms=0.05)


# ------------------------------------------------------------ app
def _app(**cfg) -> VoiceApp:
    app = VoiceApp.__new__(VoiceApp)
    app.events = queue.Queue()
    app.cfg = SimpleNamespace(barge_in=True, barge_in_rms=THRESHOLD, **cfg)
    app.in_call = True
    app.muted = False
    app.state = SPEAKING
    app._turn_gen = 5
    app._tts_stop = threading.Event()
    app._user_speaking = False
    app._ignore_utterance = False
    app._post_tts_ignore = False
    app._post_tts_until = 0.0
    app._close_after_answer = False
    app._close_mode = ""
    app.agent = None
    calls: list = []
    app._set_state = lambda state: calls.append(state)
    app.calls = calls
    return app


def test_app_un_arranque_de_voz_en_pleno_tts_no_corta_al_modelo():
    app = _app()
    VoiceApp._on_speech_start(app, 0.5)  # ni un nivel altísimo basta por sí solo
    assert not app._tts_stop.is_set()
    assert app._turn_gen == 5 and app.calls == []
    assert app._ignore_utterance is True


def test_app_la_interrupcion_confirmada_si_corta_y_abre_turno():
    app = _app()
    VoiceApp._on_barge_in(app, 0.05)
    assert app._tts_stop.is_set()
    assert app._turn_gen == 6 and app.calls == ["listening"]
    assert app._user_speaking is True


def test_app_interrumpir_el_cierre_lo_cancela():
    app = _app()
    app._close_after_answer = True
    app._close_mode = "pause"
    VoiceApp._on_barge_in(app, 0.05)
    assert app._close_after_answer is False and app._close_mode == ""


def test_app_sin_umbral_configurado_se_interrumpe_al_hablar():
    app = _app()
    app.cfg.barge_in_rms = 0.0
    VoiceApp._on_speech_start(app, 0.05)
    assert app._tts_stop.is_set() and app.calls == ["listening"]


def test_app_con_la_interrupcion_apagada_nunca_corta():
    app = _app()
    app.cfg.barge_in = False
    VoiceApp._on_speech_start(app, 0.5)
    VoiceApp._on_barge_in(app, 0.5)
    assert not app._tts_stop.is_set() and app.calls == []


def test_app_el_slider_cambia_el_umbral_del_micro_en_vivo():
    app = _app()
    app.mic = SimpleNamespace(barge_in_rms=THRESHOLD)
    VoiceApp._apply_live_setting(app, "barge_in_rms", 0.03)
    assert app.cfg.barge_in_rms == 0.03
    assert app.mic.barge_in_rms == 0.03  # antes solo cambiaba cfg


def test_config_trae_la_duracion_por_defecto():
    from ezagent.voice.config import VoiceConfig

    cfg = VoiceConfig()
    assert cfg.barge_in_secs == 0.25
