"""Fixtures de los textos de la app (dominio `app_text`, F1-02).

Cubre `_Sentences`, `_strip_courtesy`, `_is_filler_sentence`, `_hotkey_key` y
`_fast_command` del módulo real. `_fast_command` se ejecuta sobre un doble
mínimo de `VoiceApp` que registra lo observable: retorno, textos hablados,
estado del micrófono, cierre y eventos. Los campos opcionales de cada caso
(`close_action`, `last_summary`, `tasks`, `agents`) describen ese doble; los
agentes por defecto son tres y no hay tareas pendientes.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "app_text"

_LAST = "Resumen anterior."
_EMPTY: list[dict[str, str]] = []
_AGENTS: list[dict[str, str]] = [
    {"label": "Claude Code", "state": "working"},
    {"label": "Codex", "state": "done"},
    {"label": "OpenCode", "state": ""},
]
_TASKS: list[dict[str, str]] = [
    {"address": "t1", "agent": "codex", "task": "revisar los tests"},
    {"address": "t2", "agent": "claude", "task": "arreglar " + "x" * 120},
]

# (chunks que se pasan a feed, max_chars, first_chars, llamar a flush al final)
_SENTENCE_CASES: list[tuple[list[str], int, int, bool]] = [
    (["Hola mundo. "], 160, 40, True),
    (["Perfecto, sigamos con el resto"], 160, 40, True),
    (["Sí, claro que sí y más"], 160, 40, True),
    (["Escucha esto; es importante"], 160, 40, True),
    (["Escucha: esto es importante."], 160, 40, True),
    (["Listo."], 160, 40, True),
    (["¡Claro que sí! Vamos"], 160, 40, True),
    (["¿Qué hora es? No sé."], 160, 40, True),
    (["El Dr. Pérez llegó. Bien."], 160, 40, True),
    (["Visita https://example.com ahora. Luego sigue."], 160, 40, True),
    (["El valor es 3.14159 y sube."], 160, 40, True),
    (["El valor es 3. y sube"], 160, 40, True),
    (["palabra " * 20], 40, 20, True),
    (["x" * 200], 40, 20, True),
    (["Hola", " mundo", ". ¿Qué", " tal?"], 160, 40, True),
    (["Una frase bastante larga que", " sigue aquí. Otra."], 160, 40, True),
    ([], 160, 40, True),
    (["", "Texto normal. ", ""], 160, 40, True),
    (["   "], 160, 40, True),
    (["Mañana revisaré la ñ. ¡Hecho! 🙂"], 160, 40, True),
    (["Hola. Adiós."], 160, 0, True),
    (["abc"], 0, 40, True),
    (["Primera.\nSegunda."], 160, 40, True),
    (["Uno. Dos. Tres."], 160, 40, True),
    (["Uno\ndos\ntres"], 160, 40, False),
    (["  Primera frase con coma, y sigue el texto. "], 160, 40, True),
    (["a b c d e f g h i j k l m n o p q r s t u v w x y z"], 20, 10, True),
    (["Texto ", "muy ", "largo ", "sin ", "punto ", "final"], 20, 40, True),
]

_FLUSH_BUFFERS = [
    "",
    "   ",
    "  hola  ",
    "hola\nmundo",
    "Texto. ",
    "a",
    "ñandú",
    "emoji 🙂",
    "\n\n",
    "  x  \n",
    "uno dos",
    "Una frase completa.",
    "con\t\ttabulador",
    "- viñeta",
    "123",
    "¿pregunta?",
    "¡exclamación!",
    'comillas "x"',
    "  ",
    "final",
]

_COURTESY_KEYS = [
    "",
    "ok",
    "ok mutéate",
    "Ok, gracias. Mutéate",
    "ok gracias mutéate",
    "vale",
    "va",
    "bueno,",
    "muchas gracias",
    "mil gracias",
    "eh este pues va",
    "listo cierra la escucha",
    "perfecto dale",
    "ok   mutéate",
    "ok\tmutéate",
    "OK mutéate",
    "ok ñandú",
    "gracias",
    "muchas gracias cierra",
    "  ok  mutéate  ",
    "dale",
    "okay oki bueno genial",
    "gracias por todo",
    "este es el bueno",
]

_FILLER_SENTENCES = [
    "Un momento, lo reviso.",
    "Pensando…",
    "Va, déjame ver.",
    "A ver.",
    "Ya lo veo",
    "Enseguida",
    "Dame un segundo",
    "Déjame pensar",
    "Lo busco",
    "Lo miro",
    "Ya lo hago",
    "Veamos",
    "un momento",
    "UN MOMENTO",
    "Dame un momento por favor ya",
    "Dame un momento por favor ya mismo",
    "A ver si funciona el despliegue",
    "Estoy pensando en otra cosa",
    "",
    "   ",
    "Sí, está abierto.",
    "Un momento: el agente t51 terminó y dejó tres archivos nuevos.",
    "No hay nada pendiente",
    "Pensando 🙂",
    "ñ",
    "Un momento—listo",
    "dame un respiro",
    "Lo reviso y te digo",
]

_HOTKEY_SPECS = [
    "Ctrl+Alt+Space",
    " ctrl + alt + space ",
    "",
    "+",
    "++",
    "Ctrl+",
    "+Ctrl",
    "shift+Shift",
    "Alt+F4",
    "Ctrl + Ñ",
    "Meta+🚀",
    "Ctrl\t+\tAlt",
    "ctrl alt",
    "Ctrl + + Space",
    "A+B+C+D+E+F+G+H+I+J",
    "1+2",
    "ctrl+alt+space",
    "  ",
    "Alt+ ",
    "Alt + Enter",
    "Ctrl+\nAlt",
    None,
    7,
]

# (texto, close_action, last_summary, tasks, agents)
_FAST_CASES: list[tuple[Any, ...]] = [
    ("mutéate", "pause", _LAST, _EMPTY, _AGENTS),
    ("Mutea el micrófono.", "pause", _LAST, _EMPTY, _AGENTS),
    ("silencia el micro", "pause", _LAST, _EMPTY, _AGENTS),
    ("silencia el micrófono", "pause", _LAST, _EMPTY, _AGENTS),
    ("silencio el micro", "pause", _LAST, _EMPTY, _AGENTS),
    ("apaga el micrófono", "pause", _LAST, _EMPTY, _AGENTS),
    ("desmutea", "pause", _LAST, _EMPTY, _AGENTS),
    ("activa el micrófono", "pause", _LAST, _EMPTY, _AGENTS),
    ("unmute", "pause", _LAST, _EMPTY, _AGENTS),
    ("reactiva el micro", "pause", _LAST, _EMPTY, _AGENTS),
    ("cierra la escucha", "pause", _LAST, _EMPTY, _AGENTS),
    ("cierra la escucha", "hangup", _LAST, _EMPTY, _AGENTS),
    ("pausa", "pause", _LAST, _EMPTY, _AGENTS),
    ("adiós", "pause", _LAST, _EMPTY, _AGENTS),
    ("cuelga", "pause", _LAST, _EMPTY, _AGENTS),
    ("termina la llamada", "pause", _LAST, _EMPTY, _AGENTS),
    ("colgar", "hangup", _LAST, _EMPTY, _AGENTS),
    ("repítelo", "pause", _LAST, _EMPTY, _AGENTS),
    ("repite", "pause", "", _EMPTY, _AGENTS),
    ("otra vez", "pause", _LAST, _EMPTY, _AGENTS),
    ("hola", "pause", _LAST, _EMPTY, _AGENTS),
    ("buenas ezagent", "pause", _LAST, _EMPTY, _AGENTS),
    ("HOLA 🙂", "pause", _LAST, _EMPTY, _AGENTS),
    ("gracias", "pause", _LAST, _EMPTY, _AGENTS),
    ("muchas gracias", "pause", _LAST, _EMPTY, _AGENTS),
    ("ok gracias", "pause", _LAST, _EMPTY, _AGENTS),
    ("gracias, mutéate", "pause", _LAST, _EMPTY, _AGENTS),
    ("qué agentes hay", "pause", _LAST, _EMPTY, _AGENTS),
    ("lista los agentes", "pause", _LAST, _EMPTY, []),
    ("cuántos agentes hay", "pause", _LAST, _EMPTY, "raise"),
    ("qué tareas pendientes", "pause", _LAST, _EMPTY, _AGENTS),
    ("cómo van las tareas", "pause", _LAST, _TASKS, _AGENTS),
    ("silencio", "pause", _LAST, _EMPTY, _AGENTS),
    ("mute", "pause", _LAST, _EMPTY, _AGENTS),
    ("", "pause", _LAST, _EMPTY, _AGENTS),
    ("haz algo", "pause", _LAST, _EMPTY, _AGENTS),
    ("abre el archivo", "pause", _LAST, _EMPTY, _AGENTS),
    ("cierra el micro", "hangup", _LAST, _EMPTY, _AGENTS),
    ("dame un momento", "pause", _LAST, _EMPTY, _AGENTS),
    ("ok, gracias. mutéate", "pause", _LAST, _EMPTY, _AGENTS),
]


class _FakeEvents:
    def __init__(self) -> None:
        self.items: list[list[Any]] = []

    def put(self, item: Any) -> None:
        self.items.append(list(item))


class _FakeHerdr:
    def __init__(self, agents: Any) -> None:
        self.agents = agents

    def list_agents(self) -> list[dict[str, str]]:
        if self.agents == "raise":
            raise RuntimeError("herdr caído")
        return list(self.agents)

    def resolve_agent(self, agent: str) -> str:
        return agent

    def label_for(self, address: str) -> str:
        return ""


class _FakeTasks:
    def __init__(self, tasks: list[dict[str, str]]) -> None:
        self.tasks = tasks

    def all(self) -> list[dict[str, str]]:
        return list(self.tasks)


def _fake_app(app_module: Any, options: dict[str, Any]) -> Any:
    from types import SimpleNamespace

    app = app_module.VoiceApp.__new__(app_module.VoiceApp)
    app.cfg = SimpleNamespace(
        close_action=options.get("close_action", "pause"),
        voice_wake=True,
        wake_phrase="oye ez",
        agent_profiles={},
    )
    app.events = _FakeEvents()
    app.herdr = _FakeHerdr(options.get("agents", _AGENTS))
    app.tasks = _FakeTasks(options.get("tasks", _EMPTY))
    app.hud = None
    app.last_summary = options.get("last_summary", _LAST)
    app.muted = False
    app._close_after_answer = False
    app._close_mode = ""
    app.spoken: list[list[str]] = []
    app._speak = lambda text, kind="announce": app.spoken.append([text, kind])
    app._set_mic_muted = lambda muted, source="": setattr(app, "muted", bool(muted))
    app._start_worker = lambda target, args=(), name="worker": target(*args)
    return app


def _emit_sentences(out: FixtureFile, module: Any) -> None:
    for index, (chunks, max_chars, first_chars, flush) in enumerate(_SENTENCE_CASES, start=1):
        sentences = module._Sentences(max_chars=max_chars, first_chars=first_chars)
        emitted: list[str] = []
        for chunk in chunks:
            emitted.extend(sentences.feed(chunk))
        flushed = sentences.flush() if flush else []
        out.add(
            f"app_text/sentences/{index:03d}",
            "_Sentences.feed",
            {
                "chunks": chunks,
                "max_chars": max_chars,
                "first_chars": first_chars,
                "flush": flush,
            },
            {"emitted": emitted, "flushed": flushed, "buffer": sentences.buf},
        )


def _emit_flush(out: FixtureFile, module: Any) -> None:
    for index, buffer in enumerate(_FLUSH_BUFFERS, start=1):
        sentences = module._Sentences()
        sentences.buf = buffer
        flushed = sentences.flush()
        out.add(
            f"app_text/flush/{index:03d}",
            "_Sentences.flush",
            {"buffer": buffer},
            {"flushed": flushed, "buffer": sentences.buf},
        )


def _emit_courtesy(out: FixtureFile, module: Any) -> None:
    for index, key in enumerate(_COURTESY_KEYS, start=1):
        out.add(
            f"app_text/courtesy/{index:03d}",
            "_strip_courtesy",
            {"key": key},
            {"text": module._strip_courtesy(key)},
        )


def _emit_filler(out: FixtureFile, module: Any) -> None:
    for index, sentence in enumerate(_FILLER_SENTENCES, start=1):
        out.add(
            f"app_text/filler/{index:03d}",
            "_is_filler_sentence",
            {"sentence": sentence},
            {"is_filler": module._is_filler_sentence(sentence)},
        )


def _emit_hotkeys(out: FixtureFile, module: Any) -> None:
    for index, spec in enumerate(_HOTKEY_SPECS, start=1):
        out.add(
            f"app_text/hotkey/{index:03d}",
            "_hotkey_key",
            {"spec": spec},
            {"key": module._hotkey_key(spec)},
        )


def _emit_fast(out: FixtureFile, module: Any) -> None:
    for index, case in enumerate(_FAST_CASES, start=1):
        text, close_action, last_summary, tasks, agents = case
        options = {
            "close_action": close_action,
            "last_summary": last_summary,
            "tasks": tasks,
            "agents": agents,
        }
        app = _fake_app(module, options)
        handled = module.VoiceApp._fast_command(app, text)
        out.add(
            f"app_text/fast/{index:03d}",
            "_fast_command",
            {"text": text, **options},
            {
                "handled": handled,
                "spoken": app.spoken,
                "muted": app.muted,
                "close_after_answer": app._close_after_answer,
                "close_mode": app._close_mode,
                "events": app.events.items,
            },
        )


def _emit_errors(out: FixtureFile, module: Any) -> None:
    out.add_error(
        "app_text/sentences/err/none",
        "_Sentences.feed",
        {"chunks": [None], "max_chars": 160, "first_chars": 40, "flush": True},
        _error_text(module._Sentences().feed, None),
    )
    out.add_error(
        "app_text/courtesy/err/none",
        "_strip_courtesy",
        {"key": None},
        _error_text(module._strip_courtesy, None),
    )
    out.add_error(
        "app_text/courtesy/err/int",
        "_strip_courtesy",
        {"key": 5},
        _error_text(module._strip_courtesy, 5),
    )
    out.add_error(
        "app_text/filler/err/none",
        "_is_filler_sentence",
        {"sentence": None},
        _error_text(module._is_filler_sentence, None),
    )
    app = _fake_app(module, {})
    out.add_error(
        "app_text/fast/err/none",
        "_fast_command",
        {"text": None},
        _error_text(module.VoiceApp._fast_command, app, None),
    )
    out.add_error(
        "app_text/fast/err/int",
        "_fast_command",
        {"text": 5},
        _error_text(module.VoiceApp._fast_command, app, 5),
    )


def _error_text(fn: Any, *args: Any) -> str:
    try:
        fn(*args)
    except Exception as error:  # noqa: BLE001 - el oráculo guarda el mensaje real
        return str(error)
    raise AssertionError("la llamada no falló")


def generate(out: FixtureFile) -> None:
    from ezagent.voice import app as module

    _emit_sentences(out, module)
    _emit_flush(out, module)
    _emit_courtesy(out, module)
    _emit_filler(out, module)
    _emit_hotkeys(out, module)
    _emit_fast(out, module)
    _emit_errors(out, module)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
