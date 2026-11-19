## Resultado
- Brief/tarea: IR-1b
- Estado: completado

## Cambios
- `rust/Cargo.toml`: feature `Win32_System_ProcessStatus` del crate `windows` (solo la que usa `GetPerformanceInfo`).
- `rust/crates/ez-scan/Cargo.toml`: dependencia `windows` del workspace.
- `rust/crates/ez-scan/src/real/commit.rs`: archivo nuevo (24 líneas) con el único `unsafe` (bloque `// SAFETY:`) que lee `GetPerformanceInfo` y devuelve `(commit libre, commit límite)` en MB; fuera de Windows devuelve `None`.
- `rust/crates/ez-scan/src/real.rs`: `free_commit_mb` usa el commit real; el árbol de procesos ahora devuelve memoria y nombres; cada hijo calcula `heavy`/`heavy_reason` con los umbrales.
- `rust/crates/ez-scan/src/probe.rs`: `subagents(parent, &Thresholds)`.
- `rust/crates/ez-scan/src/resources.rs`: `SubagentUsage` gana `heavy` y `heavy_reason`; `heavy_reason()` decide por compilador o por memoria; `collect` recibe los umbrales.
- `rust/crates/ez-scan/src/capacity.rs`: `Thresholds` gana `heavy_processes` y `heavy_mb` (1500 por omisión), se leen de `resources.*`, y `check` cuenta solo los hijos pesados.
- `rust/crates/ez-scan/src/lib.rs`: `resources()` lee los umbrales de `voice.json` y los pasa al medidor.
- `rust/crates/ez-scan/src/agents.rs`: normaliza los separadores de la ruta nativa (Windows, `\`).
- `rust/crates/ez-scan/src/agents/tests.rs`, `src/resources/tests.rs`: dobles adaptados a la nueva firma; pruebas de `heavy_reason`.
- `rust/crates/ez-scan/tests/real_system.rs`: la prueba real ya no asume que el commit libre sea mayor que la RAM libre; exige `free_commit_mb > 0`.
- `rust/crates/ez-ctl/src/scan_cmds.rs`: `ez resources` muestra «— pesado: <motivo>».

## Decisiones
- `// DECISION`: fuera de Windows (o si `GetPerformanceInfo` falla) se conserva la aproximación RAM libre + swap libre.
- `// DECISION`: un `.exe` hallado en un guion de npm solo cuenta si su nombre se parece al del programa; si el guion solo lanza `node.exe` (Pi), se queda el `.cmd`.
- La lista `resources.heavy_processes` se respeta tal cual: si existe y está vacía, ningún proceso cuenta como compilador; si falta, van `cargo`, `rustc`, `link`, `lld-link`, `cl`.
- El motivo de pesado es el compilador encontrado (`rustc`) o la memoria (`2048 MB`).
- `Thresholds` deja de ser `Copy` al llevar la lista de procesos (pasa a `Clone`).
- La ruta nativa se normaliza solo en Windows; en otros sistemas queda igual.
- **Excepción autorizada por IR-1b:** un único bloque `unsafe` en `real/commit.rs`, documentado con `// SAFETY:`; el resto del crate sigue sin `unsafe`.

## Verificación
- `cargo fmt -p ez-scan -p ez-ctl -- --check`: `fmt_exit=0` (sin diferencias).
- `cargo clippy -p ez-scan -p ez-ctl --all-targets -- -D warnings`:
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.85s`
- `cargo test -p ez-scan`:
  `test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s`
  `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s`
- `cargo test -p ez-ctl`:
  `test result: ok. 4 passed; 0 failed; 0 ignored` · `7 passed` · `9 passed` · `16 passed` · `6 passed` · `2 passed` (scan_init) · `7 passed` · `2 passed`; 0 fallos.
- Pruebas nuevas de la guarda: 4 hijos vivos con 2 compilando → pasa; 3 compilando → `Recursos justos: ya hay 3 subagentes pesados (máximo 3). Espera a que termine alguno o usa --force.`; 1 hijo de 2048 MB sin compilador → pesado; `heavy_processes`/`heavy_mb` de `voice.json` respetados.
- Verificación real simultánea (`APPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME` y `OPENCODE_CONFIG_DIR` temporales; exit 0):

```
--- ez resources --json
{"ok":true,"parent":"t18q","resources":{"free_ram_mb":14152,"free_commit_mb":13725,"cpu_percent":33.17829132080078,"disk_free_mb":238101,"subagents":[]}}
--- CIM
{
    "FreePhysicalMemory":  14468864,
    "FreeVirtualMemory":  13981648
}
ez free_commit_mb: 13725
CIM FreeVirtualMemory MB: 13653.953125
diferencia: 0.52 %
```

  `FreeVirtualMemory` viene en KB; 13 981 648 KB ÷ 1024 = 13 653,95 MB frente a 13 725 MB: **0,52 %** de diferencia (< 5 %). Antes IR-1 daba ~27 179 MB (inflado ~13 GB).
- Prueba real de pesados con `HC_TERMINAL_ID=t6h` (solo lectura de `hc ls`/`get`), 5 hijos vivos y 2 pesados:

```json
{"ok":true,"parent":"t6h","resources":{"free_ram_mb":14121,"free_commit_mb":13429,"cpu_percent":38.623626708984375,"disk_free_mb":238104,"subagents":[{"name":"ir-1","terminal":"t18q","agent":"opencode","memory_mb":693,"heavy":false,"heavy_reason":null},{"name":"dx-1c","terminal":"t19v","agent":"mcode","memory_mb":279,"heavy":false,"heavy_reason":null},{"name":"dx-1d","terminal":"t1f6","agent":"opencode","memory_mb":1490,"heavy":true,"heavy_reason":"cargo"},{"name":"hd-0e","terminal":"t1f7","agent":"opencode","memory_mb":596,"heavy":false,"heavy_reason":null},{"name":"hd-10","terminal":"t1nb","agent":"opencode","memory_mb":780,"heavy":true,"heavy_reason":"cargo"}]}}
```

  Con la regla anterior (5 vivos ≥ 3) la guarda habría rechazado; ahora pasan 3 no pesados.

## Pendiente o riesgos
- `link` es un nombre de proceso genérico: la lista es editable en `voice.json` (`resources.heavy_processes`) para ajustarla.
- El commit libre fluctúa entre muestras; la comparación real quedó en 0,52 %, pero en momentos de mucha actividad puede acercarse al 5 %.
- IR-3 sigue pendiente de conectar `check_capacity` en `ez delegate`; la firma y los umbrales ya están listos.
