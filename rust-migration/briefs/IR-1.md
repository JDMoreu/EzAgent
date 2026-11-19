# IR-1 — Sistema, recursos en vivo, inventario de agentes y `ez init`

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/inventario-recursos.md`
(§2–§4).** Rama `work/IR-1` desde `eco/main`. En paralelo, HD-9 toca
`ez-herdr/src/herdr/{child,text,delegate*}.rs`, `subagents/watch.rs` y `ez-ctl/src/local*`:
**no los toques**; lo tuyo va en un crate nuevo y en un archivo nuevo de `ez-ctl`.

## Archivos que se tocan
- Nuevo crate `rust/crates/ez-scan/` (`Cargo.toml`, `src/**`, pruebas). Dependencias:
  `serde`, `serde_json`, `tracing`, `ez-core` (ruta), `ez-herdr` (ruta, para el catálogo y
  la resolución de agentes) y, **nombradas por este brief**, `sysinfo` (CPU, RAM, memoria
  virtual, disco, procesos; fija versión) y `windows` (la del workspace, solo para DXGI:
  nombre de la GPU y VRAM dedicada). Añade el crate a `members` del `Cargo.toml` raíz
  (excepción a la regla de no tocarlo: solo esa línea y, si hace falta, `sysinfo` en
  `[workspace.dependencies]`).
- Nuevo `rust/crates/ez-ctl/src/scan_cmds.rs` con `ez init`, `ez resources` y
  `ez inventory --local`, y lo mínimo en `ez-ctl/src/cli.rs` y `lib.rs` para enrutarlos
  (coordina: HD-9 toca `local*.rs`, no `cli.rs`).

## Lo que hace
1. `ez_scan::system()`: SO y versión, CPU (modelo, físicos, lógicos), RAM total, disco
   libre de una carpeta dada, GPU (DXGI), versiones de `hc` y `herdr-app` (`--version`).
2. `ez_scan::resources(parent)`: RAM libre, virtual libre, CPU % (dos muestras), disco
   libre, y por cada terminal viva con `ez.parent=<p>` (`hc ls --json --label`) la memoria
   de su árbol de procesos (`pid` de `hc get`, hijos por `sysinfo`).
3. `ez_scan::agents(cfg)`: por cada tipo del catálogo de `voice.json`: instalado, **ruta
   nativa** (si el comando resuelve a un guion de npm sin extensión, busca el `.exe` real
   o el `.cmd`), versión con tope de 10 s, modelos (catálogo + `discover` que ya existe en
   `ez-herdr` + `opencode models` si está instalado), esfuerzos. **Nunca** abre un agente.
4. `ez init [--refresh] [--json]`: escribe `entorno.json` (§3) de forma atómica e imprime
   el resumen y lo que falta declarar. `ez resources [--json]` y `ez inventory --local
   [--json]` muestran lo suyo sin escribir.
5. **Guarda de recursos en `ez delegate`**: NO la implementes en `local.rs` (lo toca HD-9):
   expón `ez_scan::check_capacity(parent, &thresholds) -> Result<(), String>` con los
   umbrales de §4 leídos de `voice.json` (`resources.*`) y su prueba; IR-3 la conecta.

## Pruebas
Con un sistema falso (trait inyectable) para todo lo que no sea medir de verdad;
`ez init` contra `APPDATA` temporal y `PATH` con agentes falsos (incluido un guion `.cmd`
y un guion sin extensión que apunta a un `.exe` falso); `check_capacity` con umbrales y
procesos falsos. Una prueba real acotada de `system()` (solo que los números sean
positivos). Nada pasa de 1 GB.

## Criterios de aceptación
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde; **ejecuta** `ez init --json` con
`APPDATA` temporal en esta máquina y pega la salida (es real: lista tus agentes de verdad
sin abrirlos). Informe en `rust-migration/reports/IR-1-r1.md`.
