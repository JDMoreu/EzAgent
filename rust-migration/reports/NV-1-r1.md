## Resultado
- Brief/tarea: NV-1 (ola HD)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents.rs`: nuevo `TaskState::Blocked` («bloqueado»), tabla de transiciones (`en_proceso ⇄ bloqueado`, `bloqueado → fallido | terminado`), aviso de §2 y aviso «siguió trabajando» al volver de `bloqueado`.
- `rust/crates/ez-herdr/src/subagents/watch.rs`: el vigilante lee `status.state` de cada hijo, guarda la pantalla vista, llama al detector de novedades antes de avisar y considera `bloqueado` como trabajo pendiente (no se apaga el vigilante ni se marca `fallido` una terminal bloqueada viva).
- `rust/crates/ez-herdr/src/subagents/watch/novelties.rs`: detector de §3: límites por patrones de `%APPDATA%\EzAgent\novedades.toml` (fábrica incluida, sin distinguir mayúsculas), hora de reinicio (`Resets at 7:00 p.m.`, `resets in 2h`), diálogos (`status.state = blocked` o pantalla que clasifica el módulo `permission`), «sin avance» a los `stall_min` minutos y vuelta a `en_proceso`; escribe `<raíz>/uso.json` con escritura atómica.
- `rust/crates/ez-herdr/src/subagents/tests.rs`: estados de §2 (nombre/serde) y textos exactos de bloqueo y de reanudación.
- `rust/crates/ez-herdr/src/subagents/tests/registry.rs`: tabla completa de 25 pares de transiciones con `bloqueado`.
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs` y `watch/tests/novelties.rs` (nuevo): doble de `hc` con pantallas; pruebas de límite de OpenCode/Codex/Claude Code, `uso.json`, diálogo de `hc` y de pantalla, sin avance, vuelta a trabajar, un aviso por episodio, `hc read <t> --lines 0` y bloqueado que muere sin informe.
- `rust/crates/ez-herdr/src/subagents/watch/novelties/tests.rs` (nuevo): pruebas unitarias de patrones, `stall_min`, horas de reinicio, tope de 4 000 caracteres y `uso.json`.

## Decisiones
- `novedades.toml` se lee con un formato propio, una regla por línea (`agente = patrón`, `*` para todos, `stall_min = N`), para no añadir la dependencia `toml` (el brief no la autoriza); el formato está documentado en el módulo. Un archivo existente sin patrones no apaga la detección: se usan los de fábrica.
- `uso.json` es una lista JSON de `{agente, modelo, hasta, visto_ms}` (una entrada por agente+modelo, las vencidas se descartan). Escritura temporal + renombrar, sin candado: el último vigilante que escribe gana (es una caché de agotamientos).
- El aviso de `bloqueado` usa el texto y las opciones de §2 para cualquier causa; el `detail` lo genera el código («límite de uso del plan gratuito (se reinicia a las HH:MM)», «diálogo o permiso pendiente») y nunca copia la pantalla.
- `subagents/registry.rs` no necesitó cambios: el registro ya delega la tabla de transiciones en `transition_allowed`, que vive en `subagents.rs`.
- El diseño §3 incluye «pantalla con un diálogo conocido»; el brief solo nombraba `status.state = blocked`. Se cubre reutilizando `permission::classify` (sin heurísticas nuevas) y solo cuando `hc` no dice `working`.
- Pruebas con `Watcher::with_novelty_file`: nunca leen el `novedades.toml` real de la persona. No se lanzó ningún agente ni se tocó la ventana de Herdr.
- Para respetar el límite de 600 líneas por archivo, las pruebas se repartieron en `watch/tests/novelties.rs` y `watch/novelties/tests.rs`.

## Verificación
- `cargo fmt -p ez-herdr --check`: ok
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2.02s` (sin avisos)
- `cargo test -p ez-herdr`:
  - `test result: ok. 293 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.21s` (unittests)
  - `test result: ok. 1 passed; 0 failed` (agent_driver_parity)
  - `test result: ok. 1 passed; 0 failed` (herdr_parity)
  - `test result: ok. 2 passed; 0 failed` (metrics_parity)
  - `test result: ok. 27 passed; 0 failed` (permission)
  - `test result: ok. 1 passed; 0 failed` (tasks_activity_parity)
  - doc-tests: `0 passed; 0 failed`
- Métricas: `watch.rs` 510 → 555 líneas; `subagents.rs` 296 → 332; nuevo `watch/novelties.rs` 543. Ningún `.rs` pasa de 600 líneas.

## Pendiente o riesgos
- No se ejecutó el ejemplo de punta a punta con `hc`/`herdr-app` reales: NV-1 no lo pide y las pruebas usan `hc` falso, como manda `_COMMON-HD`.
- El formato de `novedades.toml` es propio (no TOML estricto); si la persona ya tuviera un archivo con otra forma, se ignorará y se usarán los patrones de fábrica hasta corregirlo.
- La detección por pantalla de diálogos conocidos comparte el clasificador del modo autónomo; puede bloquear si la salida del agente imita un diálogo, pero se descarta cuando `hc` reporta `working`.
- `uso.json` no tiene candado entre vigilantes de padres distintos: dos escrituras simultáneas pueden perder una entrada (se reescribe en el siguiente aviso). NV-2 puede endurecerlo si lo necesita.
