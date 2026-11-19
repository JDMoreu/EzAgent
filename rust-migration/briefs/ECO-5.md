# ECO-5 — `ez-hub`: la ventana (Tauri 2 + xterm.js) con modo demostración

**Antes de nada lee `rust-migration/briefs/_COMMON-ECO.md` y `docs/designs/espacio-elefante.md`
(sobre todo §1, §3, §10 y §11).** En esta ola la ventana no habla todavía con el árbitro
ni con hc: deja listas las piezas y un modo demostración; la ola 2 las conecta.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/**` (incluido su `Cargo.toml`). Dependencias permitidas del
  workspace: `tauri`, `tauri-build` (en `[build-dependencies]`), `portable-pty`, `serde`,
  `serde_json`, `tracing`, `tracing-subscriber`, `uuid`, `parking_lot`,
  `crossbeam-channel` y `ez-core` (ruta).
- Puedes copiar `packaging/icon.ico` a `rust/crates/ez-hub/icons/icon.ico`.

## Estructura
- Binario `ezagent-hub` (`src/main.rs`); la lógica en módulos de la biblioteca para
  poder probarla. `build.rs` con `tauri_build::build()`.
- `tauri.conf.json`: `productName` «EzAgent», `identifier` `dev.ezagent.hub`,
  `build.frontendDist` `"ui"`, `app.withGlobalTauri: true`, ventana `main` «EzAgent»
  1400×900 (mínimo 900×600), `bundle.active: false`, icono `icons/icon.ico`, CSP
  `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.
- `capabilities/default.json`: ventana `main`, permisos `core:default` y nada más.
- `ui/`: `index.html`, `css/app.css`, `js/main.js`, `js/terminal.js`, `js/spaces.js`,
  `js/panel.js` (peces y traza), `js/texts.js` (todos los textos, español neutro) y
  `vendor/` con `@xterm/xterm 6.0.0` (`lib/xterm.mjs`, `css/xterm.css`) y
  `@xterm/addon-fit 0.11.0` (`lib/addon-fit.mjs`), bajados de
  `https://registry.npmjs.org/<paquete>/-/<nombre>-<versión>.tgz`, con su `LICENSE`.
  Módulos ES sin npm ni empaquetador; `window.__TAURI__.core.invoke` y `Channel`.

## Pantalla (ver el esquema de §10)
- **Columna izquierda (220 px), espacios:** lista desde
  `%APPDATA%\EzAgent\hub\espacios.json` (`{"espacios":[{"id","nombre","carpeta","elefante":{"agente","argv"}}]}`)
  y «+ Nuevo espacio»: formulario en línea con nombre, carpeta (texto; debe existir y ser
  carpeta) y elefante (Claude Code → `["claude"]`, OpenCode → `["opencode"]`, Codex →
  `["codex"]`). Se guarda al crear.
- **Centro, elefante:** cabecera «Elefante · {agente}», terminal en vivo y, abajo, la
  barra de entrada: área de texto («Escribe o dicta…»), botones «Nota de voz» y «Llamar»
  (deshabilitados, con título «Disponible pronto»), y «Enviar». Enter envía,
  Mayús+Enter salta de línea. Si el texto tiene saltos de línea, se envía como pegado
  entre corchetes (`ESC[200~` … `ESC[201~`) y luego `\r`; si no, el texto y `\r`.
- **Derecha (40 %), pestañas «Peces» y «Traza»:**
  - Peces: una tarjeta por pez con cabecera (id, papel, `agente · modelo`, chip de estado
    `idle`/`working`/`done`/`blocked`/`error` con color) y su terminal; clic en la
    cabecera la amplía a toda la columna y otro clic la devuelve.
  - Traza: eventos (hora, tipo, texto) y tarjetas de aprobación (título, cuerpo en texto
    preformateado, «Aprobar» y «Rechazar»; rechazar abre un campo de comentario obligatorio).
- Tema oscuro con variables CSS en `:root` (fondo `#0f1115`, superficie `#161922`, borde
  `#262b36`, texto `#e6e9ef`, tenue `#8b93a7`, acento `#7aa2f7`, bien `#9ece6a`, aviso
  `#e0af68`, error `#f7768e`); letra de interfaz `"Segoe UI Variable", system-ui`; de
  terminal `"Cascadia Mono", Consolas, monospace`. Las terminales usan los mismos colores.

## Terminales (PTY)
- Módulo `pty`: `portable-pty` (ConPTY). `open(argv, cwd, env, cols, rows)` crea la vista
  con un id; un hilo `ez-hub-pty-<id>` lee bloques de 8 KiB, los junta hasta 16 ms y los
  envía a la vista por un `tauri::ipc::Channel` como bytes crudos
  (`InvokeResponseBody::Raw`); si tu versión no lo permite, como arreglo JSON y marcado
  `// DECISION`. `write(id, datos)`, `resize(id, cols, rows)`, `close(id)` (mata el
  proceso) y aviso de salida con su código.
- Programa no encontrado: la vista muestra «No encontré «{programa}» en PATH.» en lugar
  de la terminal.
- En la ola 2 cada vista ejecutará `hc attach <t>`: el módulo debe aceptar cualquier `argv`.

## Comandos de Tauri y estado
`spaces_list`, `space_create {nombre, carpeta, agente}`, `view_open {argv, cwd, cols, rows, channel}`
→ id, `view_write {id, data}`, `view_resize {id, cols, rows}`, `view_close {id}`,
`elephant_input {space, text}`, `hub_state {space}` → `{fish: [...], timeline: [...], cards: [...]}`,
`card_action {card, action, comment}`. Evento `hub://changed` cuando cambia el estado.
En esta ola `card_action` registra la acción con `tracing` y quita la tarjeta.

## Modo demostración (`ezagent-hub --demo`)
Espacio en memoria «Demostración» (carpeta actual) con el elefante en
`powershell.exe -NoLogo`; dos peces con `powershell.exe -NoLogo`: `dev-r1-1`
(Desarrollador · OpenCode · deepseek-v4.1-flash, `working`) y `qa-r1` (QA · Codex ·
gpt-6-sol, `idle`); cinco eventos de traza de ejemplo y una tarjeta «Equipo propuesto
para «Transcriptor con whisper.cpp»» con el formato de §6. No escribe `espacios.json`.

## Pruebas
- Rust: guardar y leer `espacios.json` (carpeta temporal); validación de la carpeta;
  codificación de la entrada (una línea y varias líneas); estado de demostración; PTY que
  ejecuta `hostname` y recibe su salida y el código de salida (sin abrir ventanas).
- Puedes abrir `ezagent-hub --demo` como máximo 15 s para confirmar que arranca sin
  errores en el log, y ciérralo. No dejes ventanas abiertas. La revisión visual la hace el
  elefante.

## Criterios de aceptación
fmt, clippy y test de `ez-hub` en verde; `cargo build -p ez-hub --bin ezagent-hub` compila;
`--demo` arranca sin errores; ningún recurso remoto en la vista.
