"""Cola de voz: la síntesis de la frase siguiente se solapa con la reproducción."""

from __future__ import annotations

import threading
import time

import pytest

from ezagent.voice.speech_queue import SpeechQueue


class FakeVoice:
    """Síntesis y reproducción falsas que registran cuándo ocurre cada cosa."""

    def __init__(self, play_secs: float = 0.15) -> None:
        self.play_secs = play_secs
        self.events: list[tuple[str, str, float]] = []
        self.played: list[str] = []
        self._lock = threading.Lock()

    def _log(self, kind: str, text: str) -> None:
        with self._lock:
            self.events.append((kind, text, time.monotonic()))

    def synthesize(self, text: str) -> tuple[str, int]:
        self._log("synth", text)
        return text, 24000

    def play(self, samples: str, rate: int, on_start) -> bool:
        on_start()
        self._log("play_start", samples)
        time.sleep(self.play_secs)
        self.played.append(samples)
        self._log("play_end", samples)
        return True

    def at(self, kind: str, text: str) -> float:
        return next(t for k, s, t in self.events if k == kind and s == text)


def test_next_sentence_is_synthesized_while_previous_plays() -> None:
    voice = FakeVoice()
    speech = SpeechQueue(voice.synthesize, voice.play, threading.Event())
    assert speech.say("uno")
    time.sleep(0.03)  # «uno» ya está sonando
    assert speech.say("dos")
    assert speech.finish(timeout=5)
    assert voice.played == ["uno", "dos"]
    assert voice.at("synth", "dos") < voice.at("play_end", "uno")


def test_order_is_kept_and_first_sound_fires_once() -> None:
    voice = FakeVoice(play_secs=0.01)
    sounds: list[int] = []
    synth: list[float] = []
    played: list[float] = []
    speech = SpeechQueue(
        voice.synthesize,
        voice.play,
        threading.Event(),
        on_sound=lambda: sounds.append(1),
        on_synth=synth.append,
        on_play=played.append,
    )
    for text in ("a", "b", "c"):
        speech.say(text)
    speech.finish(timeout=5)
    assert voice.played == ["a", "b", "c"]
    assert sounds == [1]
    assert len(synth) == 3 and len(played) == 3


def test_stop_drops_pending_audio() -> None:
    voice = FakeVoice(play_secs=0.1)
    stop = threading.Event()
    speech = SpeechQueue(voice.synthesize, voice.play, stop)
    speech.say("uno")
    speech.say("dos")
    speech.say("tres")
    time.sleep(0.03)
    stop.set()
    assert not speech.say("cuatro")
    assert not speech.finish(timeout=5)
    assert voice.played == ["uno"]


def test_abort_discards_queue_without_waiting() -> None:
    voice = FakeVoice(play_secs=0.1)
    speech = SpeechQueue(voice.synthesize, voice.play, threading.Event())
    speech.say("uno")
    speech.say("dos")
    time.sleep(0.03)
    speech.abort()
    assert not speech.say("tres")
    time.sleep(0.25)
    assert voice.played == ["uno"]


def test_playback_error_surfaces_on_finish() -> None:
    def broken_play(samples, rate, on_start) -> bool:
        raise RuntimeError("sin altavoz")

    speech = SpeechQueue(lambda text: (text, 1), broken_play, threading.Event())
    speech.say("hola")
    with pytest.raises(RuntimeError, match="sin altavoz"):
        speech.finish(timeout=5)
