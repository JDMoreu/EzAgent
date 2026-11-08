# Arranca whisper-server (Vulkan o CPU) desde una carpeta local de EzAgent.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1 -Port 8088 -Language es
#   powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1 -WhisperHome "D:\whisper"
#
# Deja este proceso en primer plano; Ctrl+C lo detiene.

param(
    [int]$Port = 8088,
    [string]$Engine = "auto",
    [string]$Language = "es",
    [int]$Threads = 6,
    [string]$Server = "",
    [string]$Model = "",
    [string]$WhisperHome = ""
)

$ErrorActionPreference = "Stop"

if (-not $WhisperHome) {
    $WhisperHome = Join-Path $env:LOCALAPPDATA "EzAgent\whisper"
}

if (-not $Server) {
    $Server = Join-Path $WhisperHome "whisper-server.exe"
    if (-not (Test-Path $Server)) {
        $Server = Join-Path $WhisperHome "whisper_vulkan\whisper-server.exe"
    }
}
if (-not $Model)  { $Model  = Join-Path $WhisperHome "models\ggml-large-v3-turbo-q5_0.bin" }

if (-not (Test-Path $Server)) {
    throw "No encuentro whisper-server.exe en '$Server'. Pasa -Server o -WhisperHome."
}
if (-not (Test-Path $Model)) {
    throw "No encuentro el modelo en '$Model'. Pasa -Model o -WhisperHome."
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
