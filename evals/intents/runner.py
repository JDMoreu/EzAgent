"""Ejecutor del set dorado: una llamada por caso, sin ejecutar herramientas.

Puntúa tres cosas por caso: herramienta correcta (o ninguna en charla),
argumentos esperados (con alias resueltos) y palabras clave en los textos
libres. Mide además el tiempo hasta el primer token (TTFT).
"""

from __future__ import annotations

import json
import time
import unicodedata
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

HERE = Path(__file__).parent

# Formas habladas (incluidos errores típicos de whisper) → clave canónica.
ALIASES = {
    "claude": "claude", "claude code": "claude", "cloud code": "claude", "clod": "claude",
    "opencode": "opencode", "open code": "opencode", "open cod": "opencode",
    "codex": "codex",
    "ezagent": "ezagent", "ez agent": "ezagent", "easy agent": "ezagent",
    "api": "api", "web": "web",
}
LANG_NAME = {"es": "Spanish (neutral Latin American)", "en": "English"}


def norm(value: object) -> str:
    text = unicodedata.normalize("NFKD", str(value)).lower().strip()
    text = "".join(c for c in text if not unicodedata.combining(c))
    return " ".join("".join(c if c.isalnum() or c.isspace() else " " for c in text).split())


def canon(value: object) -> object:
    if isinstance(value, bool) or value is None:
        return value
    key = norm(value)
    return ALIASES.get(key, key)


@dataclass
class Call:
    """Lo que devolvió el modelo."""

    tool: str | None
    args: dict[str, Any]
    ttft_ms: float
    text: str = ""
    prompt_tokens: int = 0
    completion_tokens: int = 0
    error: str = ""


@dataclass
class Score:
    id: str
    lang: str
    group: str
    ok: bool
    tool_ok: bool
    args_ok: bool
    expected: dict
    got_tool: str | None
    got_args: dict
    ttft_ms: float
    text: str
    reason: str = ""
    tokens: tuple[int, int] = (0, 0)


def load_cases(path: Path | None = None) -> list[dict]:
    lines = (path or HERE / "cases.jsonl").read_text(encoding="utf-8").splitlines()
    return [json.loads(x) for x in lines if x.strip()]


def load_tools() -> list[dict]:
    return json.loads((HERE / "tools_v2.json").read_text(encoding="utf-8"))


def build_system(lang: str, template: str | None = None) -> str:
    text = template or (HERE / "prompt_v2.txt").read_text(encoding="utf-8")
    return text.format(
        lang_name=LANG_NAME.get(lang, "Spanish"),
        agent_labels="Claude Code, OpenCode, Codex",
        project_names="ezagent, api, web",
        default_agent="OpenCode",
        profile="(empty)",
    )


def score_case(case: dict, call: Call) -> Score:
    exp = case["expect"]
    group = case["id"].rsplit("-", 2)[0]
    tool_ok = (call.tool == exp["tool"]) and not call.error
    args_ok = True
    reason = call.error
    if tool_ok and exp["tool"] is not None:
        for key, want in exp.get("args", {}).items():
            have = call.args.get(key)
            if canon(have) != canon(want):
                args_ok = False
                reason = f"{key}: esperado {want!r}, obtenido {have!r}"
                break
        if args_ok:
            for key, needles in exp.get("contains", {}).items():
                value = norm(call.args.get(key, ""))
                if not all(norm(n) in value for n in needles):
                    args_ok = False
                    reason = f"{key} no contiene {needles}: {call.args.get(key)!r}"
                    break
    elif not tool_ok and not call.error:
        reason = f"herramienta esperada {exp['tool']!r}, obtenida {call.tool!r}"
    return Score(
        id=case["id"], lang=case["lang"], group=group, ok=tool_ok and args_ok,
        tool_ok=tool_ok, args_ok=args_ok, expected=exp, got_tool=call.tool,
        got_args=call.args, ttft_ms=call.ttft_ms, text=case["text"], reason=reason,
        tokens=(call.prompt_tokens, call.completion_tokens),
    )


# ------------------------------------------------------------- proveedores
def oracle(case: dict) -> Call:
    """Proveedor falso que devuelve exactamente lo esperado."""
    exp = case["expect"]
    args = dict(exp.get("args", {}))
    for key, needles in exp.get("contains", {}).items():
        args[key] = " ".join(needles)
    return Call(tool=exp["tool"], args=args, ttft_ms=1.0)


def openai_caller(provider: str | None, model: str | None) -> Callable[[dict], Call]:
    from openai import OpenAI

    from ezagent.llm_providers import resolve_llm

    cfg = resolve_llm(provider=provider, model=model)
    client = OpenAI(base_url=cfg.base_url, api_key=cfg.api_key, timeout=60.0)
    tools = load_tools()
    extra = {"reasoning_effort": "none"} if cfg.provider in {"opencode", "openai"} else {}

    def call(case: dict) -> Call:
        t0 = time.monotonic()
        messages = [{"role": "system", "content": build_system(case["lang"])},
                    {"role": "user", "content": case["text"]}]
        tool_name: str | None = None
        raw_args = ""
        text = ""
        first: float | None = None
        usage: Any = None
        try:
            stream = client.chat.completions.create(
                model=cfg.model, messages=messages, tools=tools, tool_choice="auto",
                temperature=1.0 if cfg.provider == "anthropic" else 0.1, max_tokens=400,
                stream=True, stream_options={"include_usage": True}, extra_body=extra or None,
            )
            for chunk in stream:
                if getattr(chunk, "usage", None):
                    usage = chunk.usage
                if not chunk.choices:
                    continue
                delta = chunk.choices[0].delta
                if first is None and (delta.content or delta.tool_calls):
                    first = (time.monotonic() - t0) * 1000.0
                if delta.content:
                    text += delta.content
                for tc in delta.tool_calls or []:
                    if tc.function and tc.function.name:
                        tool_name = tc.function.name
                    if tc.function and tc.function.arguments:
                        raw_args += tc.function.arguments
        except Exception as exc:  # noqa: BLE001 - un fallo de red puntúa como fallo del caso
            return Call(None, {}, first or 0.0, error=f"{type(exc).__name__}: {exc}"[:200])
        try:
            args = json.loads(raw_args) if raw_args else {}
        except json.JSONDecodeError:
            return Call(tool_name, {}, first or 0.0, text, error="argumentos JSON inválidos")
        return Call(
            tool_name, args if isinstance(args, dict) else {}, first or 0.0, text,
            getattr(usage, "prompt_tokens", 0) or 0, getattr(usage, "completion_tokens", 0) or 0,
        )

    return call


# ---------------------------------------------------------------- informe
@dataclass
class Report:
    meta: dict = field(default_factory=dict)
    scores: list[Score] = field(default_factory=list)

    def pct(self, items: list[Score]) -> float:
        return 100.0 * sum(s.ok for s in items) / len(items) if items else 0.0

    def to_json(self) -> dict:
        def p(values: list[float], q: float) -> float:
            if not values:
                return 0.0
            ordered = sorted(values)
            return ordered[min(len(ordered) - 1, int(round((len(ordered) - 1) * q)))]
        ttft = [s.ttft_ms for s in self.scores if s.ttft_ms > 0]
        return {
            "meta": self.meta, "accuracy": round(self.pct(self.scores), 1),
            "cases": len(self.scores),
            "by_lang": {lg: round(self.pct([s for s in self.scores if s.lang == lg]), 1)
                        for lg in sorted({s.lang for s in self.scores})},
            "ttft_ms": {"p50": round(p(ttft, 0.5), 1), "p95": round(p(ttft, 0.95), 1)},
            "tokens": {"prompt": sum(s.tokens[0] for s in self.scores),
                       "completion": sum(s.tokens[1] for s in self.scores)},
        }

    def to_markdown(self) -> str:
        j = self.to_json()
        lines = [f"# Evals de intenciones · {self.meta.get('model', '?')}", "",
                 f"- Casos: {j['cases']} · Precisión: **{j['accuracy']} %**",
                 "- Por idioma: " + ", ".join(f"{k} {v} %" for k, v in j["by_lang"].items()),
                 f"- TTFT P50 / P95: {j['ttft_ms']['p50']} / {j['ttft_ms']['p95']} ms",
                 f"- Tokens: {j['tokens']['prompt']} entrada, {j['tokens']['completion']} salida", "",
                 "## Por grupo", "", "| Grupo | Casos | Precisión |", "|---|---|---|"]
        for g in sorted({s.group for s in self.scores}):
            items = [s for s in self.scores if s.group == g]
            lines.append(f"| {g} | {len(items)} | {self.pct(items):.0f} % |")
        fails = [s for s in self.scores if not s.ok]
        lines += ["", f"## Fallos ({len(fails)})", ""]
        for s in fails:
            lines.append(f"- `{s.id}` «{s.text}» → {s.reason or 'sin detalle'} "
                         f"(esperado `{s.expected.get('tool')}`, obtenido `{s.got_tool}` {s.got_args})")
        return "\n".join(lines) + "\n"


def run(cases: list[dict], caller: Callable[[dict], Call], meta: dict | None = None) -> Report:
    report = Report(meta=meta or {})
    for case in cases:
        report.scores.append(score_case(case, caller(case)))
    return report
