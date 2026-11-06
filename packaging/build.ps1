# Compila EzAgent portable e instalable.
# Uso:
#   powershell -ExecutionPolicy Bypass -File packaging\build.ps1
#
# Requisitos: uv sync, PyInstaller (grupo dev) e Inno Setup 6.
# Salidas:
#   dist\EzAgent\EzAgent.exe          app onedir
#   dist\EzAgent-portable.zip         versión portable
#   dist\EzAgent-Setup.exe            instalador con desinstalador

[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$SkipInstaller
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$python = Join-Path $root ".venv\Scripts\python.exe"
$dist = Join-Path $root "dist"
$build = Join-Path $root "build"
$appDir = Join-Path $dist "EzAgent"
$zip = Join-Path $dist "EzAgent-portable.zip"
$iss = Join-Path $PSScriptRoot "EzAgent.iss"

if (-not (Test-Path $python)) {
    throw "No existe .venv. Ejecuta 'uv sync' en $root."
}

Push-Location $root
try {
    if (-not $SkipTests) {
        Write-Host "== Tests ==" -ForegroundColor Cyan
        & $python -m pytest -q
        if ($LASTEXITCODE -ne 0) { throw "Los tests fallaron." }
    }

    Write-Host "== PyInstaller ==" -ForegroundColor Cyan
    & $python -m PyInstaller --clean --noconfirm "$PSScriptRoot\EzAgent.spec"
    if ($LASTEXITCODE -ne 0) { throw "PyInstaller falló." }

    # Archivos de consulta que no deben quedar dentro de _internal.
    Copy-Item (Join-Path $root "README.md") $appDir -Force
    Copy-Item (Join-Path $root ".env.example") (Join-Path $appDir ".env.example") -Force
    Copy-Item (Join-Path $root "llm.example.json") $appDir -Force

    Write-Host "== Portable ZIP ==" -ForegroundColor Cyan
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path $appDir -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "Portable: $zip"

    if (-not $SkipInstaller) {
        Write-Host "== Inno Setup ==" -ForegroundColor Cyan
        $iscc = @(
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
            "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
            "C:\Program Files\Inno Setup 6\ISCC.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $iscc) {
            throw "No encuentro ISCC.exe. Instala Inno Setup 6."
        }
        & $iscc $iss
        if ($LASTEXITCODE -ne 0) { throw "Inno Setup falló." }
        Write-Host "Instalador: $(Join-Path $dist 'EzAgent-Setup.exe')"
    }

    Write-Host "== Listo ==" -ForegroundColor Green
}
finally {
    Pop-Location
}
