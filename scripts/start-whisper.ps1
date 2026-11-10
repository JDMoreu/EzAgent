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
    [string]$WhisperHome = "",
    [string]$VadModel = "",
    [switch]$NoVad
)

$ErrorActionPreference = "Stop"

if (-not $WhisperHome) {
    $WhisperHome = Join-Path $env:LOCALAPPDATA "EzAgent\whisper"
}

if (-not $Server) {
    # Vulkan primero (si el motor lo permite); la compilacion de CPU es el respaldo.
    $gpu = @("whisper-server.exe", "whisper_vulkan\whisper-server.exe", "vulkan\whisper-server.exe")
    $cpu = @("cpu\whisper-server.exe")
    $order = if ($Engine -ieq "cpu") { $cpu + $gpu } else { $gpu + $cpu }
    foreach ($rel in $order) {
        $candidate = Join-Path $WhisperHome $rel
        if (Test-Path $candidate) { $Server = $candidate; break }
    }
    if ($Server -and $Server -like "*\cpu\whisper-server.exe") { $Engine = "cpu" }
}
if (-not $Model)  { $Model  = Join-Path $WhisperHome "models\ggml-large-v3-turbo-q5_0.bin" }

# VAD Silero dentro del servidor: sin voz, Whisper devuelve texto vacio en vez de
# alucinar «Gracias.». Anade ~3 ms por frase. Si falta el modelo, arranca sin VAD.
if (-not $VadModel) {
    $VadModel = Join-Path (Split-Path -Parent $Model) "ggml-silero-v5.1.2.bin"
}
$UseVad = (-not $NoVad) -and (Test-Path $VadModel)

if (-not (Test-Path $Server)) {
    throw "No encuentro whisper-server.exe en '$Server'. Pasa -Server o -WhisperHome."
}
if (-not (Test-Path $Model)) {
    throw "No encuentro el modelo en '$Model'. Pasa -Model o -WhisperHome."
}

Write-Host "whisper-server : $Server"
Write-Host "modelo         : $Model"
Write-Host ("vad            : " + $(if ($UseVad) { $VadModel } else { "desactivado" }))
Write-Host "escuchando en : http://127.0.0.1:$Port/inference  (idioma $Language)"
Write-Host "Ctrl+C para detener."
Write-Host ""

$serverArgs = @(
    "-m", $Model, "--host", "127.0.0.1", "--port", "$Port",
    "-l", $Language, "-t", "$Threads"
)
if ($Engine -ieq "cpu") {
    $serverArgs += "-ng"
}
if ($UseVad) {
    $serverArgs += @("--vad", "--vad-model", $VadModel)
}
& $Server @serverArgs
