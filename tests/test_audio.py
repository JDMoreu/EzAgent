"""Audio: nivel, voz débil, segmentador VAD y detector de interrupciones."""

from __future__ import annotations

import io
import wave
from concurrent.futures import Future

import numpy as np
import pytest

from ezagent.voice.audio import (
    BARGE_IN_MAX_GAP,
    CHUNK_SECS,
    BargeInDetector,
    Segmenter,
    pcm_is_weak,
    pcm_rms,
    pcm_to_wav,
)

RATE = 16000
CHUNK = b"\x00\x00" * int(RATE * CHUNK_SECS)


def tone_pcm(level: float, secs: float, *, seed: int = 0) -> bytes:
    """Ruido gaussiano con la desviación indicada (rms ≈ level)."""
    rng = np.random.default_rng(seed)
    data = rng.normal(0.0, level, int(RATE * secs))
    return (np.clip(data, -1, 1) * 32767).astype(np.int16).tobytes()


# --------------------------------------------------------------- nivel
def test_pcm_rms() -> None:
    assert pcm_rms(b"") == 0.0
    assert pcm_rms(b"\x00\x00" * 100) == 0.0
    full = np.full(100, 32767, dtype=np.int16).tobytes()
    assert pcm_rms(full) == pytest.approx(1.0, abs=1e-3)
    half = np.full(100, 16384, dtype=np.int16).tobytes()
    assert pcm_rms(half) == pytest.approx(0.5, abs=1e-3)
    assert pcm_rms(tone_pcm(0.05, 1.0)) == pytest.approx(0.05, rel=0.05)


def test_un_buffer_de_longitud_impar_se_tolera() -> None:
    assert pcm_rms(b"\x01") == 0.0  # un byte suelto no es una muestra
    even = tone_pcm(0.05, 0.5)
    assert pcm_rms(even + b"\x07") == pcm_rms(even)
    assert pcm_is_weak(b"\x01", RATE) is True
    assert pcm_is_weak(tone_pcm(0.05, 0.8) + b"\x01", RATE) is False


def test_pcm_to_wav_es_un_wav_valido() -> None:
    pcm = tone_pcm(0.1, 0.5)
    with wave.open(io.BytesIO(pcm_to_wav(pcm, RATE))) as wav:
        assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) == (1, 2, RATE)
        assert wav.readframes(wav.getnframes()) == pcm


# ------------------------------------------------------------ voz débil
@pytest.mark.parametrize(
    "pcm",
    [b"", b"\x00\x00" * RATE, b"\x00\x00" * 5, tone_pcm(0.002, 2.0), tone_pcm(0.004, 3.0)],
    ids=["vacio", "silencio", "muy-corto", "ruido-bajo", "ruido-de-sala"],
)
def test_silencio_y_ruido_son_audio_debil(pcm: bytes) -> None:
    assert pcm_is_weak(pcm, RATE) is True


def test_un_clic_aislado_es_audio_debil() -> None:
    click = bytearray(tone_pcm(0.001, 2.0))
    click[16000:16400] = tone_pcm(0.05, 0.0125)
    assert pcm_is_weak(bytes(click), RATE) is True


def test_voz_sostenida_no_es_debil_aunque_haya_silencio_alrededor() -> None:
    assert pcm_is_weak(tone_pcm(0.05, 0.8) + tone_pcm(0.001, 1.0), RATE) is False
    assert pcm_is_weak(tone_pcm(0.02, 0.5), RATE) is False


def test_el_umbral_de_duracion_de_voz() -> None:
    # 0,3 s de voz es el mínimo; muy por debajo es debil, muy por encima no.
    assert pcm_is_weak(tone_pcm(0.05, 0.1) + tone_pcm(0.0005, 2.0), RATE) is True
    assert pcm_is_weak(tone_pcm(0.05, 0.6) + tone_pcm(0.0005, 2.0), RATE) is False


# ------------------------------------------------------------- segmentador
def run(segmenter: Segmenter, levels: list[float]) -> list[tuple[int, bytes]]:
    closes = []
    for index, level in enumerate(levels):
        pcm, _ = segmenter.feed(CHUNK, level)
        if pcm is not None:
            closes.append((index, pcm))
    return closes


def seg(**kwargs) -> Segmenter:
    defaults = dict(min_rms=0.0006, silence_secs=1.3, pre_roll_secs=0.0)
    defaults.update(kwargs)
    return Segmenter(**defaults)


QUIET = [0.0003] * 30
SILENCE_TAIL = [0.0003] * int(1.5 / CHUNK_SECS)


def test_una_frase_se_cierra_tras_el_silencio_configurado() -> None:
    speech = [0.03] * 20
    closes = run(seg(), QUIET + speech + SILENCE_TAIL)
    assert len(closes) == 1 and closes[0][1]
    waited = (closes[0][0] - (len(QUIET) + len(speech)) + 1) * CHUNK_SECS
    assert 1.2 <= waited <= 1.4
    segmenter = seg()
    run(segmenter, QUIET + speech + SILENCE_TAIL)
    assert segmenter.last_close_reason == "silence"


def test_una_pausa_corta_no_parte_la_frase() -> None:
    levels = QUIET + [0.03] * 10 + [0.0003] * 20 + [0.03] * 10 + SILENCE_TAIL  # pausa de 0,6 s
    closes = run(seg(), levels)
    assert len(closes) == 1


def test_dos_frases_separadas_por_silencio_largo_son_dos_turnos() -> None:
    levels = QUIET + [0.03] * 15 + SILENCE_TAIL + [0.03] * 15 + SILENCE_TAIL
    assert len(run(seg(), levels)) == 2


def test_el_audio_devuelto_incluye_la_voz_y_el_pre_roll() -> None:
    marked = b"\x01\x00" * int(RATE * CHUNK_SECS)
    segmenter = seg(pre_roll_secs=0.3)
    out = None
    for level in QUIET + [0.03] * 10 + SILENCE_TAIL:
        pcm, _ = segmenter.feed(marked if level > 0.01 else CHUNK, level)
        out = pcm or out
    assert out is not None
    assert out.count(marked) >= 10  # toda la voz
    assert out.startswith(CHUNK)  # y algo de silencio previo
    assert len(out) > len(marked) * 10


def test_un_clic_se_descarta_con_bytes_vacios() -> None:
    closes = run(seg(), QUIET + [0.05] * 4 + SILENCE_TAIL)
    assert [pcm for _, pcm in closes] == [b""]


def test_dos_chunks_fuertes_no_arrancan_un_turno() -> None:
    segmenter = seg()
    assert run(segmenter, QUIET + [0.05] * 2 + SILENCE_TAIL) == []


def test_speech_started_se_avisa_una_sola_vez() -> None:
    segmenter = seg()
    flags = [segmenter.feed(CHUNK, level)[1] for level in QUIET + [0.03] * 20]
    assert flags.count(True) == 1


def test_el_turno_se_corta_al_llegar_al_maximo() -> None:
    segmenter = seg(max_utterance_secs=1.0)
    closes = run(segmenter, QUIET + [0.03] * 100)
    assert closes and closes[0][1] and segmenter.last_close_reason == "max"


def test_ruido_estable_no_cuenta_como_voz_y_sube_el_suelo() -> None:
    segmenter = seg()
    assert run(segmenter, [0.002] * 200) == []
    assert segmenter.noise == pytest.approx(0.002, rel=0.3)
    start, end = segmenter.thresholds()
    assert start > end and start >= 0.002 * 1.3 * 0.9


def test_umbrales_respetan_el_minimo_configurado() -> None:
    start, end = seg(min_rms=0.01).thresholds()
    assert start >= 0.01 and end >= 0.006


def test_end_probe_cierra_antes_si_la_frase_esta_completa() -> None:
    done: Future[bool] = Future()
    done.set_result(True)
    calls: list[bytes] = []

    def probe(pcm: bytes) -> Future[bool]:
        calls.append(pcm)
        return done

    segmenter = seg(end_probe=probe, quick_silence_secs=0.3)
    closes = run(segmenter, QUIET + [0.03] * 20 + SILENCE_TAIL)
    assert len(closes) == 1 and segmenter.last_close_reason == "turn"
    assert len(calls) == 1
    assert closes[0][0] < len(QUIET) + 20 + int(1.2 / CHUNK_SECS)  # mucho antes de 1,3 s


def test_end_probe_negativo_o_roto_deja_mandar_al_silencio() -> None:
    not_done: Future[bool] = Future()
    not_done.set_result(False)

    def raising(pcm: bytes):
        raise RuntimeError("modelo caído")

    for probe in (lambda pcm: not_done, raising):
        segmenter = seg(end_probe=probe, quick_silence_secs=0.3)
        closes = run(segmenter, QUIET + [0.03] * 20 + SILENCE_TAIL)
        assert len(closes) == 1 and segmenter.last_close_reason == "silence"


def test_end_probe_pendiente_no_cierra_y_si_la_voz_vuelve_se_olvida() -> None:
    pending: Future[bool] = Future()  # nunca termina
    segmenter = seg(end_probe=lambda pcm: pending, quick_silence_secs=0.3)
    levels = QUIET + [0.03] * 20 + [0.0003] * 15 + [0.03] * 10
    assert run(segmenter, levels) == []
    assert segmenter._pending is None  # la voz volvió: la evaluación ya no vale


def test_flush_devuelve_lo_capturado_solo_si_hay_habla_suficiente() -> None:
    segmenter = seg()
    run(segmenter, QUIET + [0.03] * 20)
    assert segmenter.flush()
    assert segmenter.flush() is None  # ya está limpio
    fresh = seg()
    run(fresh, QUIET + [0.03] * 4)
    assert fresh.flush() is None


def test_reset_descarta_el_turno_en_curso() -> None:
    segmenter = seg()
    run(segmenter, QUIET + [0.03] * 20)
    segmenter.reset()
    assert segmenter.flush() is None
    assert run(segmenter, SILENCE_TAIL) == []


def test_set_pre_roll_conserva_los_chunks_y_tolera_basura() -> None:
    segmenter = seg(pre_roll_secs=0.3)
    for _ in range(5):
        segmenter.feed(CHUNK, 0.0003)
    segmenter.set_pre_roll(0.6)
    assert segmenter._pre.maxlen == int(0.6 / CHUNK_SECS) and len(segmenter._pre) == 5
    segmenter.set_pre_roll("no es número")  # type: ignore[arg-type]
    segmenter.set_pre_roll(-4)
    assert segmenter.pre_roll_secs == 0.0 and segmenter._pre.maxlen == 1


# ------------------------------------------------------------ interrupciones
def feed_all(detector: BargeInDetector, levels: list[float]) -> list:
    out = []
    for level in levels:
        result = detector.feed(CHUNK, level)
        if result is not None:
            out.append(result)
    return out


def test_una_voz_sostenida_interrumpe_y_devuelve_el_principio_de_la_frase() -> None:
    detector = BargeInDetector(threshold=0.008, secs=0.25)
    need = detector.need
    assert need == 9  # ceil(0,25 / 0,03)
    (run_,) = feed_all(detector, [0.001] * 5 + [0.05] * need)
    assert len(run_) == need and all(level == 0.05 for _, level in run_)


def test_un_golpe_corto_no_interrumpe() -> None:
    detector = BargeInDetector()
    assert feed_all(detector, [0.05] * 3 + [0.001] * 10 + [0.05] * 3 + [0.001] * 10) == []


def test_las_pausas_entre_silabas_se_toleran_hasta_el_limite() -> None:
    detector = BargeInDetector(secs=0.12)  # 4 chunks fuertes
    gap = [0.001] * BARGE_IN_MAX_GAP
    assert len(feed_all(detector, [0.05, 0.05] + gap + [0.05, 0.05])) == 1
    detector = BargeInDetector(secs=0.12)
    too_long = [0.001] * (BARGE_IN_MAX_GAP + 1)
    assert feed_all(detector, [0.05, 0.05] + too_long + [0.05, 0.05]) == []


def test_reset_y_need_minimo() -> None:
    detector = BargeInDetector(secs=0)
    assert detector.need == 1
    assert BargeInDetector(secs=-5).need == 1
    detector = BargeInDetector(secs=0.3)
    feed_all(detector, [0.05] * 5)
    detector.reset()
    assert feed_all(detector, [0.05] * 5) == []  # empezó de cero


def test_despues_de_interrumpir_el_detector_queda_listo_para_otra() -> None:
    detector = BargeInDetector(secs=0.1)
    assert len(feed_all(detector, [0.05] * 20)) >= 2
