## Resultado
- Brief/tarea: NV-1c (corrección de la revisión de NV-1b, ola HD)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents/watch/novelties.rs`: `last_lines` ahora toma las últimas 15 líneas **con texto** (al menos una letra o un dígito); las vacías y las de solo bordes (`┃`, `╹▀▀▀`) no cuentan. Se aplica al bloqueo, a la hora de reinicio y a la reanudación (todo pasa por `limit_hit`). `observe_screen` devuelve `Option<bool>`: `None` en la primera lectura, y un límite solo bloquea con `change == Some(false)`, es decir, con dos lecturas iguales.
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs`: `usage` y `live_children` pasan a ser ayudantes compartidos y se declara `mod pantalla`.
- `rust/crates/ez-herdr/src/subagents/watch/tests/novelties.rs`: las pruebas de límite originales de NV-1 hacen dos vueltas (la primera ya no cuenta como estable); los avisos se buscan con `any` porque la primera vuelta también avisa «está en proceso». La prueba de episodios exige una vuelta estable más.
- `rust/crates/ez-herdr/src/subagents/watch/tests/pantalla.rs` (nuevo): cuatro pruebas con la captura real: límite al final de la conversación + `idle` + dos vueltas → `bloqueado` con 19:00 en `uso.json`; la misma captura en la primera vuelta → no bloquea; la frase con 20 líneas de texto debajo → no bloquea; la captura sin frase y estable → no bloquea.
- `rust/crates/ez-herdr/src/subagents/watch/tests/fixtures/pantalla-opencode.txt` (nuevo): copia literal de `rust-migration/briefs/NV-1c-pantalla-opencode.txt` dentro del crate; las pruebas no leen `briefs/`.
- `rust/crates/ez-herdr/src/subagents/watch/novelties/tests.rs`: `last_lines` se prueba con líneas vacías y de bordes.

## Decisiones
- El tope de `SCREEN_LIMIT` (4 000 caracteres) **no** cambió: con la captura real, en los últimos 4 000 caracteres quedan 20 líneas con texto (más de las 15) y la frase insertada al final de la conversación sobrevive al recorte, así que no hizo falta recortar por líneas ni añadir un `# DECISION`.
- La estabilidad se compara con la pantalla completa que ya se guardaba (incluye la caja de entrada). Es la misma regla que pide §3; si el pie animado cambia cada vuelta, el límite no se marca, y se prefiere eso a un falso positivo.

## Verificación
Desde `rust/`, con `ez-herdr` (salida literal de la última corrida):
```
FMT-OK
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.61s
     Running unittests src\lib.rs (target\debug\deps\ez_herdr-54ac537f318fe849.exe)
running 304 tests
test result: ok. 304 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.27s
     Running tests\agent_driver_parity.rs (target\debug\deps\agent_driver_parity-a4398bcfdb8d9286.exe)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
     Running tests\herdr_parity.rs (target\debug\deps\herdr_parity-8871253e7546c781.exe)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
     Running tests\metrics_parity.rs (target\debug\deps\metrics_parity-b92f012a861933fd.exe)
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
     Running tests\permission.rs (target\debug\deps\permission-5dd950864fa35208.exe)
running 27 tests
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
     Running tests\tasks_activity_parity.rs (target\debug\deps\tasks_activity_parity-f097662ba3bdf8b5.exe)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- `cargo fmt -p ez-herdr -- --check`: `FMT-OK` (sin salida).
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.61s` (sin avisos).
- Métricas: `watch/novelties.rs` 560 → 572 líneas; `watch/tests.rs` 420 → 434; `watch/tests/novelties.rs` 510 → 515; nuevo `watch/tests/pantalla.rs` 148; fixture de 121 líneas. Ningún `.rs` pasa de 600 líneas.

## Pendiente o riesgos
- En dos corridas intermedias falló una vez `watcher::tests::arrancar_y_parar_no_se_cuelga` (módulo `watcher`, ajeno a NV-1c y sin cambios aquí): pasó aislado y en las corridas siguientes. Es una prueba de hilos sensible a la carga; no la toqué por estar fuera de alcance.
- La frase de límite sigue sin verse si queda a más de 15 líneas con texto del final; es el compromiso de §3.
- Un pie de pantalla animado (reloj, spinner) impide la segunda lectura igual y, por tanto, el bloqueo por límite; el diseño pide pantalla estable y no se añadió una excepción.
