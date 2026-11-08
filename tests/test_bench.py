"""Banco de latencia: VAD en tiempo de audio, cortes, WER y JSONL de métricas."""

from __future__ import annotations

import json
import wave
from pathlib import Path

import pytest

from bench.latency import runner
from ezagent.voice.config import VoiceConfig
from ezagent.voice.metrics import LatencyStats, TurnTimings


def _cfg(**kw) -> VoiceConfig:
    cfg = VoiceConfig()
    for key, value in kw.items():
        setattr(cfg, key, value)
    return cfg


def test_vad_wait_coincide_con_el_silencio_configurado() -> None:
    cfg = _cfg(silence_secs=1.0)
    res = runner.run_vad(runner.synthetic_speech(), cfg)
    assert len(res.utterances) == 1 and not res.cut
    assert 950 <= res.vad_wait_ms <= 1100


def test_pausa_larga_a_mitad_se_marca_como_corte() -> None:
    cfg = _cfg(silence_secs=0.6)
    pcm = runner.synthetic_speech(1.6, gap_at=0.8, gap_s=1.0)
    res = runner.run_vad(pcm, cfg)
    assert res.cut and len(res.utterances) == 2


def test_pausa_corta_no_corta() -> None:
    cfg = _cfg(silence_secs=1.3)
    pcm = runner.synthetic_speech(1.6, gap_at=0.8, gap_s=0.5)
    assert not runner.run_vad(pcm, cfg).cut


def test_run_bench_con_drivers_falsos(tmp_path: Path) -> None:
    phrases = [runner.Phrase("c01", "abre claude", "comando"),
               runner.Phrase("h01", "hola", "charla")]
    report = runner.run_bench(
        phrases, _cfg(), runner.FakeStt(), runner.FakeLlm(), runner.NullTts(),
        runs=2, synthetic=True,
    )
    assert len(report["turns"]) == 4
    row = report["turns"][0]
    for key in ("vad_wait_ms", "stt_ready_ms", "first_token_ms", "ttfa_ms", "cut", "wer"):
        assert key in row
    assert row["wer"] == 0.0
    summary = report["summary"]
    assert summary["turns"] == 4 and "p50" in summary["ttfa_ms"]
    out = tmp_path / "r.json"
    runner.write_report(report, out)
    assert json.loads(out.read_text(encoding="utf-8"))["meta"]["runs"] == 2


def test_sin_wav_y_sin_synthetic_se_omiten() -> None:
    report = runner.run_bench(
        [runner.Phrase("x", "hola", "charla")], _cfg(), runner.FakeStt(),
        runner.FakeLlm(), runner.NullTts(),
    )
    assert report["turns"] == []


def test_read_wav_exige_16k_mono(tmp_path: Path) -> None:
    bad = tmp_path / "bad.wav"
    with wave.open(str(bad), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(44100)
        w.writeframes(b"\0\0" * 100)
    with pytest.raises(ValueError):
        runner.read_wav(bad)
    good = tmp_path / "ok.wav"
    with wave.open(str(good), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(b"\0\0" * 160)
    assert len(runner.read_wav(good)) == 320


def test_wer() -> None:
    assert runner.wer("abre claude en ezagent", "Abre Claude en ezagent.") == 0.0
    assert runner.wer("a b c d", "a x c") == pytest.approx(0.5)
    assert runner.wer("", "") == 0.0


def test_phrases_json_es_valido() -> None:
    phrases = runner.load_phrases(Path("bench/latency/fixtures"))
    assert len(phrases) == 20
    assert {p.group for p in phrases} == {"comando", "pausa", "charla"}
    assert len({p.id for p in phrases}) == 20


def test_metricas_escriben_jsonl(tmp_path: Path) -> None:
    path = tmp_path / "logs" / "latency.jsonl"
    stats = LatencyStats(jsonl_path=path)
    t = TurnTimings(speech_end=10.0, vad_end=11.3)
    t.mark("stt_ready", 11.4)
    t.mark("first_sound", 12.0)
    stats.record(t)
    stats.record_command("fast_command", 4.2)
    lines = [json.loads(x) for x in path.read_text(encoding="utf-8").splitlines()]
    assert lines[0]["kind"] == "turn" and lines[0]["vad_wait_ms"] == 1300.0
    assert lines[1] == {**lines[1], "kind": "command", "handled_ms": 4.2}


def test_metricas_sin_ruta_no_escribe_ni_falla() -> None:
    stats = LatencyStats()
    stats.record_command("x", 1.0)  # no lanza
