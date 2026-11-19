@echo off
setlocal
rem Agente falso para las pruebas de supervise3.sh: ignora los argv del agente
rem real, lee .fake-id y .fake-mode de la carpeta de trabajo y escribe el informe.
rem Ademas registra argv/entorno (C0-1) y permite los modos «retry» (dos
rem intentos), «slow» (tarda antes de escribir el informe, HD-0d) y «limit»
rem (la pantalla muestra un aviso de límite de uso, HD-0d C0-6).
set "ID="
set "MODE="
for /f "usebackq delims=" %%i in (".fake-id") do set "ID=%%i"
for /f "usebackq delims=" %%i in (".fake-mode") do set "MODE=%%i"
if not defined ID exit /b 1
>>".fake-argv.log" echo ARGS=%*
>>".fake-argv.log" echo ENV=[%OPENCODE_CONFIG_CONTENT%]
ping -n 4 127.0.0.1 >nul
if /i "%MODE%"=="limit" goto limit
if /i "%MODE%"=="limit-top" goto limittop
if /i "%MODE%"=="limit-work" goto limitwork
if /i "%MODE%"=="chatter" goto chatter
if /i "%MODE%"=="slow" goto slow
if /i "%MODE%"=="fail" exit /b 0
if /i "%MODE%"=="retry" if not exist ".fake-attempt2" (
  >".fake-attempt2" echo 2
  exit /b 0
)
:escribir
set "REPORT=rust-migration\reports\%ID%-r1.md"
(
echo ## Resultado
echo - Brief/tarea: %ID%
echo - Estado: completado
echo - Verificado con el agente falso de HD-0.
) > "%REPORT%"
git add -A
git -c user.email=hd0@example.com -c user.name=HD0Test commit -q -m "Informe de prueba %ID%" >nul 2>&1
if /i "%MODE%"=="tui" goto tui
if /i "%MODE%"=="slow" goto tui
exit /b 0
:slow
rem HD-0d: tarda lo bastante para que, con un estado «desconocido», el
rem supervisor mataría al trabajador sano antes de que escribiera el informe.
ping -n 25 127.0.0.1 >nul
goto escribir
:limit
rem HD-0d (C0-6): la pantalla del trabajador muestra un aviso de límite de uso.
echo Free limit reached. Try again later.
ping -n 120 127.0.0.1 >nul
exit /b 0
:limittop
rem HD-0e: el aviso de límite está ARRIBA de una pantalla larga: no cuenta,
rem porque no está en las últimas 15 líneas.
echo Free limit reached. Try again later.
for /L %%i in (1,1,20) do echo linea de relleno %%i: el agente sigue trabajando
ping -n 50 127.0.0.1 >nul
goto escribir
:chatter
rem HD-0e: el trabajador menciona límites de uso con palabras sueltas
rem («quota», «rate limit», «usage limit») pero no está bloqueado. Vive más que
rem la ventana de pantalla quieta para que el supervisor llegue a buscar la frase.
echo revisando el informe de IR-0: "quota", "rate limit" y "usage limit" salen en el texto
ping -n 45 127.0.0.1 >nul
goto escribir
:limitwork
rem HD-0e: la frase COMPLETA de límite está en pantalla, pero el agente está
rem «working»: no está bloqueado y el supervisor no debe tocarlo.
echo Free limit reached. Try again later.
ping -n 40 127.0.0.1 >nul
goto escribir
:tui
ping -n 2 127.0.0.1 >nul
goto tui
