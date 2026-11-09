"""Reactivation phrase: the only thing EzAgent listens for while paused.

While listening is paused the microphone stays open locally, but every
utterance is thrown away unless it is short and, as a whole, is exactly the
reactivation phrase («EzAgent, escúchame»). A single loose word is never
enough: the phrase must name the assistant and say the verb, so a normal
conversation with someone else does not wake it.

Pure module: no audio, no network. ``app.py`` transcribes and calls
:func:`is_wake_phrase`.
"""

from __future__ import annotations

import re
import unicodedata

# Longest utterance that is transcribed while paused. Anything longer is a
# conversation, not a command, and is discarded without reaching Whisper.
MAX_WAKE_SECS = 3.0
MIN_WAKE_SECS = 0.35

DEFAULT_WAKE_PHRASE = "EzAgent, escúchame"

# How Whisper tends to write the assistant name in Spanish.
NAME_VARIANTS: tuple[str, ...] = (
    "ezagent",
    "ez agent",
    "easy agent",
    "easyagent",
    "isi agent",
    "isiagent",
    "esagent",
    "es agent",
    "e z agent",
    "eazy agent",
    "easy agents",
    "ezagents",
)
NAME_KEY = "ezagent"

# Always accepted with the name, besides the configured phrase.
BUILTIN_VERBS: tuple[str, ...] = (
    "escuchame",
    "vuelve",
    "abre la escucha",
    "despierta",
)
# Courtesy words that may wrap the phrase without changing it.
_LEADING = ("oye ", "hey ", "ey ", "hola ")
_TRAILING = (" por favor", " porfa", " ya")


def normalize(text: str) -> str:
    """Lowercase, no accents, no punctuation, single spaces."""
    raw = unicodedata.normalize("NFKD", str(text or "").lower())
    raw = "".join(c for c in raw if not unicodedata.combining(c))
    raw = re.sub(r"[^a-z0-9 ]+", " ", raw)
    return re.sub(r"\s+", " ", raw).strip()


def _canonical_name(text: str) -> str:
    """Replaces every known spelling of the name with ``ezagent``."""
    out = f" {text} "
    for variant in sorted(NAME_VARIANTS, key=len, reverse=True):
        out = out.replace(f" {variant} ", f" {NAME_KEY} ")
    return re.sub(r"\s+", " ", out).strip()


def _strip_courtesy(text: str) -> str:
    changed = True
    while changed:
        changed = False
        for lead in _LEADING:
            if text.startswith(lead):
                text = text[len(lead) :]
                changed = True
        for trail in _TRAILING:
            if text.endswith(trail):
                text = text[: -len(trail)]
                changed = True
    return text.strip()


def wake_forms(phrase: str = DEFAULT_WAKE_PHRASE) -> frozenset[str]:
    """All accepted normalized forms for the configured phrase.

    The configured phrase must include the assistant name; if it does not, the
    name is prepended so a single word can never wake EzAgent.
    """
    base = _canonical_name(normalize(phrase)) or _canonical_name(normalize(DEFAULT_WAKE_PHRASE))
    if NAME_KEY not in base.split():
        base = f"{NAME_KEY} {base}"
    verb = " ".join(w for w in base.split() if w != NAME_KEY)
    verbs = {verb, *BUILTIN_VERBS} - {""}
    forms = {base}
    for v in verbs:
        forms.add(f"{NAME_KEY} {v}")
        forms.add(f"{v} {NAME_KEY}")
    return frozenset(forms)


def is_wake_phrase(text: str, phrase: str = DEFAULT_WAKE_PHRASE) -> bool:
    """True only if the whole transcript is the reactivation phrase."""
    key = _strip_courtesy(_canonical_name(normalize(text)))
    if not key:
        return False
    return key in wake_forms(phrase)


def utterance_secs(pcm: bytes, rate: int) -> float:
    """Duration of 16-bit mono PCM."""
    if not pcm or rate <= 0:
        return 0.0
    return len(pcm) / (2.0 * rate)


def worth_transcribing(pcm: bytes, rate: int) -> bool:
    """Short enough to be a command, long enough to be speech."""
    secs = utterance_secs(pcm, rate)
    return MIN_WAKE_SECS <= secs <= MAX_WAKE_SECS
