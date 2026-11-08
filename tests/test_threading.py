"""R1: crear y señalar el evento de parada del TTS desde dos hilos."""

from __future__ import annotations

import threading

from ezagent.voice.app import VoiceApp


def _app() -> VoiceApp:
    # Unitario de _new_tts_stop/_signal_tts_stop: sin Tk, audio ni red.
    app = VoiceApp.__new__(VoiceApp)
    app._tts_lock = threading.Lock()
    app._tts_stop = None
    return app


def test_tts_stop_is_thread_safe():
    app = _app()
    created: list[threading.Event] = []
    barrier = threading.Barrier(2)

    def create() -> None:
        barrier.wait()
        for _ in range(1000):
            created.append(app._new_tts_stop())

    def signal() -> None:
        barrier.wait()
        for _ in range(1000):
            app._signal_tts_stop()

    threads = [
        threading.Thread(target=create),
        threading.Thread(target=signal),
    ]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()

    assert len(created) == 1000
    # Cada evento nuevo señaló al anterior; solo el vigente queda sin señalar.
    assert app._tts_stop is created[-1]
    app._signal_tts_stop()
    assert all(event.is_set() for event in created)
