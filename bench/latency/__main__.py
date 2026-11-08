"""CLI: ``uv run python -m bench.latency --wav-dir bench/latency/fixtures``."""

from __future__ import annotations

import argparse
import sys
from datetime import datetime
from pathlib import Path

from ezagent.voice.config import VoiceConfig

from . import runner


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="python -m bench.latency")
    ap.add_argument("--wav-dir", default="bench/latency/fixtures", type=Path)
    ap.add_argument("--stt", choices=["real", "fake"], default="real")
    ap.add_argument("--llm", choices=["real", "fake"], default="real")
    ap.add_argument("--tts", choices=["real", "null"], default="real")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--synthetic", action="store_true",
                    help="Usa audio sintético cuando faltan los WAV (solo para probar el banco)")
    args = ap.parse_args(argv)

    cfg = VoiceConfig.load()
    phrases = runner.load_phrases(args.wav_dir)
    missing = [p.id for p in phrases if p.wav is None]
    if missing and not args.synthetic:
        print(f"Faltan {len(missing)} WAV en {args.wav_dir}: {', '.join(missing[:5])}…")
        print("Grábalos según bench/latency/fixtures/README.md o usa --synthetic.")
        return 2
    if args.synthetic and (args.stt == "real" or args.llm == "real" or args.tts == "real"):
        print("--synthetic solo vale con --stt fake (el audio no es voz real).")
        if args.stt == "real":
            return 2

    stt = runner.FakeStt() if args.stt == "fake" else runner.RealStt(cfg)
    llm = runner.FakeLlm() if args.llm == "fake" else runner.MessengerLlm(cfg)
    tts = runner.NullTts() if args.tts == "null" else runner.RealTts(cfg)
    meta = {"llm": f"{cfg.llm_provider}/{cfg.llm_model}" if args.llm == "real" else "fake",
            "stt": args.stt, "tts": args.tts, "stt_model": cfg.stt_model,
            "synthetic_audio": bool(args.synthetic and missing)}
    report = runner.run_bench(phrases, cfg, stt, llm, tts, runs=args.runs,
                              meta_extra=meta, synthetic=args.synthetic)
    out = args.out or Path("bench/out") / f"latency-{datetime.now():%Y%m%d-%H%M%S}.json"
    runner.write_report(report, out)
    print(f"Informe: {out}")
    for key, value in report["summary"].items():
        print(f"  {key}: {value}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
