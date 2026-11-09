"""Cola de voz: sintetiza la frase siguiente mientras suena la actual.

El hilo que llama a :meth:`SpeechQueue.say` sintetiza (Kokoro) y deja el audio
en una cola; un hilo propio lo reproduce en orden. Así la síntesis de la frase
N+1 se solapa con la reproducción de la frase N y no quedan huecos de silencio
entre frases.
"""

from __future__ import annotations

import queue
import threading
import time
from collections.abc import Callable
from typing import Any

from loguru import logger

# play(samples, sample_rate, on_start) -> True si sonó entera.
PlayFn = Callable[[Any, int, Callable[[], None]], bool]
SynthFn = Callable[[str], tuple[Any, int]]

_DONE = object()


class SpeechQueue:
    """Reproduce en orden el audio que se va sintetizando.

    ``on_synth`` y ``on_play`` reciben segundos de síntesis y de reproducción
    de cada frase; ``on_sound`` se llama justo antes del primer sonido.
    """

    def __init__(
        self,
        synthesize: SynthFn,
        play: PlayFn,
        stop_event: threading.Event,
        *,
        on_sound: Callable[[], None] | None = None,
        on_synth: Callable[[float], None] | None = None,
        on_play: Callable[[float], None] | None = None,
    ) -> None:
        self._synthesize = synthesize
        self._play = play
        self._stop = stop_event
        self._on_sound = on_sound
        self._on_synth = on_synth
        self._on_play = on_play
        self._queue: queue.Queue[Any] = queue.Queue()
        self._error: BaseException | None = None
        self._aborted = threading.Event()
        self._sounded = False
        self._thread = threading.Thread(target=self._player, name="ezagent-speech", daemon=True)
        self._thread.start()

    @property
    def error(self) -> BaseException | None:
        """Error de reproducción, si lo hubo."""
        return self._error

    def say(self, text: str) -> bool:
        """Sintetiza ``text`` en este hilo y lo encola. False si hay que parar."""
        if self._stop.is_set() or self._aborted.is_set() or self._error:
            return False
        began = time.monotonic()
        samples, rate = self._synthesize(text)
        if self._on_synth is not None:
            self._on_synth(time.monotonic() - began)
        if self._stop.is_set():
            return False
        self._queue.put((samples, rate))
        return True

    def finish(self, timeout: float | None = None) -> bool:
        """Espera a que suene todo lo encolado. True si terminó sin cortes."""
        self._queue.put(_DONE)
        self._thread.join(timeout)
        if self._error is not None:
            raise self._error
        return not self._stop.is_set()

    def abort(self) -> None:
        """Descarta lo que falte por sonar sin esperar (la frase en curso acaba)."""
        self._aborted.set()
        self._queue.put(_DONE)

    def _player(self) -> None:
        while True:
            item = self._queue.get()
            if item is _DONE:
                return
            if self._stop.is_set() or self._aborted.is_set() or self._error:
                continue  # se vacía la cola sin sonar
            samples, rate = item
            self._play_one(samples, rate)

    def _play_one(self, samples: Any, rate: int) -> None:
        started: list[float] = []

        def on_start() -> None:
            started.append(time.monotonic())
            if not self._sounded:
                self._sounded = True
                if self._on_sound is not None:
                    self._on_sound()

        try:
            self._play(samples, rate, on_start)
        except Exception as exc:  # noqa: BLE001
            logger.warning("Reproducción: {}", exc)
            self._error = exc
            return
        if self._on_play is not None and started:
            self._on_play(time.monotonic() - started[0])
