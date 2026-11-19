# ECO-5b — `ez-hub`: la demostración no se ve (correcciones)

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/briefs/ECO-5.md` (la tarea
original, ya integrada) y `docs/designs/espacio-elefante.md` §10 y §14.10 (excepción B09).**

## Lo que vio el elefante
Captura de `ezagent-hub --demo` (1416×939): el panel central queda vacío, «Peces» y
«Traza» vacíos, sin tarjeta, y en la barra aparece en rojo
`TypeError: Cannot read properties of undefined (reading 'agent')`. Causa: `Space` se
serializa con las claves de `espacios.json` (`nombre`, `carpeta`, `elefante.agente`), pero
`ui/js/main.js:43` y `ui/js/spaces.js:32` leen `space.elephant.agent`. Además el formulario
«Nuevo espacio» aparece abierto al arrancar.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/**`

## Cambios
1. **Un solo formato.** La vista usa la misma forma que `espacios.json` (claves en
   español: `id`, `nombre`, `carpeta`, `elefante.agente`, `elefante.argv`). Corrige todos
   los lectores de la vista; no añadas un segundo formato.
2. **Formulario cerrado** al arrancar; se abre con «+ Nuevo espacio» y se cierra con
   «Cancelar» o al crear.
3. **La demostración se ve entera:** el espacio «Demostración» queda seleccionado; su
   cabecera dice «Elefante · PowerShell»; la terminal central muestra el prompt de
   PowerShell y acepta lo que se escribe en la barra; «Peces» muestra las dos tarjetas con
   sus terminales vivas; «Traza» los cinco eventos y la tarjeta del equipo.
4. **Autocomprobación** (el bucle de verificación de la vista):
   `ezagent-hub --demo --selfcheck <ruta.json>`. La vista registra todo error de JS
   (`window.onerror` y `unhandledrejection`) desde el primer momento; 6 s después de cargar
   llama al comando `selfcheck_report` con
   `{ "errores": [...], "cabecera": "<texto de la cabecera del elefante>",
      "terminales": <xterm montadas con datos recibidos>, "peces": <tarjetas de pez>,
      "eventos": <eventos de traza>, "tarjetas": <tarjetas de aprobación>,
      "formulario_abierto": <bool> }`.
   El backend escribe ese JSON en la ruta y cierra la app con código 0 si `errores` está
   vacío, `cabecera` empieza por «Elefante · », `terminales` = 3, `peces` = 2,
   `eventos` = 5, `tarjetas` = 1 y `formulario_abierto` = false; si no, código 1. Si la
   vista no informa en 20 s, escribe `{"errores":["la vista no informó"]}` y sale con 1.
   «Terminal montada con datos» = recibió al menos un bloque de bytes del PTY.

## Pruebas
- Rust: lo existente, más la evaluación de un informe de autocomprobación (bueno, con
  errores, con conteos distintos) como función pura.
- **Ejecuta** `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck <temporal>.json`
  desde `rust/`, y pega en el informe el JSON y el código de salida. Debe salir con 0.

## Criterios de aceptación
fmt (`-p ez-hub`), clippy y test de `ez-hub` en verde y la autocomprobación con código 0.
Informe en `rust-migration/reports/ECO-5b-r1.md`.
