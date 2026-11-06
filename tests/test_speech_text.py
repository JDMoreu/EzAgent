import pytest

from ezagent.speech_text import normalize_for_kokoro, normalize_for_speech


@pytest.mark.asyncio
async def test_strips_markdown_urls_and_symbols():
    text = "**Hola**, Sr. Pérez: visita https://example.com y paga 15 € al 20%."
    out = await normalize_for_speech(text, "sentence")
    assert "**" not in out
    assert "https://" not in out
    assert "señor Pérez" in out
    assert "quince euros" in out
    assert "veinte por ciento" in out


@pytest.mark.asyncio
async def test_normalizes_units_and_abbreviations():
    out = await normalize_for_speech(
        "Quedan 5km, 300ms y 2 GB; p. ej. en el Dr. Ruiz.", "sentence"
    )
    assert "cinco kilómetros" in out
    assert "trescientos milisegundos" in out
    assert "dos gigabytes" in out
    assert "por ejemplo" in out
    assert "doctor Ruiz" in out


@pytest.mark.asyncio
async def test_numbers_dates_and_ordinals():
    out = await normalize_for_speech(
        "El 04/11/2026 a las 15:30 cuesta 1.234,56 €; el 3,5% y el 1º día.",
        "sentence",
    )
    assert "cuatro de noviembre de dos mil veintiséis" in out
    assert "quince y media" in out
    assert "mil doscientos treinta y cuatro coma cincuenta y seis euros" in out
    assert "tres coma cinco por ciento" in out
    assert "primero" in out


@pytest.mark.asyncio
async def test_preserves_regular_sentences():
    text = "Mañana hará 18 grados y el cielo estará despejado."
    out = await normalize_for_speech(text, "sentence")
    assert out == "Mañana hará dieciocho grados y el cielo estará despejado."


@pytest.mark.asyncio
async def test_kokoro_pronounces_brand_stably():
    out = await normalize_for_kokoro("EzAgent paga 15 €.", "sentence")
    assert out == "Es Agente paga quince euros."
