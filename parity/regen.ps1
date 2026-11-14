# Regenera el oraculo de paridad desde el Python de py-baseline.
#   powershell -File parity/regen.ps1          # regenera y actualiza el manifiesto
#   powershell -File parity/regen.ps1 -Check   # exige que nada haya cambiado
param([switch]$Check)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if ($Check) {
    uv run python parity/gen/run_all.py --check
} else {
    uv run python parity/gen/run_all.py
}
exit $LASTEXITCODE
