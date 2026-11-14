"""Fixtures de confirmaciones de operaciones destructivas (dominio `confirm`, F1-04).

Fija qué respuestas de voz o clic aceptan, rechazan o dejan pendiente una
confirmación, incluida la confirmación fuerte con número dictado. La interfaz
se simula dentro de `on_ask`, así que no hay hilos ni esperas reales.
"""

from __future__ import annotations

import threading
import time
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "confirm"

ACCEPT = ["sí", "si", "confirmo", "adelante", "dale"]
REJECT = ["no", "cancela", "para"]

SUMMARY = "Voy a borrar 3 archivos de la carpeta Descargas y mandarlos a la Papelera."

ASK_CASES: list[dict[str, Any]] = [
    {"id": "text-yes", "reply": [{"kind": "text", "text": "sí"}]},
    {"id": "text-yes-phrase", "reply": [{"kind": "text", "text": "Sí, adelante"}]},
    {"id": "text-accent-fold", "reply": [{"kind": "text", "text": "SÍ"}]},
    {"id": "text-reject", "reply": [{"kind": "text", "text": "No."}]},
    {"id": "reject-wins", "reply": [{"kind": "text", "text": "no, dale"}]},
    {"id": "reject-wins-reverse", "reply": [{"kind": "text", "text": "para, sí"}]},
    {"id": "ambiguous", "reply": [{"kind": "text", "text": "quizás"}, {"kind": "cancel"}]},
    {"id": "unsure", "reply": [{"kind": "text", "text": "hmm no sé"}, {"kind": "cancel"}]},
    {"id": "empty", "reply": [{"kind": "text", "text": ""}, {"kind": "cancel"}]},
    {"id": "substring-only", "reply": [{"kind": "text", "text": "daleee"}, {"kind": "cancel"}]},
    {"id": "strong-sí-only", "strong": True, "reply": [{"kind": "text", "text": "sí"}, {"kind": "cancel"}]},
    {"id": "strong-confirmo", "strong": True, "reply": [{"kind": "text", "text": "confirmo"}]},
    {"id": "strong-negation", "strong": True, "reply": [{"kind": "text", "text": "no confirmo"}]},
    {
        "id": "number-digits",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "confirmo 7"}],
    },
    {
        "id": "number-words",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "confirmo siete"}],
    },
    {
        "id": "number-digits-mismatch",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "confirmo 8"}],
    },
    {
        "id": "number-words-mismatch",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "confirmo seis"}],
    },
    {
        "id": "number-missing",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "confirmo"}, {"kind": "cancel"}],
    },
    {
        "id": "number-without-confirm",
        "strong": True,
        "require_number": 7,
        "reply": [{"kind": "text", "text": "siete"}, {"kind": "cancel"}],
    },
    {
        "id": "number-compound",
        "strong": True,
        "require_number": 32,
        "reply": [{"kind": "text", "text": "confirmo treinta y dos"}],
    },
    {
        "id": "number-thousands",
        "strong": True,
        "require_number": 1200,
        "reply": [{"kind": "text", "text": "confirmo mil doscientos"}],
    },
    {
        "id": "number-thousand",
        "strong": True,
        "require_number": 1000,
        "reply": [{"kind": "text", "text": "confirmo mil"}],
    },
    {
        "id": "number-twenty-one",
        "strong": True,
        "require_number": 21,
        "reply": [{"kind": "text", "text": "confirmo veintiuno"}],
    },
    {
        "id": "number-zero",
        "strong": True,
        "require_number": 0,
        "reply": [{"kind": "text", "text": "confirmo cero"}],
    },
    {
        "id": "number-counted",
        "strong": True,
        "require_number": 12,
        "reply": [{"kind": "text", "text": "confirmo 12 elementos"}],
    },
    {"id": "click-yes", "reply": [{"kind": "click", "yes": True}]},
    {"id": "click-no", "reply": [{"kind": "click", "yes": False}]},
    {"id": "cancel", "reply": [{"kind": "cancel"}]},
    {
        "id": "token-wrong-then-right",
        "reply": [
            {"kind": "text", "text": "sí", "token": "wrong"},
            {"kind": "text", "text": "sí", "token": "current"},
        ],
    },
    {
        "id": "arm-then-text",
        "reply": [{"kind": "arm"}, {"kind": "text", "text": "dale"}],
    },
    {
        "id": "second-ask-unavailable",
        "reply": [{"kind": "second_ask"}, {"kind": "text", "text": "dale"}],
    },
    {"id": "arm-then-timeout", "reply": [{"kind": "arm"}], "timeout_s": 0.0},
    {"id": "late-answer-ignored", "reply": [{"kind": "text", "text": "sí"}], "late": "no"},
]


def _apply(manager: Any, action: dict[str, Any]) -> Any:
    kind = action["kind"]
    if kind == "text":
        token_mode = action.get("token")
        token = None
        if token_mode == "current":
            token = manager.pending_token
        elif token_mode == "wrong":
            token = "token-atrasado"
        return manager.submit_text(action["text"], token)
    if kind == "click":
        manager.submit_click(bool(action["yes"]))
        return None
    if kind == "cancel":
        manager.cancel("el usuario habló de otra cosa")
        return None
    if kind == "arm":
        manager.arm()
        return None
    return None


def _ask_case(ConfirmationManager: Any, case: dict[str, Any]) -> dict[str, Any]:
    seen: list[list[Any]] = []
    nested: list[Any] = []
    tokens: list[bool] = []
    results: list[Any] = []
    holder: list[Any] = []

    def on_ask(summary: str, strong: bool) -> None:
        manager = holder[0]
        seen.append([summary, strong])
        tokens.append(manager.pending_token is not None)
        for action in case["reply"]:
            if action["kind"] == "second_ask":
                nested.append(manager.ask("otra cosa"))
                nested.append(manager.last_outcome)
                continue
            results.append(_apply(manager, action))

    manager = ConfirmationManager(
        accept=ACCEPT,
        reject=REJECT,
        timeout_s=case.get("timeout_s", 0.0),
        on_ask=on_ask,
    )
    holder.append(manager)
    accepted = manager.ask(
        SUMMARY,
        strong=case.get("strong", False),
        require_number=case.get("require_number"),
    )
    late = None
    if "late" in case:
        late = manager.submit_text(case["late"])
    return {
        "accepted": accepted,
        "seen": seen,
        "nested": nested,
        "results": results,
        "token_present": tokens,
        "last_outcome": manager.last_outcome,
        "pending": manager.pending,
        "pending_token_present": manager.pending_token is not None,
        "late": late,
    }


def _input_for(case: dict[str, Any]) -> dict[str, Any]:
    return {
        "summary": SUMMARY,
        "strong": case.get("strong", False),
        "require_number": case.get("require_number"),
        "reply": case["reply"],
        "timeout_s": case.get("timeout_s", 0.0),
    }


def _broken_ui_case(ConfirmationManager: Any) -> dict[str, Any]:
    """La UI revienta al plantear la pregunta: la confirmación sigue viva."""

    def boom(summary: str, strong: bool) -> None:
        raise RuntimeError("ui rota")

    manager = ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=5.0, on_ask=boom)
    outcome: list[bool] = []
    thread = threading.Thread(target=lambda: outcome.append(manager.ask(SUMMARY)), daemon=True)
    thread.start()
    deadline = time.monotonic() + 5
    while not manager.pending and time.monotonic() < deadline:
        time.sleep(0.001)
    manager.cancel("prueba")
    thread.join(5)
    return {
        "accepted": outcome[0] if outcome else None,
        "last_outcome": manager.last_outcome,
        "pending": manager.pending,
    }


def generate(out: FixtureFile) -> None:
    from ezagent.agent.confirm import ConfirmationManager

    for case in ASK_CASES:
        out.add(
            f"confirm/ask/{case['id']}",
            "ask",
            _input_for(case),
            _ask_case(ConfirmationManager, case),
        )

    out.add(
        "confirm/ask/ui-broken",
        "ask",
        {"summary": SUMMARY, "strong": False, "require_number": None, "on_ask": "raises"},
        _broken_ui_case(ConfirmationManager),
    )

    silent = ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=0.0)
    out.add(
        "confirm/ask/timeout-no-ui",
        "ask",
        {"summary": SUMMARY, "strong": False, "require_number": None, "timeout_s": 0.0},
        {
            "accepted": silent.ask(SUMMARY),
            "last_outcome": silent.last_outcome,
            "pending": silent.pending,
        },
    )

    idle = ConfirmationManager(accept=ACCEPT, reject=REJECT, timeout_s=0.0)
    out.add(
        "confirm/no-pending",
        "submit_text",
        {"text": "sí", "token": None},
        {
            "submitted": idle.submit_text("sí"),
            "clicked": idle.submit_click(True),
            "cancelled": idle.cancel("nada pendiente"),
            "last_outcome": idle.last_outcome,
            "pending": idle.pending,
        },
    )

    out.add(
        "confirm/constants",
        "module_constants",
        {},
        {"confirm_word": _constant("CONFIRM_WORD")},
    )


def _constant(name: str) -> Any:
    from ezagent.agent import confirm

    return getattr(confirm, name)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
