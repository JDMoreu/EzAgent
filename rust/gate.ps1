# Puerta de calidad por tarea (plan 5.3). Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File rust/gate.ps1 -Task F3-07
#   powershell ... -File rust/gate.ps1 -SelfTest
param(
    [string]$Task,
    [switch]$SelfTest,
    [string]$Base = "rust/main",
    [switch]$SkipPytest
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Fail([string]$Message) {
    Write-Host "GATE FALLA: $Message" -ForegroundColor Red
    exit 1
}

function Step([string]$Name, [scriptblock]$Body) {
    Write-Host "== $Name" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) { Fail $Name }
}

if ($SelfTest) {
    Step "autoprueba de reglas" { uv run python "rust-migration/tools/scan_diff.py" --selftest }
    Write-Host "GATE OK (autoprueba)" -ForegroundColor Green
    exit 0
}

if (-not $Task) { Fail "falta -Task <ID>" }

$info = uv run python "rust-migration/tools/taskinfo.py" $Task | ConvertFrom-Json
if (-not $info) { Fail "tarea desconocida: $Task" }

Step "1. alcance del diff" {
    uv run python "rust-migration/tools/scan_diff.py" --task $Task --base $Base
}
Step "2. cargo fmt" { cargo fmt --all --check --manifest-path "rust/Cargo.toml" }
Step "3. clippy" { cargo clippy --workspace --all-targets --manifest-path "rust/Cargo.toml" -- -D warnings }
Step "4. tests de la tarea" {
    if ($info.crates.Count -gt 0) {
        $cargoArgs = @("test", "--manifest-path", "rust/Cargo.toml")
        foreach ($crate in $info.crates) { $cargoArgs += @("-p", $crate) }
        & cargo @cargoArgs
    } else { & cargo test --workspace --manifest-path "rust/Cargo.toml" }
}
Step "5. tests del workspace" { cargo test --workspace --manifest-path "rust/Cargo.toml" }
if (-not $SkipPytest) {
    Step "6. paridad Python intacta" { uv run pytest -q }
}

Write-Host "GATE OK: $Task" -ForegroundColor Green
