# DX-3b: humo reproducible de whisper-server atado a la vida de la app (DX-3).
#
# Uso: powershell -NoProfile -File scripts\tests\humo-whisper.ps1
#
# Requiere, desde rust/:
#   cargo build --release -p ez-app --bin ezagent-voice
#   cargo build --release -p ez-fakes --bin fake-whisper
#
# Pasos: 1) la app arranca, se mata sin aviso y el servidor muere con ella;
# 2) un segundo arranque reutiliza uno sano; 3) uno colgado se reemplaza;
# 4) no queda ningun proceso de la prueba.
#
# No toca la app ni el whisper-server de la persona: usa APPDATA y LOCALAPPDATA
# temporales y solo cierra los PIDs que anota este guion.

param(
    [string]$App = "",
    [string]$Fake = ""
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $App) { $App = Join-Path $repo "rust\target\release\ezagent-voice.exe" }
if (-not $Fake) { $Fake = Join-Path $repo "rust\target\release\fake-whisper.exe" }
foreach ($binary in @($App, $Fake)) {
    if (-not (Test-Path $binary)) {
        throw ("Falta {0}. Compila desde rust/: cargo build --release -p ez-app " +
            "--bin ezagent-voice ; cargo build --release -p ez-fakes --bin fake-whisper") -f $binary
    }
}

$root = Join-Path $env:TEMP ("dx3-humo-" + [guid]::NewGuid().ToString("N"))
$appdata = Join-Path $root "appdata"
$localappdata = Join-Path $root "localappdata"
$whisperHome = Join-Path $localappdata "EzAgent\whisper"
$models = Join-Path $whisperHome "models"
$statePath = Join-Path $appdata "EzAgent\whisper-server.json"
$configPath = Join-Path $appdata "voice.json"
$started = New-Object System.Collections.Generic.List[int]
$originalAppData = $env:APPDATA
$originalLocalAppData = $env:LOCALAPPDATA
$originalWhisperHome = $env:EZAGENT_WHISPER_HOME
$originalWhisperUrl = $env:WHISPER_URL
$originalWhisperEngine = $env:WHISPER_ENGINE
$originalHome = $env:EZAGENT_HOME
$originalLlm = @{}
foreach ($name in @("LLM_PROVIDER", "LLM_BASE_URL", "LLM_MODEL", "LLM_API_KEY", "LLM_API_STYLE")) {
    $originalLlm[$name] = [Environment]::GetEnvironmentVariable($name)
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    return $port
}

function Test-Port([int]$Port) {
    try {
        $client = [System.Net.Sockets.TcpClient]::new()
        $client.Connect("127.0.0.1", $Port)
        $client.Close()
        return $true
    } catch {
        return $false
    }
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Condition) { return $true }
        Start-Sleep -Milliseconds 200
    }
    return $false
}

function Assert-Alive([int]$ProcessId, [string]$Label) {
    if (-not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) {
        throw "$Label (PID $ProcessId) no esta vivo."
    }
}

function Assert-Dead([int]$ProcessId, [string]$Label) {
    if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
        throw "$Label (PID $ProcessId) sigue vivo."
    }
}

function Stop-OwnProcess([int]$ProcessId) {
    if (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
        Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
    }
}

function Start-VoiceApp {
    $process = Start-Process -PassThru -FilePath $App -ArgumentList @("--config", $configPath)
    $script:started.Add($process.Id)
    return $process
}

function Start-OwnWhisper([int]$Port, [string]$Exe) {
    $process = Start-Process -PassThru -FilePath $Exe -ArgumentList @("--port", "$Port")
    $script:started.Add($process.Id)
    return $process
}

function Write-State([int]$ProcessId, [int]$Port) {
    $json = @{
        pid = $ProcessId
        port = $Port
        url = "http://127.0.0.1:$Port/inference"
    } | ConvertTo-Json
    [System.IO.File]::WriteAllText($statePath, $json, [System.Text.UTF8Encoding]::new($false))
}

try {
    New-Item -ItemType Directory -Force -Path $appdata, $whisperHome, $models | Out-Null
    Copy-Item $Fake (Join-Path $whisperHome "whisper-server.exe")
    Set-Content -Path (Join-Path $models "ggml-large-v3-turbo-q8_0.bin") -Value "x" -NoNewline
    # Kokoro necesita estas carpetas para que la app siga viva; se copian de la
    # instalacion real (solo lectura) a la temporal. Si faltan, el humo avisa.
    foreach ($folder in @("espeak-ng", "onnxruntime")) {
        $source = Join-Path $originalLocalAppData "EzAgent\$folder"
        if (Test-Path $source) {
            Copy-Item -Recurse $source (Join-Path $localappdata "EzAgent\$folder")
        } else {
            Write-Host "Aviso: no encontre $source; la app puede no cargar Kokoro."
        }
    }
    # Un puerto cerrado en la config: el 8088 por omision es el whisper de la
    # persona y la app lo daria por bueno sin arrancar nada.
    $appPort = Get-FreePort
    $configJson = @{
        hud_hidden = $true
        llm_pinned = $true
        smart_turn = $false
        llm_provider = "opencode"
        llm_base_url = "http://127.0.0.1:9/v1"
        api_key = "humo"
        whisper_url = "http://127.0.0.1:$appPort/inference"
    } | ConvertTo-Json
    [System.IO.File]::WriteAllText($configPath, $configJson, [System.Text.UTF8Encoding]::new($false))

    $env:APPDATA = $appdata
    $env:LOCALAPPDATA = $localappdata
    $env:EZAGENT_WHISPER_HOME = $whisperHome
    $env:EZAGENT_HOME = ""
    $env:WHISPER_URL = "http://127.0.0.1:$appPort/inference"
    $env:WHISPER_ENGINE = "auto"
    $env:LLM_PROVIDER = "custom"
    $env:LLM_BASE_URL = "http://127.0.0.1:9/v1"
    $env:LLM_MODEL = "humo"
    $env:LLM_API_KEY = "humo"
    $env:LLM_API_STYLE = "openai"

    Write-Host "== 1. arranca, se mata sin aviso y el servidor muere =="
    $first = Start-VoiceApp
    if (-not (Wait-Until { Test-Path $statePath } 30)) {
        throw "La app no arranco whisper-server en 30 s."
    }
    $state = Get-Content -Raw $statePath | ConvertFrom-Json
    $serverPid = [int]$state.pid
    $started.Add($serverPid)
    Assert-Alive $serverPid "whisper-server del primer arranque"
    $server = Get-CimInstance Win32_Process -Filter "ProcessId=$serverPid"
    if ($server.ParentProcessId -ne $first.Id) {
        throw "El servidor no es hijo de la app (padre $($server.ParentProcessId), app $($first.Id))."
    }
    if (-not (Wait-Until { Test-Port ([int]$state.port) } 10)) {
        throw "El servidor no escucha en el puerto $($state.port)."
    }
    if ($first.HasExited) {
        throw "La app salio antes de la prueba (revisa Kokoro); no se puede matar sin aviso."
    }
    $peak = (Get-Process -Id $first.Id).PeakWorkingSet64
    Write-Host ("   app PID {0}, servidor PID {1}, puerto {2}, pico {3} MB" -f `
        $first.Id, $serverPid, $state.port, [math]::Round($peak / 1MB))
    Stop-Process -Id $first.Id -Force
    if (-not (Wait-Until { -not (Get-Process -Id $serverPid -ErrorAction SilentlyContinue) } 15)) {
        throw "El servidor (PID $serverPid) sobrevivio al cierre forzado de la app."
    }
    Write-Host "   OK: app $($first.Id) y servidor $serverPid muertos"

    Write-Host "== 2. un segundo arranque reutiliza uno sano =="
    $reusePort = Get-FreePort
    $manual = Start-OwnWhisper $reusePort (Join-Path $whisperHome "whisper-server.exe")
    if (-not (Wait-Until { Test-Port $reusePort } 10)) {
        throw "El servidor manual no escucha en $reusePort."
    }
    Write-State $manual.Id $reusePort
    $second = Start-VoiceApp
    Start-Sleep -Seconds 4
    Assert-Alive $manual.Id "servidor manual reutilizado"
    $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($second.Id)")
    $spawned = @($children | Where-Object { $_.Name -eq "whisper-server.exe" })
    if ($spawned.Count -ne 0) {
        throw "La app lanzo otro whisper-server (PIDs $($spawned.ProcessId -join ', ')) en vez de reutilizar."
    }
    $reused = Get-Content -Raw $statePath | ConvertFrom-Json
    if ([int]$reused.pid -ne $manual.Id) {
        throw "El estado cambio de PID ($($reused.pid) en vez de $($manual.Id))."
    }
    Write-Host "   OK: reutiliza el PID $($manual.Id) en el puerto $reusePort"
    Stop-Process -Id $second.Id -Force
    Start-Sleep -Milliseconds 800
    Assert-Alive $manual.Id "servidor manual tras matar la app"
    Stop-OwnProcess $manual.Id
    Write-Host "   OK: el servidor manual no estaba en el Job y se cierra aparte"

    Write-Host "== 3. uno colgado se reemplaza =="
    $hungDir = Join-Path $root "hung"
    New-Item -ItemType Directory -Force -Path $hungDir | Out-Null
    Copy-Item (Join-Path $env:SystemRoot "System32\PING.EXE") (Join-Path $hungDir "whisper-server.exe")
    $hung = Start-Process -PassThru -FilePath (Join-Path $hungDir "whisper-server.exe") `
        -ArgumentList @("-n", "60", "127.0.0.1")
    $started.Add($hung.Id)
    Start-Sleep -Milliseconds 500
    Assert-Alive $hung.Id "servidor colgado"
    $deadPort = Get-FreePort
    Write-State $hung.Id $deadPort
    $third = Start-VoiceApp
    if (-not (Wait-Until {
        (Test-Path $statePath) -and
        ([int](Get-Content -Raw $statePath | ConvertFrom-Json).pid -ne $hung.Id)
    } 30)) {
        throw "La app no reemplazo el estado del servidor colgado."
    }
    $replaced = Get-Content -Raw $statePath | ConvertFrom-Json
    $started.Add([int]$replaced.pid)
    Assert-Dead $hung.Id "servidor colgado"
    if ($third.HasExited) {
        Write-Host "   Aviso: la app salio antes (Kokoro no disponible); el Job cerro el servidor nuevo."
    } else {
        Assert-Alive ([int]$replaced.pid) "servidor nuevo"
        if (-not (Wait-Until { Test-Port ([int]$replaced.port) } 10)) {
            throw "El servidor nuevo no escucha en $($replaced.port)."
        }
        Write-Host "   OK: colgado $($hung.Id) cerrado; nuevo PID $($replaced.pid) en $($replaced.port)"
        Stop-Process -Id $third.Id -Force
        if (-not (Wait-Until { -not (Get-Process -Id ([int]$replaced.pid) -ErrorAction SilentlyContinue) } 15)) {
            throw "El servidor nuevo ($($replaced.pid)) sobrevivio."
        }
        Write-Host "   OK: el servidor nuevo muere con la app"
    }

    Write-Host "== 4. no quedan procesos de la prueba =="
    $ours = @($started | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    if ($ours.Count -ne 0) {
        throw "Quedaron procesos de la prueba: $($ours -join ', ')"
    }
    $others = @(Get-Process whisper-server -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    Write-Host "   OK: ninguno de los PIDs de la prueba sigue vivo; siguen los de la persona: $($others -join ', ')"
    Write-Host "HUMO: TODO OK"
}
finally {
    foreach ($processId in $started) {
        Stop-OwnProcess $processId
    }
    $env:APPDATA = $originalAppData
    $env:LOCALAPPDATA = $originalLocalAppData
    $env:EZAGENT_WHISPER_HOME = $originalWhisperHome
    $env:EZAGENT_HOME = $originalHome
    $env:WHISPER_URL = $originalWhisperUrl
    $env:WHISPER_ENGINE = $originalWhisperEngine
    foreach ($name in $originalLlm.Keys) {
        [Environment]::SetEnvironmentVariable($name, $originalLlm[$name])
    }
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
