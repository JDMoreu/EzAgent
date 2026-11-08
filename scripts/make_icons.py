"""Regenera los iconos de EzAgent desde ``ezagent.voice.brand``.

    uv run python scripts/make_icons.py
"""

from __future__ import annotations

from pathlib import Path

from ezagent.voice.brand import SVG, mark_image

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    assets = ROOT / "assets"
    packaging = ROOT / "packaging"
    assets.mkdir(exist_ok=True)
    packaging.mkdir(exist_ok=True)
    mark_image(256).save(assets / "icon.png")
    (assets / "icon.svg").write_text(SVG, encoding="utf-8")
    mark_image(256).save(
        packaging / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("Iconos: assets/icon.png, assets/icon.svg, packaging/icon.ico")


if __name__ == "__main__":
    main()
