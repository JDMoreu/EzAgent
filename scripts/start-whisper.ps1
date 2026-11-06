# Arranca el whisper-server Vulkan que ya viene con Dicta, en un puerto propio.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1 -Port 8088 -Language es
#
# Deja este proceso en primer plano; Ctrl+C lo detiene.

param(
    [int]$Port = 8088,
    [string]$Engine = "auto",
    [string]$Language = "es",
    [int]$Threads = 6,
    [string]$Server = "",
    [string]$Model = "",
    [string]$DictaHome = ""
)

$ErrorActionPreference = "Stop"

if (-not $DictaHome) {
    $DictaHome = Join-Path (Split-Path $PSScriptRoot -Parent) "..\dicta"
    $DictaHome = [System.IO.Path]::GetFullPath($DictaHome)
}

if (-not $Server) { $Server = Join-Path $DictaHome "whisper_vulkan\whisper-server.exe" }
if (-not $Model)  { $Model  = Join-Path $DictaHome "models\ggml-large-v3-turbo-q5_0.bin" }

if (-not (Test-Path $Server)) {
    throw "No encuentro whisper-server.exe en '$Server'. Pasa -Server o -DictaHome."
}
if (-not (Test-Path $Model)) {
    throw "No encuentro el modelo en '$Model'. Pasa -Model o -DictaHome."
}

Write-Host "whisper-server : $Server"
Write-Host "modelo         : $Model"
Write-Host "escuchando en  : http://127.0.0.1:$Port/inference  (idioma $Language)"
Write-Host "Ctrl+C para detener."
Write-Host ""

$serverArgs = @(
    "-m", $Model, "--host", "127.0.0.1", "--port", "$Port",
    "-l", $Language, "-t", "$Threads"
)
if ($Engine -ieq "cpu") {
    $serverArgs += "-ng"
}
& $Server @serverArgs
