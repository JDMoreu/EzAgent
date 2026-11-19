# Informe ECO-8-r1 — Prueba de punta a punta con agentes falsos y herdr-core real

## Resultado
- Brief/tarea: ECO-8 (ola 2; rama `work/ECO-8`, copia de `eco/main`)
- Estado: completado

## Cambios
- `rust/crates/ez-fakes/src/fake_ws_agent.rs` (+ `state.rs`, `instructions.rs`, `report.rs`, `script.rs`, `process.rs`): agente falso con estado OSC 7501, modo `--exec`/`--verify`/`--main <guion.toml>`, extracción de la ruta del informe y de las banderas de mutaciones desde las instrucciones, lectura del guion y ejecución de `ez` sin shell. Pruebas unitarias de cada pieza.
- `rust/crates/ez-fakes/src/bin/fake_ws_agent.rs`, `src/lib.rs`, `Cargo.toml`: binario `fake-ws-agent`, módulo público y dependencia `toml` (ya estaba en `[workspace.dependencies]`); `Cargo.lock` cambió una línea por efecto de compilar. No se tocó `fake_agent.rs`.
- `rust/crates/ez-hub/src/runtime/mod.rs` y `engine.rs`: `RuntimeOptions { ez_dir: Option<PathBuf> }` y `HubRuntime::start_with(data_dir, options)`; `start` delega con valores por omisión. Es lo único que se cambió del motor.
- `rust/crates/ez-hub/examples/e2e_fake.rs` (+ `e2e_fake/{fixtures,scenario,watch}.rs`): ejemplo que localiza `hc`, siembra un repositorio git y los datos de la app con tres perfiles falsos, arranca el motor, aprueba tarjetas, elige `merge`, verifica rondas limpias/merge/ausencia de tokens y limpia sus terminales y temporales.

## Decisiones
- `fake_ws_agent/report.rs`: el informe del doble siempre es `pass` con 0 hallazgos; el valor lo fija el brief, no el modelo. Las banderas de mutaciones se añaden solo si las instrucciones piden `--mutations-tried`.
- `fake_ws_agent.rs`: los candados de stdin/stdout se toman solo para cada escritura/lectura y se sueltan antes de ejecutar `ez` (regla de no retener locks durante subprocesos).
- `fixtures.rs`: se siembra el agente principal con id `e00000000` en `agentes.json`, `elefantes.json` y `principales.json` en vez de usar `main_agent_create`. Motivo: `ez_hub::runtime::handler::wait` llama a `wait_for_elephant(space, …)`, que escucha `ElephantKey::default_for(space)` (`runtime/mod.rs:327-333`), mientras los eventos de la tarea se emiten a la cola del agente real (`e<8 hex>`, ver `emit.rs:43-59`); con un agente recién creado, `ez wait` nunca vería las aprobaciones. No se tocó `handler.rs` por estar fuera de alcance.
- `scenario.rs`: `plan_reviewer` lleva `etapa = 5` y corre después de `qa` en la ronda 1. Motivo: `ez_arbiter::arbiter::fish::order::lower_stage_waiting` (`order.rs:112-123`) exige el informe de todo verificador de etapa menor en cada ronda, sin excluir los papeles `solo_primera_ronda`; la primera ronda solo de verificación fue rechazada con `R5: Todavía no: falta el informe de revplan-r2 en esta ronda.` (evidencia: bitácora del primer ensayo, `seq` 23). No se tocó `ez-arbiter` por estar fuera de alcance.
- `e2e_fake.rs::handle_card`: el banco de pruebas hace `git add fake` + `git commit` en el worktree justo antes de la tarjeta de integración. Motivo: `agents::merge` usa `git merge --no-ff` y el doble deja el archivo sin confirmar; sin este paso la integración terminaba bien pero la rama base no contenía `fake/*.txt` (primer ensayo: `EXIT=1`, `Fallo: no pude leer …\repo\fake`). Es scaffolding del banco, no un cambio del producto ni del doble.
- `toml` se añadió como dependencia directa de `ez-fakes` para leer el guion; es una dependencia permitida del workspace.

## Verificación
- `cargo fmt -p ez-fakes -p ez-hub --check`: exit 0 (`FMT_OK`).
- `cargo clippy -p ez-fakes -p ez-hub --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.64s`, exit 0, sin avisos.
- `cargo test -p ez-fakes -p ez-hub`: **142 passed; 0 failed; 0 ignored** (57 de `ez-fakes`, 84 de la biblioteca de `ez-hub`, 1 de `tests/neutral_names.rs`). En una ejecución intermedia falló una vez `fake_hc::tests::concurrent_sends_do_not_lose_entries` (prueba de concurrencia sobre un JSON, archivo no tocado por este brief); pasa al repetirla de forma aislada y en la ejecución final.
- `cargo run -p ez-hub --example e2e_fake`: exit **0**. Salida literal:
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s
       Running `target\debug\examples\e2e_fake.exe`
  Motor normal en http://127.0.0.1:47830; agente principal e00000000.
  Agente principal lanzado en t8h.
  Aprobada la tarjeta c8716dda3f606 (task).
  Aprobada la tarjeta c1087f84b8d9a (team).
  Integrada la rama desde la tarjeta c1a8761a11fd7.
  Éxito: task_closed, integrated, 2 rondas limpias, 2 archivos fake y merge ec30b790.
  ```
- Límites: ningún `.rs` añadido supera 600 líneas (máximo 215 en `examples/e2e_fake.rs`) ni función nueva 60 líneas; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas; ningún lock retenido durante E/S, subproceso o espera.
- Limpieza: al terminar el ejemplo no quedan procesos `fake-ws-agent`, terminales de hc con la etiqueta del espacio ni carpetas `ez-e2e-fake-*`.

## Pendiente o riesgos
- **Hallazgo de producto (fuera de alcance):** `ez-hub/src/runtime/handler.rs:49-51` ignora el agente principal del `Caller` en `wait`, de modo que `ez wait` de un agente principal con id aleatorio no recibe sus propios eventos. Afecta al caso de varios agentes principales por espacio. El ejemplo lo esquiva sembrando el agente `e00000000`.
- **Hallazgo de producto (fuera de alcance):** `ez-arbiter/src/arbiter/fish/order.rs:112-123` no excluye a los verificadores `solo_primera_ronda` de la espera por etapas en rondas posteriores, bloqueando la «ronda solo de verificación» del diseño §15.4. El ejemplo lo esquiva con `etapa = 5` para `plan_reviewer`.
- Riesgo de integración: `git merge` no incorpora cambios sin confirmar y las plantillas de papel prohíben hacer commits; en un ciclo real har falta que alguien confirme el trabajo antes de integrar. Aquí lo hace el banco de pruebas.
- El ejemplo usa un `hc` real y solo cierra las terminales que crea; no detiene el servidor `hc` que el propio motor arranca.
- No se probó el caso M07 (sandbox de Codex que no deja ejecutar `ez.exe`): aquí se usó `hc` local y los binarios compilados.
