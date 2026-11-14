"""Transcripciones del comando `ezs` (dominio `swarm`, F1-06).

Cada caso parte de un tablero declarado, ejecuta un comando con el token de un
miembro y guarda la salida exacta (stdout/stderr y código) más el volcado
normalizado de `board.db`.
"""

from __future__ import annotations

from typing import Any

import swarm_board as sb
from swarm_board import TEAM

# --------------------------------------------------------------- ezs (cli)
PLAIN = {"goal": "Migrar el núcleo a Rust", "ops": list(TEAM)}
DEV_TASK = {
    "goal": "Migrar el núcleo a Rust",
    "ops": [*TEAM, {"op": "task", "title": "Portar el núcleo", "role": "dev"},
            {"op": "task", "title": "Probar el núcleo", "role": "qa", "deps": [1]}],
}
TRANSCRIPTS: list[dict[str, Any]] = [
    {"id": "next-primero", "board": DEV_TASK, "token_of": "bo", "argv": ["next"]},
    {"id": "next-otra-vez", "board": DEV_TASK, "token_of": "bo", "argv": ["next"],
     "before": [["next"]]},
    {"id": "next-sin-tareas", "board": PLAIN, "token_of": "bo", "argv": ["next"]},
    {"id": "next-todo-hecho", "board": {"goal": "X", "ops": [*TEAM,
                                                             {"op": "task", "title": "Portar", "role": "dev"},
                                                             {"op": "claim", "member": "bo", "task": 1},
                                                             {"op": "finish", "member": "bo", "task": 1,
                                                              "result": "ok"}]},
     "token_of": "bo", "argv": ["next"]},
    {"id": "next-all", "board": {"goal": "X", "ops": [*TEAM,
                                                      {"op": "task", "title": "Lexer", "role": "dev"},
                                                      {"op": "task", "title": "Parser", "role": "dev"}]},
     "token_of": "bo", "argv": ["next", "--all"]},
    {"id": "next-pausado", "board": {"goal": "X", "status": "paused", "ops": list(TEAM)},
     "token_of": "bo", "argv": ["next"]},
    {"id": "next-detenido", "board": {"goal": "X", "status": "stopped", "ops": list(TEAM)},
     "token_of": "bo", "argv": ["next"]},
    {"id": "next-sin-asiento", "board": PLAIN, "token_of": "bo", "argv": ["next"], "bad_token": True},
    {"id": "next-dependencias", "board": DEV_TASK, "token_of": "bo", "argv": ["next"]},
    {"id": "done", "board": DEV_TASK, "token_of": "bo", "argv": ["done", "listo en una línea"],
     "before": [["next"]]},
    {"id": "done-con-t", "board": {"goal": "X", "ops": [*TEAM,
                                                        {"op": "task", "title": "Lexer", "role": "dev"},
                                                        {"op": "task", "title": "Parser", "role": "dev"},
                                                        {"op": "claim", "member": "bo", "task": 1},
                                                        {"op": "claim", "member": "bo", "task": 2}]},
     "token_of": "bo", "argv": ["done", "T2", "parser listo"]},
    {"id": "done-sin-resumen", "board": DEV_TASK, "token_of": "bo", "argv": ["done"],
     "before": [["next"]]},
    {"id": "done-sin-tareas", "board": DEV_TASK, "token_of": "bo", "argv": ["done", "nada"]},
    {"id": "done-unicode", "board": DEV_TASK, "token_of": "bo",
     "argv": ["done", "quedó la «ñ» y el emoji 🙂"], "before": [["next"]]},
    {"id": "fail", "board": DEV_TASK, "token_of": "bo", "argv": ["fail", "no puedo seguir"],
     "before": [["next"]]},
    {"id": "fail-sin-motivo", "board": DEV_TASK, "token_of": "bo", "argv": ["fail"],
     "before": [["next"]]},
    {"id": "fix", "board": {"goal": "X", "ops": [*TEAM,
                                                 {"op": "task", "title": "Portar", "role": "dev"},
                                                 {"op": "claim", "member": "bo", "task": 1},
                                                 {"op": "finish", "member": "bo", "task": 1,
                                                  "result": "ok"}]},
     "token_of": "cy", "argv": ["fix", "T1", "falta la prueba"]},
    {"id": "fix-uso", "board": PLAIN, "token_of": "cy", "argv": ["fix"]},
    {"id": "fix-rondas", "board": {"goal": "X", "ops": [
        *TEAM, {"op": "task", "title": "Portar", "role": "dev"},
        {"op": "claim", "member": "bo", "task": 1},
        {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
        {"op": "reopen", "task": 1, "by": "cy", "note": "r1"},
        {"op": "claim", "member": "bo", "task": 1},
        {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
        {"op": "reopen", "task": 1, "by": "cy", "note": "r2"},
        {"op": "claim", "member": "bo", "task": 1},
        {"op": "finish", "member": "bo", "task": 1, "result": "ok"},
        {"op": "reopen", "task": 1, "by": "cy", "note": "r3"},
        {"op": "claim", "member": "bo", "task": 1},
        {"op": "finish", "member": "bo", "task": 1, "result": "ok"}]},
     "token_of": "cy", "argv": ["fix", "T1", "otra vez"]},
    {"id": "add", "board": PLAIN, "token_of": "ana", "argv": ["add", "Documentar la API", "--role", "docs"]},
    {"id": "add-after", "board": DEV_TASK, "token_of": "ana",
     "argv": ["add", "Publicar", "--after", "T1,T2"]},
    {"id": "add-in", "board": DEV_TASK, "token_of": "ana", "argv": ["add", "Lexer", "--in", "T1"]},
    {"id": "add-rol-invalido", "board": PLAIN, "token_of": "ana",
     "argv": ["add", "X", "--role", "ROL!"]},
    {"id": "add-a-ajeno", "board": PLAIN, "token_of": "ana", "argv": ["add", "X", "--to", "nadie"]},
    {"id": "say", "board": PLAIN, "token_of": "ana", "argv": ["say", "all", "empezamos"]},
    {"id": "say-a-uno", "board": PLAIN, "token_of": "ana", "argv": ["say", "bo", "revisa el PR"]},
    {"id": "say-uso", "board": PLAIN, "token_of": "ana", "argv": ["say"]},
    {"id": "say-largo", "board": PLAIN, "token_of": "ana",
     "argv": ["say", "all", "x" * 300]},
    {"id": "say-unicode", "board": PLAIN, "token_of": "ana",
     "argv": ["say", "all", "empezamos con la «ñ» 🙂"]},
    {"id": "ask", "board": PLAIN, "token_of": "bo", "argv": ["ask", "ana", "¿qué API usamos?"]},
    {"id": "answer", "board": PLAIN, "token_of": "ana", "argv": ["answer", "bo", "la del plan"]},
    {"id": "inbox", "board": {"goal": "X", "ops": [*TEAM,
                                                   {"op": "post", "sender": "ana", "body": "para bo", "to": "bo"},
                                                   {"op": "post", "sender": "cy", "body": "a todos", "to": "*"}]},
     "token_of": "bo", "argv": ["inbox"]},
    {"id": "inbox-vacio", "board": PLAIN, "token_of": "bo", "argv": ["inbox"]},
    {"id": "lock", "board": PLAIN, "token_of": "bo", "argv": ["lock", "src/a.rs", "src/b.rs"]},
    {"id": "lock-conflicto", "board": {"goal": "X", "ops": [*TEAM,
                                                            {"op": "reserve", "member": "ana",
                                                             "paths": ["src/a.rs"]}]},
     "token_of": "bo", "argv": ["lock", "src/a.rs"]},
    {"id": "lock-sin-rutas", "board": PLAIN, "token_of": "bo", "argv": ["lock"]},
    {"id": "unlock", "board": {"goal": "X", "ops": [*TEAM,
                                                    {"op": "reserve", "member": "bo",
                                                     "paths": ["src/a.rs"]}]},
     "token_of": "bo", "argv": ["unlock", "src/a.rs"]},
    {"id": "board", "board": DEV_TASK, "token_of": "bo", "argv": ["board"]},
    {"id": "me", "board": DEV_TASK, "token_of": "bo", "argv": ["me"], "before": [["next"]]},
    {"id": "help", "board": PLAIN, "token_of": "bo", "argv": ["help"]},
    {"id": "comando-desconocido", "board": PLAIN, "token_of": "bo", "argv": ["nope"]},
    {"id": "opcion-desconocida", "board": PLAIN, "token_of": "bo",
     "argv": ["say", "all", "x", "--zzz"]},
    {"id": "wait-sin-novedades", "board": PLAIN, "token_of": "bo", "argv": ["wait", "1"]},
    {"id": "wait-con-novedades", "board": {"goal": "X", "ops": [*TEAM,
                                                                {"op": "post", "sender": "ana",
                                                                 "body": "para bo", "to": "bo"}]},
     "token_of": "bo", "argv": ["wait"]},
    {"id": "wait-cerrado", "board": PLAIN, "token_of": "bo", "argv": ["wait"], "closed": True},
    {"id": "duplicado-next", "board": DEV_TASK, "token_of": "bo", "argv": ["next"],
     "before": [["next"], ["next"]]},
    {"id": "add-lleno", "board": {"goal": "X", "ops": [*TEAM, {"op": "fill", "count": 60, "role": "dev"}]},
     "token_of": "ana", "argv": ["add", "Una más"], "dump": False},
    {"id": "add-duplicado", "board": PLAIN, "token_of": "ana", "argv": ["add", "Documentar"],
     "before": [["add", "Documentar"]]},
    {"id": "say-rafaga", "board": PLAIN, "token_of": "ana", "argv": ["say", "all", "once"],
     "before": [["say", "all", f"m{index}"] for index in range(10)]},
]


def _transcript_cases(out: Any, home: Any) -> None:
    for case in TRANSCRIPTS:
        out.add(f"swarm/ezs/{case['id']}", "service.handle", case, sb.transcript(case, home))




def cases(out: Any, home: Any) -> None:
    _transcript_cases(out, home)
