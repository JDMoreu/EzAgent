"""Frase de reactivación: solo el nombre más el verbo despierta a EzAgent."""

from __future__ import annotations

import pytest

from ezagent.voice.wake import (
    DEFAULT_WAKE_PHRASE,
    MAX_WAKE_SECS,
    MIN_WAKE_SECS,
    is_wake_phrase,
    normalize,
    utterance_secs,
    wake_forms,
    worth_transcribing,
)


@pytest.mark.parametrize(
    "text",
    [
        "EzAgent, escúchame",
        "ezagent escuchame",
        "Escúchame, EzAgent",
        "Oye EzAgent, escúchame",
        "hey ezagent escúchame por favor",
        "Hola, EzAgent escúchame ya",
        "easy agent escúchame",  # cómo lo suele escribir Whisper
        "Ez Agent, escúchame",
        "isi agent escuchame",
        "EzAgent vuelve",
        "EzAgent despierta",
        "EzAgent, abre la escucha",
        "¡EzAgent, escúchame!",
        "  EZAGENT   ESCUCHAME  ",
    ],
)
def test_frases_que_despiertan(text: str) -> None:
    assert is_wake_phrase(text) is True


@pytest.mark.parametrize(
    "text",
    [
        "",
        "   ",
        "escúchame",  # sin el nombre
        "ezagent",  # sin el verbo
        "vuelve",
        "despierta",
        "oye",
        "hola cómo estás",
        "EzAgent, escúchame esta canción que me gusta mucho",  # conversación
        "le dije a ezagent escúchame",  # no es la frase completa
        "EzAgent no me escuches",
        "ezagent escuchame y borra todo",
    ],
)
def test_frases_que_no_despiertan(text: str) -> None:
    assert is_wake_phrase(text) is False


def test_una_palabra_suelta_nunca_basta_aunque_la_frase_este_mal_configurada() -> None:
    # Si el usuario configura solo un verbo, el nombre se antepone.
    assert is_wake_phrase("hola", "hola") is False
    assert is_wake_phrase("ezagent hola", "hola") is True
    assert is_wake_phrase("", "") is False


def test_frase_personalizada() -> None:
    phrase = "EzAgent, a trabajar"
    assert is_wake_phrase("ezagent a trabajar", phrase)
    assert is_wake_phrase("a trabajar ezagent", phrase)
    assert is_wake_phrase("ezagent, escúchame", phrase)  # los verbos integrados siguen
    assert not is_wake_phrase("a trabajar", phrase)


def test_wake_forms_siempre_incluyen_el_nombre() -> None:
    for phrase in [DEFAULT_WAKE_PHRASE, "hola", "a trabajar", "", "EzAgent"]:
        forms = wake_forms(phrase)
        assert forms and all("ezagent" in form.split() for form in forms), phrase


def test_normalize() -> None:
    assert normalize("¡Escúchame, EzAgent!") == "escuchame ezagent"
    assert normalize(None) == ""  # type: ignore[arg-type]
    assert normalize("A\t\nB") == "a b"


def test_duracion_del_audio() -> None:
    assert utterance_secs(b"", 16000) == 0.0
    assert utterance_secs(b"\0" * 32000, 16000) == pytest.approx(1.0)
    assert utterance_secs(b"\0" * 32000, 0) == 0.0
    assert utterance_secs(b"\0" * 32000, -1) == 0.0


def test_solo_vale_la_pena_transcribir_audio_de_duracion_razonable() -> None:
    rate = 16000

    def pcm(seconds: float) -> bytes:
        return b"\0" * int(seconds * rate * 2)

    # Los límites se fijan con literales: así cambiar la constante rompe la prueba
    # (usar la constante importada la volvería una tautología).
    assert (MIN_WAKE_SECS, MAX_WAKE_SECS) == (0.35, 3.0)
    assert not worth_transcribing(pcm(0.30), rate)
    assert worth_transcribing(pcm(0.36), rate)
    assert worth_transcribing(pcm(1.5), rate)
    assert worth_transcribing(pcm(3.0), rate)
    assert not worth_transcribing(pcm(3.1), rate)
    assert not worth_transcribing(pcm(8.0), rate)  # una conversación, no una orden
    assert not worth_transcribing(b"", rate)
