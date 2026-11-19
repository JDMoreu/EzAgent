# Informe ECO-1 — `ez-term`: cliente JSON-RPC de herdr-core

## Resultado
- Brief/tarea: ECO-1
- Estado: completado

## Cambios
- `rust/crates/ez-term/Cargo.toml`: añade `parking_lot` del workspace y
  `tracing-subscriber` como dependencia de desarrollo para el ejemplo.
- `rust/crates/ez-term/src/lib.rs`: documentación y exportaciones públicas del
  crate.
- `rust/crates/ez-term/src/error.rs`: `TermError` con `Display` en español
  neutro (`NotFound`, `Spawn`, `Disconnected`, `Timeout`, `Rpc`, `Decode`).
- `rust/crates/ez-term/src/types.rs`: `Terminal`, `Status`, `RunSpec`,
  `ReadText` y `SendOutcome`, todos con `#[serde(default)]` y campos
  desconocidos ignorados; `RunSpec` construye los parámetros de `terminal.run`.
- `rust/crates/ez-term/src/locate.rs`: `locate_hc` (configurada →
  `%LOCALAPPDATA%\Programs\herdr-core\hc.exe` → `PATH`) con su parte pura
  probada con un árbol temporal.
- `rust/crates/ez-term/src/conn.rs`: transporte sin locks durante E/S ni
  esperas: hilo `ez-term-writer`, hilo `ez-term-reader` (o `ez-term-events`),
  emparejamiento por `id`, EOF → `Disconnected` para pendientes y futuras,
  cierre de stdin y del hijo al soltar la última referencia, subproceso sin
  shell y con `CREATE_NO_WINDOW`.
- `rust/crates/ez-term/src/client.rs`: `HcClient` (`Clone + Send + Sync`) con
  `hello`, `call` y los envoltorios `run`, `list`, `get`, `read`, `send`,
  `keys`, `interrupt`, `kill`, `rm`, `wait`.
- `rust/crates/ez-term/src/events.rs`: `EventStream` sobre una conexión
  propia (también con `hello`), foto inicial, `TermEvent` para `created`,
  `status`, `finished`, `removed`, `lost`, `restarted`, `other`, `missed`,
  `heartbeat` y `closed`.
- `rust/crates/ez-term/examples/hc_smoke.rs`: ejemplo contra el `hc` real.
- `rust/crates/ez-term/tests/{client,events,protocolo}.rs` y
  `tests/support/mod.rs`: hc falso en memoria sobre canales; 25 pruebas.
- `rust/Cargo.lock`: único efecto de añadir `parking_lot` al crate; no se
  tocó el `Cargo.toml` raíz.

## Decisiones
- `// DECISION` en `client.rs`: `send` no acepta timeout del llamador; usa el
  tope normal de 30 s más el margen de 10 s.
- `// DECISION` en `client.rs`: `list` no envía `all`, así que `hc` devuelve
  solo terminales vivas (lo que necesita el inventario).
- `// DECISION` en `conn.rs`: el `stderr` de `hc connect` se descarta porque
  nadie lo drena y una tubería llena bloquearía al subproceso.
- `// DECISION` en `events.rs`: si el aviso `missed` no trae la foto en
  `terminals`, `snapshot` o `data.terminals`, se entrega `Missed { snapshot:
  vec![] }`. El esquema dice que `missed` va seguido de una foto nueva pero no
  fija la forma del mensaje; se aceptan esas tres formas.
- Sin marcador: los errores JSON-RPC se mapean todos a `TermError::Rpc`
  (incluido `kind: "not_found"`), tal como pide el brief; `NotFound` se usa
  solo cuando no existe `hc` en la ruta buscada.
- Sin marcador: `wait` acota `timeout_s` a `[0, 86400]` para que
  `Duration::from_secs_f64` no entre en pánico con `NaN`, infinitos o valores
  enormes.
- La conexión de eventos también hace `hello`: el protocolo lo exige como
  primer mensaje y así funciona contra el `hc` real. El diseño y el brief
  no contradicen esta lectura.

## Verificación
Comandos ejecutados desde `rust/`, solo para `-p ez-term`:

```text
$ cargo fmt --all --check
FMT_CHECK_EXIT:0

$ cargo clippy -p ez-term --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.47s
CLIPPY_EXIT:0

$ cargo test -p ez-term
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (src/lib.rs)
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s  (tests/client.rs)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (tests/events.rs)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (tests/protocolo.rs)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (doc-tests)
TEST_EXIT:0
```

Comprobación adicional del límite de 60 líneas por función
(`cargo clippy -p ez-term --all-targets --no-deps -- -W clippy::too_many_lines
-D warnings`): sin avisos en `ez-term`; ningún `.rs` del crate supera 600
líneas (máximo: 328 en `tests/client.rs`, 305 en `src/conn.rs`).

Ejemplo contra el `hc` real (`cargo run -p ez-term --example hc_smoke`),
ejecutado una vez después del cambio final y con código de salida 0
(se quitan los códigos de color de `tracing` al pegarlo aquí):

```text
2026-11-19T01:38:50.054455Z  INFO hc_smoke: humo de hc: encontrado hc=C:\Users\JDMoreu\AppData\Local\Programs\herdr-core\hc.exe
2026-11-19T01:38:50.082030Z  INFO hc_smoke: suscripción lista foto=0
2026-11-19T01:38:50.102797Z  INFO hc_smoke: hello version=0.1.21
2026-11-19T01:38:50.115611Z  INFO hc_smoke: terminal lanzada address=t79 name=ez-smoke-17896
2026-11-19T01:38:50.116787Z  INFO hc_smoke: terminal listada vivas=1
2026-11-19T01:38:50.139003Z  INFO hc_smoke: evento finished terminal=t79 exit_code=Some(0) reason=Some("exited")
2026-11-19T01:38:50.145901Z  INFO hc_smoke: texto:
DESKTOP-IG8CHFG completo=true
2026-11-19T01:38:50.150452Z  INFO hc_smoke: terminal borrada address=t79
2026-11-19T01:38:50.153917Z  INFO hc_smoke: humo de hc: todo bien
SMOKE_EXIT:0
```

## Pendiente o riesgos
- No se pudo provocar un `missed` real con el `hc` instalado (ni con 20 000
  cambios de título ni con 1 000 cambios de etiqueta); la forma del aviso se
  cubre de forma tolerante y el caso sin foto entrega un snapshot vacío.
- `list` solo devuelve terminales vivas; si otra integración necesita las
  terminadas, habrá que pasar `all: true` en un método nuevo.
- Los ejemplos y las pruebas no dependen de agentes reales; el único uso de
  red es `127.0.0.1` a través del `hc` local del ejemplo.
