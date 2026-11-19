# HD-8 — Estado y avisos: `ez report`, `ez validate`, `ez tasks` y el vigilante

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` completo
(sobre todo §13).** Parte de `eco/main` con HD-4 (`ez delegate` local) y HD-7
(`ez_herdr::subagents`) integrados: úsalos tal cual.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-herdr/src/herdr/delegate.rs` (+ `delegate/…`): crear el registro y
  arrancar el vigilante.
- Nuevo `rust/crates/ez-herdr/src/subagents/watch.rs` (bucle del vigilante, con hc y
  reloj inyectables) y la línea `mod`/`pub use` en `subagents.rs`.
- `rust/crates/ez-ctl/src/cli.rs`, `rust/crates/ez-ctl/src/local.rs` (+ `local/…`) y sus
  pruebas.
- `rust/crates/ez-fakes/src/fake_ws_agent.rs` y `fake_ws_agent/echo.rs`: opciones de
  informe del modo `--echo`.

## Cambios
1. **`ez delegate`** (modo local), tras `run_agent`: `Registry::create` con el registro de
   §13 (`en_proceso` si el encargo se entregó o quedó sin confirmar; `fallido` con el
   motivo si no) y asegura el vigilante: `hc run --bg --json --no-parent --no-notify
   --name ez-watch-<hc_slug(padre)> --label ez.watch=<padre> -- <ruta de ez> watch
   --parent <padre> --dir <raíz> --hc <ruta de hc>` (`name_taken` = ya corre: bien). La
   ruta de `ez` es `std::env::current_exe()`. La salida de `ez delegate` añade `state`.
   Por omisión `notify = false` (avisa EzAgent); `--hc-notes` enciende las notas de hc.
2. **`ez report --state terminado|fallido [--summary T] [--file F]`** (modo local: sin
   `EZ_URL`/`EZ_TOKEN` y con `--state`; en modo árbitro, `report` sigue igual): busca su
   registro con `find_by_terminal(HC_TERMINAL_ID)` y hace la transición (`by` = su
   terminal); `--file` debe existir (se guarda la ruta absoluta). Asegura el vigilante.
   Sin registro → código 1 «Esta terminal no tiene una tarea de EzAgent.».
3. **`ez validate <nombre> --pass|--fail [--summary T] [--file F]`**: solo el padre
   (`HC_TERMINAL_ID` = padre del registro) o un hermano (una terminal cuyo padre en hc es
   ese padre); otro → código 1. `terminado` → `validado` (pasa) o `fallido` («no pasó la
   validación de <quién>: …») con `validation` lleno.
4. **`ez tasks [--parent P] [--json]`**: tabla legible (nombre, tipo · modelo, papel,
   estado, hace cuánto, resumen, validación) o JSON con los registros. Padre por omisión:
   `HC_TERMINAL_ID`.
5. **`ez watch --parent P --dir D [--hc H]`** (interno): cada 3 s lee `D/<P>` y `hc ls
   --all --json --label ez.parent=P`; un registro `en_proceso` cuya terminal ya terminó o
   no existe → `fallido` («la terminal terminó sin informe (código N)», `by` =
   «vigilante»); por cada `pending_notices` → `hc send P "<notice_text>"` y
   `mark_notified`; si `hc send` falla, reintenta en la vuelta siguiente. Termina si `P`
   ya no está vivo o si lleva 10 min sin nada `en_proceso` ni avisos pendientes.
6. **`fake-ws-agent --echo DIR [--report terminado|fallido|none] [--ez RUTA]
   [libres…]`**: con `--report` distinto de `none`, al recibir la primera línea escribe
   `DIR/<id>-informe.md` y ejecuta `RUTA report --state <estado> --file <informe>
   --summary "eco <primera marca E2E-… que vea, o 'sin marca'>"` (sin shell). Por omisión
   `none`. Todo lo que recibe por stdin (incluidos los avisos de EzAgent cuando es el
   padre) queda en `lines`.

## Pruebas
- Vigilante con hc y reloj falsos: terminal terminada sin informe → `fallido` + aviso;
  `terminado` → un aviso y no se repite; cambio hecho por el padre → sin aviso; fallo de
  `hc send` → reintento; salida por padre muerto y por 10 min sin nada.
- `ez-ctl`: análisis y códigos de `report`/`validate`/`tasks`/`watch`; `validate` desde un
  extraño (código 1); `report` sin registro (código 1); `--file` inexistente (código 2).
- `delegate`: crea el registro `en_proceso` o `fallido`; arranca el vigilante una vez
  (`name_taken` no es error).
- `fake-ws-agent`: el modo `--report` arma bien el comando.

## Criterios de aceptación
fmt, clippy y test de `ez-herdr`, `ez-ctl` y `ez-fakes` en verde. Informe en
`rust-migration/reports/HD-8-r1.md`.

## Correcciones y reglas heredadas de las revisiones
- **R4-1**: `delegate/names.rs::valid_name` duplica `child.rs::hc_valid_name`: deja una sola
  (`pub(crate)` en `child.rs`) y reúsala. Autorizado tocar esa visibilidad en `child.rs`.
- **Nunca ejecutes `ez delegate` (ni `ez report`/`validate`/`watch`) contra el `hc` real**:
  tu terminal tiene `HC_TERMINAL_ID` y lanzaría agentes de verdad (pasó en HD-4). Prueba
  solo con el runtime falso; si quieres probar el binario, quita `HC_TERMINAL_ID` del
  entorno y usa un `hc` falso.
- **Memoria**: ninguna prueba o medición tuya puede pasar de 1 GB; el equipo es compartido
  con otros subagentes.
