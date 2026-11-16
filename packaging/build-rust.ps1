# Compila EzAgent (versión Rust) portable e instalable.
# Uso:
#   powershell -ExecutionPolicy Bypass -File packaging\build-rust.ps1
#
# Requisitos: Rust estable (x86_64-pc-windows-msvc), uv sync (para las DLL de
# ONNX Runtime y espeak-ng, igual que rust\run-dev.cmd) e Inno Setup 6.
#
# Entorno de compilación: el CRT se enlaza estáticamente
# (-C target-feature=+crt-static) para que el .exe no necesite el redistribuible
# de VC++ en la máquina destino. Con -DynamicCrt se compila con el CRT dinámico:
# entonces hay que instalar "Microsoft Visual C++ 2015-2022 Redistributable
# (x64)" en cada equipo antes de abrir la app.
#
# Salidas:
#   dist\rust-stage\                    carpeta lista para ejecutar
#   dist\EzAgent-portable-0.2.0.zip     versión portable
#   dist\EzAgent-Setup-0.2.0.exe        instalador con desinstalador

[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$SkipInstaller,
    [switch]$DynamicCrt
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$rust = Join-Path $root "rust"
$dist = Join-Path $root "dist"
$stage = Join-Path $dist "rust-stage"
$pkg = Join-Path $root ".venv\Lib\site-packages"
$version = "0.2.0"
$zip = Join-Path $dist "EzAgent-portable-$version.zip"
$iss = Join-Path $PSScriptRoot "EzAgent-rust.iss"
$setup = Join-Path $dist "EzAgent-Setup-$version.exe"

function Copy-Required([string]$From, [string]$To) {
    if (-not (Test-Path $From)) {
        throw "No encuentro $From. Ejecuta 'uv sync' en $root."
    }
    Copy-Item $From $To -Recurse -Force
}

function Get-Sha256([string]$Path) {
    (Get-FileHash $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Show-Artifact([string]$Path) {
    $item = Get-Item $Path
    $megabytes = [math]::Round($item.Length / 1MB, 1)
    Write-Host ("{0}  {1} MB  sha256={2}" -f $item.Name, $megabytes, (Get-Sha256 $Path))
}

Push-Location $rust
try {
    if (-not $SkipTests) {
        Write-Host "== Tests ==" -ForegroundColor Cyan
        cargo test -p ez-app -p ez-swarm
        if ($LASTEXITCODE -ne 0) { throw "Los tests fallaron." }
    }

    Write-Host "== cargo build --release ==" -ForegroundColor Cyan
    $savedFlags = $env:RUSTFLAGS
    if (-not $DynamicCrt) {
        $static = "-C target-feature=+crt-static"
        $env:RUSTFLAGS = if ($savedFlags) { "$savedFlags $static" } else { $static }
    }
    try {
        cargo build --release -p ez-app --bin ezagent-voice
        if ($LASTEXITCODE -ne 0) { throw "cargo build falló (¿CRT estático? prueba -DynamicCrt)." }
        cargo build --release -p ez-swarm --bin ezs
        if ($LASTEXITCODE -ne 0) { throw "cargo build falló (¿CRT estático? prueba -DynamicCrt)." }
    }
    finally {
        $env:RUSTFLAGS = $savedFlags
    }

    Write-Host "== dist\rust-stage ==" -ForegroundColor Cyan
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item $stage -ItemType Directory -Force | Out-Null
    Copy-Required (Join-Path $rust "target\release\ezagent-voice.exe") $stage
    Copy-Required (Join-Path $rust "target\release\ezs.exe") $stage
    # Las mismas DLL que rust\run-dev.cmd toma del entorno de Python.
    Copy-Required (Join-Path $pkg "onnxruntime\capi\onnxruntime.dll") $stage
    Copy-Required (Join-Path $pkg "onnxruntime\capi\onnxruntime_providers_shared.dll") $stage
    Copy-Required (Join-Path $pkg "espeakng_loader\espeak-ng.dll") $stage
    Copy-Required (Join-Path $pkg "espeakng_loader\espeak-ng-data") $stage
    foreach ($extra in @(".env.example", "llm.example.json", "README.md")) {
        $source = Join-Path $root $extra
        if (Test-Path $source) { Copy-Item $source $stage -Force }
    }

    # Licencias de lo que se redistribuye (obligación de onnxruntime MIT y
    # espeak-ng GPL-3: se ofrece el código fuente de espeak-ng).
    $licenses = Join-Path $stage "LICENSES"
    New-Item $licenses -ItemType Directory -Force | Out-Null
    Copy-Required (Join-Path $root "LICENSE") (Join-Path $licenses "EzAgent-LICENSE.txt")
    Copy-Required (Join-Path $pkg "onnxruntime\LICENSE") (Join-Path $licenses "onnxruntime-LICENSE.txt")
    Copy-Required (Join-Path $pkg "onnxruntime\ThirdPartyNotices.txt") (Join-Path $licenses "onnxruntime-ThirdPartyNotices.txt")
    $notice = @"
Componentes redistribuidos en esta carpeta:

- ezagent-voice.exe, ezs.exe: EzAgent, licencia Apache-2.0
  (LICENSES\EzAgent-LICENSE.txt).
- onnxruntime.dll, onnxruntime_providers_shared.dll: ONNX Runtime, licencia
  MIT (LICENSES\onnxruntime-LICENSE.txt y
  LICENSES\onnxruntime-ThirdPartyNotices.txt).
- espeak-ng.dll y espeak-ng-data\: eSpeak NG, licencia GPL-3.0-or-later.
  El código fuente está en https://github.com/espeak-ng/espeak-ng y se puede
  pedir una copia del código correspondiente a esta versión abriendo un issue
  en el repositorio de EzAgent. Es la misma obligación que ya cumple la
  versión Python, que también redistribuye eSpeak NG.
"@
    Set-Content (Join-Path $licenses "README.txt") $notice -Encoding UTF8

    Write-Host "== Portable ZIP ==" -ForegroundColor Cyan
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
    Show-Artifact $zip

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
        if (Test-Path $setup) { Remove-Item $setup -Force }
        & $iscc $iss
        if ($LASTEXITCODE -ne 0) { throw "Inno Setup falló." }
        Show-Artifact $setup
    }

    Get-ChildItem $stage -File | ForEach-Object { Show-Artifact $_.FullName }
    Write-Host "== Listo ==" -ForegroundColor Green
}
finally {
    Pop-Location
}
