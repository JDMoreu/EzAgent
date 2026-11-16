# Compila EzAgent (versión Rust) portable e instalable.
# Uso:
#   powershell -ExecutionPolicy Bypass -File packaging\build-rust.ps1
#
# Requisitos: Rust estable (x86_64-pc-windows-msvc), los recursos descargados
# con "ezagent-voice downloads --all" (ONNX Runtime y espeak-ng; los deja
# EzAgent.bat install) e Inno Setup 6.
# Las DLL salen de la carpeta de datos de la app (%LOCALAPPDATA%\EzAgent o
# EZAGENT_HOME), no de un entorno de Python.
#
# Entorno de compilación: el CRT se enlaza estáticamente
# (-C target-feature=+crt-static) para que el .exe no necesite el redistribuible
# de VC++ en la máquina destino. Con -DynamicCrt se compila con el CRT dinámico:
# entonces hay que instalar "Microsoft Visual C++ 2015-2022 Redistributable
# (x64)" en cada equipo antes de abrir la app.
# DECISION: el flag va en RUSTFLAGS de esta invocación y no en
# rust\.cargo\config.toml, que afectaría también al desarrollo y a los tests
# (cargo no admite rustflags por perfil).
#
# Salidas:
#   dist\rust-stage\                    carpeta lista para ejecutar
#   dist\EzAgent-portable-0.2.0.zip     versión portable
#   dist\EzAgent-Setup-0.2.0.exe        instalador con desinstalador
# DECISION: el ZIP va plano (sin carpeta contenedora), como una carpeta portable
# que se descomprime donde el usuario quiera.

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
# Carpeta de datos de la app: ahí deja las DLL "ezagent-voice downloads --all".
$dataHome = if ($env:EZAGENT_HOME) { $env:EZAGENT_HOME } else { Join-Path $env:LOCALAPPDATA "EzAgent" }
$version = "0.2.0"
$zip = Join-Path $dist "EzAgent-portable-$version.zip"
$iss = Join-Path $PSScriptRoot "EzAgent-rust.iss"
$setup = Join-Path $dist "EzAgent-Setup-$version.exe"

function Copy-Required([string]$From, [string]$To) {
    if (-not (Test-Path $From)) {
        throw "No encuentro $From. Ejecuta 'EzAgent.bat install' o 'ezagent-voice downloads --all'."
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
    # Las DLL y los datos de espeak-ng salen de las descargas de la propia app.
    $onnxruntime = Join-Path $dataHome "onnxruntime"
    $espeak = Join-Path $dataHome "espeak-ng"
    Copy-Required (Join-Path $onnxruntime "onnxruntime.dll") $stage
    Copy-Required (Join-Path $onnxruntime "onnxruntime_providers_shared.dll") $stage
    Copy-Required (Join-Path $espeak "espeak-ng.dll") $stage
    Copy-Required (Join-Path $espeak "espeak-ng-data") $stage
    foreach ($extra in @(".env.example", "llm.example.json", "README.md")) {
        $source = Join-Path $root $extra
        if (Test-Path $source) { Copy-Item $source $stage -Force }
    }
    # Recursos que la app busca junto a la raíz del repo en desarrollo: el icono
    # de la bandeja, el script de whisper, las fuentes del HUD y las skills.
    Copy-Required (Join-Path $root "assets") $stage
    Copy-Required (Join-Path $root "scripts") $stage
    Copy-Required (Join-Path $root "src\ezagent\voice\assets\fonts") (Join-Path $stage "assets\fonts")
    Copy-Required (Join-Path $root "src\ezagent\agent\default_skills") (Join-Path $stage "default_skills")

    # Licencias de lo que se redistribuye (obligación de onnxruntime MIT y
    # espeak-ng GPL-3: se ofrece el código fuente de espeak-ng).
    $licenses = Join-Path $stage "LICENSES"
    New-Item $licenses -ItemType Directory -Force | Out-Null
    Copy-Required (Join-Path $root "LICENSE") (Join-Path $licenses "EzAgent-LICENSE.txt")
    # La descarga de ONNX Runtime es el paquete NuGet, que solo deja en la
    # carpeta de datos la subcarpeta de las DLL; su licencia MIT va aquí.
    $onnxruntimeLicense = @"
MIT License

Copyright (c) Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"@
    Set-Content (Join-Path $licenses "onnxruntime-LICENSE.txt") $onnxruntimeLicense -Encoding UTF8
    $notice = @"
Componentes redistribuidos en esta carpeta:

- ezagent-voice.exe, ezs.exe: EzAgent, licencia Apache-2.0
  (LICENSES\EzAgent-LICENSE.txt).
- onnxruntime.dll, onnxruntime_providers_shared.dll: ONNX Runtime 1.31.1,
  licencia MIT (LICENSES\onnxruntime-LICENSE.txt). Avisos de terceros del
  paquete: https://github.com/microsoft/onnxruntime/releases/tag/v1.31.1
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
