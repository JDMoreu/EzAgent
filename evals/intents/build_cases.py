"""Genera ``cases.jsonl`` de forma determinista (sin azar).

Regenerar: ``uv run python -m evals.intents.build_cases``. Los casos son
plantillas × agentes × proyectos; para añadir cobertura, se amplía una lista de
aquí y se vuelve a generar, nunca se edita el JSONL a mano.
"""

from __future__ import annotations

import json
from pathlib import Path

OUT = Path(__file__).with_name("cases.jsonl")

# (clave canónica, formas habladas correctas, errores típicos de whisper)
AGENTS = [
    ("claude", ["claude", "claude code"], ["cloud code", "clod"]),
    ("opencode", ["opencode", "open code"], ["open cod"]),
    ("codex", ["codex"], ["códex"]),
]
PROJECTS = [
    ("ezagent", ["ezagent"], ["easy agent", "ez agent"]),
    ("api", ["api"], []),
    ("web", ["web"], []),
]
# (imperativo, subjuntivo, palabra clave que debe aparecer en `task`)
TASKS_ES = [
    ("arregla los tests", "arregle los tests", "test"),
    ("revisa los errores de lint", "revise los errores de lint", "lint"),
    ("escribe pruebas para el módulo de descargas", "escriba pruebas para el módulo de descargas", "prueba"),
    ("actualiza el readme", "actualice el readme", "readme"),
    ("refactoriza el parser", "refactorice el parser", "parser"),
]
TASKS_EN = [
    ("fix the failing tests", "test"),
    ("update the readme", "readme"),
    ("refactor the parser", "parser"),
    ("add tests for the download module", "test"),
]


def case(cid: str, lang: str, text: str, route: str, tool: str | None,
         args: dict | None = None, contains: dict | None = None) -> dict:
    expect: dict = {"tool": tool}
    if args:
        expect["args"] = args
    if contains:
        expect["contains"] = contains
    return {"id": cid, "lang": lang, "text": text, "route": route, "expect": expect}


def build() -> list[dict]:
    cases: list[dict] = []
    n = {"v": 0}

    def add(prefix: str, *a, **k) -> None:
        n["v"] += 1
        cases.append(case(f"{prefix}-{n['v']:03d}", *a, **k))

    # ---- run_agent (es): plantillas rotando agente, proyecto y tarea
    es_templates = [
        "abre {a} en {p} y {imp}",
        "lanza {a} en el proyecto {p} para que {sub}",
        "pídele a {a} que {sub} en {p}",
        "ponle a {a} a trabajar en {p}: {imp}",
        "con {a} en {p}, {imp}",
        "necesito que {a} {sub} en {p}",
    ]
    for i, tpl in enumerate(es_templates * 4):
        key, forms, _ = AGENTS[i % 3]
        pkey, pforms, _ = PROJECTS[(i // 3) % 3]
        imp, sub, kw = TASKS_ES[i % len(TASKS_ES)]
        text = tpl.format(a=forms[i % len(forms)], p=pforms[0], imp=imp, sub=sub)
        add("run-es", "es", text, "action", "run_agent",
            {"agent": key, "project": pkey}, {"task": [kw]})

    # ---- run_agent con modelo / esfuerzo / aviso / encadenado (es)
    extras_es = [
        ("abre claude en ezagent en esfuerzo alto y arregla los tests", {"agent": "claude", "project": "ezagent", "effort": "high"}, {"task": ["test"]}),
        ("lanza opencode en api con deepseek y revisa el lint", {"agent": "opencode", "project": "api", "model": "deepseek"}, {"task": ["lint"]}),
        ("abre codex en web en esfuerzo bajo y actualiza el readme", {"agent": "codex", "project": "web", "effort": "low"}, {"task": ["readme"]}),
        ("lanza claude en ezagent para arreglar los tests y avísame cuando termine", {"agent": "claude", "project": "ezagent", "notify": True}, {"task": ["test"]}),
        ("abre opencode en api en segundo plano y revisa el lint", {"agent": "opencode", "project": "api", "background": True}, {"task": ["lint"]}),
        ("abre claude en ezagent para planificar el parser y pásale su salida a opencode", {"agent": "claude", "project": "ezagent", "after": "opencode"}, {"task": ["parser"]}),
    ]
    for text, args, contains in extras_es:
        add("runx-es", "es", text, "action", "run_agent", args, contains)

    # ---- errores típicos de whisper (alias mal transcritos)
    stt_cases = [
        ("abre cloud code en easy agent y arregla los tests", "claude", "ezagent", "test"),
        ("lanza open cod en api y revisa el lint", "opencode", "api", "lint"),
        ("pídele a códex que actualice el readme en web", "codex", "web", "readme"),
        ("abre clod en ez agent y refactoriza el parser", "claude", "ezagent", "parser"),
        ("ponle a cloud code a escribir pruebas en easy agent", "claude", "ezagent", "prueba"),
    ]
    for text, a, p, kw in stt_cases:
        add("stt-es", "es", text, "action", "run_agent", {"agent": a, "project": p}, {"task": [kw]})

    # ---- agent_status (es)
    for text, args in [
        ("cómo va claude", {"agent": "claude"}),
        ("cómo va opencode", {"agent": "opencode"}),
        ("qué está haciendo codex", {"agent": "codex"}),
        ("qué agentes hay trabajando", {}),
        ("resúmeme lo que hizo claude", {"agent": "claude", "summarize": True}),
        ("dime qué hizo opencode en api", {"summarize": True}),
        ("cómo va lo de ezagent", {}),
        ("hay algún agente atascado", {}),
        ("qué ha terminado ya", {}),
        ("cómo va todo", {}),
    ]:
        add("stat-es", "es", text, "action", "agent_status", args)

    # ---- send_to_agent (es)
    for text, args, contains in [
        ("dile a claude que use pytest con el flag q", {"agent": "claude"}, {"message": ["pytest"]}),
        ("respóndele a opencode que sí", {"agent": "opencode"}, {"message": ["sí"]}),
        ("dile que también revise los imports", {}, {"message": ["import"]}),
        ("avísale a codex que el archivo está en la carpeta docs", {"agent": "codex"}, {"message": ["docs"]}),
        ("dile a claude que pare de tocar el parser y siga con los tests", {"agent": "claude"}, {"message": ["test"]}),
        ("contéstale a opencode que adelante con el cambio", {"agent": "opencode"}, {"message": ["adelante"]}),
        ("pásale a codex la ruta src barra ezagent", {"agent": "codex"}, {"message": ["src"]}),
        ("dile que use la rama principal", {}, {"message": ["rama"]}),
    ]:
        add("send-es", "es", text, "action", "send_to_agent", args, contains)

    # ---- stop_agent (es)
    for text, args in [
        ("para claude", {"agent": "claude"}),
        ("detén opencode en api", {"agent": "opencode"}),
        ("cancela lo de codex", {"agent": "codex"}),
        ("mata el agente de ezagent", {}),
        ("detén a claude", {"agent": "claude"}),
    ]:
        add("stop-es", "es", text, "action", "stop_agent", args)

    # ---- voice_control (es)
    for text, args in [
        ("silencia el micrófono", {"action": "mute"}),
        ("silencio", {"action": "mute"}),
        ("reactiva el micro", {"action": "unmute"}),
        ("cuelga", {"action": "hang_up"}),
        ("cierra la escucha", {"action": "hang_up"}),
        ("repite lo último", {"action": "repeat"}),
        ("no te oí, repítelo", {"action": "repeat"}),
        ("avísame cuando termine claude", {"action": "watch", "agent": "claude"}),
        ("estate pendiente de opencode", {"action": "watch", "agent": "opencode"}),
    ]:
        add("voice-es", "es", text, "command" if args["action"] != "watch" else "action", "voice_control", args)

    # ---- remember (es)
    for text, args in [
        ("recuerda que mi proyecto principal es ezagent", {"kind": "profile"}),
        ("acuérdate de que prefiero respuestas cortas", {"kind": "profile"}),
        ("recuerda que easy agent es ezagent", {"kind": "project_alias"}),
        ("guarda que el proyecto api está en la carpeta de trabajo", {"kind": "project_alias"}),
    ]:
        add("mem-es", "es", text, "action", "remember", args)

    # ---- charla sin herramienta (es)
    for text in [
        "hola", "gracias, eso es todo", "¿quién eres?", "¿me escuchas?", "buenos días",
        "qué puedes hacer por mí", "cuéntame un chiste corto", "vale, perfecto",
        "no, déjalo", "¿qué hora es?", "espera un momento", "¿puedes hablar más despacio?", "ya está, gracias",
    ]:
        add("chat-es", "es", text, "chat", None)

    # ---- inglés
    en_templates = ["open {a} on {p} and {t}", "start {a} in the {p} project to {t}", "ask {a} to {t} on {p}"]
    for i, tpl in enumerate(en_templates * 4):
        key, forms, _ = AGENTS[i % 3]
        pkey, pforms, _ = PROJECTS[(i // 2) % 3]
        t, kw = TASKS_EN[i % len(TASKS_EN)]
        add("run-en", "en", tpl.format(a=forms[0], p=pforms[0], t=t), "action", "run_agent",
            {"agent": key, "project": pkey}, {"task": [kw]})
    for text, args in [("how is claude doing", {"agent": "claude"}), ("what agents are running", {}),
                       ("summarize what opencode did", {"agent": "opencode", "summarize": True}),
                       ("is anything stuck", {}), ("how is it going", {})]:
        add("stat-en", "en", text, "action", "agent_status", args)
    for text, args, contains in [
        ("tell claude to use pytest", {"agent": "claude"}, {"message": ["pytest"]}),
        ("answer yes to opencode", {"agent": "opencode"}, {"message": ["yes"]}),
        ("tell codex the file is in the docs folder", {"agent": "codex"}, {"message": ["docs"]}),
        ("tell it to also check the imports", {}, {"message": ["import"]}),
    ]:
        add("send-en", "en", text, "action", "send_to_agent", args, contains)
    for text, args in [("stop claude", {"agent": "claude"}), ("kill the opencode run", {"agent": "opencode"}),
                       ("cancel codex", {"agent": "codex"})]:
        add("stop-en", "en", text, "action", "stop_agent", args)
    for text, args in [("mute the microphone", {"action": "mute"}), ("unmute", {"action": "unmute"}),
                       ("hang up", {"action": "hang_up"}), ("repeat that", {"action": "repeat"})]:
        add("voice-en", "en", text, "command", "voice_control", args)
    for text, args in [("remember that I prefer short answers", {"kind": "profile"}),
                       ("remember that easy agent means ezagent", {"kind": "project_alias"})]:
        add("mem-en", "en", text, "action", "remember", args)
    for text in ["hello", "thanks, that's all", "who are you", "can you hear me", "what time is it", "never mind"]:
        add("chat-en", "en", text, "chat", None)
    return cases


def main() -> int:
    cases = build()
    OUT.write_text("\n".join(json.dumps(c, ensure_ascii=False) for c in cases) + "\n", encoding="utf-8")
    es = sum(1 for c in cases if c["lang"] == "es")
    print(f"{len(cases)} casos ({es} es, {len(cases) - es} en) -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
