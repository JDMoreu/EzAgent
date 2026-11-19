# Detiene el mensajero de voz de EzAgent.
# Uso: powershell -ExecutionPolicy Bypass -File scripts\stop-voice.ps1

Get-CimInstance Win32_Process |
    Where-Object {
        $_.Name -eq "python.exe" -and
        $_.CommandLine -like "*-m ezagent.voice*"
    } |
    ForEach-Object {
        Write-Host "Deteniendo ezagent-voice (PID $($_.ProcessId))..."
        Stop-Process -Id $_.ProcessId -Force
    }

# DX-3: el servidor que lanzó la app se anota por PID en un archivo de estado
# propio, para cerrar ese y no otro whisper-server de la persona.
$statePath = Join-Path $env:APPDATA "EzAgent\whisper-server.json"
if (Test-Path $statePath) {
    try {
        $state = Get-Content -Raw -Path $statePath | ConvertFrom-Json
        $serverPid = [int]$state.pid
        $server = Get-Process -Id $serverPid -ErrorAction SilentlyContinue
        if ($server -and $server.ProcessName -eq "whisper-server") {
            Write-Host "Deteniendo whisper-server de EzAgent (PID $serverPid)..."
            Stop-Process -Id $serverPid -Force
        }
    } catch {
        Write-Host "No pude leer $statePath; reviso por línea de comandos."
    }
    Remove-Item -Path $statePath -ErrorAction SilentlyContinue
}

# El mensajero de la versión Python arranca su whisper-server en el puerto 8088.
Get-CimInstance Win32_Process |
    Where-Object {
        $_.Name -eq "whisper-server.exe" -and
        $_.CommandLine -like "*--port 8088*"
    } |
    ForEach-Object {
        Write-Host "Deteniendo whisper-server del mensajero (PID $($_.ProcessId))..."
        Stop-Process -Id $_.ProcessId -Force
    }
