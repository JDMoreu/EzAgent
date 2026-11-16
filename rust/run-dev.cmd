@echo off
rem Lanza la version Rust de EzAgent para probarla en desarrollo.
rem Las DLL de ONNX Runtime y espeak-ng ya no salen de un entorno de Python:
rem viven en la carpeta de datos de la app, donde las deja
rem "ezagent-voice downloads --all" (o EzAgent.bat install).
rem Uso: run-dev.cmd [argumentos de ezagent-voice, p. ej. --demo-ui]

setlocal
set "DATA=%EZAGENT_HOME%"
if not defined DATA set "DATA=%LOCALAPPDATA%\EzAgent"

if not exist "%DATA%\onnxruntime\onnxruntime.dll" goto missing_dlls
if not exist "%DATA%\espeak-ng\espeak-ng.dll" goto missing_dlls

set "EXE=%~dp0target\release\ezagent-voice.exe"
if not exist "%EXE%" (
  set "EXE=%~dp0target\debug\ezagent-voice.exe"
  echo Aviso: usando la compilacion debug, que es lenta. Para una rapida:
  echo   cd rust ^&^& cargo build --release -p ez-app --bin ezagent-voice
)
if not exist "%EXE%" (
  echo No existe %EXE%. Compila con: cd rust ^&^& cargo build -p ez-app --bin ezagent-voice
  pause
  exit /b 1
)

"%EXE%" %*
set "CODE=%ERRORLEVEL%"
echo.
echo EzAgent termino con codigo %CODE%.
pause
exit /b %CODE%

:missing_dlls
echo Faltan ONNX Runtime o espeak-ng en %DATA%.
echo Ejecuta antes: cd rust ^&^& cargo run -p ez-app --bin ezagent-voice -- downloads --all
echo (o EzAgent.bat install, que tambien las descarga).
pause
exit /b 1
