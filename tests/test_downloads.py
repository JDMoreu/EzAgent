"""Gestor de descargas: hash, reanudación, zip seguro y migración."""

from __future__ import annotations

import hashlib
import io
import zipfile
from pathlib import Path

import httpx
import pytest

from ezagent.core import downloads as dl
from ezagent.core.downloads import Asset, DownloadError


def _asset(data: bytes, **kw) -> Asset:
    base = dict(
        id="x", label="x", url="https://example.test/x.bin",
        sha256=hashlib.sha256(data).hexdigest(), size=len(data), dest="m/x.bin",
    )
    base.update(kw)
    return Asset(**base)


def _client(handler) -> httpx.Client:
    return httpx.Client(transport=httpx.MockTransport(handler))


def test_download_ok_y_progreso(tmp_path: Path) -> None:
    data = b"hola" * 1000
    seen: list[int] = []
    with _client(lambda r: httpx.Response(200, content=data)) as c:
        path = dl.download(
            _asset(data), root=tmp_path, client=c,
            progress=lambda _i, done, _t: seen.append(done),
        )
    assert path.read_bytes() == data
    assert seen and seen[-1] == len(data)
    assert not list(tmp_path.rglob("*.part"))


def test_hash_incorrecto_descarta_y_falla(tmp_path: Path) -> None:
    data = b"abc" * 100
    calls = {"n": 0}

    def handler(_r: httpx.Request) -> httpx.Response:
        calls["n"] += 1
        return httpx.Response(200, content=b"x" * len(data))

    with _client(handler) as c, pytest.raises(DownloadError):
        dl.download(_asset(data), root=tmp_path, client=c)
    assert calls["n"] == 2  # un reintento
    assert not (tmp_path / "m" / "x.bin").exists()
    assert not list(tmp_path.rglob("*.part"))


def test_reanuda_con_range(tmp_path: Path) -> None:
    data = bytes(range(256)) * 40
    asset = _asset(data)
    part = tmp_path / "m" / "x.bin.part"
    part.parent.mkdir(parents=True)
    part.write_bytes(data[:3000])
    seen: dict[str, str] = {}

    def handler(r: httpx.Request) -> httpx.Response:
        seen["range"] = r.headers.get("range", "")
        return httpx.Response(206, content=data[3000:])

    with _client(handler) as c:
        path = dl.download(asset, root=tmp_path, client=c)
    assert seen["range"] == "bytes=3000-"
    assert path.read_bytes() == data


def test_servidor_ignora_range_reinicia(tmp_path: Path) -> None:
    data = b"z" * 5000
    part = tmp_path / "m" / "x.bin.part"
    part.parent.mkdir(parents=True)
    part.write_bytes(b"z" * 100)
    with _client(lambda r: httpx.Response(200, content=data)) as c:
        path = dl.download(_asset(data), root=tmp_path, client=c)
    assert path.read_bytes() == data


def test_http_error(tmp_path: Path) -> None:
    with _client(lambda r: httpx.Response(404)) as c, pytest.raises(DownloadError):
        dl.download(_asset(b"a"), root=tmp_path, client=c)


def _zip(files: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for name, content in files.items():
            zf.writestr(name, content)
    return buf.getvalue()


def test_zip_con_prefijo_y_marcador(tmp_path: Path) -> None:
    blob = _zip({"Release/whisper-server.exe": b"exe", "Release/a.dll": b"d", "otro.txt": b"n"})
    asset = _asset(blob, dest="whisper/cpu", unzip=True, strip_prefix="Release/")
    with _client(lambda r: httpx.Response(200, content=blob)) as c:
        dl.download(asset, root=tmp_path, client=c)
    assert (tmp_path / "whisper/cpu/whisper-server.exe").read_bytes() == b"exe"
    assert not (tmp_path / "whisper/cpu/otro.txt").exists()
    assert dl.is_installed(asset, tmp_path)
    assert not list(tmp_path.rglob("*.zip"))


def test_zip_slip_rechazado(tmp_path: Path) -> None:
    blob = _zip({"Release/../../evil.txt": b"x"})
    asset = _asset(blob, dest="w", unzip=True, strip_prefix="Release/")
    with _client(lambda r: httpx.Response(200, content=blob)) as c, pytest.raises(DownloadError):
        dl.download(asset, root=tmp_path, client=c)
    assert not (tmp_path.parent / "evil.txt").exists()


def test_ya_instalado_no_descarga(tmp_path: Path) -> None:
    data = b"q" * 50
    asset = _asset(data)
    target = tmp_path / "m" / "x.bin"
    target.parent.mkdir(parents=True)
    target.write_bytes(data)

    def boom(_r: httpx.Request) -> httpx.Response:
        raise AssertionError("no debe tocar la red")

    with _client(boom) as c:
        assert dl.download(asset, root=tmp_path, client=c) == target


def test_adopt_solo_con_hash_oficial(tmp_path: Path) -> None:
    data = b"k" * 64
    asset = _asset(data)
    good = tmp_path / "src.bin"
    good.write_bytes(data)
    bad = tmp_path / "bad.bin"
    bad.write_bytes(b"j" * 64)
    root = tmp_path / "root"
    assert dl.adopt(asset, bad, root=root) is False
    assert dl.adopt(asset, good, root=root) is True
    assert (root / "m" / "x.bin").read_bytes() == data
    assert good.exists()  # la fuente no se toca


def test_manifest_valido_y_https() -> None:
    assets = dl.load_manifest()
    assert {"whisper-cpu", "whisper-model-turbo", "kokoro-int8", "kokoro-voices", "silero-vad"} <= set(assets)
    for a in assets.values():
        assert a.url.startswith("https://")
        assert len(a.sha256) == 64 and a.size > 0


def test_manifest_rechaza_http(tmp_path: Path) -> None:
    m = tmp_path / "m.toml"
    m.write_text(
        '[[asset]]\nid="a"\nurl="http://x/y"\nsha256="' + "0" * 64 + '"\nsize=1\ndest="d"\n',
        encoding="utf-8",
    )
    with pytest.raises(DownloadError):
        dl.load_manifest(m)


def test_data_dir_respeta_env(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("EZAGENT_HOME", str(tmp_path / "h"))
    assert dl.data_dir() == tmp_path / "h"
    assert (tmp_path / "h").is_dir()
