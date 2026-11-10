"""Literal, bounded transcription corrections shared by settings and STT."""

from __future__ import annotations

import re
from collections.abc import Iterable
from functools import lru_cache


def parse_rules(value: str) -> tuple[tuple[str, str], ...]:
    """Parse ``heard → written`` pairs separated by semicolons or newlines."""
    if len(value) > 12000:
        raise ValueError("El diccionario admite hasta 12000 caracteres.")
    rules: list[tuple[str, str]] = []
    seen: set[str] = set()
    for line in re.split(r"[;\r\n]+", value):
        if not line.strip():
            continue
        pair = re.split(r"→|=>", line, maxsplit=1)
        if len(pair) != 2 or not all(part.strip() for part in pair):
            raise ValueError("Usa origen → corrección; separa las reglas con punto y coma.")
        source, target = (" ".join(part.split()) for part in pair)
        if source.casefold() in seen:
            raise ValueError("Cada expresión de origen debe aparecer una sola vez.")
        seen.add(source.casefold())
        rules.append((source, target))
        if len(rules) > 100:
            raise ValueError("El diccionario admite hasta 100 correcciones.")
    return tuple(rules)


def normalize_rules(value: str) -> str:
    """Validate rules and produce the editable single-line representation."""
    return "; ".join(f"{source} → {target}" for source, target in parse_rules(value))


def transcription_hint(prompt: str, dictionary: str, agents: Iterable[str] = ()) -> str:
    """Suggest spellings to Whisper without rewriting ambiguous spoken names."""
    if prompt.strip():
        return prompt
    candidates = [target for _, target in parse_rules(dictionary)]
    candidates += [
        "EzAgent",
        "Herdr",
        "OpenCode",
        "Claude Code",
        "Codex",
        "Gemini",
        "GPT",
        "Qwen",
        "DeepSeek",
        "MiniMax",
    ]
    candidates += [str(name).replace("_", " ") for name in agents]
    words: list[str] = []
    seen: set[str] = set()
    for candidate in candidates:
        word = " ".join(candidate.split())[:64]
        if not word or word.casefold() in seen:
            continue
        if len(", ".join([*words, word])) > 512 or len(words) >= 24:
            break
        words.append(word)
        seen.add(word.casefold())
    # DECISION: these are spelling hints only; uncertain brand names are never
    # replaced automatically unless the user configured a dictionary rule.
    return ", ".join(words)


@lru_cache(maxsize=32)
def _compiled(value: str) -> tuple[re.Pattern[str] | None, dict[str, str]]:
    rules = sorted(parse_rules(value), key=lambda pair: len(pair[0]), reverse=True)
    if not rules:
        return None, {}
    alternatives = [
        f"(?P<rule{index}>" + r"\s+".join(re.escape(word) for word in source.split()) + ")"
        for index, (source, _) in enumerate(rules)
    ]
    pattern = re.compile(r"(?<!\w)(?:" + "|".join(alternatives) + r")(?!\w)", re.IGNORECASE)
    return pattern, {f"rule{index}": target for index, (_, target) in enumerate(rules)}


def correct_transcription(text: str, dictionary: str) -> str:
    """Apply complete words/phrases once, preserving unconfigured text and punctuation."""
    pattern, replacements = _compiled(dictionary)
    if pattern is None:
        return text
    # DECISION: one literal pass, longest phrase first, prevents cascaded
    # replacements and never treats a user's dictionary as executable regex.
    return pattern.sub(lambda match: replacements[match.lastgroup or ""], text)
