"""Secret redaction shared by logs, metrics and the audit journal.

One filter for every diagnostic channel: API keys, bearer tokens, header or
query credentials, URL passwords and long hex/base64 tokens. Text only, so a
caller can redact a whole line without changing its shape.
"""

from __future__ import annotations

import re

_REDACTED = "[REDACTED]"

_URL_CREDENTIALS = re.compile(r"(?i)\b([a-z][a-z0-9+.-]*://)[^\s/:@]+:[^\s/@]+@")
_SK_TOKEN = re.compile(r"\bsk-[A-Za-z0-9][A-Za-z0-9_-]{2,}")
_BEARER = re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+")
_KEY_NAME = r"((?:x[-_])?api[-_]?key|access[-_]?token|auth[-_]?token|token|password|passwd|secret)"
_KEY_VALUE_QUOTED = re.compile(rf"(?i)\b{_KEY_NAME}(\s*[:=]\s*)([\"'])(.*?)\3")
_KEY_VALUE = re.compile(rf"(?i)\b{_KEY_NAME}(\s*[:=]\s*)([\"']?)(?!\[REDACTED\])([^\s\"',;&]+)")
_LONG_HEX = re.compile(r"\b[A-Fa-f0-9]{32,}\b")
_LONG_TOKEN = re.compile(
    r"(?<![A-Za-z0-9+/_-])"
    r"(?=[A-Za-z0-9+/_-]*\d)(?=[A-Za-z0-9+/_-]*[A-Za-z])"
    r"[A-Za-z0-9+/_-]{32,}={0,2}"
    r"(?![A-Za-z0-9+/_=-])"
)


def _redact_pair(match: re.Match[str]) -> str:
    name, separator, quote = match.group(1), match.group(2), match.group(3)
    return f"{name}{separator}{quote}{_REDACTED}{quote}"


def redact(text: str) -> str:
    """Return ``text`` with credentials replaced by ``[REDACTED]``."""
    if not text:
        return text
    text = _URL_CREDENTIALS.sub(rf"\1{_REDACTED}@", text)
    text = _SK_TOKEN.sub(_REDACTED, text)
    text = _BEARER.sub(f"Bearer {_REDACTED}", text)
    text = _KEY_VALUE_QUOTED.sub(_redact_pair, text)
    text = _KEY_VALUE.sub(_redact_pair, text)
    text = _LONG_HEX.sub(_REDACTED, text)
    text = _LONG_TOKEN.sub(_REDACTED, text)
    return text


__all__ = ["redact"]
