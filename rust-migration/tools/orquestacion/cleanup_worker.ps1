# Uso: cleanup_worker.ps1 -Dir <carpeta del worktree>
# Cierra los procesos que un worker dejó corriendo en su carpeta (cargo, rustc,
# binarios de prueba, shells del servicio de OpenCode). El supervisor lo llama
# cuando el worker termina: `opencode run` muere, pero lo que lanzó dentro del
# servicio compartido de OpenCode sigue vivo y se queda con la CPU.
#
# Nunca toca: el propio supervisor (supervise.sh), la prueba de fondo
# (--soak / run_robustness) ni procesos fuera de esa carpeta.
param([Parameter(Mandatory = $true)][string]$Dir, [switch]$DryRun)

$norm = ($Dir -replace '/', '\').TrimEnd('\')
$escaped = [regex]::Escape($norm)
$slash = [regex]::Escape(($norm -replace '\\', '/'))
$pattern = "(?i)($escaped|$slash)([\\/`"\s]|$)"
$keep = '(?i)supervise[23]?\.sh|--soak|run_robustness|cleanup_worker\.ps1'

$all = Get-CimInstance Win32_Process
$byId = @{}
foreach ($proc in $all) { $byId[[int]$proc.ProcessId] = $proc }

# Protegido: el proceso o cualquiera de sus antecesores coincide con $keep
# (así sobreviven también los fake-llm/fake-whisper que lanza la prueba de fondo).
function Test-Protected($proc) {
    $seen = @{}
    while ($proc -and -not $seen.ContainsKey([int]$proc.ProcessId)) {
        $seen[[int]$proc.ProcessId] = $true
        if ("$($proc.CommandLine)" -match $keep) { return $true }
        $proc = $byId[[int]$proc.ParentProcessId]
    }
    return $false
}

$victims = $all | Where-Object {
    $line = "$($_.CommandLine) $($_.ExecutablePath)"
    $line -match $pattern -and $_.ProcessId -ne $PID -and -not (Test-Protected $_)
}
foreach ($proc in $victims) {
    if ($DryRun) {
        Write-Output ("cerraría {0} {1}" -f $proc.ProcessId, $proc.Name)
        continue
    }
    try {
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
        Write-Output ("cerrado {0} {1}" -f $proc.ProcessId, $proc.Name)
    } catch {
        # Ya terminó por su cuenta.
    }
}
