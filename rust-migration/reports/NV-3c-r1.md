# NV-3c — El humo de la app como padre, con el `hc` real

## Resultado
- Brief/tarea: NV-3c
- Estado: completado

## Cambios
- `rust/crates/ez-app/examples/diag_subagents_smoke.rs`: el modo por omisión sigue siendo `fake-hc` (conservado, con la salida limpia) y se añade `--real-hc`; los helpers comunes (configuración, registro, chat, espera, pánicos) se comparten entre los dos modos.
- `rust/crates/ez-app/examples/diag_subagents_smoke/real.rs`: modo real. Usa el `hc` y el `herdr-app` reales, copias de `fake-ws-agent` en el `PATH` (y en `agents` de `voice.json`) para que ningún tipo del catálogo abra un agente real, lanza dos hijos falsos con nombre único y etiqueta `ez.nv3c`, acepta la oferta de reintento por el camino de voz real y ejecuta el `ez` compilado. Limpia con un `Drop`: cierra por id/prefijo solo las terminales de la prueba, devuelve el foco y borra la carpeta temporal.
- `rust-migration/reports/NV-3c-fake.txt`, `rust-migration/reports/NV-3c-real.txt`: salidas literales de los dos modos.

## Decisiones
- `// DECISION` en el ejemplo: el modo real vive en `diag_subagents_smoke/real.rs` para no pasar de 600 líneas por archivo; el CLI y el modo `fake-hc` siguen en la raíz. Es el único archivo añadido; no se tocó `Cargo.toml` ni ningún otro crate.
- `// DECISION` en `real.rs`: la oferta se acepta con el mismo camino que una transcripción de voz (`start_call`, `advance_turn` y `AppEvent::Transcribed("sí", gen)`), así se ejercita `answer_retry_offer` y el `ez` real de producción sin abrir una API nueva solo para el humo.
- `// DECISION` en `real.rs`: los hijos se crean con `hc run --bg --no-parent` y etiqueta `ez.nv3c=<id>`; el del límite es `fake-ws-agent --echo` + `hc send "Free limit reached. Resets at 7:00 p.m."` (queda parado) y el muerto es `fake-agent --scene working --delay-ms 0` (sale sin informe).
- `// DECISION` en `real.rs`: todos los tipos del catálogo apuntan a copias de `fake-ws-agent` en el `PATH` y en `agents`; el `ez` compilado solo puede lanzar agentes falsos.
- `// DECISION` en `real.rs`: el cierre filtra por el id de la corrida y el prefijo `nv3c`, hace `kill` + `rm`, y devuelve el foco al panel que estaba enfocado (`herdr-app do pane.focus`). No se cierra ninguna terminal de la persona.
- No se modificó código de producción (`ez-app/src`, `ez-herdr`, `ez-ctl`): el cambio es solo el ejemplo, como pedía el brief.

## Verificación
- `cargo fmt -p ez-app -- --check`: código 0.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.94s` (código 0).
- `cargo test -p ez-app`: **369 pruebas de librería en verde** (la última corrida completa; el cambio posterior es solo del ejemplo):
  `test result: ok. 369 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo build -p ez-fakes --bin fake-hc --bin fake-agent --bin fake-ws-agent` y `cargo build -p ez-ctl --bin ez`, previos a las corridas.
- Modo `fake-hc` (salida literal, código 0):

```text
app viva; fake-hc C:\Users\JDMoreu\Documents\EzAgent-wt\NV-3\rust\target\debug\examples\../fake-hc.exe
ANUNCIO: Ojo: ir-2 (OpenCode · glm-5.3) falló: la terminal terminó sin informe.
ANUNCIO: Ojo: ir-1 (OpenCode · glm-5.3) está bloqueado: límite de uso del plan gratuito (se reinicia a las 19:00). ¿Lo reintento con la recomendación?
estado real: ir-1 = bloqueado; ir-2 = fallido
oferta de reintento: Some("ir-1")
atajo «¿cómo va ir-1?»: handled=true
PÁNICO en el registro temporal: 0
RESULT modo=fake-hc bloqueado=true fallido=true oferta=ir-1 atajo=true viva=true panico=0
```

- Modo `--real-hc` (salida literal, código 0; `hc` y `herdr-app` reales):

```text
app viva; hc real C:\Users\JDMoreu\AppData\Local\Programs\herdr-core\hc.exe; ez C:\Users\JDMoreu\Documents\EzAgent-wt\NV-3\rust\target\debug\examples\../ez.exe
hijos reales: nv3c4540a (t21d) y nv3c4540b (t21f)
ANUNCIO: Ojo: nv3c4540a (OpenCode · glm-5.3) está bloqueado: límite de uso del plan gratuito (se reinicia a las 19:00). ¿Lo reintento con la recomendación?
ANUNCIO: Ojo: nv3c4540b (OpenCode · glm-5.3) falló: la terminal terminó sin informe (código 0).
estado real: nv3c4540a = bloqueado (vigilante); nv3c4540b = fallido (vigilante)
uso.json con glm-5.3: true
oferta de reintento: Some("nv3c4540a")
reintento: nv3c4540a-r1 terminal=t21g padre_hc="" viejo_vivo=false retry_of=Some("nv3c4540a")
PÁNICO en el registro temporal: 0
RESULT modo=real-hc bloqueado=true fallido=true reintento=true padre_voz=false viejo_vivo=false viva=true restos=0 panico=0
```

- Comprobación posterior con el `hc` real: `hc ls --all --json | grep nv3c` no devuelve nada (0 terminales de la prueba); `herdr-app status` sigue con el foco en el panel original (`p3bc8…`, `agent t50`). La carpeta temporal se borra en el `Drop` y el registro temporal no tiene `PÁNICO`.
- Ningún agente real: los hijos y el reintento usan copias de `fake-ws-agent`/`fake-agent`; no se leyó ni escribió la configuración, el historial ni los registros de la persona. No se cerró ninguna de sus terminales.

## Pendiente o riesgos
- El reintento lanza una pestaña **visible** en el espacio actual de la persona (es el camino de `ez retry`); la prueba la cierra y devuelve el foco, pero durante los pocos segundos que dura puede verse. Si la persona cambia de foco en ese intervalo, la restauración puede devolverlo al panel anterior.
- `--real-hc` exige `hc`, `herdr-app`, `ez` y los binarios de `ez-fakes` compilados, y la ventana de Herdr abierta; si falta algo sale con 2 sin tocar nada.
- El modelo que elige `ez retry --auto` depende de `modelos.toml` y de `uso.json`; aquí se comprobó que `glm-5.3` queda agotado y el reintento arranca con el agente falso de `opencode`. Si cambia el catálogo real, el `PATH` falso cubre cualquier tipo, pero el perfil podría añadir flags que el agente falso acepta como argumentos libres.
- El `hc get` del hijo nuevo devuelve `parent` nulo/vacío (`padre_hc=""`), que es la prueba de que no hubo `--parent voz`; si una versión de `hc` omitiera el campo, la comprobación lo trataría como vacío.
