# HD-0 — Paso 1: el supervisor lanza a cada trabajador como subagente en Herdr

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§2).**
Aquí no hay Rust: es la herramienta con la que el agente principal (Claude Code) lanza y
vigila a sus trabajadores. Nadie más toca estos archivos.

## Archivos que se tocan (solo estos)
- `rust-migration/tools/orquestacion/supervise3.sh` (existe: escrito por el agente
  principal antes de repartir el trabajo; **revísalo, corrígelo y hazlo tuyo**).
- `rust-migration/tools/orquestacion/cleanup_worker.ps1` (solo si hace falta).
- Nuevo `rust-migration/tools/orquestacion/pruebas/` con los agentes falsos y el guion de
  pruebas de este brief.

## Lo que ya hace (verificado en vivo con un `ping`)
Lanza cada intento con `hc run --bg --name <id> --cwd <worktree> --no-notify --label …`
desde la terminal de quien lo ejecuta, así que el trabajador queda como hijo de esa
terminal y Herdr lo muestra bajo ella; vigila con `hc get`; guarda la pantalla con
`hc read`; `EZ_SHOW=tab|right|down` le da pestaña o panel (reintenta mientras Herdr
responde «no running terminal»); `EZ_WORKER_CMD` lanza un comando de prueba.

## Cambios
1. **Tipo de trabajador**: `EZ_AGENT=opencode|mcode` (por omisión `opencode`).
   - `opencode`: como hoy (`opencode run --model … --title "<ID>-s1" --auto "<prompt>"`;
     reintentos con `--session <id>`). Modelo por omisión `opencode/hy4-flash-free`.
   - `mcode`: modo interactivo, visible y corregible: `mcode -m <modelo> "<prompt>"` con
     `--cwd` del worktree; reintentos `mcode -c "<texto de continuar>"` (continúa la última
     sesión de esa carpeta). Modelo por omisión `minimax/MiniMax-M3.1-Flash`.
   - Etiquetas `ez.agent=<tipo>` y `ez.model=<modelo>` según el tipo.
2. **Fin del trabajo con agentes que no salen solos** (el TUI de mcode queda abierto): con
   el informe completado, espera a que `git -C <worktree> status --porcelain` quede vacío
   (todo confirmado) o 15 min como máximo, guarda la pantalla y mata la terminal.
3. `EZ_AGENT_BIN`: ruta del programa en lugar de `opencode`/`mcode` (solo para pruebas).
4. `EZ_WORKER_CMD` deja de terminar como «hecho» sin informe: sin informe completado
   cuenta como intento fallido igual que un trabajador real.
5. Mensajes en español neutro; el script no imprime claves ni tokens.

## Pruebas (sin cuota: nunca ejecutes opencode ni mcode de verdad)
En `pruebas/`: un agente falso (`fake-worker.cmd`, o el binario `fake-ws-agent` si te
sirve) que escribe el informe con «Estado: completado» tras unos segundos, hace commit en
una carpeta git temporal y, en modo «tui», no sale. Un guion `pruebas/probar.sh` que,
contra una carpeta git temporal bajo `%TEMP%`:
- con `EZ_AGENT=opencode` + `EZ_AGENT_BIN` falso: termina con código 0, el hijo tuvo
  `parent` = la terminal del guion (`hc get`), nombre y etiquetas;
- con `EZ_AGENT=mcode` + falso «tui»: detecta el informe, espera el commit, mata la
  terminal y sale con 0;
- sin informe: reintenta y sale con 2 tras `MAX` (usa un `MAX` pequeño por variable);
- limpia sus terminales (`hc rm`) y carpetas.
Ejecuta `probar.sh` dentro de una terminal de hc (por ejemplo `hc run --name hd0-pruebas
-- bash pruebas/probar.sh` y luego `hc read`) para que exista `HC_TERMINAL_ID`, y pega su
salida en el informe. **No** uses `EZ_SHOW` en las pruebas (no toques la ventana de
Herdr de la persona).

## Criterios de aceptación
`bash -n` sin errores, `probar.sh` en verde con su salida pegada. Informe en
`rust-migration/reports/HD-0-r1.md`.
