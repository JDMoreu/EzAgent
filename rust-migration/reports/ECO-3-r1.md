# ECO-3 — `ez-ctl`: protocolo, servidor local y comando `ez`

## Resultado
- Brief/tarea: ECO-3
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/src/protocol.rs`: `Caller`, `Request`, `Reply`, `Handler`, `Tokens` y `new_token`; `args` ausente vale `{}` y hay pruebas de tokens y del JSON del contrato.
- `rust/crates/ez-ctl/src/server.rs`: `CtlServer` con `tiny_http` en `127.0.0.1:0`, ocho hilos `ez-ctl-http-<n>`, ruteo `POST /v1/command`, Bearer, 401/400/404/413 y cierre ordenado con espera de hilos.
- `rust/crates/ez-ctl/src/client.rs`: cliente `reqwest::blocking` con tope por comando; separa «sin conexión», 401 y otros rechazos.
- `rust/crates/ez-ctl/src/cli.rs`: `main_with`, análisis de argumentos, tabla de §7, rutas relativas → absolutas, `--skill`/`--help`/`--version`, `--json` y códigos 0/1/2/3.
- `rust/crates/ez-ctl/src/lib.rs`: módulos privados y reexportación de la API pública del brief.
- `rust/crates/ez-ctl/src/bin/ez.rs`: `main` delgado que llama a `main_with` con argv, entorno, `cwd`, stdout y stderr.
- `rust/crates/ez-ctl/tests/support/mod.rs`: `Handler` falso que registra, `Harness` con servidor real, carpeta temporal única y ejecución de `main_with`.
- `rust/crates/ez-ctl/tests/roundtrip.rs`: ida y vuelta de los 13 comandos de la tabla (args y rutas absolutas), `--json`, `report` con y sin mutaciones, y `wait` de 2 s junto a un `status` simultáneo.
- `rust/crates/ez-ctl/tests/server_errors.rs`: 401 (sin token y token desconocido), 400, 404, 413 y cierre del puerto al parar.
- `rust/crates/ez-ctl/tests/cli_errors.rs`: `--skill`/`--help`/`--version`, falta de entorno, servidor apagado, token malo, rechazo (salida 1), uso incorrecto (salida 2) y archivo inexistente.
- `rust/crates/ez-ctl/src/skill.md`: **sin tocar** (contrato); se incluye con `include_str!`.

## Decisiones
- `// DECISION`: la ruta se valida antes que el token, para que una ruta desconocida dé 404 aunque no traiga credenciales.
- `// DECISION`: toda respuesta de error añade `Connection: close`, porque el cuerpo puede quedar sin leer tras un 413.
- `// DECISION`: `--json` vale para cualquier comando; en `inventory` además viaja como `args.json`.
- `// DECISION`: para el archivo basta con que la ruta exista; el árbitro decide si es un archivo válido.
- `// DECISION`: cualquier estado distinto de 200 o 401 se trata como rechazo (salida 1).
- `--version` imprime solo el número del crate (`0.1.0`), sin prefijo.
- `--skill`, `--help` y `--version` solo se reconocen como único argumento; con más argumentos cuentan como uso incorrecto.
- El cuerpo de exactamente 1 MiB se acepta; 413 solo cuando supera 1 MiB. Para poder responder sin desincronizar la conexión se leen hasta `1 MiB + 1` bytes.
- Se rechaza también `command` vacío o en blanco con 400, aunque el brief solo nombra el `command` ausente.
- El mensaje de 400/404/413 lo genera este crate (`Petición inválida.`, `No encontrado.`, `Petición demasiado grande.`); el brief solo fija el texto del 401.
- En el error de archivo inexistente se imprime la ruta ya resuelta contra `cwd` (absoluta).

## Verificación
- `cargo fmt --all --check`: exit 0, sin diferencias.
- `cargo clippy -p ez-ctl --all-targets -- -D warnings`: exit 0.
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.37s
  ```
- `cargo test -p ez-ctl`: exit 0, **20 passed, 0 failed** (3 unitarias + 7 de CLI + 5 de ida y vuelta + 5 de servidor).
  ```
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.11s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.14s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `cargo build -p ez-ctl --bin ez`: exit 0; produce `rust/target/debug/ez.exe` (8.587.776 bytes).
- Binario real: `ez.exe --version` → `0.1.0`; `ez.exe --skill` empieza por `# Manual del elefante (EzAgent)`; `ez.exe status` sin entorno imprime el mensaje de terminal y sale con 3.
- Métricas: no aplica.

## Pendiente o riesgos
- `ez-ctl` no valida la semántica de `args`: la decide el `Handler` del árbitro (ola 2). No se probó contra `ez-arbiter` real porque aún no existe.
- El servidor no impone tope de tiempo a los handlers: un `wait` colgado ocupa su hilo hasta que vuelva. Con ocho hilos, haría falta más de ocho esperas colgadas para bloquear el resto; el corte normal lo pone el cliente con `tope + 15 s`.
- Un cuerpo mucho mayor que 1 MiB se responde con 413 tras leer `1 MiB + 1`; un cliente que siga subiendo puede ver un corte de conexión en lugar del 413. El `ez` real nunca envía cuerpos así.
- `CtlServer` no implementa `Drop`: hay que llamar a `stop()` (el diseño lo pide así); las pruebas y la ola 2 deben hacerlo.
- Solo se compiló y probó `-p ez-ctl`, como pide el reparto; el resto del workspace no se tocó.
