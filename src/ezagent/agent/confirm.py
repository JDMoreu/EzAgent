"""Confirmaciones por voz o clic para operaciones de nivel 2.

El hilo de herramientas llama a :meth:`ConfirmationManager.ask` y queda
bloqueado hasta que la interfaz entrega una respuesta o vence el tiempo
límite. El hilo de UI nunca se bloquea: usa ``submit_text``, ``submit_click``
o ``cancel``.

# DECISION: una respuesta se acepta si contiene alguna palabra completa de la
# lista ``accept`` y ninguna de ``reject``. Las palabras desconocidas se
# ignoran para tolerar fórmulas naturales («sí, adelante»), pero si aparece
# una palabra de rechazo la respuesta es ambigua y se rechaza.
"""

from __future__ import annotations

import re
import threading
import time
import unicodedata
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field

CONFIRM_WORD = "confirmo"


def _fold(text: str) -> str:
    """Pasa a minúsculas y elimina tildes para comparar palabras."""
    decomposed = unicodedata.normalize("NFKD", text.casefold())
    return "".join(ch for ch in decomposed if not unicodedata.combining(ch))


def _words(text: str) -> list[str]:
    """Devuelve palabras completas (letras y dígitos), sin puntuación."""
    return re.findall(r"[^\W_]+", _fold(text), flags=re.UNICODE)


def _phrases(values: Sequence[str]) -> tuple[tuple[str, ...], ...]:
    """Normaliza cada valor a una secuencia de palabras completas."""
    phrases: list[tuple[str, ...]] = []
    for value in values:
        phrase = tuple(_words(str(value)))
        if phrase:
            phrases.append(phrase)
    return tuple(phrases)


def _contains_phrase(tokens: list[str], phrase: tuple[str, ...]) -> bool:
    size = len(phrase)
    if size == 0:
        return False
    if size == 1:
        return phrase[0] in tokens
    for index in range(len(tokens) - size + 1):
        if tuple(tokens[index : index + size]) == phrase:
            return True
    return False


_UNITS: dict[str, int] = {
    "cero": 0,
    "uno": 1,
    "un": 1,
    "una": 1,
    "dos": 2,
    "tres": 3,
    "cuatro": 4,
    "cinco": 5,
    "seis": 6,
    "siete": 7,
    "ocho": 8,
    "nueve": 9,
    "diez": 10,
    "once": 11,
    "doce": 12,
    "trece": 13,
    "catorce": 14,
    "quince": 15,
    "dieciseis": 16,
    "diecisiete": 17,
    "dieciocho": 18,
    "diecinueve": 19,
    "veinte": 20,
    "veintiuno": 21,
    "veintidos": 22,
    "veintitres": 23,
    "veinticuatro": 24,
    "veinticinco": 25,
    "veintiseis": 26,
    "veintisiete": 27,
    "veintiocho": 28,
    "veintinueve": 29,
}
_TENS: dict[str, int] = {
    "treinta": 30,
    "cuarenta": 40,
    "cincuenta": 50,
    "sesenta": 60,
    "setenta": 70,
    "ochenta": 80,
    "noventa": 90,
}
_HUNDREDS: dict[str, int] = {
    "cien": 100,
    "ciento": 100,
    "doscientos": 200,
    "trescientos": 300,
    "cuatrocientos": 400,
    "quinientos": 500,
    "seiscientos": 600,
    "setecientos": 700,
    "ochocientos": 800,
    "novecientos": 900,
}


def _number_from_words(words: Sequence[str]) -> int | None:
    """Interpreta «doce», «treinta y dos» o «mil doscientos»."""
    total = 0
    current = 0
    found = False
    for word in words:
        if word in _UNITS:
            current += _UNITS[word]
            found = True
        elif word in _TENS:
            current += _TENS[word]
            found = True
        elif word in _HUNDREDS:
            current += _HUNDREDS[word]
            found = True
        elif word == "mil":
            total += (current or 1) * 1000
            current = 0
            found = True
    if not found:
        return None
    return total + current


@dataclass
class _Pending:
    summary: str
    strong: bool
    require_number: int | None
    event: threading.Event = field(default_factory=threading.Event)
    result: bool = False
    resolved: bool = False


class ConfirmationManager:
    """Coordina una confirmación pendiente entre el hilo de herramientas y la UI."""

    def __init__(
        self,
        *,
        accept: Sequence[str],
        reject: Sequence[str],
        timeout_s: float = 15.0,
        on_ask: Callable[[str, bool], None] | None = None,
    ) -> None:
        self._accept = _phrases(accept)
        self._reject = _phrases(reject)
        self._timeout_s = max(0.0, float(timeout_s))
        self._on_ask = on_ask
        self._lock = threading.Lock()
        self._pending: _Pending | None = None

    @property
    def pending(self) -> bool:
        with self._lock:
            return self._pending is not None and not self._pending.resolved

    def ask(
        self,
        summary: str,
        *,
        strong: bool = False,
        require_number: int | None = None,
    ) -> bool:
        """Muestra el resumen y espera la respuesta. Devuelve True si acepta.

        Bloquea solo al hilo que ejecuta herramientas. Un timeout equivale a
        rechazo.
        """
        request = _Pending(
            summary=summary,
            strong=strong,
            require_number=require_number,
        )
        with self._lock:
            # DECISION: no se encolan varias confirmaciones. Si ya hay una
            # pendiente, la nueva se rechaza para no confundir al usuario.
            if self._pending is not None and not self._pending.resolved:
                return False
            self._pending = request

        started = time.monotonic()
        if self._on_ask is not None:
            try:
                self._on_ask(summary, strong)
            except Exception:
                # DECISION: un fallo de la UI no debe tumbar la herramienta.
                pass

        # El plazo cuenta desde que se plantea la confirmación, no desde que
        # termina el callback de la UI.
        remaining = max(0.0, self._timeout_s - (time.monotonic() - started))
        completed = request.event.wait(remaining)

        with self._lock:
            if request.resolved:
                accepted = request.result
            elif not completed:
                # Sin respuesta dentro del plazo: rechazo.
                request.resolved = True
                request.result = False
                request.event.set()
                accepted = False
            else:
                # El evento se activó sin resultado: no debería ocurrir.
                request.resolved = True
                request.result = False
                accepted = False
            if self._pending is request:
                self._pending = None
        return bool(accepted)

    def submit_text(self, text: str) -> bool | None:
        """Resuelve la confirmación pendiente con una respuesta de voz.

        Devuelve True/False si había una pendiente y None si no había ninguna.
        """
        words = _words(str(text or ""))
        word_set = set(words)
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return None
            accepted = self._accepts(request, word_set, words)
            request.resolved = True
            request.result = accepted
            request.event.set()
            self._pending = None
            return accepted

    def submit_click(self, yes: bool) -> None:
        """Resuelve la confirmación pendiente con un clic en la interfaz."""
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return
            request.resolved = True
            request.result = bool(yes)
            request.event.set()
            self._pending = None

    def cancel(self, reason: str = "") -> None:
        """Cierra la confirmación pendiente como rechazo."""
        # ``reason`` queda para la UI/auditoría del llamador.
        _ = reason
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return
            request.resolved = True
            request.result = False
            request.event.set()
            self._pending = None

    def _accepts(self, request: _Pending, word_set: set[str], words: list[str]) -> bool:
        if any(_contains_phrase(words, phrase) for phrase in self._reject):
            return False
        if request.strong:
            if CONFIRM_WORD not in word_set:
                return False
            if request.require_number is not None:
                numbers = {int(word) for word in words if word.isdigit()}
                spoken = _number_from_words(words)
                if spoken is not None:
                    numbers.add(spoken)
                if request.require_number not in numbers:
                    return False
            return True
        return any(_contains_phrase(words, phrase) for phrase in self._accept)
