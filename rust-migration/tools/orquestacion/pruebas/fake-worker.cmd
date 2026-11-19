@echo off
setlocal
rem Agente falso para las pruebas de supervise3.sh: ignora los argv del agente
rem real, lee .fake-id y .fake-mode de la carpeta de trabajo y escribe el informe.
set "ID="
set "MODE="
for /f "usebackq delims=" %%i in (".fake-id") do set "ID=%%i"
for /f "usebackq delims=" %%i in (".fake-mode") do set "MODE=%%i"
if not defined ID exit /b 1
ping -n 4 127.0.0.1 >nul
if /i "%MODE%"=="fail" exit /b 0
python -c "import os; os.makedirs('rust-migration/reports', exist_ok=True)"
set "REPORT=rust-migration\reports\%ID%-r1.md"
(
echo ## Resultado
echo - Brief/tarea: %ID%
echo - Estado: completado
echo - Verificado con el agente falso de HD-0.
) > "%REPORT%"
git add "%REPORT%"
git -c user.email=hd0@example.com -c user.name=HD0Test commit -q -m "Informe de prueba %ID%" >nul 2>&1
if /i "%MODE%"=="tui" goto tui
exit /b 0
:tui
ping -n 2 127.0.0.1 >nul
goto tui
