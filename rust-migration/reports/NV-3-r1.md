# NV-3 — La app de voz como padre: cuenta las novedades y responde por ellas

## Resultado
- Brief/tarea: NV-3 (con la adenda del 19-nov noche)
- Estado: completado

## Cambios
- `rust/crates/ez-app/src/app/subagents.rs`: módulo nuevo. Latido de novedades del registro bajo el padre `voz` (aviso por voz y chat, una vez por episodio), «¿cómo va X?» y «¿cómo van los agentes?», oferta de reintento y `ez retry <nombre> --auto` tras «sí/dale», marca `fallido` de un lanzamiento asíncrono que no llegó y la herramienta `subagent_status` para las paráfrasis del LLM.
- `rust/crates/ez-app/src/app/subagents/phrases.rs`: textos deterministas de cada estado (`bloqueado`, `fallido`, `terminado`, `validado`, retoma desde `bloqueado`), lista del registro y análisis de las formas directas de la pregunta (conserva guiones de los nombres; «¿cómo va todo?» no se toma por un subagente).
- `rust/crates/ez-app/src/app/subagents/port.rs`: `AppHerdrPort`, el mismo `HerdrPort` de siempre con registro: tras un lanzamiento confirmado guarda un `Record` bajo `voz` con terminal, tipo, modelo, esfuerzo, etiqueta, carpeta, encargo (primeras 200 letras) e historial `by = "voz"`; un fallo al guardar solo deja aviso, nunca tumba el lanzamiento.
- `rust/crates/ez-app/src/app/subagents/tests.rs`: pruebas con app real de dobles, registro temporal y reintento falso: cada estado se anuncia una vez, los cambios del padre no se anuncian, oferta aceptada/rechazada/caducada, respuestas del registro, atajo de voz, herramienta y registro de lanzamientos/nombres.
- `rust/crates/ez-app/src/app/core/state.rs`: `AgentsState` gana `registry`, `retry_offer` y `announced`; `RuntimeState` gana `retry_runner`.
- `rust/crates/ez-app/src/app/core/services.rs`: `AppParts` gana `subagents_dir` y `retry_runner`; tipo `RetryRunner` (costura de prueba del reintento).
- `rust/crates/ez-app/src/app/core/init.rs`: construye el `Registry` (carpeta inyectada, o `EZ_SUBAGENTS_DIR`/`%LOCALAPPDATA%`; en pruebas, temporal) y guarda el runner.
- `rust/crates/ez-app/src/app/core/parts.rs`: las herramientas de Herdr usan `AppHerdrPort`; se añade la herramienta `subagent_status`; `launch_failed` marca `fallido` el registro del lanzamiento que no llegó.
- `rust/crates/ez-app/src/app/activity.rs`: `activity_tick` (cada 1,5 s) late el registro de subagentes.
- `rust/crates/ez-app/src/app/events/branches.rs`: antes del atajo y del LLM, un «sí/dale» resuelve la oferta de reintento pendiente.
- `rust/crates/ez-app/src/fast.rs`, `rust/crates/ez-app/src/app/settings_apply.rs`: camino rápido de «¿cómo va …?» / «¿cómo van los agentes?» con la frase escrita por código, como los acuses de DX-2.
- `rust/crates/ez-app/src/app/mod.rs`, `rust/crates/ez-app/src/app/core.rs`: declaración y exportaciones del módulo/tipo nuevos.
- `rust/crates/ez-app/src/app/states/tests/support.rs`: `Fixture::with_setup` para inyectar el registro y el reintento sin tocar el registro real.
- `rust/crates/ez-app/examples/diag_subagents_smoke.rs`: prueba de humo de la app real sin micrófono (registros falsos `en_proceso` → `bloqueado` → `terminado`, anuncios en el chat y cero `PÁNICO`).
- `rust-migration/reports/NV-3-smoke.txt`: salida literal de la prueba de humo.

## Decisiones
- `// DECISION` en `port.rs`: el padre fijo es `voz`, un nombre de `hc` válido que no existe como terminal; el papel del registro también es `voz`, así el nombre automático queda `voz1-opencode` (§6).
- `// DECISION` en `port.rs`: el registro se guarda después de que `hc` confirmó la terminal (`launching`/`task_sent`); si no se puede guardar, se avisa por `tracing` y el lanzamiento sigue.
- `// DECISION` en `subagents.rs`: `ez retry` se invoca como el `ez` compilado con argumentos en lista y `HC_TERMINAL_ID=voz` (lo autoriza la adenda), en vez de duplicar la elección `--auto` de `ez-ctl`/IR-3. `--json` devuelve el nombre del hijo nuevo para contarlo.
- `// DECISION` en `subagents.rs`: `ez.exe` se busca junto al ejecutable de la app y, si no está, en el `PATH`.
- `// DECISION` en `init.rs`: en `cfg(test)`, sin carpeta explícita, el registro va a una carpeta temporal del proceso; ninguna prueba toca el registro real (DX-2c).
- `// DECISION` en `phrases.rs`: «todo», «esto», «el proyecto»… no se toman como nombre de subagente en la pregunta directa.
- `// DECISION` en `phrases.rs`: un `fallido` con detalle «reemplazado por …» no se anuncia: ese cambio lo cuenta la propia oferta de reintento.
- `// DECISION` en `phrases.rs`: el alta `en_proceso` no se anuncia; solo se anuncia volver de un bloqueo («siguió trabajando»).
- `// DECISION` en `subagents.rs`: el `notified` del disco lo escribe un hilo `ezagent-subagents-mark` (el candado del registro puede esperar a otro escritor); dentro de la sesión manda el mapa `announced`, para que un marcado atrasado no silencie un episodio nuevo.
- Desviaciones del brief: se invoca el `ez` compilado (previsto en la adenda) y **no** se toca `Cargo.toml` ni `ez-scan`; no se tocó ningún archivo de `ez-herdr`, `ez-ctl`, `ez-hud` ni `ez-stt`. Los archivos extra tocados son todos de `ez-app` y son el cableado de estado/atajo/herramienta que el módulo nuevo necesita.

## Verificación
- `cargo fmt -p ez-app -- --check`: sin diferencias (código 0).
- `cargo clippy -p ez-app --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.64s` (código 0).
- `cargo test -p ez-app`: código 0; **368 pruebas de la librería en verde** (359 previas + 9 nuevas), más las de integración:
  `test result: ok. 368 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
  (y el resto de binarios/ejemplos en 0/1/2 pruebas, todas en verde).
- Prueba de humo real: `cargo build -p ez-app --example diag_subagents_smoke` y `./target/debug/examples/diag_subagents_smoke.exe` (salida literal del programa, código 0):

```text
app viva; registros en C:\Users\JDMoreu\AppData\Local\Temp\ezagent-nv3-humo-10996-1795138239708176500\subagentes
estado inicial: ir-1 en_proceso; anuncios=0
ANUNCIO bloqueado: Ojo: ir-1 (OpenCode · glm-5.3) está bloqueado: límite de uso del plan gratuito (se reinicia a las 7:00 p.m.). ¿Lo reintento con la recomendación?
ANUNCIO terminado: Listo: ir-1 (OpenCode · glm-5.3) terminó. Resumen: «Rama NV-3 en verde.».
atajo «¿cómo va ir-1?»: handled=true
app viva al final: true
PÁNICO en el registro temporal: 0
RESULT bloqueado=true terminado=true atajo=true viva=true panico=0
```

- No se ejecutaron agentes reales (`claude`, `opencode`, `codex`, `qwen`, `mcode`, `pi`) ni `ez retry` real; el reintento se probó con el runner inyectado y la prueba de humo no lo acepta. La app de la persona, su whisper-server y la ventana de Herdr no se tocaron. Las pruebas y el humo usan `APPDATA`, `EZAGENT_LOGS_DIR` y `EZ_SUBAGENTS_DIR` temporales y borran lo que crean.

## Pendiente o riesgos
- El camino de producción de `ez retry` (proceso `ez` real) no se ejecutó de punta a punta: habría lanzado un agente real. Se apoya en el contrato del CLI de `ez-ctl` (`retry --auto --json`, `HC_TERMINAL_ID`).
- La app no corre un vigilante propio para el padre `voz` (no es una terminal de Herdr y el vigilante de NV-1 exige un padre vivo). Por eso un agente de la app que muera sin informe se queda `en_proceso` hasta que alguien escriba el registro; los estados que sí llegan (`terminado`/`fallido`/`validado`) los escribe `ez report`/`ez validate` y se anuncian. Detectar «límite de uso» para estos agentes requiere que el supervisor/vigilante escriba bajo `voz` (fuera de los archivos de NV-3).
- Los agentes de cadena (`then`, prelanzamiento) no pasan por el puerto envuelto y no dejan registro propio; el enjambre ya anuncia su avance por el camino existente de `SwarmNotice` (no usa los registros `voz`).
- El marcado `notified` es best-effort: si la app se cierra entre el anuncio y el hilo de marcado, la novedad se repite una vez al arrancar.
