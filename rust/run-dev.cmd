@echo off
rem Lanza la version Rust de EzAgent para probarla en desarrollo.
rem Mientras no exista el instalador (F8), el ejecutable necesita las DLL de
rem ONNX Runtime y espeak-ng; aqui se toman del entorno de Python del repo.
rem Uso: run-dev.cmd [argumentos de ezagent-voice, p. ej. --demo-ui]

setlocal
set "REPO=%~dp0.."
set "PKG=%REPO%\.venv\Lib\site-packages"

set "ORT_DYLIB_PATH=%PKG%\onnxruntime\capi\onnxruntime.dll"
set "EZAGENT_ESPEAK_LIBRARY=%PKG%\espeakng_loader\espeak-ng.dll"
set "EZAGENT_ESPEAK_DATA_PATH=%PKG%\espeakng_loader\espeak-ng-data"

if not exist "%ORT_DYLIB_PATH%" (
  echo No encuentro onnxruntime.dll en %ORT_DYLIB_PATH%
  echo Ejecuta antes "uv sync" en la carpeta del repo.
  pause
  exit /b 1
)

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
echo.
echo EzAgent termino con codigo %ERRORLEVEL%.
pause
