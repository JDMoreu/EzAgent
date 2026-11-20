# NV-3b — Corrección: la app vigila de verdad a sus hijos y el reintento no usa `--parent voz`

## Resultado
- Brief/tarea: NV-3b (corrección de la revisión de NV-3)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents/watch.rs`: `NoticeSink` (a quién entrega el vigilante; antes siempre `hc send`), `Watcher::with_notice_sink`, `Watcher::without_parent_terminal` (sin `hc get` de vida y sin filtro `ez.parent`: el registro es la lista de hijos y se cruzan por la dirección de su terminal) y el `send` por el sink.
- `rust/crates/ez-herdr/src/subagents/watch/runtime.rs`: `HcOutput`, `WatchRuntime` y `HcRuntime`, movidos desde `watch.rs` (el archivo pasaba de 600 líneas).
- `rust/crates/ez-herdr/src/subagents/watch/memory.rs`, `watch/novelties.rs`: los avisos pasan el registro al sink (`None` en memoria baja e ilegibles).
- `rust/crates/ez-herdr/src/subagents.rs`: exporta `NoticeSink`.
- `rust/crates/ez-herdr/src/subagents/retry.rs`: antes de lanzar, comprueba si el padre del registro es una terminal (`hc get`); si no lo es (la app, `voz`), el lanzamiento va **sin** `--parent` y el registro nuevo sigue bajo el padre del viejo con `retry_of`. El resultado informa el padre del registro.
- `rust/crates/ez-herdr/src/subagents/retry/tests.rs`, `retry/tests/nombres.rs`, `retry/tests/sin_padre.rs`: prueba del padre sin terminal (ningún `hc run`, ninguna llamada con `--parent`, registro `app1-opencode-r1` bajo `voz` con `retry_of`) y conservación de `--parent t6h` para un padre terminal.
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs`, `watch/tests/sink.rs`: sink con padre sin terminal (hijo muerto → `fallido`; «Free limit reached» → `bloqueado`) sin `hc get` ni `hc send`, con `notified` marcado.
- `rust/crates/ez-app/src/app/core/services.rs`: `SubagentWatch` (runtime, `novedades.toml`, intervalo y margen) y `AppParts.subagent_watch` / `watch_subagents`.
- `rust/crates/ez-app/src/app/core/state.rs`: `subagents_watch_stop` (detiene el hilo al cerrar).
- `rust/crates/ez-app/src/app/core.rs`: `VoiceApp::new` activa `watch_subagents` (producción); el arnés de replay y los dobles no.
- `rust/crates/ez-app/src/app/core/init.rs`, `startup.rs`: arranca el vigilante en `finish_start` y lo para en el apagado.
- `rust/crates/ez-app/src/app/subagents.rs`: `start_subagent_watch` (hilo `ezagent-subagents-watch` con el `Watcher` real de `ez-herdr`), `AppNoticeSink`, `subagent_notice` (frase corta de `news_text`; el texto del vigilante solo para «sin avance»/ilegibles), deduplicación atómica con el latido, `pending_retry` y búsqueda de `ez.exe` en la carpeta padre (desarrollo).
- `rust/crates/ez-app/src/app/subagents/tests.rs`, `tests/watch.rs`, `tests/port.rs`: la prueba del vigilante de la app usa un `WatchRuntime` de guion: nadie escribe el registro, el vigilante marca `bloqueado`/`fallido`, se anuncia y la oferta de reintento se acepta.
- `rust/crates/ez-app/examples/diag_subagents_smoke.rs`: el humo usa el `fake-hc` real de `ez-fakes` con un hijo vivo parado en «Free limit reached» y otro muerto sin informe; los estados los escribe el vigilante, no la prueba. Salida literal en `rust-migration/reports/NV-3b-smoke.txt`.

## Decisiones
- `// DECISION` en `retry.rs`: si `hc get <padre>` no lo encuentra, el padre no es una terminal y el hijo se lanza sin `--parent`; el registro conserva el padre del viejo y `retry_of`. No se toca `ez-ctl`: su `ez retry` usa esta misma biblioteca.
- `// DECISION` en `watch.rs`: en modo sin padre-terminal se listan todas las terminales de `hc ls --all` y el registro decide cuáles importan; no hay `hc get` del padre y la vida la fija el proceso de la app.
- `// DECISION` en `subagents.rs` (app): el sink usa la frase corta de NV-3 (`news_text`); el texto del vigilante se usa solo cuando no hay estado nuevo (sin avance o archivo ilegible). El latido y el sink comparten `announced`, así una novedad se anuncia una sola vez aunque los dos la vean.
- `// DECISION` en `services.rs`/`core.rs`: el vigilante solo se arranca en producción (`watch_subagents`) o con un `SubagentWatch` inyectado; el arnés de replay y los dobles de prueba no abren `hc`.
- Los tests nuevos del vigilante se dividieron en subarchivos (`watch/tests/sink.rs`, `app/subagents/tests/{port,watch}.rs`, `retry/tests/{nombres,sin_padre}.rs`) para no pasar de 600 líneas.
- No se tocó `ez-ctl`; sí `ez-herdr/src/subagents.rs` (solo el `pub use NoticeSink`).

## Verificación
- `cargo fmt -p ez-herdr -p ez-app -- --check`: sin diferencias (código 0).
- `cargo clippy -p ez-herdr -p ez-app --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4.58s` (código 0).
- `cargo test -p ez-herdr`: **327 pruebas de librería en verde** (más 1/1/2/27/1 de integración):
  `test result: ok. 327 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo test -p ez-app`: **369 pruebas de librería en verde** (más 3/1/2/1/2/1 de integración):
  `test result: ok. 369 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- Prueba de humo real con `fake-hc` (`cargo build -p ez-fakes --bin fake-hc`, `cargo build -p ez-app --example diag_subagents_smoke`, ejecutable): código 0; salida literal:

```text
app viva; fake-hc C:\Users\JDMoreu\Documents\EzAgent-wt\NV-3\rust\target\debug\examples\../fake-hc.exe; registros en C:\Users\JDMoreu\AppData\Local\Temp\ezagent-nv3-humo-21760-1795141889635036200\subagentes
registros: ir-1 (t1, límite) e ir-2 (t2, muerto) en_proceso
ANUNCIO bloqueado: Ojo: ir-1 (OpenCode · glm-5.3) está bloqueado: límite de uso del plan gratuito (se reinicia a las 19:00). ¿Lo reintento con la recomendación?
ANUNCIO fallido: Ojo: ir-2 (OpenCode · glm-5.3) falló: la terminal terminó sin informe.
estado real: ir-1 = bloqueado (vigilante); ir-2 = fallido (vigilante)
oferta de reintento: Some("ir-1")
atajo «¿cómo va ir-1?»: handled=true
app viva al final: true
PÁNICO en el registro temporal: 0
RESULT bloqueado=true fallido=true oferta=ir-1 atajo=true viva=true panico=0
```

- No se ejecutaron agentes reales: el humo usa `fake-hc` de `ez-fakes`; no se lanzó `ez retry` real (el camino de lanzamiento se prueba con el runtime falso de `ez-herdr`). No se tocó la app abierta, su whisper-server ni la ventana de Herdr de la persona; todo usa `APPDATA`, `EZAGENT_LOGS_DIR`, `EZ_SUBAGENTS_DIR` y `FAKE_HC_STATE` temporales y borra lo que crea.

## Pendiente o riesgos
- Si la persona tuviera una terminal de Herdr llamada `voz`, `retry()` la vería como padre terminal y volvería a pasar `--parent voz`. Es el nombre fijo que pidió el diseño; si molesta, habría que reservarlo o pasar una marca explícita desde la app.
- El vigilante de la app necesita `hc` (el de `voice.json`); si Herdr está cerrado, no hay novedades hasta que vuelva, como antes. `ez report`/`ez validate` siguen actualizando el registro y el latido los anuncia.
- El hilo del vigilante llama a `hc` con tiempos de hasta 30 s; al cerrar la app se pide parar y `drain_workers` espera 3 s, así que una llamada colgada puede sobrevivir al apagado hasta que el proceso termina.
- Cadenas (`then`, prelanzamiento) y miembros del enjambre siguen sin registro `voz`; el enjambre conserva su aviso por `SwarmNotice`.
- El reintento real desde la app (`ez.exe` compilado) no se ejecutó de punta a punta; la decisión de lanzar sin `--parent` y el `retry_of` sí están cubiertos por la prueba de `ez-herdr`.
