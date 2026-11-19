## Resultado
- Brief/tarea: HD-0f — corrección de la revisión de HD-0e (que el producto vea lo que escribe el supervisor)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: v6. (1) `SUBAGENTS_ROOT` por omisión pasa a `${LOCALAPPDATA}/EzAgent/subagentes` (la carpeta que lee `Registry::from_env`; `EZ_SUBAGENTS_DIR` sigue mandando). (2) El registro escribe `"validation": d.get('validation')`: `null` en un registro nuevo y conserva una validación ya puesta. (3) `export PYTHONIOENCODING=utf-8 PYTHONUTF8=1` al principio: todo Python del script (resumen, `hc get`, registro) escribe en UTF-8; antes, en una tubería, los resúmenes con «ó» salían en cp1252 y los que traían «→» salían vacíos. (4) `screen_tail` cuenta como línea con texto la que tiene al menos una letra o un dígito (`/[[:alnum:]]/`), el mismo criterio que NV-1c: `┃`, `╹▀▀▀` y filas en blanco no ocupan cupo.
- `rust-migration/tools/orquestacion/pruebas/probar.sh`: casos 16 (pantalla que cambia con una frase de límite), 17 (captura real de OpenCode con el límite al final), 18 (resumen con acentos y «→») y la comprobación de que el producto lee el registro (`ez tasks --parent <padre falso> --json`, código 0, los 18 nombres, stderr sin «registro ilegible»; si falta `EZ_BIN` es FALLO, no se salta). `mkrepo` deja el informe del caso 18 en `.fake-report` (hc no hereda el entorno del supervisor). El caso 8 pasa a 60 s y comprueba que la terminal previa está viva (una vez la de 25 s ya había terminado).
- `rust-migration/tools/orquestacion/pruebas/fake-hc.sh`: `EZ_FAKE_SCREEN=<archivo>` — `hc read` devuelve ese archivo en vez de la pantalla real.
- `rust-migration/tools/orquestacion/pruebas/fake-worker.cmd`: modos nuevos `changing` (contador + frase de límite durante ~24 s), `quiet` (no escribe informe; deja evaluar la pantalla falsa) y `acentos` (copia `.fake-report`); `:commit` separado de `:escribir`.
- `rust-migration/tools/orquestacion/pruebas/NV-1c-pantalla-opencode.txt`: la captura real de NV-1c (121 líneas) como dato de la prueba.

## Decisiones
- `# DECISION (HD-0f): validation = d.get('validation') y no null fijo.` — un registro nuevo queda `"validation": null` (la forma canónica que pide el brief) y una validación que ya puso `ez validate` no se borra cuando el supervisor reescribe el registro.
- `# DECISION (HD-0f): el informe del caso 18 viaja en `.fake-report` dentro del worktree.` — `hc` no hereda el entorno del supervisor; el primer intento con `EZ_FAKE_REPORT` en el entorno dio «sin informe» porque el trabajador no veía la variable.
- `# DECISION (HD-0f): el caso 16 repite la frase completa en cada vuelta del contador.` — así la frase sigue en las últimas 15 líneas con texto mientras la pantalla cambia; el contador es lo que cambia cada ~2 s durante ~24 s (más que la ventana de 15 s).
- `# DECISION (HD-0f): el caso 17 inserta la frase antes de la primera fila en blanco después de la línea 30.` — es «al final de la conversación, antes de las filas vacías»; con el criterio de líneas con texto la frase queda la 11.ª desde el final.
- `# DECISION (HD-0f): el caso 8 vive 60 s y comprueba que la terminal previa está viva.` — una corrida falló porque la de 25 s había terminado antes de que el supervisor la viera; no cambia lo que se prueba y deja un diagnóstico claro.
- La mutación pedida hay que hacerla sobre **todo** el bloque de pantalla quieta: quitar solo la comparación de segundos no rompe el caso 16, porque el corte por «pantalla cambiada» (`PREV_*`) sigue vigente. La mutación usada quita el `if` completo y la comprobación de segundos.

## Verificación
- Entorno: Windows, Git Bash 5.3.15 (Cygwin). Los `.sh` tienen CRLF; todo con `bash -o igncr`. No se compiló Rust ni se lanzaron agentes reales.
- `bash -n`, **fuera de hc** (`env -u HC_TERMINAL_ID -u HC_SCOPE -u HC_TERMINAL_SOCKET`):
  ```
  OK rust-migration/tools/orquestacion/supervise3.sh
  OK rust-migration/tools/orquestacion/pruebas/probar.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-hc.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-parent.sh
  bash -n exit=0
  ```
- `bash -n`, **dentro de hc** (terminal `hd0f-bashn`, lanzada con `hc run`):
  ```
  OK rust-migration/tools/orquestacion/supervise3.sh
  OK rust-migration/tools/orquestacion/pruebas/probar.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-hc.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-parent.sh
  bash -n exit=0
  ```
- `probar.sh` **fuera de hc** (`env -u HC_TERMINAL_ID -u HC_SCOPE -u HC_TERMINAL_SOCKET bash -o igncr rust-migration/tools/orquestacion/pruebas/probar.sh`): 201 líneas, 0 «FALLO», código 0. Tramo final, literal:
  ```
  [pruebas] caso 16: HD-0f una pantalla que cambia con una frase de límite no es un límite
  [HD0T16] intento 1 en t1tw (padre t1t5, agente opencode)
  [HD0T16] informe completado en el intento 1
  [HD0T16] código de supervise3.sh: 0
  OK caso HD0T16: el trabajador terminó su informe (no se lo tocó)
  OK caso 16: la frase de límite estaba en la pantalla
  OK caso 16: la pantalla cambiaba (contador)
  OK registro caso 16: terminado
  [pruebas] caso 17: HD-0f la captura real de OpenCode con el límite al final de la conversación
  [HD0T17] intento 1 en t1tx (padre t1t5, agente opencode)
  [HD0T17] límite de uso en t1tx: Free limit reached. Resets at 7:00 p.m.
  [HD0T17] código de supervise3.sh: 2
  OK registro caso 17 estado: fallido
  OK registro detalle caso 17: límite de uso: Free limit reached. Resets at 7:00 p.m.
  OK aviso aviso caso 17
  OK caso 17 sin reintento: 1
  [pruebas] caso 18: HD-0f un resumen con acentos y símbolos llega intacto al registro
  [HD0T18] intento 1 en t1ty (padre t1t5, agente opencode)
  [HD0T18] informe completado en el intento 1
  [HD0T18] código de supervise3.sh: 0
  OK resumen con acentos: Estado: completado — verificación → ok
  [pruebas] producto lee el registro del supervisor
  OK producto tasks código: 0
  OK producto tasks nombres: OK
  OK producto tasks: stderr sin «registro ilegible»
  [pruebas] resultado: VERDE
  ```
  (Se quitaron las líneas `warning: in the working copy of …` de git; el resto es literal. El producto es `C:/Users/JDMoreu/Documents/EzAgent/rust/target/debug/ez.exe` con `EZ_SUBAGENTS_DIR` = la carpeta de la prueba.)
- `probar.sh` **dentro de hc** (terminal `hd0f-suite`; `HC_TERMINAL_ID` derivado con `hc get hd0f-suite`): 200 líneas, 0 «FALLO», código 0.
  ```
  [pruebas] caso 16: HD-0f una pantalla que cambia con una frase de límite no es un límite
  [HD0T16] intento 1 en t1vp (padre t1v0, agente opencode)
  [HD0T16] informe completado en el intento 1
  [HD0T16] código de supervise3.sh: 0
  OK caso HD0T16: el trabajador terminó su informe (no se lo tocó)
  OK caso 16: la frase de límite estaba en la pantalla
  OK caso 16: la pantalla cambiaba (contador)
  OK registro caso 16: terminado
  [pruebas] caso 17: HD-0f la captura real de OpenCode con el límite al final de la conversación
  [HD0T17] intento 1 en t1vq (padre t1v0, agente opencode)
  [HD0T17] límite de uso en t1vq: Free limit reached. Resets at 7:00 p.m.
  [HD0T17] código de supervise3.sh: 2
  OK registro caso 17 estado: fallido
  OK registro detalle caso 17: límite de uso: Free limit reached. Resets at 7:00 p.m.
  OK aviso aviso caso 17
  OK caso 17 sin reintento: 1
  [pruebas] caso 18: HD-0f un resumen con acentos y símbolos llega intacto al registro
  [HD0T18] intento 1 en t1vr (padre t1v0, agente opencode)
  [HD0T18] informe completado en el intento 1
  [HD0T18] código de supervise3.sh: 0
  OK resumen con acentos: Estado: completado — verificación → ok
  [pruebas] producto lee el registro del supervisor
  OK producto tasks código: 0
  OK producto tasks nombres: OK
  OK producto tasks: stderr sin «registro ilegible»
  [pruebas] resultado: VERDE
  ```
- **Mutación del caso 16** (pedida por el brief): copia de `tools/orquestacion` en una carpeta temporal con el bloque completo de pantalla quieta de `limit_hit` eliminado; `bash -n` de la copia: ok. El resto de la suite sigue en verde y solo falla el caso 16:
  ```
  [pruebas] caso 16: HD-0f una pantalla que cambia con una frase de límite no es un límite
  [HD0T16] intento 1 en t1x7 (padre t1wj, agente opencode)
  [HD0T16] límite de uso en t1x7: Free limit reached. Resets at 7:00 p.m.
  [HD0T16] código de supervise3.sh: 2
  FALLO: caso 16 devolvió 2 (esperado 0)
  FALLO caso HD0T16: el supervisor tomó la pantalla por un límite de uso
  OK caso 16: la frase de límite estaba en la pantalla
  OK caso 16: la pantalla cambiaba (contador)
  FALLO registro caso 16: esperado «terminado», hay «fallido»
  ...
  [pruebas] resultado: ROJO
  ```
  Corrida mutada: 202 líneas, código 1, exactamente esos 3 «FALLO» (todos del caso 16).
- pytest / ruff / cargo: no aplican (solo shell). **No se compiló Rust**; el producto usado ya estaba compilado.
- Limpieza: no queda ninguna terminal `hd0*` ni `hd0f*`; `ir-1`, `nv-1b`, `dx-1c`, `dx-1d` y esta (`hd-0e`) siguen vivas.

## Pendiente o riesgos
- El cambio de carpeta por omisión no lo ejerce la suite (siempre define `EZ_SUBAGENTS_DIR`); se comprobó contra `Registry::from_env` en `rust/crates/ez-herdr/src/subagents/registry.rs`: `%LOCALAPPDATA%\EzAgent\subagentes`.
- La comprobación del producto usa el `ez.exe` ya compilado de `C:/Users/JDMoreu/Documents/EzAgent/rust/target/debug/`; si no existe, la suite falla a propósito (el brief prohíbe saltarla y compilar).
- El caso 17 depende de la captura de NV-1c; si el TUI de OpenCode cambia, hay que actualizar el dato.
- El caso 8 falló una vez por una terminal previa de 25 s que ya había terminado; quedó en 60 s y con la comprobación de que está viva. Si vuelve a pasar, hay que mirar el tiempo de arranque de `hc` bajo carga.
- `[[:alnum:]]` de gawk siempre reconoce letras y dígitos ASCII (las frases de límite son ASCII); el producto usa el criterio Unicode, que en la práctica cubre lo mismo para estas pantallas.
