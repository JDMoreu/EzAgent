"""Fixtures de TTS: normalización, fonemas, tokens, estilos de voz y PCM (dominio `tts`, F1-09).

El oráculo es el camino real de `ezagent.voice.tts.Speaker.synthesize`:
`normalize_for_kokoro` y luego `Kokoro.create` con la voz del `VoiceConfig` por
defecto (`em_alex`, 0.95, pausas 0.30/0.12). El corpus tiene 60 frases en
español neutro latinoamericano (números, monedas, siglas, signos, vacías y
frases largas que fuerzan troceado) y 10 en inglés.

Cada frase guarda el texto normalizado, los fonemas de espeak-ng, los ids de
token, los trozos de `split_phonemes` con su pausa, el estilo de voz aplicado y
el PCM del modelo en `parity/golden/tts/<caso>.npy` (float32, 24 kHz) con su
sha256 y su RMS. El ONNX corre en CPU con un solo hilo para que dos corridas
den bytes idénticos.

No cubre `Speaker.play`/`speak` (PyAudio y hardware) ni la descarga de modelos.
"""

from __future__ import annotations

import asyncio
import contextlib
import hashlib
import importlib.metadata
import json
import os
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import numpy as np
import onnxruntime as rt
from _common import GOLDEN, FixtureFile, main_guard, sha256
from kokoro_onnx import Kokoro
from kokoro_onnx import session as ksession
from kokoro_onnx.chunker import pause_after, split_phonemes
from kokoro_onnx.config import KoKoroConfig
from kokoro_onnx.pauses import insert as insert_pauses
from kokoro_onnx.pauses import wanted_after
from kokoro_onnx.sliding import Timing, synthesize, timings, token_edges
from kokoro_onnx.tokenizer import Tokenizer
from kokoro_onnx.trim import trim as trim_audio

from ezagent.speech_text import normalize_for_kokoro
from ezagent.tts import resolve_kokoro_files
from ezagent.voice.tts import _voice_style

DOMAIN = "tts"
OUT = GOLDEN / "tts"
RATE = 24000
DEFAULTS: dict[str, Any] = {
    "lang": "es", "voice": "em_alex", "speed": 0.95, "sentence_pause": 0.30,
    "clause_pause": 0.12, "is_phonemes": False, "trim": True,
}

def _find_kokoro() -> tuple[Path, Path, str] | None:
    """Modelo y voces instalados; nunca descarga (el brief lo prohíbe)."""
    roots = []
    if os.getenv("EZAGENT_HOME"):
        roots.append(Path(os.environ["EZAGENT_HOME"]) / "models" / "kokoro")
    if os.getenv("LOCALAPPDATA"):
        roots.append(Path(os.environ["LOCALAPPDATA"]) / "EzAgent" / "models" / "kokoro")
    roots.append(Path.home() / ".cache" / "pipecat" / "kokoro-onnx")
    for root in roots:
        for variant, name in (("int8", "kokoro-v1.0.int8.onnx"), ("fp32", "kokoro-v1.0.onnx")):
            model, voices = root / name, root / "voices-v1.0.bin"
            if model.is_file() and voices.is_file():
                return model, voices, variant
    return None


ARTIFACTS = _find_kokoro()
# DECISION: una sola instancia de Kokoro para toda la corrida; su sesión ONNX
# es de un hilo y el orden de las síntesis es el del corpus.

_LONG_A = (
    "El equipo de soporte revisó los incidentes de la semana y preparó un informe completo. "
    "El documento explica qué falló, cómo se corrigió y qué medidas se tomarán para evitar que vuelva a ocurrir. "
    "También incluye una lista de tareas pendientes para el próximo mes. "
    "La prioridad es mejorar la estabilidad del servicio y reducir el tiempo de respuesta. "
    "Cada área debe confirmar su plan antes del viernes. "
    "El resumen se enviará a todas las personas del equipo y se publicará en el portal interno. "
    "Cualquier duda puede resolverse con el coordinador del proyecto. Gracias por la colaboración."
)
_LONG_B = (
    "El sistema recibió mil doscientas solicitudes durante la mañana, procesó casi todas sin errores, "
    "respondió a las consultas urgentes y dejó el resto en cola; el equipo revisó los avisos, "
    "ajustó la configuración, reinició los servicios afectados y documentó cada cambio, "
    "mientras el monitoreo seguía activo, las métricas mejoraban poco a poco y los usuarios reportaban menos demoras, "
    "aunque quedaban pendientes algunas pruebas, revisiones de seguridad y tareas de limpieza, "
    "y el coordinador pidió un informe con el detalle de cada caso, las causas probables, "
    "las acciones tomadas, los responsables asignados y las fechas comprometidas para cerrar los pendientes antes del final del mes."
)

SPANISH: list[str] = [
    "Hola, soy EzAgent y estoy listo para ayudarte.",
    "Hoy tenemos 3 tazas y 42 platos sobre la mesa.",
    "El total fue 1.250 unidades y el promedio 3,75.",
    "El plan cuesta 19,99 dólares al mes y el anual 199 dólares al año.",
    "Son 1.234,56 pesos mexicanos más impuestos.",
    "La reunión es a las 9:30 de la mañana en la sala principal.",
    "El evento empieza el 31/12/2026 a las 23:15.",
    "Ella quedó en 2º lugar y él en 1ª posición.",
    "Recorrimos 120 km/h durante 2 h y gastamos 8 GB de datos.",
    "El Sr. Pérez y la Dra. Gómez llegaron con 5 kg de equipo.",
    "Escríbeme a https://ezagent.dev o llama al 555-1234.",
    "Mira este [enlace](https://example.com) y dime qué opinas.",
    "**Importante**: revisa el archivo `config.json` antes de continuar.",
    "| Producto | Precio | Stock |\n|---|---|---|\n| Café | 25 | 10 |",
    "Listo 🙂, el despliegue salió bien ✅ y el equipo está feliz 🚀.",
    "¿Qué pasó con la canción de José Ángel? ¡No lo sé!",
    "El 45% de los 1.000 encuestados prefiere la opción A/B.",
    "Del 1/3/2026 al 31/5/2026 hay 90 días, aproximadamente 12 semanas.",
    "Compra 1º: 2 GB por $15; compra 2ª: 500 MB por $5.",
    "Resumen: 1.200 usuarios, 98,5% de disponibilidad y 3 h de soporte.",
    "EZAGENT funciona en Windows, macOS y Linux.",
    "La temperatura bajó a -5 °C y luego subió a 30 °C.",
    "El paquete pesa 80 kg y mide 15 cm por 3 mm.",
    "Quedan 250 ml de agua y 300 ms de espera.",
    "n.º 5 y N.º 7 llegaron el p. ej. día de la entrega.",
    "El Dr. House y la Sra. Gómez pidieron 2 s de silencio.",
    "EE. UU. y México firmaron el acuerdo a. C. y d. C. en el texto.",
    "La Av. Reforma y la calle C/ Mayor 5 están cerradas.",
    "El correo dice: Uds. pueden pasar por el paquete.",
    "y/o sí/no: él/ella decide la opción A/B/C.",
    "Tenemos 24/7 soporte y 1/2 del equipo de guardia.",
    "El rango es 10-20 unidades y el plan cuesta 20 dólares por usuario.",
    "Llama al +34 600 123 456 o al +52 55 1234 5678.",
    "Plan de 2 meses sin costo: 10 dólares al mes.",
    "La tasa es 5 GB/s y el límite 100 peticiones/hora.",
    "3/4 de los clientes pagaron 15,50 euros por el servicio.",
    "La factura 2026-11-13 incluye el 13-11-2026 y el 32/13/2026.",
    "El número 2026 y el código 1234567890 aparecen en el reporte.",
    "Menos 7 grados a las 0:05 de la madrugada.",
    "El costo aproximado es $1k por mes o $2,5k por año.",
    "La empresa vale 2.5M USD y factura 1200 MXN por día.",
    "35 COP, 500 ARS, 3 CLP, 10 PEN y 7 BRL en la tabla.",
    "El 1 millón de dólares se reparte entre 1/8 del equipo.",
    "Ingresos: 9/10 del total; pérdidas: 1/0 del reporte.",
    "El 3-4-5 y el 1.5-2.5 son rangos distintos.",
    "Un 20-30% más de velocidad y un 5/7 de mejora.",
    "La flecha → indica el siguiente paso y ≈ 10 el promedio.",
    "El símbolo ≥ 5 y ≤ 3 aparecen en la fórmula 5 × 3.",
    "El precio es 50% menor y el descuento es ~50 unidades.",
    "Texto con  dos  espacios,   tabulaciones\t y saltos\n\n\nde línea.",
    "Comillas «dobles» y “curvas”, guiones — y puntos suspensivos…",
    "cafe\u0301 con leche y A\u0303 en el mismo texto.",
    "İstanbul, ñandú y Ñire aparecen en el mapa.",
    "Emoji 👨‍👩‍👧 y bandera 🇲🇽 entre palabras.",
    "Del 45% al 50%: 5 puntos de mejora en 10 días.",
    "El servidor responde en 300 ms con 512 MB de memoria.",
    _LONG_A,
    _LONG_B,
    "",
    "   \n\t ",
]

ENGLISH: list[str] = [
    "Hello, this is a test of the English voice.",
    "The meeting starts at 9:30 and ends at noon.",
    "We served 1,250 users and the average was 3.75.",
    "It costs $19.99 per month or $199 per year.",
    "The report mentions https://example.com and the phone 555-1234.",
    "**Important**: check the README before you deploy.",
    "¿Qué? ¡No! Wait, that was Spanish punctuation.",
    "The quick brown fox jumps over the lazy dog, again and again.",
    "Numbers 1, 2, 3 and symbols 50%, ≥, ≈ appear here.",
    "EzAgent is ready; the English pipeline sounds natural.",
]

# Cambios respecto a los valores por defecto del Speaker, por índice de frase.
OVERRIDES: dict[int, dict[str, Any]] = {
    2: {"speed": 0.5}, 5: {"speed": 1.5}, 8: {"speed": 2.0},
    11: {"voice": "ef_dora+em_alex"},
    14: {"voice": "ef_dora + em_santa + em_alex"},
    17: {"trim": False}, 20: {"voice": "em_alex+nope"},
    57: {"voice": "ef_dora"}, 58: {"voice": "em_santa"},
}
EN_OVERRIDES = [
    {"lang": "en-us", "voice": "af_heart"}, {"lang": "en-us", "voice": "af_heart"},
    {"lang": "en-us", "voice": "am_adam"}, {"lang": "en-us", "voice": "af_heart"},
    {"lang": "en-us", "voice": "am_adam"}, {"lang": "en-gb", "voice": "bf_emma"},
    {"lang": "en-gb", "voice": "bf_emma"}, {"lang": "en-gb", "voice": "bf_emma"},
    {"lang": "en-gb", "voice": "bm_george"}, {"lang": "en-us", "voice": "af_bella"},
]

def _digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def _array_summary(array: Any) -> dict[str, Any]:
    data = np.ascontiguousarray(array, dtype=np.float32)
    return {"shape": [int(value) for value in data.shape],
            "sha256": _digest(data.tobytes()),
            "first": [float(value) for value in data.reshape(-1)[:8]]}


def _save_audio(name: str, audio: Any) -> dict[str, Any]:
    data = np.ascontiguousarray(audio, dtype=np.float32)
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.npy"
    np.save(path, data)
    samples = int(data.size)
    return {
        "file": f"tts/{name}.npy", "samples": samples, "sample_rate": RATE,
        "sha256": sha256(path), "pcm_sha256": _digest(data.tobytes()),
        "rms": float(np.sqrt(np.mean(data.astype(np.float64) ** 2))) if samples else 0.0,
        "peak": float(np.max(np.abs(data))) if samples else 0.0,
    }

def _timing_dict(timing: Timing) -> dict[str, Any]:
    return {"phoneme": timing.phoneme, "start": float(timing.start), "end": float(timing.end)}

@contextlib.contextmanager
def _env(values: dict[str, str | None]) -> Iterator[None]:
    saved = {name: os.environ.get(name) for name in values}
    try:
        for name, value in values.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        yield
    finally:
        for name, value in saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value


def _add(out: FixtureFile, case: str, fn: str, payload: Any, call: Any, key: str | None, convert: Any = None) -> None:
    """Ejecuta el oráculo; un fallo va a `error`, como en los demás generadores."""
    try:
        value = call()
    except Exception as error:  # el oráculo es el mensaje real de Python
        out.add_error(case, fn, payload, str(error))
        return
    out.add(case, fn, payload, value if key is None else {key: convert(value) if convert else value})


def _tone(seconds: float, amplitude: float = 0.4, frequency: float = 220.0) -> np.ndarray:
    samples = int(seconds * RATE)
    times = np.arange(samples, dtype=np.float64)
    return (amplitude * np.sin(2 * np.pi * frequency * times / RATE)).astype(np.float32)


def _silence(seconds: float) -> np.ndarray:
    return np.zeros(int(seconds * RATE), dtype=np.float32)


def _cases_normalize(out: FixtureFile) -> None:
    values: list[Any] = ["  EzAgent  ", "EZAGENT", "ezagent", "EzAgent, listo.", "", "   ", "\x0bHola\x0c", "x" * 600, None, 123]
    for index, value in enumerate(values, start=1):
        _add(
            out, f"tts/normalize/{index:04d}", "normalize_for_kokoro", {"text": value},
            lambda value=value: asyncio.run(normalize_for_kokoro(value)), "text",
        )


def _cases_tokenizer(out: FixtureFile) -> None:
    tokenizer = Tokenizer()
    strips: list[Any] = ["", "   ", "\x0b\x0c", "  hola  ", "\n\thola\r\n", "\u00a0hola\u00a0", "\u2003hola\u2003", "\x1chola\x1f", "hola mundo", None]
    for index, value in enumerate(strips, start=1):
        _add(out, f"tts/tokenizer/normalize_text/{index:04d}", "tokenizer.normalize_text", {"text": value}, lambda value=value: Tokenizer.normalize_text(value), "text")

    tokenize_cases = [
        ("", None), ("", 0), ("abc", None), ("hˈola", None), ("áéí", None), ("ñ", None), ("ü", None),
        ("🙂", None), ("中", None), ("e\u0301", None), ("a" * 510, 510), ("a" * 511, 510),
        ("a" * 511, None), ("a" * 5, 5), ("a" * 6, 5), ("abc", -1),
    ]
    for index, (phonemes, limit) in enumerate(tokenize_cases, start=1):
        _add(out, f"tts/tokenizer/tokenize/{index:04d}", "tokenizer.tokenize", {"phonemes": phonemes, "limit": limit}, lambda p=phonemes, lim=limit: tokenizer.tokenize(p, lim), "tokens")

    for index, phonemes in enumerate(["", "hˈola", "…", "á", "a b", "🙂e", "a" * 600], start=1):
        _add(out, f"tts/tokenizer/known/{index:04d}", "tokenizer.known", {"phonemes": phonemes}, lambda p=phonemes: tokenizer.known(p), "known")

    phonemize_cases: list[tuple[Any, str, bool]] = [
        ("Hola mundo.", "es", True), ("Hola mundo.", "en-us", True), ("Hola mundo.", "en-gb", True),
        ("Hola mundo.", "fr-fr", True), ("", "es", True), ("   ", "es", True),
        ("Hola, ¿cómo estás?", "es", True), ("El Sr. Pérez pagó $20/mes.", "es", True),
        ("1234567890", "es", True), ("¿Qué? ¡Sí!", "es", True), ("Listo 🙂", "es", True),
        ("EZAGENT funciona", "es", True), ("line one\nline two", "es", True),
        ("line one\nline two", "es", False), ("  hola  ", "es", False), ("x" * 600, "es", True),
        ("Hola mundo.", "xx", True), (None, "es", True),
    ]
    for index, (text, lang, norm) in enumerate(phonemize_cases, start=1):
        _add(out, f"tts/tokenizer/phonemize/{index:04d}", "tokenizer.phonemize", {"text": text, "lang": lang, "norm": norm}, lambda t=text, g=lang, n=norm: tokenizer.phonemize(t, g, n), "phonemes")

    for index, vocab in enumerate([None, {}, {"a": 1, "b": 2, " ": 3}, {"a": 1, "ab": 2, "ñ": 9}], start=1):
        custom = Tokenizer(vocab=vocab)
        _add(
            out, f"tts/tokenizer/vocab/{index:04d}", "tokenizer.vocab",
            {"vocab": vocab, "phonemes": "ab ñ"},
            lambda custom=custom: {"size": len(custom.vocab), "entries": [list(item) for item in sorted(custom.vocab.items())], "tokens": custom.tokenize("ab ñ")},
            "vocab",
        )


def _cases_chunking(out: FixtureFile) -> None:
    splits = [
        ("", 510), ("   ", 510), ("\n\t", 510), ("a", 510), ("", 0), ("a", 0),
        ("a" * 10, 10), ("a" * 11, 10),
        ("a" * 25, 10), ("uno dos tres", 5), ("uno dos tres cuatro", 9), ("uno. dos! tres?", 10),
        ("uno, dos; tres: cuatro", 12), ("uno. dos, tres; cuatro", 8), ("  hola  mundo  ", 6),
        ("a    b\tc\nd", 3), ("uno… dos…", 6), ("uno。 dos", 5), ("a" * 4 + " " + "b" * 7, 6),
        ("x" * 510, 510), ("x" * 511, 510), ("x" * 620, 510), ("ñ" * 12, 5), ("🙂" * 12, 5),
        ("uno dos", 1), ("a b c d e", 2), ("   palabra   ", 4), ("El final.", 3),
    ]
    for index, (phonemes, limit) in enumerate(splits, start=1):
        _add(out, f"tts/chunker/split/{index:04d}", "chunker.split_phonemes", {"phonemes": phonemes, "max_length": limit}, lambda p=phonemes, lim=limit: split_phonemes(p, lim), "batches")

    pauses = ["hola.", "hola!", "hola?", "hola…", "hola,", "hola;", "hola:", "hola", "hola .  ", "", "   ", "hola。", "final…\n"]
    for index, phonemes in enumerate(pauses, start=1):
        _add(out, f"tts/chunker/pause_after/{index:04d}", "chunker.pause_after", {"phonemes": phonemes, "sentence": 0.3, "clause": 0.12}, lambda p=phonemes: pause_after(p, 0.3, 0.12), "pause")
    _add(out, "tts/chunker/pause_after/0014", "chunker.pause_after", {"phonemes": "hola.", "sentence": 0.0, "clause": 0.0}, lambda: pause_after("hola.", 0.0, 0.0), "pause")


def _cases_pauses(out: FixtureFile) -> None:
    marks = [".", "!", "?", "…", ",", ";", ":", "a", "", " "]
    for index, phoneme in enumerate(marks, start=1):
        _add(out, f"tts/pauses/wanted_after/{index:04d}", "pauses.wanted_after", {"phoneme": phoneme, "sentence": 0.3, "clause": 0.12}, lambda p=phoneme: wanted_after(p, 0.3, 0.12), "pause")
    _add(out, "tts/pauses/wanted_after/0011", "pauses.wanted_after", {"phoneme": ".", "sentence": 0.0, "clause": 0.0}, lambda: wanted_after(".", 0.0, 0.0), "pause")

    cases = [
        ([_tone(0.2), _silence(0.05), _tone(0.2)], [Timing(".", 0.0, 0.22)], 0.3, 0.12, False),
        ([_tone(0.2), _tone(0.2)], [Timing(".", 0.0, 0.2)], 0.3, 0.12, False),
        ([_tone(0.2), _silence(0.3), _tone(0.1)], [Timing(".", 0.0, 0.2)], 0.3, 0.12, False),
        ([_tone(0.1), _silence(0.05), _tone(0.1), _silence(0.05), _tone(0.1)], [Timing(".", 0.0, 0.12), Timing(",", 0.12, 0.24)], 0.3, 0.12, False),
        ([_tone(0.1), _silence(0.05), _tone(0.1)], [Timing(".", 0.0, 0.1), Timing(",", 0.1, 0.15)], 0.0, 0.12, False),
        ([_tone(0.2), _silence(0.05), _tone(0.2)], [], 0.3, 0.12, False),
        ([_tone(0.2), _silence(0.05), _tone(0.2)], [Timing(".", 0.0, 0.22)], 0.0, 0.0, False),
        ([_tone(0.2), _silence(0.05), _tone(0.2)], [Timing(".", 0.0, 2.0)], 0.3, 0.12, False),
        ([_silence(0.5)], [Timing(".", 0.0, 0.2)], 0.3, 0.12, False),
        ([_silence(0.02), _tone(0.5)], [Timing(".", -0.5, -0.01)], 0.3, 0.12, False),
        ([_tone(0.2), _silence(0.05), np.concatenate([_tone(0.2), np.zeros(24, dtype=np.float32)])], [Timing(".", 0.0, 0.22)], 0.3, 0.12, False),
        ([np.zeros(100, dtype=np.float32)], [Timing(".", 0.0, 0.01)], 0.3, 0.12, True),
    ]
    for index, (parts, timings_in, sentence, clause, fails) in enumerate(cases, start=1):
        audio = np.concatenate(parts) if parts else np.zeros(0, dtype=np.float32)
        case = f"tts/pauses/insert/{index:04d}"
        payload = {
            "audio": _save_audio(f"pauses-{index:04d}-in", audio),
            "timings": [_timing_dict(item) for item in timings_in],
            "sample_rate": RATE, "sentence": sentence, "clause": clause,
        }
        if fails:
            _add(out, case, "pauses.insert", payload, lambda a=audio, t=timings_in, s=sentence, c=clause: insert_pauses(a, t, RATE, s, c), "result")
            continue
        output, moved = insert_pauses(audio, timings_in, RATE, sentence, clause)
        out.add(case, "pauses.insert", payload, {"audio": _save_audio(f"pauses-{index:04d}-out", output), "timings": [_timing_dict(item) for item in moved]})


def _cases_sliding(out: FixtureFile) -> None:
    edges = [
        ([2, 2, 2], 240), ([1, 1, 1, 1, 1, 1], 9), ([2], 3), ([], 100), ([], 0), ([1, 1], 1),
        ([0, 0], 0), ([5, 5], 0), ([1, 2, 3, 4], 100), ([3, 1, 4, 1, 5], 7), ([100], 100),
        ([1] * 100, 24000), ([2, 2], 1), ([-1, 2], 3),
    ]
    for index, (duration, samples) in enumerate(edges, start=1):
        def call(duration=duration, samples=samples):
            with np.errstate(invalid="ignore", divide="ignore"):
                return [int(edge) for edge in token_edges(np.asarray(duration, dtype=np.int64), samples)]
        _add(out, f"tts/sliding/token_edges/{index:04d}", "sliding.token_edges", {"duration": duration, "samples": samples}, call, "edges")

    timing_cases = [
        ("abc", [0, 100, 200, 300], 1000), ("abc", [0, 100], 1000), ("abc", [0], 1000),
        ("abc", [], 1000), ("", [0, 100, 200], 1000), ("ñ🙂", [0, 1, 2], 3),
        ("abc", [0, 1, 2, 3, 4, 5], 24000), ("ab", [0, 24000, 48000], 24000), ("a", [0, 1], 3),
    ]
    for index, (phonemes, values, rate) in enumerate(timing_cases, start=1):
        _add(out, f"tts/sliding/timings/{index:04d}", "sliding.timings", {"phonemes": phonemes, "edges": values, "sample_rate": rate}, lambda p=phonemes, e=values, r=rate: [_timing_dict(item) for item in timings(p, np.asarray(e), r)], "timings")

    run_spec = {"kind": "sine-v1", "samples_per_token": 240, "frames_per_token": 2, "gap_samples": 20, "amplitude": 0.05, "amplitude_step": 0.01, "amplitude_mod": 7, "base_hz": 120, "hz_step": 30, "hz_mod": 5}

    def fake_run(window: list[int]) -> tuple[np.ndarray, np.ndarray]:
        audio = np.zeros(run_spec["samples_per_token"] * len(window), dtype=np.float32)
        for position, token in enumerate(window):
            amplitude = run_spec["amplitude"] + run_spec["amplitude_step"] * (abs(token) % run_spec["amplitude_mod"])
            hz = run_spec["base_hz"] + (token % run_spec["hz_mod"]) * run_spec["hz_step"]
            wave = (amplitude * np.sin(2 * np.pi * hz * np.arange(240) / 240)).astype(np.float32)
            wave[:20] = 0.0
            wave[-20:] = 0.0
            audio[position * 240 : (position + 1) * 240] = wave
        return audio, np.full(len(window), run_spec["frames_per_token"], dtype=np.int64)

    synth_cases = [
        ([1], {}, [], []), ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], {}, [], []),
        ([2] * 400, {}, [5, 17], [16]), ([2] * 650, {}, [9], [16]),
        ([3] * 700, {"context": 0, "lookahead": 0}, [], [16]),
        ([1] * 120, {"keep": 50}, [7], [16]), ([4] * 400, {"crossfade": 0.0}, [5, 6], [16]),
        ([], {}, [], []),
    ]
    for index, (tokens, options, stops, spaces) in enumerate(synth_cases, start=1):
        merged = {"keep": 300, "context": 60, "lookahead": 60, "crossfade": 0.025, **options}
        payload = {"tokens": tokens, "sample_rate": RATE, "stops": stops, "spaces": spaces, **merged, "run": run_spec}
        _add(
            out, f"tts/sliding/synthesize/{index:04d}", "sliding.synthesize", payload,
            lambda t=tokens, s=stops, sp=spaces, m=merged: synthesize(fake_run, t, sample_rate=RATE, stops=frozenset(s), spaces=frozenset(sp), **m),
            "result",
        )


def _cases_trim(out: FixtureFile) -> None:
    tone = _tone(0.2)
    cases = [
        ("zeros", np.zeros(5000, dtype=np.float32)), ("empty", np.zeros(0, dtype=np.float32)),
        ("padded", np.concatenate([_silence(0.1), tone, _silence(0.1)])), ("tone", tone),
        ("lead", np.concatenate([_silence(0.1), tone])), ("tail", np.concatenate([tone, _silence(0.1)])),
        ("impulse", np.eye(1, 3000, 1000, dtype=np.float32)[0]), ("faint", np.full(4000, 1e-6, dtype=np.float32)),
        ("alternating", np.resize(np.asarray([0.5, -0.5], dtype=np.float32), 2048)),
        ("short", _tone(0.004)), ("frames", np.concatenate([_tone(0.05), _silence(0.08), _tone(0.05)])),
        ("burst", np.concatenate([_silence(0.2), _tone(0.2), _silence(0.2)])),
    ]
    for index, (name, audio) in enumerate(cases, start=1):
        trimmed, (start, end) = trim_audio(audio)
        out.add(
            f"tts/trim/{index:04d}", "trim.trim",
            {"name": name, "audio": _save_audio(f"trim-{index:04d}-in", audio)},
            {"start": int(start), "end": int(end), "audio": _save_audio(f"trim-{index:04d}-out", trimmed)},
        )


def _cases_voices(out: FixtureFile, kokoro: Kokoro) -> None:
    out.add("tts/voices/names/0001", "kokoro.get_voices", {}, {"voices": kokoro.get_voices()})
    for index, name in enumerate(("ef_dora", "nope"), start=1):
        _add(out, f"tts/voices/get/{index:04d}", "kokoro.get_voice_style", {"name": name}, lambda n=name: _array_summary(kokoro.get_voice_style(n)), "style")
    styles = ["em_alex", "ef_dora", "em_alex+ef_dora", "ef_dora + em_santa + em_alex", "em_alex+nope", " nope + ", "", "ef_dora++em_alex", "EF_DORA", "em_alex+ em_alex"]
    for index, voice in enumerate(styles, start=1):
        _add(out, f"tts/voices/style/{index:04d}", "voice._voice_style", {"voice": voice}, lambda v=voice: _array_summary(_voice_style(kokoro, v)), "style")
    reference = kokoro.get_voice_style("em_alex")
    for index, length in enumerate((0, 1, 2, 509, 510, 511), start=1):
        def summary(length=length):
            return {"index": min(length, len(reference)) - 1, **_array_summary(kokoro._style_for(reference, length))}
        _add(out, f"tts/voices/style_for/{index:04d}", "kokoro._style_for", {"voice": "em_alex", "length": length}, summary, "row")


def _cases_session(out: FixtureFile, kokoro: Kokoro, session: rt.InferenceSession) -> None:
    (Path.cwd() / "voices.bin").write_text("", encoding="utf-8")
    (Path.cwd() / "model.onnx").write_text("", encoding="utf-8")
    _add(out, "tts/session/input_dtypes/0001", "session.input_dtypes", {}, lambda: {name: str(dtype) for name, dtype in ksession.input_dtypes(session).items()}, "types")
    _add(out, "tts/session/embedded_vocab/0001", "session.embedded_vocab", {}, lambda: ksession.embedded_vocab(session), "vocab")
    for index, provider in enumerate(("CPUExecutionProvider", "NoSuchProvider"), start=1):
        with _env({"ONNX_PROVIDER": provider}):
            providers = ksession.resolve_providers()
        out.add(f"tts/session/providers/{index:04d}", "session.resolve_providers", {"env": {"ONNX_PROVIDER": provider}}, {"providers": providers})

    for index, (voices, model, fails) in enumerate([("voices.bin", "model.onnx", False), ("no-existe.bin", "model.onnx", True), ("voices.bin", "no-existe.onnx", True)], start=1):
        payload = {"model_path": model, "voices_path": voices}
        if fails:
            _add(out, f"tts/config/validate/{index:04d}", "config.KoKoroConfig.validate", payload, lambda m=model, v=voices: KoKoroConfig(m, v).validate(), "result")
        else:
            out.add(f"tts/config/validate/{index:04d}", "config.KoKoroConfig.validate", payload, {"ok": True})
    (Path.cwd() / "table.json").write_text(json.dumps({"vocab": {"a": 1, "ñ": 7}}, ensure_ascii=False), encoding="utf-8")
    (Path.cwd() / "no-vocab.json").write_text(json.dumps({"other": {"a": 1}}), encoding="utf-8")
    (Path.cwd() / "broken.json").write_text("{ no json", encoding="utf-8")
    vocabulary = [("dict", {"vocab": {"a": 1, "b": 2}}), ("empty", {}), ("file", "table.json"), ("no-vocab", "no-vocab.json"), ("broken", "broken.json"), ("missing", "missing.json")]
    for index, (name, value) in enumerate(vocabulary, start=1):
        _add(out, f"tts/kokoro/load_vocab/{index:04d}", "kokoro._load_vocab", {"kind": name, "value": value}, lambda v=value: kokoro._load_vocab(v), "vocab")
    out.add("tts/artifacts/0001", "tts.artifacts", {}, _artifact_summary(kokoro))


def _artifact_summary(kokoro: Kokoro) -> dict[str, Any]:
    model, voices, variant = ARTIFACTS or (Path(""), Path(""), "none")
    import espeakng_loader

    library = Path(espeakng_loader.get_library_path())

    def version(name: str) -> str:
        try:
            return importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            return ""
    return {
        "model": {"variant": variant, "name": model.name, "sha256": sha256(model), "bytes": model.stat().st_size},
        "voices": {"name": voices.name, "sha256": sha256(voices), "bytes": voices.stat().st_size},
        "espeak": {"library": library.name, "sha256": sha256(library), "loader": version("espeakng-loader")},
        "packages": {name: version(name) for name in ("kokoro-onnx", "onnxruntime", "numpy", "phonemizer")},
        "vocab_size": len(kokoro.tokenizer.vocab), "has_timings": bool(kokoro.has_timings),
    }
def _cases_resolve(out: FixtureFile) -> None:
    cases = [("int8", "model.onnx", "voices.bin"), ("fp32", "model.onnx", "voices.bin"), ("INT8", "model.onnx", "voices.bin"), ("raro", None, "voices.bin"), ("raro", "model.onnx", "voices.bin"), (None, None, "voices.bin"), ("", "model.onnx", "voices.bin")]
    for index, (variant, model, voices) in enumerate(cases, start=1):
        _add(out, f"tts/resolve_files/{index:04d}", "tts.resolve_kokoro_files", {"variant": variant, "model_path": model, "voices_path": voices}, lambda v=variant, m=model, w=voices: {"model": resolve_kokoro_files(variant=v, model_path=m, voices_path=w)[0], "voices": resolve_kokoro_files(variant=v, model_path=m, voices_path=w)[1]}, None)


def _batches_summary(batches: list[tuple[str, float]], kokoro: Kokoro, style: np.ndarray) -> list[dict[str, Any]]:
    rows = []
    for phonemes, pause in batches:
        tokens = kokoro.tokenizer.tokenize(phonemes)
        index = min(len(tokens), len(style)) - 1
        rows.append({"phonemes": phonemes, "tokens": [int(token) for token in tokens], "token_count": len(tokens), "pause": float(pause), "style": {"index": index, **_array_summary(style[index])}})
    return rows


def _utterance(out: FixtureFile, kokoro: Kokoro, index: int, spec: dict[str, Any]) -> None:
    payload = {key: spec[key] for key in ("text", "lang", "voice", "speed", "sentence_pause", "clause_pause", "is_phonemes", "trim")}
    case = f"tts/utterance/{index:04d}"
    kwargs = {key: spec[key] for key in ("speed", "lang", "is_phonemes", "trim", "sentence_pause", "clause_pause")}
    text = spec["text"]
    try:
        normalized = text if spec["is_phonemes"] else asyncio.run(normalize_for_kokoro(text))
        style = _voice_style(kokoro, spec["voice"])
        _, phonemes, batches = kokoro._prepare(normalized, style, spec["speed"], spec["lang"], spec["is_phonemes"], spec["sentence_pause"], spec["clause_pause"])
        audio, rate, spoken = kokoro.create_timed(normalized, voice=style, **kwargs)
    except Exception as error:
        out.add_error(case, "kokoro.create_timed", payload, str(error))
        return
    out.add(case, "kokoro.create_timed", payload, {
        "normalized": normalized, "phonemes": phonemes, "has_timings": bool(kokoro.has_timings),
        "batches": _batches_summary(batches, kokoro, style),
        "spoken": [_timing_dict(item) for item in spoken], "audio": _save_audio(f"utt-{index:04d}", audio)})


def _cases_utterances(out: FixtureFile, kokoro: Kokoro) -> None:
    corpus = [(text, OVERRIDES.get(index, {})) for index, text in enumerate(SPANISH, start=1)]
    corpus += [(text, overrides) for text, overrides in zip(ENGLISH, EN_OVERRIDES, strict=True)]
    for index, (text, overrides) in enumerate(corpus, start=1):
        _utterance(out, kokoro, index, {"text": text, **DEFAULTS, **overrides})


def _cases_extra(out: FixtureFile, kokoro: Kokoro) -> None:
    style = _voice_style(kokoro, "em_alex")
    cases = [
        ("is_phonemes", {"text": "ˈola mˈundo", "is_phonemes": True}, False, False),
        ("trim_false", {"text": "Una frase corta con silencio al inicio.", "trim": False}, False, False),
        ("speed_min", {"text": "Velocidad mínima.", "speed": 0.5}, False, False),
        ("speed_max", {"text": "Velocidad máxima.", "speed": 2.0}, False, False),
        ("speed_low", {"text": "Velocidad inválida.", "speed": 0.4}, True, False),
        ("speed_high", {"text": "Velocidad inválida.", "speed": 2.1}, True, False),
        ("speed_text", {"text": "Velocidad inválida.", "speed": "1"}, True, False),
        ("voice_name", {"text": "Voz por nombre.", "voice": "ef_dora"}, False, True),
        ("voice_missing", {"text": "Voz inexistente.", "voice": "nope"}, True, True),
        ("phonemes_unknown", {"text": "中", "is_phonemes": True}, True, False), ("empty", {"text": ""}, True, False),
    ]
    for index, (name, overrides, _fails, by_name) in enumerate(cases, start=1):
        spec = {"text": overrides.pop("text"), **DEFAULTS, **overrides}
        payload = {"name": name, **spec}
        kwargs = {key: spec[key] for key in ("speed", "lang", "is_phonemes", "trim", "sentence_pause", "clause_pause")}
        case = f"tts/create/{index:04d}"
        try:
            audio, rate, spoken = kokoro.create_timed(spec["text"], voice=spec["voice"] if by_name else style, **kwargs)
        except Exception as error:
            out.add_error(case, "kokoro.create_timed", payload, str(error))
            continue
        out.add(case, "kokoro.create_timed", payload, {"spoken": [_timing_dict(item) for item in spoken], "audio": _save_audio(f"create-{index:04d}", audio)})
    out.add_error("tts/create/0012", "kokoro.create_timed", {"continuous": True}, _error_of(lambda: kokoro.create_timed("Una frase cualquiera.", voice=style, continuous=True)))


def _error_of(call: Any) -> str:
    try:
        call()
    except Exception as error:
        return str(error)
    raise AssertionError("la llamada no falló")


async def _collect_stream(kokoro: Kokoro, style: np.ndarray, speed: float) -> list[tuple[np.ndarray, int]]:
    chunks = []
    async for audio, rate in kokoro.create_stream("Hola, esto es una prueba de transmisión.", voice=style, speed=speed, lang="es", sentence_pause=0.30, clause_pause=0.12):
        chunks.append((audio, rate))
    return chunks


def _cases_stream(out: FixtureFile, kokoro: Kokoro) -> None:
    style = _voice_style(kokoro, "em_alex")
    chunks = asyncio.run(_collect_stream(kokoro, style, 0.95))
    out.add("tts/stream/0001", "kokoro.create_stream", {"text": "Hola, esto es una prueba de transmisión.", "speed": 0.95}, {
        "chunks": [{"samples": int(audio.size), "sample_rate": int(rate), "pcm_sha256": _digest(np.ascontiguousarray(audio, dtype=np.float32).tobytes())} for audio, rate in chunks],
        "total_samples": int(sum(audio.size for audio, _ in chunks))})
    out.add_error("tts/stream/0002", "kokoro.create_stream", {"text": "Hola.", "speed": 3.0}, _error_of(lambda: asyncio.run(_collect_stream(kokoro, style, 3.0))))


def generate(out: FixtureFile) -> None:
    original = os.getcwd()
    os.chdir(Path(os.environ["HOME"]))
    try:
        _cases_normalize(out)
        _cases_tokenizer(out)
        _cases_chunking(out)
        _cases_pauses(out)
        _cases_sliding(out)
        _cases_trim(out)
        _cases_resolve(out)
        if ARTIFACTS is None:
            return
        model, voices, _variant = ARTIFACTS
        options = rt.SessionOptions()
        options.intra_op_num_threads = options.inter_op_num_threads = 1
        session = rt.InferenceSession(str(model), sess_options=options, providers=["CPUExecutionProvider"])
        kokoro = Kokoro.from_session(session, str(voices))
        _cases_voices(out, kokoro)
        _cases_session(out, kokoro, session)
        _cases_utterances(out, kokoro)
        _cases_extra(out, kokoro)
        _cases_stream(out, kokoro)
    finally:
        os.chdir(original)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
