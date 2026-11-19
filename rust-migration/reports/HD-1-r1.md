# Informe HD-1-r1 — ez-term: lanzar con padre

## Resultado
- Brief/tarea: HD-1 (rama `work/HD-1`, copia de `eco/main`; ola HD)
- Estado: completado

## Cambios
- `rust/crates/ez-term/src/types.rs`: `RunSpec` gana `parent: Option<String>` y `outlive_parent: bool` (por omisión `None`/`false`); `Default` los inicializa y `params()` envía `parent` y siempre `outlive_parent`, forzando `no_parent: false` cuando hay `parent`.
- `rust/crates/ez-term/src/client.rs`: nuevo `HcClient::children(parent)` que llama a `terminal.list` sin etiquetas y devuelve solo las terminales con `parent == Some(parent)`.
- `rust/crates/ez-term/tests/client.rs`: cuatro pruebas nuevas sobre el transporte falso (params con/sin `parent` y con `outlive_parent`, filtrado de `children` y propagación de errores) más el ayudante `child()`.
- `rust/crates/ez-hub/src/runtime/launcher.rs`: se añaden `parent: None, outlive_parent: false` a los dos literales `RunSpec { … }` (`launch_main` y `FishLauncher::launch`); sin cambio de comportamiento.

## Decisiones
- `// DECISION` en `types.rs`: con `parent` se envía `no_parent: false` aunque el campo `no_parent` siga en `true`, porque `hc` exige ese valor para colgar el hijo del padre (§10 del contrato). Sin `parent`, `no_parent` se envía tal cual.
- El esquema `docs/herdr/hc-schema-0.1.21.json` confirma lo pedido en el brief: `terminal.list` devuelve `ListResult` con `terminals: [Terminal]`, y `Terminal` tiene `parent: string | null` (no está en `required`, pero el tipo `Terminal` de `ez-term` ya usa `#[serde(default)]`). No hubo que tocar nada por ello.
- `children()` filtra sobre `terminal.list` sin etiquetas (como pide el brief) en vez de añadir un parámetro de etiquetas; reutiliza `ListReply` y `decode`, sin código nuevo de transporte.
- Las pruebas de `params()` se hacen por la vía pública `client.run()` contra el `hc` falso, porque `RunSpec::params()` es `pub(crate)` y no es Visible desde `tests/`; comprueban el JSON real enviado al cable, que es lo que importa.

## Verificación
- `cargo fmt -p ez-term --check && cargo fmt -p ez-hub --check`: exit 0 (`FMT_OK`).
- `cargo clippy -p ez-term -p ez-hub --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 8.82s`, exit 0, sin avisos.
- `cargo test -p ez-term`: **39 passed / 0 failed / 0 ignored** (4 unitarias + 17 en `tests/client.rs` + 5 en `events.rs` + 6 en `launch.rs` + 7 en `protocolo.rs` + 0 doc-tests). Líneas finales: `test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s` (cliente), `... 6 passed ...`, `... 7 passed ...`, `... 4 passed ...`, `... 5 passed ...`.
- `cargo test -p ez-hub`: **91 passed / 1 failed / 0 ignored / 1 filtrable**, `test result: FAILED. 91 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s`. El único fallo es `pty::tests::runs_hostname_and_reports_output_and_exit_code` (panico en `crates/ez-hub/src/pty.rs:376`, salida con secuencias ANSI). **Preexistente y fuera de mi alcance** (`pty.rs` no está en la lista de archivos del brief): con `git stash` de mis cuatro archivos, `cargo test -p ez-hub --lib pty::tests::runs_hostname_and_reports_output_and_exit_code` da `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 91 filtered out; finished in 0.09s` con el mismo panic; afterwards `git stash pop` restauró el trabajo.
- `cargo test -p ez-hub -- --skip pty::tests::runs_hostname_and_reports_output_and_exit_code`: **92 passed / 0 failed / 1 filtered out**, `test result: ok. 91 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.86s` (más 1 de `tests/neutral_names.rs` y 0 doc-tests): todo lo demás de `ez-hub` sigue en verde con mi cambio.
- Límites: el `.rs` más largo tocado tiene 174 líneas; ninguna función nueva supera 60; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe`, `.unwrap()` fuera de pruebas ni shells. Sin locks retenidos en E/S, subprocesos o esperas; sin agentes reales ni window de Herdr tocada.

## Pendiente o riesgos
- La prueba `pty::tests::runs_hostname_and_reports_output_and_exit_code` de `ez-hub` falla en esta máquina tanto antes como después del cambio (depende de cómo `hostname.exe` y la consola emitan secuencias de escape); no está en el alcance de HD-1 y no la toqué. Si el elefante quiere `cargo test -p ez-hub` completamente en verde, hay que arreglar `pty.rs` en otro brief.
- `children()` devuelve solo terminales **vivas** (sin `all`), igual que `list()`; si se necesitan hijos terminados, habrá que añadirlo en otro cambio.
