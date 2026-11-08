"""CLI: ``uv run python -m evals.intents --provider opencode --model deepseek-v4.1-flash``."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path

from . import runner


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="python -m evals.intents")
    ap.add_argument("--provider", default="opencode", help="proveedor de llm_providers, o 'fake'")
    ap.add_argument("--model", default=None)
    ap.add_argument("--lang", choices=["es", "en", "all"], default="all")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--tools", choices=["v2"], default="v2",
                    help="solo v2 (6 herramientas planas); el sistema actual no se evalúa aquí")
    ap.add_argument("--out", type=Path, default=None, help="informe Markdown")
    args = ap.parse_args(argv)

    cases = runner.load_cases()
    if args.lang != "all":
        cases = [c for c in cases if c["lang"] == args.lang]
    if args.limit:
        cases = cases[: args.limit]

    if args.provider == "fake":
        caller, model = runner.oracle, "fake"
    else:
        caller = runner.openai_caller(args.provider, args.model)
        model = f"{args.provider}/{args.model or 'default'}"
    report = runner.run(cases, caller, {"model": model, "date": datetime.now().isoformat(timespec="seconds")})

    out = args.out or Path("evals/out") / f"intents-{model.replace('/', '_')}-{datetime.now():%Y%m%d-%H%M%S}.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(report.to_markdown(), encoding="utf-8")
    out.with_suffix(".json").write_text(json.dumps(report.to_json(), ensure_ascii=False, indent=2), encoding="utf-8")
    j = report.to_json()
    print(f"Informe: {out}")
    print(f"Casos {j['cases']} | precisión {j['accuracy']} % | es/en {j['by_lang']} | TTFT {j['ttft_ms']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
