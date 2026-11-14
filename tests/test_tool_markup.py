"""El marcado interno de herramientas (DSML…) nunca llega a la voz ni al chat."""

from __future__ import annotations

import pytest

from ezagent.voice.tool_markup import TOOL_MARKUP_NOTICE, ToolMarkupGuard

DSML = '<｜DSML｜ calls>\n<｜DSML｜ invoke name="delegate_to_agent">'


def run(*chunks: str) -> tuple[str, ToolMarkupGuard]:
    guard = ToolMarkupGuard()
    out = "".join(guard.feed(chunk) for chunk in chunks) + guard.flush()
    return out, guard


def test_normal_text_passes_through_untouched() -> None:
    out, guard = run("Hola, ", "¿qué tal? a < b y <b>negrita</b>.")
    assert out == "Hola, ¿qué tal? a < b y <b>negrita</b>." and not guard.tripped


@pytest.mark.parametrize("size", [1, 2, 3, 5, 8])
def test_a_marker_split_in_any_way_is_cut_before_it_starts(size: int) -> None:
    text = "Listo. " + DSML + "<｜DSML｜ parameter>x"
    chunks = [text[i : i + size] for i in range(0, len(text), size)]
    out, guard = run(*chunks)
    assert out == "Listo. " and guard.tripped
    assert "DSML" not in out and "invoke" not in out


def test_text_after_a_tripped_marker_is_dropped() -> None:
    guard = ToolMarkupGuard()
    assert guard.feed("Va. <tool_call>{}") == "Va. "
    assert guard.feed("más basura") == "" and guard.flush() == ""


def test_a_held_prefix_is_released_when_the_stream_ends() -> None:
    out, guard = run("Mira <", "｜")  # parecía el inicio de un marcador y no lo fue
    assert out == "Mira <｜" and not guard.tripped


def test_the_notice_is_plain_spanish_and_claims_nothing_was_done() -> None:
    assert "No se hizo nada" in TOOL_MARKUP_NOTICE and "DSML" not in TOOL_MARKUP_NOTICE
