"""Fixtures de tareas pendientes y actividad de agentes (F1-07).

Dominio `tasks_activity`. Cubre:

- `src/ezagent/voice/tasks.py` (`TaskRegistry`, `_prompt_value`)
- `src/ezagent/voice/activity.py` (`ActivityItem`, `ActivityLog`, `order`,
  `merge`, `orbit`, `live_items`, `snapshot`)

Los escenarios de `TaskRegistry` operan sobre un `tasks.json` temporal y
devuelven el resultado de cada operación, el estado y el **texto exacto** del
archivo. `live_items` recibe la foto de la app como datos (tareas vigiladas,
estados, etiquetas, cadenas y enjambres) y no toca Herdr ni el disco.

El reloj está congelado en `2025-01-01T00:00:00Z`; los relojes inyectables
(actividad) parten de instantes fijos declarados en cada caso.
"""

from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tasks_activity"


def _norm(value: Any, home: Path) -> Any:
    if isinstance(value, str):
        value = value.replace("\\\\", "/")
        for prefix in (home.as_posix(), str(home)):
            value = value.replace(prefix, "<tmp>")
        return value.replace("\\", "/")
    if isinstance(value, dict):
        return {key: _norm(item, home) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_norm(item, home) for item in value]
    return value


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def _file_text(path: Path) -> Any:
    if path.is_dir():
        return "<dir>"
    if not path.exists():
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2) + "\n"


# ------------------------------------------------------------------ tareas


class _TaskRun:
    def __init__(self, out: FixtureFile, home: Path, case: str, seed: str | None) -> None:
        from ezagent.voice.tasks import TaskRegistry

        self.out = out
        self.case = case
        self.seed = seed
        self.work = home / "task-cases" / case
        self.path = self.work / "tasks.json"
        if seed == "<dir>":
            self.path.mkdir(parents=True, exist_ok=True)
        elif seed is not None:
            _write(self.path, seed)
        self.registry = TaskRegistry(self.path)
        self.ops: list[dict[str, Any]] = []
        self.results: list[Any] = []

    def run(self, op: dict[str, Any]) -> Any:
        from ezagent.voice.tasks import TaskRegistry

        self.ops.append(op)
        action = op["op"]
        registry = self.registry
        try:
            if action == "add":
                return registry.add(
                    op.get("address", ""),
                    op.get("agent", ""),
                    op.get("task", ""),
                    created_at=op.get("created_at"),
                )
            if action == "get":
                return registry.get(op.get("address", ""))
            if action == "remove":
                return registry.remove(op.get("address", ""))
            if action == "clear":
                registry.clear()
                return True
            if action == "all":
                return registry.all()
            if action == "mutate_all":
                items = registry.all()
                if items:
                    items[0]["task"] = "mutado"
                return {"mutated": items, "after": registry.all()}
            if action == "add_chain":
                return registry.add_chain(op.get("spec"))
            if action == "chains":
                return registry.chains()
            if action == "chains_for":
                return registry.chains_for(op.get("source", ""))
            if action == "remove_chains_for":
                return registry.remove_chains_for(op.get("source", ""))
            if action == "remove_chain":
                return registry.remove_chain(op.get("spec") or {})
            if action == "prompt":
                return registry.prompt_block()
            if action == "reload":
                self.registry = TaskRegistry(self.path)
                return {"tasks": self.registry.all(), "chains": self.registry.chains()}
            if action == "write_raw":
                _write(self.path, op.get("text", ""))
                return True
            raise ValueError(f"operación desconocida: {action}")
        except Exception as error:
            return {"error": f"{type(error).__name__}: {error}"}

    def finish(self) -> None:
        payload = {
            "results": self.results,
            "tasks": self.registry.all(),
            "chains": self.registry.chains(),
            "prompt": self.registry.prompt_block(),
            "file": _file_text(self.path),
        }
        self.out.add(
            f"tasks_activity/tasks/{self.case}",
            "TaskRegistry",
            {"seed": self.seed, "ops": self.ops},
            _norm(payload, self.work.parent.parent),
        )


def _task_scenario(
    out: FixtureFile, home: Path, case: str, ops: list[dict[str, Any]], seed: Any = None
) -> None:
    text = seed if isinstance(seed, str) or seed is None else _json(seed)
    runner = _TaskRun(out, home, case, text)
    runner.results = [runner.run(op) for op in ops]
    runner.finish()


TASK_CASES: list[tuple[str, Any, list[dict[str, Any]]]] = [
    ("empty", None, [{"op": "all"}, {"op": "chains"}, {"op": "prompt"}]),
    (
        "add-get-remove",
        None,
        [
            {
                "op": "add",
                "address": "t1",
                "agent": "claude",
                "task": "  plan  ",
                "created_at": 10.0,
            },
            {"op": "add", "address": "t2", "agent": "codex"},
            {"op": "get", "address": " t1 "},
            {"op": "get", "address": ""},
            {"op": "all"},
            {"op": "remove", "address": "t1"},
            {"op": "remove", "address": "t1"},
            {"op": "get", "address": "t1"},
        ],
    ),
    (
        "add-replaces",
        None,
        [
            {"op": "add", "address": "t1", "task": "uno", "created_at": 1.0},
            {"op": "add", "address": "t1", "task": "dos", "created_at": 2.0},
            {"op": "all"},
            {"op": "get", "address": "t1"},
            {"op": "mutate_all"},
        ],
    ),
    (
        "add-sorted",
        None,
        [
            {"op": "add", "address": "b", "created_at": 20.0},
            {"op": "add", "address": "a", "created_at": 10.0},
            {"op": "add", "address": "c", "created_at": 10.0},
            {"op": "all"},
        ],
    ),
    (
        "clear",
        None,
        [
            {"op": "clear"},
            {"op": "add", "address": "t1", "created_at": 1.0},
            {"op": "add_chain", "spec": {"source": "t1", "agent": "a"}},
            {"op": "clear"},
            {"op": "all"},
            {"op": "chains"},
        ],
    ),
    (
        "add-blank",
        None,
        [
            {"op": "add", "address": "", "task": "x"},
            {"op": "add", "address": "   ", "task": "x"},
            {"op": "add", "address": None, "task": "x"},
        ],
    ),
    (
        "add-created-invalid",
        None,
        [{"op": "add", "address": "t1", "created_at": "x"}],
    ),
    (
        "add-coercions",
        None,
        [
            {"op": "add", "address": "  t1  ", "agent": None, "task": None, "created_at": "12.5"},
            {"op": "add", "address": "t2", "agent": 5, "task": 7, "created_at": 0},
        ],
    ),
    (
        "chains",
        None,
        [
            {
                "op": "add_chain",
                "spec": {"source": "t1", "agent": "claude", "task": " plan ", "name": "n1"},
            },
            {"op": "add_chain", "spec": {"source": "t1", "agent": "codex", "created_at": 5.0}},
            {"op": "add_chain", "spec": {"source": "t2", "agent": "pi", "extra": {"x": 1}}},
            {"op": "chains"},
            {"op": "chains_for", "source": " t1 "},
            {"op": "chains_for", "source": ""},
        ],
    ),
    (
        "add-chain-invalid",
        None,
        [
            {"op": "add_chain", "spec": "texto"},
            {"op": "add_chain", "spec": {}},
            {"op": "add_chain", "spec": {"source": " ", "agent": "a"}},
            {"op": "add_chain", "spec": {"source": "s", "agent": " "}},
            {
                "op": "add_chain",
                "spec": {"source": "s", "agent": "a", "created_at": "x", "task": None},
            },
        ],
    ),
    (
        "remove-chains-for",
        None,
        [
            {"op": "add_chain", "spec": {"source": "t1", "agent": "a", "created_at": 1.0}},
            {"op": "add_chain", "spec": {"source": "t1", "agent": "b", "created_at": 2.0}},
            {"op": "add_chain", "spec": {"source": "t2", "agent": "c", "created_at": 3.0}},
            {"op": "remove_chains_for", "source": "t1"},
            {"op": "remove_chains_for", "source": "t1"},
            {"op": "remove_chains_for", "source": ""},
            {"op": "chains"},
        ],
    ),
    (
        "remove-chain",
        None,
        [
            {"op": "add_chain", "spec": {"source": "t1", "agent": "a", "created_at": 1.0}},
            {"op": "add_chain", "spec": {"source": "t1", "agent": "b", "created_at": 2.0}},
            {"op": "remove_chain", "spec": {"source": "t1", "agent": "a", "created_at": 1.0}},
            {"op": "remove_chain", "spec": {"source": "t1", "agent": "a", "created_at": 1.0}},
            {"op": "remove_chain", "spec": {}},
            {"op": "chains"},
        ],
    ),
    (
        "prompt-basic",
        None,
        [
            {
                "op": "add",
                "address": "t1",
                "agent": "claude",
                "task": "plan <b> & fin",
                "created_at": 1.0,
            },
            {"op": "add", "address": "t2", "agent": "", "task": "", "created_at": 2.0},
            {"op": "add_chain", "spec": {"source": "t1", "agent": "codex", "task": "revisar"}},
            {"op": "prompt"},
        ],
    ),
    (
        "prompt-many",
        None,
        [
            {
                "op": "add",
                "address": f"t{index}",
                "agent": "claude",
                "task": f"tarea {index}",
                "created_at": float(index),
            }
            for index in range(1, 16)
        ]
        + [
            {
                "op": "add_chain",
                "spec": {"source": f"t{index}", "agent": "codex", "created_at": float(index)},
            }
            for index in range(1, 10)
        ]
        + [{"op": "prompt"}],
    ),
    (
        "prompt-html",
        None,
        [
            {
                "op": "add",
                "address": "<t&1>",
                "agent": "a<b>",
                "task": "x > y & z",
                "created_at": 1.0,
            },
            {"op": "prompt"},
        ],
    ),
    (
        "prompt-truncated",
        None,
        [
            {
                "op": "add",
                "address": "d" * 200,
                "agent": "g" * 200,
                "task": "t" * 300,
                "created_at": 1.0,
            },
            {"op": "prompt"},
        ],
    ),
    (
        "legacy-list",
        [
            {"address": "t1", "agent": "claude", "task": "uno", "created_at": 1.0},
            {"address": "t2", "agent": "codex", "task": "dos", "created_at": 2.0},
        ],
        [{"op": "all"}, {"op": "prompt"}],
    ),
    (
        "legacy-dict",
        {
            "t1": {"address": "t1", "agent": "claude", "task": "uno", "created_at": 1.0},
            "t2": {"address": "t2", "agent": "codex", "task": "dos", "created_at": 2.0},
        },
        [{"op": "all"}],
    ),
    (
        "legacy-invalid",
        {
            "tasks": [
                "texto",
                None,
                {"address": ""},
                {"address": "  "},
                {"address": 5, "agent": 7, "task": 9, "created_at": "12.5"},
                {"address": "t2", "created_at": "x"},
                {"address": "t2", "agent": "gana", "task": "dup", "created_at": 0},
            ],
            "chains": [
                "texto",
                None,
                {},
                {"source": "", "agent": "a"},
                {"source": "s", "agent": ""},
                {"source": 5, "agent": 7, "created_at": "x", "extra": "queda"},
            ],
        },
        [{"op": "all"}, {"op": "chains"}, {"op": "prompt"}],
    ),
    (
        "invalid-json",
        "{no es json",
        [{"op": "all"}, {"op": "chains"}, {"op": "prompt"}, {"op": "add", "address": "t1"}],
    ),
    (
        "path-is-directory",
        "<dir>",
        [{"op": "all"}, {"op": "add", "address": "t1", "created_at": 1.0}],
    ),
    (
        "wrong-types",
        {"tasks": "no", "chains": "tampoco"},
        [{"op": "all"}, {"op": "chains"}],
    ),
    (
        "reload",
        None,
        [
            {"op": "add", "address": "t1", "created_at": 1.0},
            {"op": "add_chain", "spec": {"source": "t1", "agent": "a", "created_at": 2.0}},
            {"op": "reload"},
            {"op": "all"},
            {"op": "chains"},
        ],
    ),
    (
        "external-edit",
        None,
        [
            {"op": "add", "address": "t1", "created_at": 1.0},
            {
                "op": "write_raw",
                "text": _json({"tasks": [{"address": "t9", "created_at": 9.0}], "chains": []}),
            },
            {"op": "reload"},
            {"op": "all"},
        ],
    ),
    (
        "prompt-after-clear",
        {"tasks": [{"address": "t1"}]},
        [{"op": "prompt"}, {"op": "clear"}, {"op": "prompt"}],
    ),
]

PROMPT_VALUE_CASES: list[dict[str, Any]] = [
    {"value": None},
    {"value": ""},
    {"value": 0},
    {"value": False},
    {"value": True},
    {"value": "  hola \n mundo  "},
    {"value": "<b>&"},
    {"value": "x" * 220},
    {"value": "x" * 221},
    {"value": "ñ" * 221},
    {"value": "a\x0bb"},
    {"value": "abcdefghij", "limit": 5},
    {"value": "abcdefghij", "limit": 1},
    {"value": "abcdefghij", "limit": 0},
    {"value": " < > & ", "limit": 220},
]


def _task_pure(out: FixtureFile, tasks_module: Any) -> None:
    for index, case in enumerate(PROMPT_VALUE_CASES, start=1):
        out.add(
            f"tasks_activity/prompt_value/{index:03d}",
            "prompt_value",
            case,
            {"text": tasks_module._prompt_value(case["value"], case.get("limit", 220))},
        )
    out.add(
        "tasks_activity/constants",
        "module_constants",
        {},
        {"task_registry": tasks_module.TaskRegistry.__name__},
    )


# ---------------------------------------------------------------- actividad


def _item(data: dict[str, Any]) -> Any:
    from ezagent.voice.activity import ActivityItem

    return ActivityItem(
        key=data.get("key", ""),
        title=data.get("title", ""),
        agent=data.get("agent", ""),
        state=data.get("state", ""),
        since=data.get("since", 0.0),
        unseen=data.get("unseen", False),
        live=data.get("live", False),
    )


def _item_dict(item: Any) -> dict[str, Any]:
    return {
        "key": item.key,
        "title": item.title,
        "agent": item.agent,
        "state": item.state,
        "since": item.since,
        "unseen": item.unseen,
        "live": item.live,
        "label": item.label,
        "marked": item.marked,
    }


CHAIN_SPECS: list[dict[str, Any]] = [
    {"source": "t1", "agent": "claude", "created_at": 12.5},
    {"source": "", "agent": "", "created_at": 0},
    {},
    {"source": 5, "agent": None, "created_at": "x"},
    {"source": "ñ", "agent": "a b", "created_at": -1.5},
]

TITLE_INPUTS: list[Any] = [
    "",
    "   ",
    "  hola   mundo ",
    "x" * 119,
    "x" * 120,
    "x" * 121,
    "ñ" * 121,
    "línea\nnueva\ttab",
    None,
    5,
]

ORDER_ITEMS: list[list[dict[str, Any]]] = [
    [],
    [
        {"key": "done", "state": "done", "since": 30, "unseen": True},
        {"key": "working", "state": "working", "since": 10, "unseen": False},
        {"key": "needs", "state": "needs_you", "since": 5, "unseen": True},
        {"key": "failed", "state": "failed", "since": 20, "unseen": False},
        {"key": "waiting", "state": "waiting", "since": 15, "unseen": True},
        {"key": "stale", "state": "stale", "since": 25, "unseen": False},
    ],
    [
        {"key": "a", "state": "working", "since": 1, "unseen": False},
        {"key": "b", "state": "working", "since": 2, "unseen": False},
        {"key": "c", "state": "working", "since": 2, "unseen": True},
        {"key": "d", "state": "desconocido", "since": 3, "unseen": True},
    ],
]

MERGE_CASES: list[dict[str, Any]] = [
    {"live": [], "recent": []},
    {
        "live": [{"key": "a", "state": "working", "live": True, "since": 5}],
        "recent": [{"key": "a", "state": "done", "unseen": True, "since": 1}],
    },
    {
        "live": [{"key": "a", "state": "working", "live": True, "since": 5}],
        "recent": [
            {"key": "b", "state": "failed", "unseen": True, "since": 1},
            {"key": "c", "state": "done", "unseen": False, "since": 2},
        ],
    },
]

ORBIT_CASES: list[dict[str, Any]] = [
    {"items": [], "limit": 5},
    {
        "items": [
            {"key": "live", "state": "working", "live": True},
            {"key": "unseen", "state": "failed", "unseen": True},
            {"key": "quiet", "state": "done", "unseen": False},
            {"key": "both", "state": "needs_you", "unseen": True, "live": True},
        ],
        "limit": 5,
    },
    {
        "items": [{"key": f"k{index}", "state": "working", "live": True} for index in range(7)],
        "limit": 2,
    },
    {"items": [{"key": "k", "state": "working", "live": True}], "limit": 0},
    {"items": [{"key": "k", "state": "working", "live": True}], "limit": -1},
]

DISMISSIBLE_CASES: list[dict[str, Any]] = [
    {"key": "swarm:abc", "live": True},
    {"key": "swarm:abc", "live": False, "unseen": True},
    {"key": "t51", "live": True},
    {"key": "chain:s:a:1", "live": True},
    {"key": "", "live": False},
]


def _activity_pure(out: FixtureFile, activity: Any) -> None:
    out.add(
        "tasks_activity/activity/constants",
        "module_constants",
        {},
        {
            "states": [
                activity.NEEDS_YOU,
                activity.FAILED,
                activity.WORKING,
                activity.WAITING,
                activity.STALE,
                activity.DONE,
            ],
            "state_order": dict(activity.STATE_ORDER),
            "state_label": dict(activity.STATE_LABEL),
            "title_limit": activity.TITLE_LIMIT,
            "keep_secs": activity.KEEP_SECS,
            "log_limit": activity.LOG_LIMIT,
            "orbit_limit": activity.ORBIT_LIMIT,
        },
    )
    for index, spec in enumerate(CHAIN_SPECS, start=1):
        out.add(
            f"tasks_activity/chain_key/{index:03d}",
            "chain_key",
            {"spec": spec},
            {"key": activity.chain_key(spec)},
        )
    for index, case in enumerate(DISMISSIBLE_CASES, start=1):
        out.add(
            f"tasks_activity/dismissible/{index:03d}",
            "dismissible",
            case,
            {"dismissible": activity.dismissible(_item(case))},
        )
    for index, text in enumerate(TITLE_INPUTS, start=1):
        out.add(
            f"tasks_activity/clean_title/{index:03d}",
            "clean_title",
            {"text": text},
            {"title": activity.clean_title(text)},
        )
        out.add(
            f"tasks_activity/clean_title/{index:03d}/fallback",
            "clean_title",
            {"text": text, "fallback": "Enjambre"},
            {"title": activity.clean_title(text, "Enjambre")},
        )
    for index, items in enumerate(ORDER_ITEMS, start=1):
        out.add(
            f"tasks_activity/order/{index:03d}",
            "order",
            {"items": items},
            {
                "items": [
                    _item_dict(item) for item in activity.order([_item(data) for data in items])
                ]
            },
        )
    for index, case in enumerate(MERGE_CASES, start=1):
        merged = activity.merge(
            [_item(data) for data in case["live"]], [_item(data) for data in case["recent"]]
        )
        out.add(
            f"tasks_activity/merge/{index:03d}",
            "merge",
            case,
            {"items": [_item_dict(item) for item in merged]},
        )
    for index, case in enumerate(ORBIT_CASES, start=1):
        out.add(
            f"tasks_activity/orbit/{index:03d}",
            "orbit",
            case,
            {
                "states": list(
                    activity.orbit([_item(data) for data in case["items"]], case["limit"])
                )
            },
        )


LOG_CASES: list[tuple[str, float, list[dict[str, Any]]]] = [
    (
        "reasons",
        1000.0,
        [
            {"op": "record", "key": "a", "agent": "claude", "title": "  uno  ", "reason": "done"},
            {"op": "record", "key": "b", "agent": "codex", "title": "dos", "reason": "idle"},
            {"op": "record", "key": "c", "agent": "", "title": "tres", "reason": "blocked"},
            {"op": "record", "key": "d", "agent": "pi", "title": "cuatro", "reason": "error"},
            {"op": "record", "key": "e", "agent": "pi", "title": "cinco", "reason": "lost"},
            {"op": "record", "key": "f", "agent": "pi", "title": "seis", "reason": "otro"},
            {"op": "items"},
        ],
    ),
    (
        "replace-same-key",
        1000.0,
        [
            {"op": "record", "key": "a", "title": "primera", "reason": "done"},
            {"op": "record", "key": "b", "title": "segunda", "reason": "done"},
            {"op": "at", "now": 1001.0},
            {"op": "record", "key": "a", "title": "primera otra vez", "reason": "failed"},
            {"op": "items"},
        ],
    ),
    (
        "limit",
        1000.0,
        [
            {"op": "record", "key": f"k{index}", "title": f"t{index}", "reason": "done"}
            for index in range(10)
        ]
        + [{"op": "items"}],
    ),
    (
        "expiry",
        1000.0,
        [
            {"op": "record", "key": "visto", "title": "visto", "reason": "done"},
            {"op": "record", "key": "nuevo", "title": "nuevo", "reason": "done"},
            {"op": "seen_all"},
            {"op": "at", "now": 2800.0},
            {"op": "items"},
            {"op": "at", "now": 2801.0},
            {"op": "items"},
        ],
    ),
    (
        "unseen-never-expires",
        1000.0,
        [
            {"op": "record", "key": "a", "title": "a", "reason": "done"},
            {"op": "at", "now": 100000.0},
            {"op": "items"},
        ],
    ),
    (
        "seen-forget",
        1000.0,
        [
            {"op": "record", "key": "a", "title": "a", "reason": "done"},
            {"op": "seen_all"},
            {"op": "seen_all"},
            {"op": "forget", "key": "a"},
            {"op": "forget", "key": "a"},
            {"op": "items"},
        ],
    ),
    (
        "title-clean",
        1000.0,
        [
            {"op": "record", "key": "a", "title": "  " + "x" * 200, "reason": "done"},
            {"op": "record", "key": "b", "title": "   ", "reason": "done"},
        ],
    ),
]


def _activity_log(out: FixtureFile, home: Path, activity: Any) -> None:
    for case, start, ops in LOG_CASES:
        now = [start]
        log = activity.ActivityLog(clock=lambda now=now: now[0])
        results: list[Any] = []
        for op in ops:
            if op["op"] == "at":
                now[0] = op["now"]
                results.append(None)
            elif op["op"] == "record":
                results.append(
                    _item_dict(
                        log.record(op["key"], op.get("agent", ""), op["title"], op["reason"])
                    )
                )
            elif op["op"] == "seen_all":
                results.append(log.seen_all())
            elif op["op"] == "forget":
                log.forget(op["key"])
                results.append(None)
            elif op["op"] == "items":
                results.append([_item_dict(item) for item in log.items()])
            else:  # pragma: no cover - el oráculo debe registrar el fallo
                raise ValueError(f"operación desconocida: {op['op']}")
        out.add(
            f"tasks_activity/log/{case}",
            "ActivityLog",
            {"start": start, "ops": ops},
            _norm(
                {
                    "results": results,
                    "items": [_item_dict(item) for item in log.items()],
                    "keys": [item.key for item in log.items()],
                },
                home,
            ),
        )


class _TasksView:
    def __init__(self, data: dict[str, Any], raise_get: bool, raise_chains: bool) -> None:
        self.data = data
        self.raise_get = raise_get
        self.raise_chains = raise_chains

    def get(self, address: str) -> Any:
        if self.raise_get:
            raise RuntimeError("tasks.get roto")
        return self.data.get(address)

    def chains(self) -> list[dict[str, Any]]:
        if self.raise_chains:
            raise RuntimeError("tasks.chains roto")
        return self.data.get("__chains__", [])


class _WatcherView:
    def __init__(self, states: dict[str, Any]) -> None:
        self.states = states

    def state_of(self, address: str) -> Any:
        return self.states.get(address, "working")


class _HerdrView:
    def __init__(self, labels: dict[str, Any]) -> None:
        self.labels = labels

    def label_for(self, address: str) -> Any:
        return self.labels.get(address, "")


def _live_app(case: dict[str, Any]) -> Any:
    tasks_data = dict(case.get("tasks") or {})
    tasks_data["__chains__"] = case.get("chains") or []
    raise_flags = case.get("raise") or {}
    if raise_flags.get("watched"):

        class _RaisingApp(SimpleNamespace):
            @property
            def watched(self) -> Any:
                raise RuntimeError("watched roto")

        app: Any = _RaisingApp()
    else:
        app = SimpleNamespace(watched=tuple(case.get("watched") or ()))
    app.tasks = (
        None
        if case.get("tasks") is None
        else _TasksView(tasks_data, bool(raise_flags.get("get")), bool(raise_flags.get("chains")))
    )
    app.watcher = None if case.get("watcher") is False else _WatcherView(case.get("states") or {})
    app.herdr = None if case.get("herdr") is False else _HerdrView(case.get("labels") or {})
    app.swarm = None
    if case.get("swarm") is not None:
        app.swarm = SimpleNamespace(
            supervisor=SimpleNamespace(active_view=[tuple(pair) for pair in case["swarm"]])
        )
    return app


LIVE_CASES: list[dict[str, Any]] = [
    {"watched": [], "tasks": {}, "states": {}, "labels": {}, "chains": [], "now": 1000.0},
    {
        "watched": ["t1", "t2", "t3", "t4", "t5"],
        "tasks": {
            "t1": {"agent": "claude", "created_at": 10.0},
            "t2": {"agent": "codex", "created_at": 0.0},
            "t3": {"agent": "", "created_at": 30.0},
            "t4": {"agent": "pi", "created_at": 40.0},
            "t5": {"agent": "claude", "created_at": 50.0},
        },
        "states": {"t1": "working", "t2": "blocked", "t3": "error", "t4": "zzz", "t5": "idle"},
        "labels": {"t1": "Claude Code de EzAgent", "t2": "", "t3": 5},
        "chains": [
            {"source": "t1", "agent": "codex", "name": " etapa", "created_at": 100.0},
            {"source": "t2", "agent": "pi", "task": "revisar", "created_at": 110.0},
            {"source": "t3", "agent": "claude", "created_at": 120.0},
            {"source": "t4", "agent": "", "created_at": 0.0},
        ],
        "swarm": [["abc12345", "Migrar todo"], ["def", ""]],
        "now": 1000.0,
    },
    {
        "watched": ["t1"],
        "tasks": {"t1": {"agent": "claude"}},
        "watcher": False,
        "herdr": False,
        "states": {},
        "labels": {},
        "now": 5.0,
    },
    {
        "watched": ["t1"],
        "tasks": {"t1": {"agent": "claude", "created_at": 7.0}},
        "watcher": False,
        "herdr": False,
        "now": 7.0,
    },
    {
        "watched": ["t1"],
        "tasks": None,
        "states": {"t1": "working"},
        "labels": {"t1": "etiqueta"},
        "now": 1.0,
    },
    {
        "watched": ["t1", "t2"],
        "tasks": {"t1": {"agent": "claude"}},
        "states": {"t1": "working", "t2": "done"},
        "labels": {},
        "raise": {"get": True, "chains": True},
        "chains": [{"source": "x", "agent": "a"}],
        "now": 2.0,
    },
    {
        "watched": ["t1"],
        "tasks": {"t1": {"agent": "claude"}},
        "states": {"t1": "working"},
        "labels": {},
        "raise": {"watched": True},
        "now": 3.0,
    },
    {
        "watched": ["t1"],
        "tasks": {"t1": {"agent": "claude", "created_at": 4.0}},
        "states": {"t1": "working"},
        "labels": {"t1": "<b>&"},
        "chains": [{"source": "s", "agent": "a", "name": "x" * 200}],
        "swarm": [],
        "now": None,
    },
]


def _activity_live(out: FixtureFile, home: Path, activity: Any) -> None:
    for index, case in enumerate(LIVE_CASES, start=1):
        app = _live_app(case)
        items = activity.live_items(app, case.get("now"))
        out.add(
            f"tasks_activity/live/{index:03d}",
            "live_items",
            case,
            _norm({"items": [_item_dict(item) for item in items]}, home),
        )
    app = _live_app(LIVE_CASES[1])
    log = activity.ActivityLog(clock=lambda: 900.0)
    log.record("t1", "claude", "reciente", "done")
    log.record("chain:t2:pi:110.0", "pi", "cadena reciente", "done")
    log.record("t9", "gemini", "solo en el registro", "lost")
    merged = activity.snapshot(app, log)
    out.add(
        "tasks_activity/snapshot",
        "snapshot",
        {
            "app": LIVE_CASES[1],
            "log": [
                ["t1", "claude", "reciente", "done"],
                ["chain:t2:pi:110.0", "pi", "cadena reciente", "done"],
                ["t9", "gemini", "solo en el registro", "lost"],
            ],
            "log_clock": 900.0,
        },
        _norm({"items": [_item_dict(item) for item in merged]}, home),
    )
    out.add(
        "tasks_activity/snapshot/no-log",
        "snapshot",
        {"app": {}, "log": None},
        _norm(
            {"items": [_item_dict(item) for item in activity.snapshot(SimpleNamespace(), None)]},
            home,
        ),
    )


def generate(out: FixtureFile) -> None:
    import os

    from ezagent.voice import activity, tasks

    home = Path(os.environ["HOME"])
    _task_pure(out, tasks)
    for case, seed, ops in TASK_CASES:
        _task_scenario(out, home, case, ops, seed)
    _activity_pure(out, activity)
    _activity_log(out, home, activity)
    _activity_live(out, home, activity)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
