"""Smart Turn: las características de audio deben ser las de Whisper, exactas y estables."""

from __future__ import annotations

import time

import numpy as np
import pytest

from ezagent.voice import turn
from ezagent.voice.turn import N_MELS, N_SAMPLES, SAMPLE_RATE, SmartTurn, log_mel, prepare


def sine(freq: float, secs: float, rate: int = SAMPLE_RATE, amp: float = 0.5) -> bytes:
    t = np.arange(int(secs * rate)) / rate
    return (np.sin(2 * np.pi * freq * t) * amp * 32767).astype(np.int16).tobytes()


def test_la_entrada_del_modelo_tiene_forma_y_tipo_correctos() -> None:
    features = prepare(sine(440, 1.0), SAMPLE_RATE)
    assert features.shape == (1, 80, 800) and features.dtype == np.float32
    assert np.isfinite(features).all()


def test_pcm_vacio_tambien_produce_una_entrada_valida() -> None:
    features = prepare(b"", SAMPLE_RATE)
    assert features.shape == (1, 80, 800) and np.isfinite(features).all()


def test_es_determinista() -> None:
    pcm = sine(300, 2.0)
    assert np.array_equal(prepare(pcm, SAMPLE_RATE), prepare(pcm, SAMPLE_RATE))


def test_solo_cuentan_los_ultimos_ocho_segundos() -> None:
    long_pcm = sine(200, 6.0) + sine(900, 8.0)
    tail = sine(900, 8.0)
    assert np.allclose(prepare(long_pcm, SAMPLE_RATE), prepare(tail, SAMPLE_RATE), atol=1e-4)


def test_el_final_de_la_voz_queda_al_final_de_la_ventana() -> None:
    features = prepare(sine(500, 1.0), SAMPLE_RATE)[0]
    energy = features.mean(axis=0)  # por fotograma
    assert energy[-100:].mean() > energy[:100].mean() + 0.1  # relleno de ceros al principio


def test_es_invariante_al_volumen() -> None:
    loud, quiet = sine(440, 2.0, amp=0.8), sine(440, 2.0, amp=0.08)
    assert np.allclose(prepare(loud, SAMPLE_RATE), prepare(quiet, SAMPLE_RATE), atol=0.02)


def test_remuestrea_otras_frecuencias_a_16k() -> None:
    at_48k = prepare(sine(440, 1.0, rate=48000), 48000)
    at_16k = prepare(sine(440, 1.0), SAMPLE_RATE)
    assert at_48k.shape == at_16k.shape
    assert np.abs(at_48k - at_16k).mean() < 0.05


def test_un_tono_enciende_la_banda_mel_que_le_corresponde() -> None:
    def peak_band(freq: float) -> int:
        window = (np.sin(2 * np.pi * freq * np.arange(N_SAMPLES) / SAMPLE_RATE)).astype(np.float64)
        return int(np.argmax(log_mel(window).mean(axis=1)))

    low, mid, high = peak_band(200), peak_band(1000), peak_band(4000)
    assert low < mid < high
    assert 20 <= mid <= 34  # 1 kHz cae a ~1/3 de la escala mel de 80 bandas
    assert 0 <= low < 15 and 55 <= high <= 79


def test_el_banco_de_filtros_es_el_de_slaney() -> None:
    filters = turn._FILTERS
    assert filters.shape == (201, N_MELS)
    assert (filters >= 0).all() and (filters.sum(axis=0) > 0).all()  # ninguna banda vacía
    # Normalización de área: cada filtro integra ≈ 2 / ancho, no suma 1.
    assert not np.allclose(filters.sum(axis=0), 1.0)
    assert turn._WINDOW.shape == (400,) and turn._WINDOW[0] == 0.0  # Hann periódica


def test_la_escala_mel_es_invertible_y_monotona() -> None:
    freqs = np.array([0.0, 100.0, 500.0, 1000.0, 2000.0, 8000.0])
    mels = turn._hz_to_mel(freqs)
    assert (np.diff(mels) > 0).all() and mels[3] == pytest.approx(15.0)
    assert np.allclose(turn._mel_to_hz(mels), freqs)


def test_smart_turn_sin_modelo_no_esta_listo_y_no_bloquea() -> None:
    detector = SmartTurn(model_path="Z:/no/existe/modelo.onnx")
    try:
        deadline = time.monotonic() + 5
        while not detector._failed and time.monotonic() < deadline:
            time.sleep(0.01)
        assert detector._failed and detector.ready is False
        assert detector.submit(b"\0\0" * 100, SAMPLE_RATE) is None
        with pytest.raises(RuntimeError, match="no está cargado"):
            detector.probability(b"\0\0", SAMPLE_RATE)
    finally:
        detector.close()
