"""Normalización del texto para el TTS: lo que Kokoro recibe debe ser hablable."""

from __future__ import annotations

import random
import re

import pytest

from ezagent.speech_text import normalize_for_kokoro, normalize_for_speech


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("Hola **mundo**", "Hola mundo"),
        ("Usa `uv sync` ahora", "Usa uv sync ahora"),
        ("[la guía](https://example.com/guía) es útil", "la guía es útil"),
        ("![logo](https://x.io/a.png) listo", "logo listo"),
        ("Mira https://example.com/a?b=1 y ya", "Mira y ya"),
        ("# Título\ntexto", "Título\ntexto"),
        ("- uno\n- dos", "uno\ndos"),
        ("1. primero\n2) segundo", "primero\nsegundo"),
        ("Listo 🎉", "Listo"),
        ("   ", ""),
        ("", ""),
    ],
)
async def test_markdown_urls_y_emojis_se_limpian(raw: str, expected: str) -> None:
    assert await normalize_for_speech(raw) == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("2026", "dos mil veintiséis"),
        ("15 €", "quince euros"),
        ("50%", "cincuenta por ciento"),
        ("3 km", "tres kilómetros"),
        ("120 km/h", "ciento veinte kilómetros por hora"),
        ("-5", "menos cinco"),
        ("1.000", "mil"),
        ("1.500", "mil quinientos"),
        ("1,5", "uno coma cinco"),
        ("3.14", "tres coma catorce"),
        ("2,05", "dos coma cero cinco"),
        ("10:00", "diez en punto"),
        ("10:15", "diez y cuarto"),
        ("10:30", "diez y media"),
        ("10:45", "diez cuarenta y cinco"),
        ("25/12/2026", "veinticinco de diciembre de dos mil veintiséis"),
        ("1/3/26", "uno de marzo de dos mil veintiséis"),
        ("1º", "primero"),
        ("3ª", "tercera"),
        ("1er", "primer"),
        ("Sr. López", "señor López"),
        ("Dra. Pérez", "doctora Pérez"),
        ("EE. UU.", "Estados Unidos"),
        ("rock & roll", "rock y roll"),
    ],
)
async def test_numeros_unidades_y_abreviaturas_se_leen_en_voz_alta(raw: str, expected: str) -> None:
    assert await normalize_for_speech(raw) == expected


async def test_una_fecha_con_mes_imposible_no_se_inventa() -> None:
    spoken = await normalize_for_speech("31/13/2026")
    # No se fabrica un mes: los números se leen sueltos, sin «de <mes> de».
    assert " de " not in spoken
    assert spoken == "treinta y uno/trece/dos mil veintiséis"


async def test_numeros_largos_se_leen_digito_a_digito() -> None:
    # Teléfonos, ids y similares: 7 o más cifras no se dicen como cantidad.
    assert await normalize_for_speech("1234567") == ("uno dos tres cuatro cinco seis siete")


async def test_la_puntuacion_no_queda_separada_del_texto() -> None:
    assert await normalize_for_speech("Hola , mundo !") == "Hola, mundo!"


async def test_ezagent_se_pronuncia_es_agente_solo_en_kokoro() -> None:
    assert await normalize_for_kokoro("Hola EzAgent") == "Hola Es Agente"
    assert await normalize_for_kokoro("hola ezagent") == "hola Es Agente"
    assert await normalize_for_speech("Hola EzAgent") == "Hola EzAgent"
    # Solo la palabra completa: no toca palabras que la contengan.
    assert await normalize_for_kokoro("EzAgentes") == "EzAgentes"


async def test_la_normalizacion_es_idempotente_con_texto_ya_hablado() -> None:
    once = await normalize_for_speech("Son las 10:30 y cuesta 15 € en 3 km")
    assert await normalize_for_speech(once) == once


async def test_fuzz_enteros_nunca_dejan_cifras() -> None:
    rng = random.Random(20260101)
    for _ in range(300):
        number = rng.randint(0, 999_999)
        spoken = await normalize_for_speech(str(number))
        assert not re.search(r"\d", spoken), (number, spoken)
        assert spoken.strip() == spoken and spoken


async def test_fuzz_horas_nunca_dejan_cifras_ni_dos_puntos() -> None:
    rng = random.Random(7)
    for _ in range(200):
        text = f"{rng.randint(0, 23)}:{rng.randint(0, 59):02d}"
        spoken = await normalize_for_speech(text)
        assert not re.search(r"[\d:]", spoken), (text, spoken)


async def test_fuzz_texto_arbitrario_no_rompe_ni_deja_markdown() -> None:
    rng = random.Random(99)
    alphabet = "abc XYZ 0123456789 .,;:!?*_`#[]()<>/\\-+€%&$ñáéü\n\t"
    for _ in range(400):
        raw = "".join(rng.choice(alphabet) for _ in range(rng.randint(0, 80)))
        spoken = await normalize_for_speech(raw)
        assert "`" not in spoken and "**" not in spoken
        assert "  " not in spoken
        assert spoken == spoken.strip()
