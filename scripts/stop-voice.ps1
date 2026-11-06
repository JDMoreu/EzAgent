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

# El mensajero arranca su propio whisper-server en el puerto 8088.
Get-CimInstance Win32_Process |
    Where-Object {
        $_.Name -eq "whisper-server.exe" -and
        $_.CommandLine -like "*--port 8088*"
    } |
    ForEach-Object {
        Write-Host "Deteniendo whisper-server del mensajero (PID $($_.ProcessId))..."
        Stop-Process -Id $_.ProcessId -Force
    }
