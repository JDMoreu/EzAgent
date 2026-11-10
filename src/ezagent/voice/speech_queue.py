"""Cola de voz: dos hilos, síntesis y reproducción, en orden.

El hilo que llama a :meth:`SpeechQueue.say` solo encola texto. Un hilo propio
sintetiza (Kokoro) y deja el audio en una cola acotada; otro hilo lo reproduce
en orden. Así el lector del LLM sigue consumiendo tokens y ejecutando
herramientas mientras Kokoro prepara la frase siguiente (F08).
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
_TEXT_MAXSIZE = 64
_AUDIO_MAXSIZE = 8
_POLL_SECS = 0.05


class SpeechQueue:
    """Sintetiza y reproduce en orden el texto que se va encolando.

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
        self._text_q: queue.Queue[object] = queue.Queue(maxsize=_TEXT_MAXSIZE)
        self._audio_q: queue.Queue[object] = queue.Queue(maxsize=_AUDIO_MAXSIZE)
        self._error: BaseException | None = None
        self._aborted = threading.Event()
        self._closed = threading.Event()
        self._lifecycle = threading.Lock()
        self._sounded = False
        self._synth_thread = threading.Thread(
            target=self._synthesizer, name="ezagent-speech-synth", daemon=True
        )
        self._play_thread = threading.Thread(
            target=self._player, name="ezagent-speech", daemon=True
        )
        self._synth_thread.start()
        self._play_thread.start()

    @property
    def error(self) -> BaseException | None:
        """Error de síntesis o reproducción, si lo hubo."""
        return self._error

    def say(self, text: str) -> bool:
        """Encola ``text`` para sintetizar. False si el turno ya se cortó."""
        if self._stop.is_set() or self._aborted.is_set() or self._closed.is_set() or self._error:
            return False
        return self._put(self._text_q, text)

    def finish(self, timeout: float | None = None) -> bool:
        """Espera a que se sintetice y suene todo lo encolado. True si terminó."""
        deadline = None if timeout is None else time.monotonic() + timeout
        with self._lifecycle:
            if not self._closed.is_set():
                self._closed.set()
                self._put_done(self._text_q, until=self._synth_thread)
        self._join(self._synth_thread, deadline)
        self._join(self._play_thread, deadline)
        if self._error is not None:
            raise self._error
        return (
            not self._stop.is_set()
            and not self._synth_thread.is_alive()
            and not self._play_thread.is_alive()
        )

    def abort(self) -> None:
        """Descarta lo pendiente sin esperar (la frase en curso acaba)."""
        self._aborted.set()
        self._closed.set()
        self._drain(self._text_q)
        self._drain(self._audio_q)
        self._put_done(self._text_q)
        self._put_done(self._audio_q)

    # ------------------------------------------------------------- hilos
    def _synthesizer(self) -> None:
        try:
            while True:
                item = self._text_q.get()
                if item is _DONE:
                    break
                if self._stop.is_set() or self._aborted.is_set() or self._error:
                    continue
                began = time.monotonic()
                try:
                    samples, rate = self._synthesize(str(item))
                    if self._on_synth is not None:
                        self._on_synth(time.monotonic() - began)
                except Exception as exc:  # noqa: BLE001
                    logger.warning("Síntesis: {}", exc)
                    self._error = exc
                    self._closed.set()
                    break
                if self._stop.is_set() or self._aborted.is_set() or self._error:
                    continue
                self._put_audio((samples, rate))
        finally:
            # El reproductor siempre recibe su fin, aunque el worker falle.
            self._put_done(self._audio_q)

    def _player(self) -> None:
        while True:
            item = self._audio_q.get()
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
            self._closed.set()
            return
        if self._on_play is not None and started:
            self._on_play(time.monotonic() - started[0])

    # ------------------------------------------------------------ colas
    def _put(self, target: queue.Queue[object], item: object) -> bool:
        """Encola esperando hueco (backpressure); False si el turno se cortó."""
        while True:
            if (
                self._stop.is_set()
                or self._aborted.is_set()
                or self._closed.is_set()
                or self._error
            ):
                return False
            try:
                target.put(item, timeout=_POLL_SECS)
                return True
            except queue.Full:
                continue

    def _put_audio(self, item: object) -> None:
        while True:
            if self._stop.is_set() or self._aborted.is_set() or self._error:
                return
            try:
                self._audio_q.put(item, timeout=_POLL_SECS)
                return
            except queue.Full:
                continue

    def _put_done(
        self, target: queue.Queue[object], *, until: threading.Thread | None = None
    ) -> None:
        while True:
            try:
                target.put(_DONE, timeout=_POLL_SECS)
                return
            except queue.Full:
                if self._aborted.is_set() or (until is not None and not until.is_alive()):
                    return

    @staticmethod
    def _drain(target: queue.Queue[object]) -> None:
        while True:
            try:
                target.get_nowait()
            except queue.Empty:
                return

    @staticmethod
    def _join(thread: threading.Thread, deadline: float | None) -> None:
        remaining = None if deadline is None else max(0.0, deadline - time.monotonic())
        thread.join(remaining)
