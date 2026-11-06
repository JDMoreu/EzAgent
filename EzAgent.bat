@echo off
setlocal EnableExtensions EnableDelayedExpansion
chcp 65001 >nul
title EzAgent
cd /d "%~dp0"

set "ROOT=%~dp0"
set "PY=%ROOT%.venv\Scripts\python.exe"
set "PS=powershell -NoProfile -ExecutionPolicy Bypass"
set "PYTHONUTF8=1"
set "PYTHONIOENCODING=utf-8"

if /I "%~1"=="voice"    ( call :start_voice & exit /b )
if /I "%~1"=="stop"     ( call :stop_voice & exit /b )
if /I "%~1"=="status"   ( call :show_status & exit /b )
if /I "%~1"=="install"  ( call :install_all & exit /b )
if /I "%~1"=="mics"     ( call :list_mics & exit /b )
if /I "%~1"=="agents"   ( call :list_agents & exit /b )
if /I "%~1"=="shortcut" ( call :create_shortcut & exit /b )
if /I "%~1"=="menu"     goto menu
if not "%~1"=="" goto usage

:menu
cls
echo.
echo   ============================================
echo      EzAgent - mensajero con notch
echo   ============================================
echo.
echo     1. Iniciar mensajero de voz
echo        (notch + whisper + Kokoro)
echo     2. Detener mensajero + whisper
echo     3. Estado del sistema
echo     4. Instalar / actualizar dependencias
echo     5. Listar microfonos
echo     6. Listar agentes de Herdr
echo     7. Crear acceso directo en el escritorio
echo.
echo     0. Salir
echo.
set "choice="
set /p "choice=  Opcion: "
if "%choice%"=="1" ( call :start_voice & goto menu_pause )
if "%choice%"=="2" ( call :stop_voice & goto menu_pause )
if "%choice%"=="3" ( call :show_status & goto menu_pause )
if "%choice%"=="4" ( call :install_all & goto menu_pause )
if "%choice%"=="5" ( call :list_mics & goto menu_pause )
if "%choice%"=="6" ( call :list_agents & goto menu_pause )
if "%choice%"=="7" ( call :create_shortcut & goto menu_pause )
if "%choice%"=="0" exit /b 0
echo.
echo   Opcion no valida.
timeout /t 1 >nul
goto menu

:menu_pause
echo.
pause
goto menu

:usage
echo.
echo Uso:
echo   EzAgent.bat              abre el menu
echo   EzAgent.bat voice        inicia el mensajero de voz
echo   EzAgent.bat stop         detiene voz y whisper
echo   EzAgent.bat status       muestra el estado
echo   EzAgent.bat install      instala/actualiza dependencias
echo   EzAgent.bat mics         lista microfonos
echo   EzAgent.bat agents       lista agentes de Herdr
echo   EzAgent.bat shortcut     crea acceso directo en el escritorio
exit /b 0

rem ---------------------------------------------------------------- voz
:start_voice
call :ensure_venv || exit /b 1
call :ensure_env  || exit /b 1
call :ensure_herdr

call :voice_running
if not errorlevel 1 (
    echo.
    echo   El mensajero ya esta corriendo.
    exit /b 0
)

echo.
echo   Iniciando EzAgent Voice...
echo   La primera vez whisper-server tarda unos segundos.
start "EzAgent Voice" powershell -NoProfile -ExecutionPolicy Bypass -NoExit -File "%ROOT%scripts\start-voice.ps1"
timeout /t 2 >nul
exit /b 0

:ensure_venv
if exist "%PY%" exit /b 0
echo.
echo   No existe .venv.
call :install_all
if exist "%PY%" exit /b 0
echo   No se pudo preparar el entorno de Python.
exit /b 1

:ensure_env
if not exist "%ROOT%.env" (
    if exist "%ROOT%.env.example" (
        copy /Y "%ROOT%.env.example" "%ROOT%.env" >nul
        echo.
        echo   Se creo .env desde .env.example.
        start "" notepad "%ROOT%.env"
        echo   Elige LLM_PROVIDER y rellena su clave; guarda y vuelve.
        pause
    )
)
exit /b 0

:ensure_herdr
where herdr-app >nul 2>nul
if errorlevel 1 exit /b 0
herdr-app status >nul 2>nul
if errorlevel 1 (
    echo.
    echo   Abriendo Herdr...
    start "Herdr" herdr-app
    timeout /t 2 >nul
)
exit /b 0

:voice_running
%PS% -Command "$v=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'python.exe' -and $_.CommandLine -like '*-m ezagent.voice*' }); if ($v.Count -gt 0) { exit 0 } else { exit 1 }"
exit /b %errorlevel%

:stop_voice
powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT%scripts\stop-voice.ps1"
exit /b 0

:show_status
echo.
echo   Procesos
%PS% -Command "$v=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'python.exe' -and $_.CommandLine -like '*-m ezagent.voice*' }); if ($v.Count -gt 0) { 'Voice: RUNNING  PID ' + (($v | ForEach-Object ProcessId) -join ', ') } else { 'Voice: stopped' }; $w=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'whisper-server.exe' }); if ($w.Count -gt 0) { 'Whisper: RUNNING  PID ' + (($w | ForEach-Object ProcessId) -join ', ') } else { 'Whisper: stopped' }"
echo.
echo   Herdr
where herdr-app >nul 2>nul
if errorlevel 1 (
    echo   herdr-app: no encontrado en PATH
) else (
    herdr-app status 2>nul
)
echo.
pause
exit /b 0

:list_mics
call :ensure_venv || exit /b 1
"%PY%" -m ezagent.voice --list-mics
pause
exit /b 0

:list_agents
where herdr-app >nul 2>nul
if errorlevel 1 (
    echo   herdr-app no esta en PATH.
) else (
    herdr-app status
)
pause
exit /b 0

:install_all
echo.
echo   Instalando dependencias de Python con uv...
where uv >nul 2>nul
if errorlevel 1 (
    echo   uv no esta en PATH. Instala uv y vuelve a ejecutar.
    pause
    exit /b 1
)
uv sync
if errorlevel 1 (
    echo   Fallo uv sync.
    pause
    exit /b 1
)
echo   Dependencias listas.
pause
exit /b 0

:create_shortcut
%PS% -Command "$w=New-Object -ComObject WScript.Shell; $p=Join-Path ([Environment]::GetFolderPath('Desktop')) 'EzAgent.lnk'; $s=$w.CreateShortcut($p); $s.TargetPath='%ROOT%EzAgent.bat'; $s.WorkingDirectory='%ROOT%'; $s.Description='EzAgent'; $s.Save(); Write-Host ('Acceso directo: ' + $p)"
pause
exit /b 0
