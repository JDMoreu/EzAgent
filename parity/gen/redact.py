"""Fixtures de redacción de secretos (dominio `redact`, F1-02).

`redact` es total: no lanza para ninguna cadena, así que los límites que se fijan
aquí son cadena vacía, secretos ya redactados, tokens casi-secreto (un carácter
de menos) y texto Unicode alrededor del secreto.
"""

from __future__ import annotations

from _common import FixtureFile, main_guard

from ezagent.voice.redact import redact

DOMAIN = "redact"

_HEX32 = "d41d8cd98f00b204e9800998ecf8427e"
_HEX31 = "d41d8cd98f00b204e9800998ecf8427"
_B64 = "AbC123xyzAbC123xyzAbC123xyzAbC123xyz12"
_B64_PAD = "AbC123xyzAbC123xyzAbC123xyzAbC123xyz=="
_URLSAFE = "AbC123xyz-_AbC123xyz-_AbC123xyz-_ab"
_JWT = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abc"

CASES: list[str] = [
    # --- sin secretos -------------------------------------------------------
    "",
    "texto normal sin secretos",
    "tildes: ñáéíóú, comillas y emoji 🙂",
    "El proceso terminó en 1,25 s con 3 reintentos.",
    # --- tokens sk- ---------------------------------------------------------
    "sk-abc123def456",
    "sk-proj-ABCD1234xyz",
    "sk-a",
    "sk-ab",
    "sk-",
    "xsk-abc123",
    "clave sk-ABC_def-123 en medio",
    # --- bearer -------------------------------------------------------------
    "Authorization: Bearer abc.def.ghi",
    "bearer token123",
    "Bearer",
    "Bearer abc123; otra cosa",
    # --- pares clave=valor --------------------------------------------------
    "api_key=secret123",
    "api-key: abcd",
    "API_KEY=xyz",
    "x-api-key: 123456",
    "password=hunter2",
    'password: "hunter2"',
    "token='abc'",
    "secret: abcdef",
    "access_token=abc123",
    "auth-token=zzz",
    "passwd=pass",
    "SECRET=ABC",
    "token  =  valor",
    'password = "a b"',
    "'api_key': 'abc123'",
    "(api_key=abc123)",
    "?token=abc123&user=1",
    "api_key=[REDACTED]",
    "clave=ñandú",
    # --- credenciales en URL ------------------------------------------------
    "https://user:pass@example.com/x",
    "postgres://admin:s3cr3t@db:5432/ez",
    "ftp://a:b@host",
    "https://example.com:8443/x",
    # --- tokens largos hex/base64 ------------------------------------------
    _HEX32,
    _HEX31,
    "a" * 40,
    "A" * 40,
    "x" * 40,
    _B64,
    _B64_PAD,
    _URLSAFE,
    _JWT,
    "0123456789abcdef0123456789abcdef",
    "token=AbC123xyzAbC123xyzAbC123xyzAbC",
    # --- líneas reales ------------------------------------------------------
    "2026-01-01 INFO request POST /v1?api_key=abc123&x=1",
    "api_key=aaa123 y token=bbb456",
    "Error 500 en https://user:pass@api.example.com/v1?token=zzz con api_key=k1",
    "sk-live-999 Authorization: Bearer zzz.111 password=p q",
]


def generate(out: FixtureFile) -> None:
    for index, text in enumerate(CASES, start=1):
        out.add(f"redact/{index:04d}", "redact", {"text": text}, {"text": redact(text)})


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
