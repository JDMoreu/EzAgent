# -*- mode: python ; coding: utf-8 -*-
"""Build reproducible del ejecutable Windows de EzAgent.

Se ejecuta con:
    .venv\\Scripts\\python.exe -m PyInstaller --clean --noconfirm packaging\\EzAgent.spec

El resultado es ``dist/EzAgent/EzAgent.exe`` (onedir). Whisper se instala en
``%LOCALAPPDATA%\EzAgent\whisper`` (binarios y modelos); Kokoro descarga sus
modelos a la caché del usuario la primera vez.
"""

from __future__ import annotations

from pathlib import Path

from PyInstaller.utils.hooks import collect_all

ROOT = Path(SPECPATH).parent
SRC = ROOT / "src"

datas = [
    (str(SRC / "ezagent" / "voice" / "assets"), "ezagent/voice/assets"),
    (str(SRC / "ezagent" / "agent" / "default_skills"), "ezagent/agent/default_skills"),
    (str(ROOT / "scripts"), "scripts"),
    (str(ROOT / "assets"), "assets"),
]
binaries = []
hiddenimports = []

for package in (
    "kokoro_onnx",
    "onnxruntime",
    "ddgs",
    "bs4",
    "lxml",
    "yaml",
    "send2trash",
    "pystray",
):
    pkg_datas, pkg_binaries, pkg_hidden = collect_all(package)
    datas += pkg_datas
    binaries += pkg_binaries
    hiddenimports += pkg_hidden

a = Analysis(
    [str(ROOT / "packaging" / "launcher.py")],
    pathex=[str(SRC)],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["pytest", "pytest_asyncio"],
    noarchive=False,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="EzAgent",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
    icon=str(ROOT / "packaging" / "icon.ico"),
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name="EzAgent",
)
