# Informe ECO-1b — `ez-term`: correcciones de la revisión 1

## Resultado
- Brief/tarea: ECO-1b
- Estado: completado

## Cambios
- `rust/crates/ez-term/src/events.rs`: cada evento de notificación lleva `seq: u64`;
  `Missed` pasa a `{ snapshot: Option<Vec<Terminal>>, seq: Option<u64> }`; el
  traductor descarta eventos con `seq` ≤ el del último `missed` con foto;
  `EventStream::snapshot_seq()` expone la secuencia de la foto inicial; el
  traductor se dividió en `typed_event` para respetar el límite de 60 líneas.
- `rust/crates/ez-term/src/conn.rs`: el hilo lector guarda el umbral de secuencia
  y lo pasa al traductor de notificaciones (sin locks durante la espera).
- `rust/crates/ez-term/src/client.rs`: tres topes documentados en `HcClient`
  (simples 30 s, `send` 75 s, `wait` `timeout_s` + 10 s), inyector `with_timing`
  para pruebas y la transacción `launch_and_deliver` con reenvío único.
- `rust/crates/ez-term/src/types.rs`: tipo público `Delivery` (`Delivered`,
  `Unconfirmed`, `Failed(String)`).
- `rust/crates/ez-term/src/lib.rs`: exporta `Delivery`.
- `rust/crates/ez-term/tests/events.rs`: `seq` en todos los eventos, `missed` con
  y sin foto, y prueba nueva de descarte de eventos viejos tras `missed`.
- `rust/crates/ez-term/tests/client.rs`: tres pruebas de tope por método con
  tiempos inyectados (simple, `send` y `wait`).
- `rust/crates/ez-term/tests/launch.rs`: seis pruebas de `launch_and_deliver`
  (entregado, `started` nulo, reintento que funciona, reintento que falla, fallo
  de `run` y fallo posterior conservando la terminal).
- `rust/crates/ez-term/tests/protocolo.rs` y
  `rust/crates/ez-term/examples/hc_smoke.rs`: ajustan los patrones de
  `TermEvent` al nuevo campo `seq`.

## Decisiones
- `// DECISION` en `events.rs`: si el aviso `missed` no trae foto legible, se
  entrega `snapshot: None` y `seq: None`; el consumidor resincroniza con
  `terminal.list` (§14.9).
- `// DECISION` en `events.rs`: si falta `params.seq` en una notificación
  `event`, se usa 0 en vez de inventar un número de secuencia.
- `// DECISION` en `client.rs`: el inyector de topes es público pero
  `#[doc(hidden)]` porque las pruebas de integración no ven el `#[cfg(test)]`
  del crate; solo lo usan las pruebas con valores cortos.
- `// DECISION` en `client.rs`: el fragmento de pantalla del reintento lee 50
  líneas y se recorta a 500 caracteres para el log.
- Sin marcador, heredadas de ECO-1: `list` no envía `all` y el `stderr` de
  `hc connect` se descarta.
- La decisión de ECO-1 «`send` = 30 s + 10 s» queda sustituida por los 75 s que
  manda §14.8/m01; es el cambio 2 de este brief, no una desviación.

## Verificación
Comandos ejecutados desde `rust/`, solo para `-p ez-term` (no se tocó el
workspace ni el `Cargo.lock`):

```text
$ cargo fmt -p ez-term --check
FMT_EXIT:0

$ cargo clippy -p ez-term --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.52s
CLIPPY_EXIT:0

$ cargo test -p ez-term
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (src/lib.rs)
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s  (tests/client.rs)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (tests/events.rs)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s   (tests/launch.rs)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (tests/protocolo.rs)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (doc-tests)
TEST_EXIT:0
```

Comprobación adicional del límite de 60 líneas por función
(`cargo clippy -p ez-term --all-targets --no-deps -- -W clippy::too_many_lines
-D warnings`): sin avisos. Ningún `.rs` del crate supera 600 líneas (máximo:
402 en `tests/client.rs`).

Ejemplo contra el `hc` real (`cargo run -p ez-term --example hc_smoke`),
ejecutado una vez tras el cambio final, con código de salida 0 (se quitan los
códigos de color de `tracing` al pegarlo aquí):

```text
2026-11-19T01:50:50.090375Z  INFO hc_smoke: humo de hc: encontrado hc=C:\Users\JDMoreu\AppData\Local\Programs\herdr-core\hc.exe
2026-11-19T01:50:50.118391Z  INFO hc_smoke: suscripción lista foto=0
2026-11-19T01:50:50.141269Z  INFO hc_smoke: hello version=0.1.21
2026-11-19T01:50:50.157147Z  INFO hc_smoke: terminal lanzada address=t7d name=ez-smoke-13548
2026-11-19T01:50:50.157576Z  INFO hc_smoke: terminal listada vivas=1
2026-11-19T01:50:50.183352Z  INFO hc_smoke: evento finished terminal=t7d exit_code=Some(0) reason=Some("exited")
2026-11-19T01:50:50.190250Z  INFO hc_smoke: texto:
DESKTOP-IG8CHFG completo=true
2026-11-19T01:50:50.193991Z  INFO hc_smoke: terminal borrada address=t7d
2026-11-19T01:50:50.197850Z  INFO hc_smoke: humo de hc: todo bien
SMOKE_EXIT:0
```

## Pendiente o riesgos
- No se pudo provocar un `missed` real con el `hc` instalado; el descarte por
  secuencia se cubre con el `hc` falso en memoria (igual que en ECO-1).
- `launch_and_deliver` no registra tokens ni decide matar la terminal: eso lo
  hace el lanzador de la ventana en ECO-6; por eso devuelve la terminal creada
  cuando el resultado es `Failed`.
- El umbral de descarte vive en el hilo lector de eventos; `snapshot_seq()` se
  expone pero no se usa para descartar, porque el brief solo pide descartar
  tras un `missed`.
- `TermEvent` cambió de forma pública (todos los eventos de `event` llevan
  `seq`). Hoy ningún otro crate del workspace lo consume; los peces de ECO-6
  deberán adaptar sus patrones.
- No se tocó `Cargo.toml` raíz ni `Cargo.lock`; el árbol de trabajo quedó limpio
  salvo este informe.
