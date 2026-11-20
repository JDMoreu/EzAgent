# DX-7b r1 — Los humos no abren ventanas en la pantalla principal

Rama `work/DX-7`. Se corrige el hueco señalado en la revisión de DX-7: los humos
verifican la pantalla secundaria **antes** de crear cualquier ventana o proceso,
fallan cerrado si no la pueden verificar y ya no ofrecen ninguna ruta de prueba
hacia la pantalla principal. No se ejecutaron pruebas, compilaciones ni ventanas
(instrucción de esta sesión); no se tocó la app de la persona, su
`whisper-server` ni Herdr.

## Resultado

- Brief/tarea: DX-7b
- Estado: parcial

## Cambios

- `rust/crates/ez-hud/src/win/monitor.rs`: `screen_choice` solo acepta
  `secondary`; `primary` y cualquier otro valor se ignoran, así que la variable
  de prueba ya no puede pedir la pantalla principal. Nuevas
  `require_secondary()` (lista simulada de monitores) y
  `require_secondary_monitor()` (enumeración real del escritorio), ambas con
  `Result<Rect, String>` y mensaje del motivo. La segunda falla si la variable
  no pide `secondary`, si Windows no enumera monitores o si no hay un monitor no
  principal. Se añadieron pruebas con monitores simulados (secundaria vertical
  con coordenadas negativas, solo principal y lista vacía).
- `rust/crates/ez-app/examples/diag_chat_smoke.rs`: se quitó `--primary`; el humo
  pone `EZAGENT_TEST_MONITOR=secondary`, valida la secundaria antes de construir
  la app o abrir ventanas y termina con código 2 y el motivo si falla. La
  comprobación posterior de los rectángulos se mantiene como segunda red.
- `rust/crates/ez-app/examples/diag_subagents_smoke.rs`: se quitó `--primary`;
  valida la secundaria antes de arrancar la app o cualquier proceso del humo.
- `rust/crates/ez-hud/examples/panic_boundary.rs`: se quitó `--primary`; valida
  la secundaria antes de crear la ventana y sale con código 2 si no existe.
- `scripts/tests/humo-whisper.ps1`: se quitó `-Primary`; exporta siempre
  `EZAGENT_TEST_MONITOR=secondary` y, antes de `Start-VoiceApp`, ejecuta
  `Assert-SecondaryMonitor` (`System.Windows.Forms.Screen`). Si no puede cargar
  la comprobación o no hay una pantalla no principal, lanza un error y no
  arranca la app.
- `rust-migration/reports/DX-7b-r1.md`: este informe.

## Decisiones

- `// DECISION` (DX-7b): la variable de prueba solo entiende `secondary`; un
  `primary` explícito se ignora. La app sin la variable sigue igual y puede usar
  la principal como siempre.
- `// DECISION` (DX-7b): la validación se hace antes de crear ventanas o
  procesos, no después de mostrarlos. La comprobación de rectángulos de DX-7 se
  conserva, pero ya no es la única protección.
- `// DECISION` (DX-7b): `require_secondary()` es puro y se prueba con listas
  simuladas; `require_secondary_monitor()` usa la misma enumeración Win32 que la
  app (`EnumDisplayMonitors` + `GetMonitorInfoW`), para que el preflight y la
  colocación real no discrepen.
- `// DECISION` (DX-7b): el guion PowerShell usa `System.Windows.Forms.Screen`
  porque no hay un binario de diagnóstico que consultar; si la comprobación no
  se puede cargar, falla cerrado en vez de arrancar la app.
- El fallback de `work_area()` a la principal cuando la variable pide una
  secundaria inexistente sigue existiendo para llamadores que no son humos, pero
  ningún humo puede llegar a él: todos validan y salen antes. El comportamiento
  normal de la app sin la variable no cambió.
- No se amplió el alcance: se tocaron solo `monitor.rs`, los tres ejemplos y el
  guion de humo que ya estaban en DX-7.

## Verificación

No ejecutada por instrucción expresa: nada de `cargo test`, `cargo clippy`,
`cargo build`, humo ni ventanas. Solo se comprobó el formato, sin compilar:

```
rustfmt --check --edition 2021 \
  rust/crates/ez-hud/src/win/monitor.rs \
  rust/crates/ez-hud/examples/panic_boundary.rs \
  rust/crates/ez-app/examples/diag_chat_smoke.rs \
  rust/crates/ez-app/examples/diag_subagents_smoke.rs
  → sin diferencias

git diff --check
  → sin errores (solo avisos LF/CRLF de Git)
```

Queda por verificar, desde `rust/`:

```
cargo fmt -p ez-hud -p ez-app -- --check
cargo clippy -p ez-hud -p ez-app --all-targets -- -D warnings
cargo test -p ez-hud -p ez-app
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-hud --example panic_boundary
```

y desde la raíz:

```
powershell -NoProfile -File scripts\tests\humo-whisper.ps1
```

La salida del humo `chat` debe empezar con
`monitor verificado: secundario · rect (-1080, -393, 0, 1527)` (en esta máquina)
y solo después abrir la barra y el chat; al final, `ventana "barra"` y
`ventana "chat"` deben quedar `dentro=true`. No se capturó ninguna salida real
porque no se abrieron ventanas.

## Pendiente o riesgos

- **Verificación pendiente**: fmt, clippy, tests y los humos no se ejecutaron.
  El brief pide además la salida real del humo `chat` en `DISPLAY2`; hay que
  correrla antes de dar DX-7b por completado.
- `Assert-SecondaryMonitor` depende de `System.Windows.Forms` en PowerShell. Si
  esa assembly no está disponible, el guion falla cerrado con el motivo, que es
  el comportamiento pedido, pero puede requerir ajustar el entorno de ejecución.
- Si hay más de un monitor no principal, Rust elige el primero de
  `EnumDisplayMonitors` y PowerShell el primero de `Screen.AllScreens`; ambos
  son no principales, así que la garantía de «no en la principal» se mantiene
  aunque el monitor exacto no coincida.
- `work_area()` conserva el aviso y el fallback a la principal para llamadores
  ajenos a los humos. Si en el futuro se agrega otro humo, debe llamar a
  `require_secondary_monitor()` antes de crear ventanas; no basta con exportar
  la variable.
- La rama `work/DX-7` está 16 commits detrás de `eco/main` (solo briefs y
  registros de orquestación). Conviene integrarlos antes de la entrega.
