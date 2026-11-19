# IR-1b — Corrección de la revisión de IR-1: memoria virtual real y «pesados» de verdad

Revisión del agente principal sobre `work/IR-1` (726f90d + 7817afa). Bien: crate `ez-scan`
con `Probe` inyectable, `entorno.json` atómico, inventario real sin abrir agentes, ruta
nativa del shim de npm, `ez init`/`resources`/`inventory`, pruebas en verde. **Mal**:

1. **La memoria virtual libre está inflada ~13 GB.** Medido a la vez en esta máquina:
   `ez resources` → `free_commit_mb 27179`; Windows (`Win32_OperatingSystem.
   FreeVirtualMemory`, límite de commit − commit en uso) → **14162 MB**. RAM libre + swap
   libre de `sysinfo` no es el commit libre. En el incidente de hoy (virtual libre 3,8 GB)
   la guarda habría dicho «hay ~16 GB» y habría lanzado: justo lo que debe impedir.
2. **«Pesados» = todos los hijos vivos.** §4 dice «subagentes vivos **compilando o
   midiendo**». Contar todos bloquea el uso normal: el agente principal tiene hoy 4 hijos
   vivos (dos de ellos parados) y no podría delegar un quinto. Sí hay forma local de
   saberlo: ya recorres el árbol de procesos de cada hijo.

## Cambios
1. **Commit libre real en Windows.** `#[cfg(windows)]`: `GetPerformanceInfo`
   (`(CommitLimit − CommitTotal) × PageSize`) o `GlobalMemoryStatusEx.ullAvailPageFile`,
   con el crate `windows` del workspace (ya lo nombraba IR-1; añade solo las features que
   hagan falta). **Excepción a `_COMMON-ECO`**: un único bloque `unsafe`, en un archivo
   propio `ez-scan/src/real/commit.rs` (≤ 40 líneas), con comentario `// SAFETY:`. Fuera de
   Windows queda la aproximación actual con `# DECISION`.
2. **Pesado** = el árbol de procesos del hijo contiene un proceso de compilación (`cargo`,
   `rustc`, `link`, `lld-link`, `cl`; lista en `voice.json` `resources.heavy_processes`)
   **o** su árbol usa ≥ `resources.heavy_mb` (1500 por omisión). `ez resources` dice qué
   hijos son pesados y por qué (`"heavy": true, "heavy_reason": "rustc"`).
   `check_capacity` cuenta solo esos.
3. Cosmético: la ruta de OpenCode sale con barras mezcladas (`current/node_modules/...`):
   normaliza separadores.

## Pruebas
- `check_capacity` con un `Probe` falso: 4 hijos vivos, 2 compilando → pasa; 3 compilando →
  error con el texto de §4; 1 hijo de 2 GB sin compilador → cuenta como pesado; umbrales de
  `voice.json` respetados.
- Prueba real acotada: `free_commit_mb > 0` y ≤ el commit total.

## Entrega
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde. **Verificación real obligatoria**:
ejecuta `ez resources --json` y, en el mismo momento, `Get-CimInstance
Win32_OperatingSystem | Select FreePhysicalMemory, FreeVirtualMemory` (solo en tu
verificación, nunca en el código); pega las dos salidas: el commit libre debe coincidir
con `FreeVirtualMemory` con menos de un 5 % de diferencia. Commit en `work/IR-1`, informe
`rust-migration/reports/IR-1b-r1.md`. Compila en primer plano y espera el resultado.
