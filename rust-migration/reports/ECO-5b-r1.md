# Informe ECO-5b — `ez-hub`: la demostración no se ve (correcciones)

## Resultado
- Brief/tarea: ECO-5b
- Estado: completado

## Cambios
- `rust/crates/ez-hub/ui/js/main.js`: la vista lee `elefante.agente`, `elefante.argv` y `carpeta` (misma forma que `espacios.json`); envía el informe de autocomprobación 6 s después de `boot()` con errores de JS, cabecera, conteos y estado del formulario.
- `rust/crates/ez-hub/ui/js/spaces.js`: `nombre`, `elefante.agente` y `carpeta` en lugar de `name`, `elephant.agent` y `folder`.
- `rust/crates/ez-hub/ui/css/app.css`: `.space-form[hidden] { display: none; }` para que `display: flex` no dejara el formulario abierto al arrancar.
- `rust/crates/ez-hub/ui/index.html`: carga `js/errors.js` antes del módulo principal.
- `rust/crates/ez-hub/ui/js/errors.js` (nuevo): captura `window.onerror` y `unhandledrejection` en `window.__ezErrors` desde el primer momento (script clásico, respeta la CSP sin `unsafe-inline`).
- `rust/crates/ez-hub/ui/js/terminal.js`: `terminalsWithData()` cuenta las xterm que recibieron al menos un bloque de bytes del PTY; se limpia al cerrar la vista.
- `rust/crates/ez-hub/src/selfcheck.rs` (nuevo): `SelfcheckReport`, `evaluate`/`failures` (función pura), escritura JSON y informe de tiempo agotado, con 6 pruebas.
- `rust/crates/ez-hub/src/commands.rs`: ruta de autocomprobación en `AppState`, `selfcheck_enabled` y `selfcheck_report`, que escriben el informe y cierran la app con 0/1; 2 pruebas de cableado.
- `rust/crates/ez-hub/src/lib.rs`: `run(demo, selfcheck)`, registro de los dos comandos nuevos y watchdog que a los 20 s escribe `{"errores":["la vista no informó"]}` y sale con 1 si no hay informe.
- `rust/crates/ez-hub/src/main.rs`: parsea `--selfcheck <ruta.json>` junto a `--demo`.

## Decisiones
- Ningún `// DECISION` nuevo. Decisiones de implementación (sin desviación del brief ni del diseño):
  - El watchdog detecta «la vista no informó» por la existencia de la ruta: `selfcheck_report` escribe el archivo antes de cerrar, así que si a los 20 s no existe, no hubo informe. Evita un `AtomicBool` compartido y no retiene candados durante la espera.
  - El temporizador de 6 s arranca al terminar `boot()` (no al evento `load`), para que los tres PTY y la traza ya estén montados; cualquier error de `boot()` se agrega a `errores` y también dispara el informe.
  - La captura de errores va en un script clásico separado porque la CSP de `tauri.conf.json` prohíbe JavaScript en línea.

## Verificación
- pytest: no aplica (crate Rust).
- ruff: no aplica (crate Rust).
- `cargo fmt -p ez-hub`: sin cambios.
- `cargo clippy -p ez-hub --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.98s` (sin advertencias).
- `cargo test -p ez-hub`: `test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.75s` (28 antes → 36 después; +8 pruebas).
- Autocomprobación real, ejecutada desde `rust/`:
  `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck "C:\Users\JDMoreu\AppData\Local\Temp\opencode\eco5b-selfcheck-r1.json"`
  Código de salida: **0**. JSON escrito:
  ```json
  {
    "errores": [],
    "cabecera": "Elefante · PowerShell",
    "terminales": 3,
    "peces": 2,
    "eventos": 5,
    "tarjetas": 1,
    "formulario_abierto": false
  }
  ```
- Métricas: antes, panel central y columna derecha vacíos y `TypeError` en la barra; después, 3 terminales con datos, 2 peces, 5 eventos, 1 tarjeta, cabecera «Elefante · PowerShell», 0 errores y formulario cerrado.
- Commits en la rama `work/ECO-5b`: `ba2cddc` (claves y formulario), `b18a544` (autocomprobación).

## Pendiente o riesgos
- Al cerrarse la app aparece en stderr `Failed to unregister class Chrome_WidgetWin_0. Error = 1412`, propio del teardown de WebView2; no afecta al código de salida (0).
- La autocomprobación valida cantidades, cabecera y ausencia de errores de JS, no la estética; la revisión visual sigue siendo del elefante (§12.6).
- La puerta de workspace (`cargo fmt --all --check`, `cargo clippy --workspace`, `cargo test --workspace`) queda para el elefante; aquí solo se comprobó `-p ez-hub`, como manda el brief.
- El contenido de los PTY depende de que xterm.js responda a la consulta de cursor de ConPTY; en esta ejecución las tres terminales recibieron datos.
