"""Confirmaciones por voz o clic para operaciones de nivel 2.

El hilo de herramientas llama a :meth:`ConfirmationManager.ask` y queda
bloqueado hasta que la interfaz entrega una respuesta o vence el tiempo
límite. El hilo de UI nunca se bloquea: usa ``submit_text``, ``submit_click``
o ``cancel``.

El plazo no empieza al plantear la pregunta, sino cuando la interfaz llama a
:meth:`ConfirmationManager.arm` tras terminar de leerla por TTS. Cada
confirmación tiene un token: una respuesta con otro token (una transcripción
atrasada) no la resuelve.

# DECISION: una respuesta solo resuelve si contiene alguna palabra completa de
# ``accept`` o de ``reject``. Una frase ajena o vacía devuelve None y deja la
# confirmación pendiente para que el llamador pueda repreguntar una vez. Si
# aparecen palabras de ambas listas gana el rechazo.
"""

from __future__ import annotations

import re
import threading
import time
import unicodedata
import uuid
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field

CONFIRM_WORD = "confirmo"

# Si la interfaz nunca arma la pregunta (TTS caído), el plazo cuenta igual
# después de esta gracia para no bloquear el hilo de herramientas para siempre.
_ARM_GRACE_S = 30.0

Outcome = str  # yes | no | timeout | cancelled | unavailable


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
    token: str = field(default_factory=lambda: uuid.uuid4().hex)
    condition: threading.Condition = field(default_factory=threading.Condition)
    armed: bool = False
    result: bool = False
    resolved: bool = False
    outcome: Outcome | None = None


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
        self._last_outcome: Outcome | None = None

    @property
    def pending(self) -> bool:
        with self._lock:
            return self._pending is not None and not self._pending.resolved

    @property
    def pending_token(self) -> str | None:
        """Token de la confirmación vigente, o None si no hay ninguna."""
        with self._lock:
            if self._pending is None or self._pending.resolved:
                return None
            return self._pending.token

    @property
    def last_outcome(self) -> Outcome | None:
        """Último resultado: yes, no, timeout, cancelled o unavailable.

        ``unavailable`` significa que no se pudo plantear otra confirmación
        porque ya había una pendiente.
        """
        with self._lock:
            return self._last_outcome

    def arm(self, token: str | None = None) -> None:
        """Empieza a contar el plazo cuando la pregunta ya se terminó de leer.

        Con ``token``, un armado atrasado de otra pregunta no arma esta.
        """
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return
            if token is not None and token != request.token:
                return
            with request.condition:
                request.armed = True
                request.condition.notify_all()

    def ask(
        self,
        summary: str,
        *,
        strong: bool = False,
        require_number: int | None = None,
    ) -> bool:
        """Muestra el resumen y espera la respuesta. Devuelve True si acepta.

        Bloquea solo al hilo que ejecuta herramientas. Espera a :meth:`arm`
        antes de contar el plazo; un timeout equivale a rechazo.
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
                self._last_outcome = "unavailable"
                return False
            self._pending = request

        if self._on_ask is not None:
            try:
                self._on_ask(summary, strong)
            except Exception:
                # DECISION: un fallo de la UI no debe tumbar la herramienta.
                pass

        with request.condition:
            if self._on_ask is None:
                # DECISION: sin interfaz no hay TTS que esperar; el plazo cuenta ya.
                request.armed = True
            self._wait_armed(request)
            if not request.resolved:
                self._wait_answer(request)
            accepted = bool(request.result)
            outcome = request.outcome

        with self._lock:
            # Si otra confirmación ya ocupó el lugar, no pises su resultado.
            if self._pending is request:
                if outcome is not None:
                    self._last_outcome = outcome
                self._pending = None
        return accepted

    def submit_text(self, text: str, token: str | None = None) -> bool | None:
        """Resuelve la confirmación pendiente con una respuesta de voz.

        Devuelve True/False si la respuesta es clara, y None si no había una
        pendiente, el token no coincide o el texto no es ni aceptación ni
        rechazo (la confirmación sigue pendiente).
        """
        words = _words(str(text or ""))
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return None
            if token is not None and token != request.token:
                return None
            outcome = self._intent(request, words)
            if outcome is None:
                return None
            accepted = outcome == "yes"
            self._resolve(request, accepted, outcome)
            self._pending = None
            return accepted

    def submit_click(self, yes: bool) -> None:
        """Resuelve la confirmación pendiente con un clic en la interfaz."""
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return
            outcome = "yes" if yes else "no"
            self._resolve(request, bool(yes), outcome)
            self._pending = None

    def cancel(self, reason: str = "") -> None:
        """Cierra la confirmación pendiente como rechazo."""
        # ``reason`` queda para la UI/auditoría del llamador.
        _ = reason
        with self._lock:
            request = self._pending
            if request is None or request.resolved:
                return
            self._resolve(request, False, "cancelled")
            self._pending = None

    def _resolve(self, request: _Pending, accepted: bool, outcome: Outcome) -> None:
        """Cierra ``request`` y despierta a ``ask``. Se llama con el lock tomado."""
        with request.condition:
            request.resolved = True
            request.result = bool(accepted)
            request.outcome = outcome
            self._last_outcome = outcome
            request.condition.notify_all()

    def _wait_armed(self, request: _Pending) -> None:
        """Espera a ``arm`` con una gracia máxima. Se llama con la condición tomada."""
        deadline = time.monotonic() + _ARM_GRACE_S
        while not request.armed and not request.resolved:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                # DECISION: sin armado, el plazo cuenta desde que vence la gracia.
                request.armed = True
                return
            request.condition.wait(remaining)

    def _wait_answer(self, request: _Pending) -> None:
        """Espera la respuesta hasta ``timeout_s``. Se llama con la condición tomada."""
        deadline = time.monotonic() + self._timeout_s
        while not request.resolved:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                request.resolved = True
                request.result = False
                request.outcome = "timeout"
                return
            request.condition.wait(remaining)

    def _intent(self, request: _Pending, words: list[str]) -> Outcome | None:
        """Devuelve yes/no para una respuesta clara, o None si es ajena."""
        word_set = set(words)
        folded = " ".join(words)
        if any(
            phrase in folded
            for phrase in ("no se", "no estoy seguro", "no sabria", "no sabría", "quizas")
        ):
            return None
        if any(_contains_phrase(words, phrase) for phrase in self._reject):
            return "no"
        if not any(_contains_phrase(words, phrase) for phrase in self._accept):
            return None
        if request.strong:
            if CONFIRM_WORD not in word_set:
                # Una frase sin «confirmo» no es un rechazo: es ambigua y se
                # repregunta, igual que el ruido (F48).
                return None
            if request.require_number is not None:
                numbers = {int(word) for word in words if word.isdigit()}
                spoken = _number_from_words(words)
                if spoken is not None:
                    numbers.add(spoken)
                if not numbers:
                    return None  # «confirmo» sin número: aún no es respuesta
                if request.require_number not in numbers:
                    return "no"
        return "yes"
