"""Trazabilidad del chat: fecha y hora debajo de cada mensaje."""

from __future__ import annotations

import time
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

from ezagent.voice.chat import trace
from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore
from ezagent.voice.hud import chat_labels
from ezagent.voice.hud.text import baseline_top, cap_height
from ezagent.voice.hud.tokens import FS_MICRO


def _msg(id_: int, role: str, t: float, streaming: bool = False) -> SimpleNamespace:
    return SimpleNamespace(id=id_, role=role, t=t, streaming=streaming)


def test_stamp_has_date_and_time_in_local_time() -> None:
    t = time.mktime((2026, 11, 13, 16, 46, 12, 0, 0, -1))
    assert trace.stamp(t) == "13 nov 2026 · 16:46:12"


def test_only_finished_user_and_agent_messages_get_a_stamp() -> None:
    t = time.mktime((2026, 11, 13, 16, 46, 17, 0, 0, -1))
    assert trace.stamp_for(_msg(1, "user", t)) == "13 nov 2026 · 16:46:17"
    assert trace.stamp_for(_msg(2, "assistant", t)) == "13 nov 2026 · 16:46:17"
    assert trace.stamp_for(_msg(3, "assistant", t, streaming=True)) == ""
    assert trace.stamp_for(_msg(4, "system", t)) == ""


def test_the_stamp_never_says_how_long_the_reply_took() -> None:
    assert "respondió" not in trace.stamp_for(_msg(2, "assistant", time.time()))
    assert not hasattr(trace, "reply_times")


def test_a_reply_keeps_the_time_it_finished_in_the_history(tmp_path: Path) -> None:
    path = tmp_path / "chat.jsonl"
    model = ChatModel(ChatStore(path))
    model.add_user("hola")
    reply = model.begin_assistant()
    model.append(reply.id, "qué tal")
    model.finish(reply.id)
    _, live = model.snapshot()
    _, again = ChatModel(ChatStore(path)).snapshot()
    assert [trace.stamp_for(m) for m in live] == [trace.stamp_for(m) for m in again]


# ---------------------------------------------------------------- dibujo
def _calls(fn, *args) -> list[tuple]:
    calls: list[tuple] = []
    fn(lambda text, style, color, x, y, align: calls.append((text, style, x, y, align)), *args)
    return calls


def test_the_stamp_goes_below_the_bubble_aligned_with_its_edge() -> None:
    t = time.time()
    ((_, _, x, y, align),) = _calls(
        chat_labels.draw_stamp, _msg(1, "user", t), 100.0, 200.0, 50.0, (0, 0, 0), 1.0
    )
    assert align == "right" and x == pytest.approx(300.0 - chat_labels.STAMP_INSET)
    assert y > 50.0  # debajo del borde inferior de la burbuja
    ((_, _, x, _, align),) = _calls(
        chat_labels.draw_stamp, _msg(2, "assistant", t), 16.0, 400.0, 50.0, (0, 0, 0), 1.0
    )
    assert align == "left" and x == pytest.approx(16.0 + chat_labels.STAMP_INSET)


def test_the_label_above_is_placed_by_its_baseline_even_with_accents() -> None:
    ((label, _, _, y, align),) = _calls(
        chat_labels.draw_label, "TÚ · AUDIO", 400.0, 10.0, (0, 0, 0), 1.0, True
    )
    base = 10.0 + 2 + cap_height(FS_MICRO, 1.0)
    assert label == "TÚ · AUDIO" and align == "right"
    assert y == pytest.approx(baseline_top(label, FS_MICRO, base, 1.0))


def test_the_chat_reserves_a_line_below_each_message(tmp_path: Path) -> None:
    import sys

    sys.path.insert(0, str(Path(__file__).parent))
    from test_chat_view import make

    model, view = make(tmp_path)
    model.add_user("hola")
    reply = model.begin_assistant()
    model.append(reply.id, "qué tal")
    model.finish(reply.id)
    drawn: list[tuple[str, float]] = []
    original = view._text

    def spy(buf, text, style, color, x, y, align="left"):
        drawn.append((text, y))
        return original(buf, text, style, color, x, y, align)

    view._text = spy
    view.render(1.0)
    stamps = [y for text, y in drawn if text == trace.stamp_for(model.snapshot()[1][0])]
    labels = {text: y for text, y in drawn if text in ("TÚ", "EZAGENT")}
    assert stamps and labels
    assert min(stamps) > labels["TÚ"]  # la hora va debajo del mensaje, no junto al rótulo
    assert np.isfinite(min(stamps))
