"""Avisos de avance del enjambre: qué etapa terminó y cuál empieza.

El supervisor compara el tablero en cada pasada con lo que ya contó y arma
frases cortas para decir por voz («Terminó la planificación. Ahora empieza el
desarrollo.»). Solo mira tareas de primer nivel: las partes avanzan en silencio
y su tarea madre se cierra sola. El final del enjambre no pasa por aquí: lo
anuncia el supervisor con el resumen completo.
"""

from __future__ import annotations

from collections.abc import Sequence

from .models import MAX_ROUNDS, Task
from .protocol import stage_name


def _join(names: Sequence[str]) -> str:
    unique = list(dict.fromkeys(names))
    if len(unique) == 1:
        return unique[0]
    return ", ".join(unique[:-1]) + " y " + unique[-1]


def _name(task: Task) -> str:
    return stage_name(task.role, task.title)


class Progress:
    """Recuerda por enjambre qué tareas ya se contaron como hechas o devueltas."""

    def __init__(self) -> None:
        self._done: dict[str, set[int]] = {}
        self._rounds: dict[str, dict[int, int]] = {}

    def forget(self, swarm: str) -> None:
        self._done.pop(swarm, None)
        self._rounds.pop(swarm, None)

    def check(self, swarm: str, tasks: Sequence[Task]) -> list[str]:
        """Frases nuevas desde la última pasada; la primera vez solo toma nota."""
        top = [task for task in tasks if not task.parent]
        done_now = {task.id for task in top if task.state == "done"}
        rounds_now = {task.id: task.rounds for task in top}
        if swarm not in self._done:
            # Primera vista (arranque o reinicio de EzAgent): sin avisos atrasados.
            self._done[swarm] = done_now
            self._rounds[swarm] = rounds_now
            return []
        seen, rounds = self._done[swarm], self._rounds[swarm]
        news: list[str] = []
        returned = [t for t in top if t.rounds > rounds.get(t.id, 0) and t.state != "done"]
        for task in returned:
            news.append(
                f"Enjambre: devolvieron {_name(task)} para corregir "
                f"(ronda {task.rounds} de {MAX_ROUNDS})."
            )
        finished = [t for t in top if t.id in done_now - seen]
        if finished and len(done_now) < len(top):  # todo hecho: lo dice el resumen final
            news.append(self._stage_text(finished, top))
        self._done[swarm] = done_now
        self._rounds[swarm] = rounds_now
        return news

    @staticmethod
    def _stage_text(finished: Sequence[Task], top: Sequence[Task]) -> str:
        ids = {task.id for task in finished}
        verb = "terminó" if len({_name(t) for t in finished}) == 1 else "terminaron"
        text = f"Enjambre: {verb} {_join([_name(t) for t in finished])}."
        nxt = [t for t in top if t.state != "done" and ids & set(t.deps)]
        if nxt:
            names = [_name(t) for t in nxt]
            start = "empieza" if len(set(names)) == 1 else "empiezan"
            return f"{text} Ahora {start} {_join(names)}."
        running = [t for t in top if t.state == "claimed"]
        if running:
            names = [_name(t) for t in running]
            still = "sigue en curso" if len(set(names)) == 1 else "siguen en curso"
            who = _join(names)
            return f"{text} {who[:1].upper()}{who[1:]} {still}."
        return text


__all__ = ["Progress"]
