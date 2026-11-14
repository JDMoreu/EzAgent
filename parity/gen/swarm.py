"""Fixtures del enjambre (dominio `swarm`, F1-06).

Punto de entrada: textos de `protocol`, tablero y salud de `view`, avisos de
`progress`, clasificación de `incidents`, decisiones de `recovery` y
transcripciones del comando `ezs`. El resto vive en `swarm_board` (tablero y
volcado), `swarm_view`, `swarm_flow`, `swarm_ezs` y `swarm_extra`.

Normalizaciones: el id del enjambre, los tokens y el perfil temporal se escriben
como «<swarm>», «<token>» y «<tmp>», y las columnas de tiempo como segundos
respecto al reloj fijo del tablero.
"""

from __future__ import annotations

from typing import Any

import swarm_board as sb
from _common import FixtureFile, main_guard

DOMAIN = "swarm"

ROLES = ("plan", "arch", "lead", "dev", "back", "front", "ux", "data", "ops", "docs", "qa", "test", "sec")


# ----------------------------------------------------------------- protocol
def _protocol(out: FixtureFile) -> None:
    from ezagent.swarm import protocol

    for index, role in enumerate((*ROLES, "", "otro", "DEV", "plan ", "PLAN", "dev/qa", "rol con espacios"), start=1):
        out.add(
            f"swarm/protocol/role-label/{index:02d}",
            "protocol.role_label",
            {"role": role},
            {"text": protocol.role_label(role)},
        )

    stages = [(role, "") for role in ROLES] + [
        ("otro", "Migrar el núcleo"),
        ("", "Migrar el núcleo"),
        ("", ""),
        ("otro", ""),
        ("DEV", "Migrar"),
        ("dev", "Parte 2"),
        ("", "  "),
    ]
    for index, (role, title) in enumerate(stages, start=1):
        out.add(
            f"swarm/protocol/stage-name/{index:02d}",
            "protocol.stage_name",
            {"role": role, "title": title},
            {"text": protocol.stage_name(role, title)},
        )

    for index, role in enumerate(
        (*ROLES, "", "otro", "otro rol", "plan ", "PLAN", "dev/qa", "qa/qa"), start=1
    ):
        out.add(
            f"swarm/protocol/role-task/{index:02d}",
            "protocol.role_task",
            {"role": role},
            {"text": protocol.role_task(role)},
        )

    for index, roles in enumerate(
        [[], ["dev"], ["dev", "qa"], ["dev", "qa", "dev"], ["", "dev"], ["otro", "otro"],
         ["plan", "arch", "ux", "qa"], ["dev", "  dev  "]],
        start=1,
    ):
        out.add(
            f"swarm/protocol/default-tasks/{index:02d}",
            "protocol.default_tasks",
            {"roles": roles},
            {"tasks": protocol.default_tasks(roles)},
        )


def _briefing_cases() -> list[dict[str, Any]]:
    team = [sb.member("ana", "plan"), sb.member("bo", "dev", "opencode"), sb.member("cy", "qa", "codex")]
    two_devs = [sb.member("ana", "plan"), sb.member("bo", "dev"), sb.member("ca", "dev")]
    return [
        {"id": "plan", "goal": "Migrar el núcleo a Rust", "me": team[0], "members": team},
        {"id": "dev", "goal": "Migrar el núcleo a Rust", "me": team[1], "members": team},
        {"id": "qa", "goal": "Migrar el núcleo a Rust", "me": team[2], "members": team},
        {"id": "solo", "goal": "Ordenar el repo", "me": sb.member("ana"), "members": [sb.member("ana")]},
        {"id": "mismo-rol", "goal": "Dos devs", "me": two_devs[1], "members": two_devs},
        {"id": "unicode", "goal": "Revisar la «ñ» y el emoji 🙂", "me": team[0], "members": team},
        {"id": "otro-rol", "goal": "X", "me": sb.member("zo", "otro rol"),
         "members": [sb.member("zo", "otro rol")]},
        {"id": "ajeno", "goal": "X", "me": sb.member("zz", "qa", "codex"), "members": team},
        {"id": "mixto", "goal": "X", "me": team[0],
         "members": [sb.member("ana", "plan"), sb.member("bo", "")]},
    ] + [
        {"id": f"rol-{role}", "goal": "Migrar", "me": sb.member("ana", role),
         "members": [sb.member("ana", role)]}
        for role in ROLES
    ]


def _protocol_more(out: FixtureFile) -> None:
    from ezagent.swarm import protocol

    for case in _briefing_cases():
        out.add(
            f"swarm/protocol/briefing/{case['id']}",
            "protocol.briefing",
            {
                "swarm": {"id": "abc12345", "goal": case["goal"], "status": "active",
                          "cwd": "C:/proyecto"},
                "me": case["me"].name,
                "members": [m.name for m in case["members"]],
                "command": "ezs",
            },
            {
                "text": protocol.briefing(
                    sb.info(case["goal"]), case["me"], case["members"], command="ezs"
                )
            },
        )

    _protocol_lines(out)


def _protocol_lines(out: FixtureFile) -> None:
    from ezagent.swarm import protocol

    for index, command in enumerate(["ezs", "", "agente-x"], start=1):
        out.add(
            f"swarm/protocol/help/{index:02d}",
            "protocol.help_text",
            {"command": command},
            {"text": protocol.help_text(command)},
        )

    for index, (command, role) in enumerate(
        [(command, "dev") for command in ("ezs", "", "agente-x")]
        + [("ezs", role) for role in ROLES]
        + [("", "qa"), ("agente-x", "plan"), ("ezs", ""), ("", "")],
        start=1,
    ):
        out.add(
            f"swarm/protocol/join/{index:02d}",
            "protocol.join_line",
            {"command": command, "name": "bo", "role": role},
            {"text": protocol.join_line(sb.info(), sb.member("bo", role), command=command)},
        )

    for index, tasks in enumerate([[1], [1, 2, 3], [], [12]], start=1):
        out.add(
            f"swarm/protocol/open-task/{index:02d}",
            "protocol.open_task_line",
            {"tasks": tasks},
            {"text": protocol.open_task_line(tasks, command="ezs")},
        )

    for index, (unread, ready) in enumerate(
        [(0, 0), (1, 0), (0, 1), (2, 3), (1, 1), (5, 0), (2, 2), (10, 0), (0, 10), (3, 4),
         (1, 5), (12, 3), (4, 4), (7, 1), (1, 7), (100, 100), (0, 2), (2, 0), (9, 9), (6, 0)],
        start=1,
    ):
        out.add(
            f"swarm/protocol/wake/{index:02d}",
            "protocol.wake_line",
            {"unread": unread, "ready": ready},
            {"text": protocol.wake_line(unread, ready, command="ezs")},
        )

    out.add(
        "swarm/protocol/constants",
        "protocol.constants",
        {},
        {
            "idle": protocol.IDLE,
            "all_done": protocol.ALL_DONE,
            "stopped": protocol.STOPPED,
            "paused": protocol.PAUSED,
            "no_seat": protocol.NO_SEAT,
            "final_offer": protocol.FINAL_OFFER,
            "role_labels": protocol.ROLE_LABELS,
            "stage_names": protocol.STAGE_NAMES,
        },
    )


def generate(out: FixtureFile) -> None:
    import os
    import pathlib

    import swarm_extra
    import swarm_ezs
    import swarm_flow
    import swarm_view

    home = pathlib.Path(os.environ["HOME"])
    _protocol(out)
    _protocol_more(out)
    swarm_view.cases(out, home)
    swarm_flow.cases(out)
    swarm_ezs.cases(out, home)
    swarm_extra.generate(out, home)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
