# AUTO-A3 — Modo autónomo, fase A3: el bucle en la app

## Resultado
- Brief/tarea: AUTO-A3 (10.6, 10.7, 10.9, 10.10, 10.13, 10.16 y la parte de 10.19 que es de la app)
- Estado: completado

## Cambios
- `rust/crates/ez-app/src/app/autonomous/mod.rs`: el bucle (`AutonomousLoop`), el hilo `ezagent-autonomo`, el estado compartido mínimo (resumen, direcciones cubiertas, verificadores, peticiones), el `AtomicU64` de generación y el ciclo de vida (arrancar, adoptar, apagar). Ningún cerrojo se retiene durante Herdr, disco, runtime, barra o confirmaciones.
- `rust/crates/ez-app/src/app/autonomous/driver.rs`: sondeo cada segundo (10.6), fin del ejecutor (`done`, `fin-N.md` en reposo, 3 min sin fin, `lost`), cuota/contexto/sesión perdida, verificación y aplicación de las `Action` puras.
- `rust/crates/ez-app/src/app/autonomous/approve.rs`: la aprobación como transacción de 10.7 (S1, espera de 1,5 s, S2 con la misma huella, generación, teclas, S3 en ≤ 2 s), antirrebote por huella, auditoría con `log_approval` y bitácora con `audit_summary` (10.15), y la aprobación de `Trust` del lanzamiento (10.9).
- `rust/crates/ez-app/src/app/autonomous/verify.rs`: verificador nuevo por ronda con su carpeta `verificacion-K`, su nonce y `VERIFIER_PROMPT`; espera del veredicto, copia privada, cierre del verificador, corrección con `send_agent` y `privado/resumen.md` con la evidencia y `git diff`/`log` (10.10, 10.12 y 10.16).
- `rust/crates/ez-app/src/app/autonomous/host.rs` y `host/{start,restore,git}.rs`: `AutonomousHost` (arranque, estado, «sigue» con prórroga, «para»), `workdir` canónico, terminal ya abierta validada por espacio, lanzamiento del ejecutor y del verificador, captura del commit base y reinicio en pausa (9.3, 10.2, 10.9, 10.13).
- `rust/crates/ez-app/src/app/autonomous/time.rs`: reloj inyectable (`RealClock`/`TestClock`) para que las pruebas no esperen minutos.
- `rust/crates/ez-app/src/app/autonomous/wiring.rs`: creación e instalación del bucle antes del runtime (A1b), auditoría del runtime, `autonomous_covers`, y el atajo de cancelar que detiene el modo (decisión 7).
- `rust/crates/ez-app/src/app/autonomous/tests*.rs`: 10 pruebas con un `HerdrPort` falso, pantallas reales de Claude Code, Codex y un directorio temporal.
- `rust/crates/ez-app/src/app/mod.rs`: declara el módulo nuevo.
- `rust/crates/ez-app/src/app/core/{state,init,parts}.rs`: campo del bucle en `AgentsState`, creación antes de `AgentRuntime::new` y auditoría/reinicio después. En el arnés de reproducción (`runtime_wrap`) no se instala nada, para no cambiar sus esquemas de herramientas ni leer datos reales.
- `rust/crates/ez-app/src/app/watch.rs`: `should_announce` falso y sin `DonePrompt` para las direcciones de `scope` (10.6).
- `rust/crates/ez-app/src/app/events.rs`: el atajo de cancelar llama a `cancel_hotkey` (cancela la confirmación y detiene la tarea).
- `rust/crates/ez-app/src/app/startup.rs`: apagado del bucle en el orden de cierre.
- `rust/crates/ez-app/src/app/core/tests.rs`: el paso «autonomo» en el orden de apagado esperado.
- `rust/crates/ez-agent/src/autonomous/texts.rs`: `VERIFIER_PROMPT` con rutas y token, la frase de corrección y los textos de voz que faltaban (sesión perdida, verificador, diálogo cambiado, teclas, lectura, equipo, carpeta y terminal).
- `rust/crates/ez-agent/src/autonomous/mod.rs`: puente `screen_incident_reason` sobre `ez_swarm::incidents::classify` (único arreglo fuera de `texts.rs`, explicado abajo).
- `rust-migration/reports/AUTO-A3-r1.md`: este informe.

## Decisiones
- `// DECISION` (`host.rs`): el encargo de A1 dice literalmente «fin-N.md (con la N de la ronda en curso)»; el bucle escribe en la ronda 1 el nombre exacto `fin-1.md` para que el ejecutor no cree un archivo que el bucle nunca vería.
- `// DECISION` (`host.rs`): si el espacio de una terminal ya abierta no se puede leer de Herdr (ni `where` ni `space`), se acepta la terminal que el usuario nombró; si se lee y no coincide, se devuelve error (decisión 2 del brief).
- `// DECISION` (`host.rs`): `start_autonomous_team` devuelve «El modo autónomo con equipo todavía no está disponible.» y no arranca nada; el modo B es la fase siguiente (10.18).
- `// DECISION` (`host/start.rs`): el ejecutor se lanza en el hilo de la herramienta, dentro de `host.start`, para devolver su error al modelo; el hilo `ezagent-autonomo` solo vigila y aplica transiciones.
- `// DECISION` (`host/restore.rs`): la tarea restaurada se reconstruye desde `estado.json`, `encargo.md` y `criterios.md` (el `verifier_agent` vuelve a ser el del ejecutor porque `estado.json` no lo guarda). Una tarea ya en pausa se restaura sin repetir el aviso.
- `// DECISION` (`driver.rs`): el hilo sigue vivo en `done`/`stopped` esperando peticiones para poder aceptar el resultado; sale al arrancar otra tarea o al apagar la app.
- `// DECISION` (`driver.rs`): al pausar se cierran los verificadores abiertos; el ejecutor se deja abierto siempre (10.13 solo lo dice para la parada, pero un verificador sin supervisión no debe quedar vivo).
- `// DECISION` (`driver.rs`): los dos errores seguidos del ejecutor se cuentan por rachas (un `working` los reinicia), como pide 10.10.
- `// DECISION` (`driver.rs`): el aviso de ronda se dice una vez por número de ronda (`announced_round`), no en cada verificación limpia que relanza el verificador.
- `// DECISION` (`approve.rs`): la bitácora usa `audit_summary` de `ez-herdr` con la dirección añadida y la auditoría usa `log_approval`; ninguna de las dos guarda pantalla.
- `// DECISION` (`wiring.rs`): el bucle se crea solo en producción; con `runtime_wrap` (arnés de reproducción) no se instala ni se restaura nada.
- `// DECISION` (`mod.rs`): el bucle es un `OnceLock` de host en `ez-agent` (A1); se instala antes de `AgentRuntime::new`, así que el grupo de herramientas lo ve sin reconstruir el registro.
- `// DECISION` (`time.rs`): el reloj es inyectable solo para las pruebas; producción usa tiempo real y esperas reales.
- `// DECISION` (`ez-agent/autonomous/mod.rs`): `screen_incident_reason` reutiliza el clasificador de `ez-swarm` (9.5). `ez-app` no declara `chrono`, que es el tipo que pide `classify`; el puente de una función en `ez-agent` evita duplicar los patrones y es el único cambio de `ez-agent` fuera de `texts.rs`.

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy -p ez-agent -p ez-app -p ez-herdr --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-app --lib`: **305 pruebas en verde**, 0 fallos; 10 son del bucle autónomo.
- `cargo test -p ez-agent -p ez-herdr`: **247 + 172 en verde** (más las integraciones), 0 fallos.
- `cargo test -p ez-app --features replay --test replay_parity`: en verde (ver nota de entorno abajo).
- Pruebas del brief: recorrido feliz (aprobación real de Claude Code → teclas → `fin-1.md` → dos verificadores con token → `done` y `resumen.md`), ronda sucia con `correccion-1.md`, comando denegado sin teclas, `stop` entre S1 y el envío, diálogo que cambia entre S1 y S2, verificador que intenta editar fuera de su carpeta, reposo sin `fin`, reinicio en pausa, clave falsa fuera de auditoría y bitácora, tope de rondas con prórroga de +3 y `fin-1.md` en el encargo.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command` ni `.unwrap()` fuera de pruebas. Ningún `.rs` pasa de 600 líneas (el mayor: `mod.rs`, 383 líneas tras `cargo fmt`).

## Fixtures consumidos
- `rust/crates/ez-herdr/tests/permission/claude_bash.txt`, `codex_exec.txt` y `codex_edit.txt`: 3 pantallas reales, usadas en 5 pruebas (aprobación feliz, diálogo que cambia, verificador fuera de carpeta y la clave falsa). No hay fixtures de paridad del dominio autónomo (0 casos).

## Pruebas portadas (`parity/test-map.csv`)
- Ninguna fila: es funcionalidad nueva aprobada por el usuario, sin Python de referencia en `src/`; `test-map.csv` no tiene entradas de autonomía. Las 10 pruebas propias cubren los nueve escenarios obligatorios del brief más el tope de rondas y la prórroga.

## Pendiente o riesgos
- **`start_autonomous_team` pide la confirmación y después devuelve error** (fase B): la herramienta de A1 confirma antes de llamar al host; no se tocó `ez-agent` para saltarla porque el modo B es la tarea siguiente.
- **`verifier_model`**: `StartSpec` de A1b no lo lleva; A3 usa `verifier_agent` y no hay modelo de verificador. Queda para B.
- **Escenario de reproducción nuevo**: el brief no lo lista entre los archivos y las pruebas del bucle son unitarias; no se añadió un escenario a `replay/`. El arnés no instala el bucle para no alterar la paridad.
- **Terminal ya abierta sin espacio legible**: se acepta (ver decisión). Si Herdr empieza a devolver `space` siempre, conviene volverlo estricto.
- **Antirrebote por huella**: un permiso idéntico repetido más tarde no se vuelve a aprobar (es lo que pide 10.7); el ejecutor acabará en reposo y el bucle pausará a los 3 min, que es la salida segura.
- **Aprobación durante el lanzamiento**: el `Trust` del arranque cuenta como una aprobación del tope; es coherente con 10.15, pero un tope de 1 aprobación pausaría justo después de lanzar.
- **Tiempo de lanzamiento**: `run_agent` puede tardar hasta 120 s y corre en el hilo del ejecutor o de la herramienta; ninguna cerrojo se retiene, pero un `stop` durante ese lanzamiento se aplica al volver.
- **`install_host` es un `OnceLock`**: si el proceso recreara el runtime (hoy no lo hace), las herramientas seguirían apuntando al bucle viejo.
