"""Imágenes y documentos del chat: se muestra lo seguro, se abre solo con permiso."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from ezagent.voice.chat import media


def _png(path: Path, size=(40, 20)) -> Path:
    Image.new("RGB", size, (200, 30, 30)).save(path)
    return path


# ---------------------------------------------------------------- rutas
def test_absolute_and_file_urls_resolve(tmp_path: Path) -> None:
    target = tmp_path / "a b.txt"
    assert media.resolve_target(str(target)) == target
    uri = "file:///" + str(target).replace("\\", "/").replace(" ", "%20")
    assert media.resolve_target(uri) == target


def test_relative_paths_only_resolve_inside_known_bases(tmp_path: Path) -> None:
    (tmp_path / "x.txt").write_text("hola", encoding="utf-8")
    assert media.resolve_target("x.txt", [tmp_path]) == tmp_path / "x.txt"
    assert media.resolve_target("x.txt") is None
    assert media.resolve_target("x.txt", [tmp_path / "otra"]) is None
    assert media.resolve_target("") is None and media.resolve_target("   ") is None


def test_home_is_expanded() -> None:
    path = media.resolve_target("~/algo.txt")
    assert path is not None and path.is_absolute() and "~" not in str(path)


def test_is_within_handles_dotdot_and_siblings(tmp_path: Path) -> None:
    root = tmp_path / "ws"
    (root / "sub").mkdir(parents=True)
    inside = root / "sub" / "a.txt"
    inside.write_text("x", encoding="utf-8")
    assert media.is_within(inside, [root])
    assert not media.is_within(root / "sub" / ".." / ".." / "fuera.txt", [root])
    assert not media.is_within(tmp_path / "ws2" / "a.txt", [root])  # «ws2» no está en «ws»
    assert not media.is_within(inside, [])


# ---------------------------------------------------------------- fichas
def test_file_info_describes_a_file_without_reading_it(tmp_path: Path) -> None:
    doc = tmp_path / "Informe.DOCX"
    doc.write_bytes(b"x" * 2048)
    info = media.file_info(doc)
    assert info is not None
    assert (info.name, info.ext, info.size, info.kind) == ("Informe.DOCX", ".docx", 2048, "office")
    assert info.openable and info.allowed


def test_missing_files_and_folders_have_no_card(tmp_path: Path) -> None:
    assert media.file_info(tmp_path / "no-existe.pdf") is None
    assert media.file_info(tmp_path) is None
    assert media.file_info(None) is None


@pytest.mark.parametrize(
    "name", ["virus.exe", "run.bat", "x.cmd", "s.ps1", "a.lnk", "j.js", "v.vbs", "x.msi"]
)
def test_executables_and_scripts_are_never_openable(tmp_path: Path, name: str) -> None:
    path = tmp_path / name
    path.write_text("x", encoding="utf-8")
    info = media.file_info(path)
    assert info is not None and info.kind == "other" and not info.openable
    assert media.open_file(info) is False


def test_files_outside_the_allowed_roots_are_flagged(tmp_path: Path) -> None:
    allowed = tmp_path / "ok"
    allowed.mkdir()
    inside, outside = allowed / "a.txt", tmp_path / "b.txt"
    inside.write_text("x", encoding="utf-8")
    outside.write_text("y", encoding="utf-8")
    assert media.file_info(inside, [allowed]).allowed
    assert not media.file_info(outside, [allowed]).allowed


def test_open_and_reveal_refuse_files_outside_the_roots(monkeypatch, tmp_path: Path) -> None:
    opened: list[str] = []
    monkeypatch.setattr(media.os, "startfile", opened.append, raising=False)
    path = tmp_path / "a.pdf"
    path.write_bytes(b"%PDF")
    forbidden = media.file_info(path, [tmp_path / "otra"])
    assert media.open_file(forbidden) is False and media.reveal_file(forbidden) is False
    assert opened == []
    allowed = media.file_info(path, [tmp_path])
    assert media.open_file(allowed) is True and opened == [str(path)]


def test_reveal_uses_an_argument_list_and_no_shell(monkeypatch, tmp_path: Path) -> None:
    calls: list = []
    monkeypatch.setattr(media.subprocess, "Popen", lambda *a, **k: calls.append((a, k)))
    path = tmp_path / "a.txt"
    path.write_text("x", encoding="utf-8")
    assert media.reveal_file(media.file_info(path)) is True
    ((args, kwargs),) = calls
    assert args[0] == ["explorer", f"/select,{path}"]
    assert not kwargs.get("shell")


@pytest.mark.parametrize(
    ("size", "text"),
    [
        (0, "0 B"),
        (12, "12 B"),
        (820 * 1024, "820 KB"),
        (int(1.4 * 1024 * 1024), "1,4 MB"),
        (5 * 1024**3, "5,0 GB"),
    ],
)
def test_human_size(size: int, text: str) -> None:
    assert media.human_size(size) == text


# -------------------------------------------------------------- imágenes
def test_a_local_image_loads_as_rgba(tmp_path: Path) -> None:
    image = media.load_image(media.file_info(_png(tmp_path / "a.png")))
    assert image is not None and image.mode == "RGBA" and image.size == (40, 20)


def test_non_images_and_forbidden_images_do_not_load(tmp_path: Path) -> None:
    text = tmp_path / "a.txt"
    text.write_text("x", encoding="utf-8")
    assert media.load_image(media.file_info(text)) is None
    png = _png(tmp_path / "b.png")
    assert media.load_image(media.file_info(png, [tmp_path / "otra"])) is None
    assert media.load_image(None) is None


def test_a_corrupt_image_does_not_crash(tmp_path: Path) -> None:
    bad = tmp_path / "rota.png"
    bad.write_bytes(b"no soy una imagen")
    assert media.load_image(media.file_info(bad)) is None


def test_oversized_images_are_refused(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.setattr(media, "MAX_IMAGE_PIXELS", 100)
    assert media.load_image(media.file_info(_png(tmp_path / "grande.png", (50, 50)))) is None
    monkeypatch.setattr(media, "MAX_IMAGE_BYTES", 10)
    assert media.load_image(media.file_info(_png(tmp_path / "pesa.png"))) is None


def test_an_animated_gif_gives_only_its_first_frame(tmp_path: Path) -> None:
    frames = [Image.new("RGB", (10, 10), c) for c in ((255, 0, 0), (0, 255, 0))]
    path = tmp_path / "a.gif"
    frames[0].save(path, save_all=True, append_images=frames[1:])
    image = media.load_image(media.file_info(path))
    assert image is not None and image.size == (10, 10)


# --------------------------------------------------------- vista previa
def test_text_preview_truncates_and_says_there_is_more(tmp_path: Path) -> None:
    path = tmp_path / "notas.md"
    path.write_text("# Notas\n" + "línea larga " * 400, encoding="utf-8")
    text, more = media.text_preview(media.file_info(path))
    assert text.startswith("# Notas") and len(text) <= media.PREVIEW_CHARS and more is True


def test_short_text_has_no_more_marker(tmp_path: Path) -> None:
    path = tmp_path / "a.txt"
    path.write_text("corto", encoding="utf-8")
    assert media.text_preview(media.file_info(path)) == ("corto", False)


def test_control_characters_are_stripped_from_previews(tmp_path: Path) -> None:
    path = tmp_path / "a.txt"
    path.write_bytes(b"hola\x00\x1b[31m rojo\x07\r\nfin")
    text, _ = media.text_preview(media.file_info(path))
    assert "\x00" not in text and "\x1b" not in text and "\x07" not in text
    assert text.splitlines()[-1] == "fin"


def test_a_csv_preview_is_a_markdown_table(tmp_path: Path) -> None:
    path = tmp_path / "ventas.csv"
    path.write_text("mes,total\nenero,10\nfebrero,12\n", encoding="utf-8")
    text, more = media.text_preview(media.file_info(path))
    assert text.splitlines()[0] == "| mes | total |"
    assert text.splitlines()[1] == "|---|---|"
    assert "| febrero | 12 |" in text and more is False


def test_a_long_csv_is_cut_to_the_first_rows(tmp_path: Path) -> None:
    path = tmp_path / "grande.csv"
    path.write_text("a,b\n" + "".join(f"{i},{i}\n" for i in range(100)), encoding="utf-8")
    text, more = media.text_preview(media.file_info(path))
    assert len(text.splitlines()) <= media.PREVIEW_ROWS + 3 and more is True


def test_previews_only_for_allowed_text_files(tmp_path: Path) -> None:
    path = tmp_path / "a.txt"
    path.write_text("x", encoding="utf-8")
    assert media.text_preview(media.file_info(path, [tmp_path / "otra"])) is None
    assert media.text_preview(media.file_info(_png(tmp_path / "a.png"))) is None
    assert media.text_preview(None) is None
