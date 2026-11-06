# Arranca el mensajero de voz de EzAgent (Herdr + Kokoro + whisper.cpp).
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1 --mic 2
#
# Deja este proceso en primer plano; Ctrl+C lo detiene.

$ErrorActionPreference = "Stop"
$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"
$root = Split-Path $PSScriptRoot -Parent
$python = Join-Path $root ".venv\Scripts\python.exe"
if (-not (Test-Path $python)) {
    throw "No encuentro $python. Ejecuta 'uv sync' en $root."
}
Set-Location $root
& $python -m ezagent.voice @args
