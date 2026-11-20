# DX-7 r1 — Las pruebas con ventanas se abren en la pantalla secundaria

Rama `work/DX-7`, worktree `EzAgent-wt/DX-7`. Se retomaron los cambios sin confirmar
del intento anterior, se revisaron y se completaron dentro de los archivos del brief.
No se ejecutaron pruebas ni compilaciones, no se abrieron ventanas y no se tocó la app
de la persona, su `whisper-server` ni la ventana de Herdr (instrucción de esta sesión).
Por eso la verificación de la puerta de calidad queda pendiente y el estado es
**parcial**, no «completado».

## Resultado

- Brief/tarea: DX-7
- Estado: parcial

## Cambios

- `rust/crates/ez-hud/src/win/monitor.rs`: `EZAGENT_TEST_MONITOR` (`secondary`/`primary`,
  sin distinguir mayúsculas; un valor desconocido se ignora), `ScreenChoice`,
  `choose_monitor` sobre monitores falsos, `rect_inside`, enumeración real con
  `EnumDisplayMonitors` + `GetMonitorInfoW` y `MONITORINFOF_PRIMARY`. `work_area()`
  devuelve el `rcWork` del primer monitor no principal y, si no hay, avisa por
  `tracing` y usa la principal. `monitor_rect()` respeta el monitor pedido y
  `taskbar_rect()` no recorta contra la barra de tareas de la principal cuando las
  ventanas viven en la secundaria. Sin la variable, el camino es el de siempre.
- `rust/crates/ez-app/examples/diag_chat_smoke.rs`: la secundaria es el valor por
  omisión; `--primary` o un `EZAGENT_TEST_MONITOR` explícito permiten verlo en la
  principal. Al final enumera las ventanas layered del proceso, imprime el monitor
  pedido y el rectángulo de la barra (`drop`), el chat (`EzAgent`), el menú (`card`) y
  la vista rápida (`peek`), y falla si alguna quedó fuera del monitor pedido.
- `rust/crates/ez-app/examples/diag_subagents_smoke.rs`: pone la secundaria por omisión
  (respeta el entorno y `--primary`) e imprime el monitor y su rectángulo. Este humo
  corre con `hud_hidden`, así que no tiene ventanas propias que comprobar.
- `rust/crates/ez-hud/examples/panic_boundary.rs`: pone la secundaria por omisión
  (respeta el entorno y `--primary`), coloca su ventana en la esquina del área de
  trabajo pedida y falla si el rectángulo de la ventana se sale de ese monitor.
- `scripts/tests/humo-whisper.ps1`: `-Primary` para la pantalla principal; por omisión
  exporta `EZAGENT_TEST_MONITOR=secondary` antes de lanzar la app real de la prueba y
  restaura el valor anterior en `finally`.
- `rust-migration/reports/DX-7-r1.md`: este informe.

No hizo falta tocar `win/drop/ui.rs`, `win/chat/window/*` ni `win/peek/*`: todos sus
cálculos de posición y recorte ya pasan por `work_area()`/`monitor_rect()`, que son
los que cambiaron de área. `win/mod.rs` quedó como estaba (los ejemplos importan de
`ez_hud::win::monitor`, que ya era público).

## Decisiones

- `// DECISION` (DX-7): la variable es solo para pruebas; la app de la persona no la
  define y el comportamiento sin ella no cambia.
- `// DECISION` (DX-7): los humos ponen `secondary` por omisión, pero no pisan un
  `EZAGENT_TEST_MONITOR` ya presente en el entorno; `--primary` tiene prioridad. Así
  también se puede pedir la principal sin editar el guion.
- `// DECISION` (DX-7): en modo secundaria `taskbar_rect()` devuelve `None`. La barra
  de tareas de la principal (por ejemplo `(0, 1040, 1920, 1080)`) cae dentro del rango
  vertical de la secundaria y el recorte antiguo habría subido las ventanas al centro
  de la pantalla.
- `// DECISION` (DX-7): la comprobación del humo cubre **todas** las ventanas layered
  del proceso, no solo la barra y el chat; cualquier ventana fuera del monitor pedido
  hace fallar la corrida. Fuera de Windows no hay ventanas Win32 que comprobar.
- `// DECISION` (DX-7): fuera de Windows `layered_windows()` devuelve una lista vacía
  y la comprobación no falla.
- Desviación de `_COMMON-ECO`: el humo usa bloques `unsafe` de Win32 para leer los
  rectángulos (`EnumWindows`, `GetWindowRect`, `GetClassNameW`, `GetWindowTextW`).
  Es la única forma de comprobar desde `ez-app` dónde quedaron las ventanas layered
  que crea `ez-hud`; cada bloque lleva su comentario `SAFETY`. En `monitor.rs` la
  enumeración de monitores también es FFI de Win32, como el resto del módulo.
- No se cambió la lógica de animación del chat (DX-1i) ni la de la barra/vista rápida
  (DX-6): solo el área que consumen.

## Verificación

No ejecutada por instrucción expresa de esta sesión: nada de `cargo test`, `cargo
clippy`, `cargo build` ni ventanas. Lo único que se comprobó fue el formato, sin
compilar:

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

La salida pendiente del humo `chat` debe mostrar el monitor pedido con el rectángulo
de `DISPLAY2` (esperado `(-1080, -393, 0, 1527)` en esta máquina) y las líneas
`ventana "barra"` y `ventana "chat"` con `dentro=true`. No se capturó ninguna salida
real porque no se abrieron ventanas.

## Pendiente o riesgos

- **Verificación pendiente**: fmt, clippy, test y los humos no se ejecutaron. El brief
  exige la salida real del humo `chat` en `DISPLAY2`; hay que correrla antes de dar
  DX-7 por completado.
- La rama `work/DX-7` está 14 commits detrás de `eco/main`. Los commits que faltan son
  solo briefs y registros de orquestación (no tocan `rust/crates`), pero conviene
  integrarlos antes de la entrega para no reportar contra una base vieja.
- El margen de sombra de la ventana del chat (`MARGIN = 16`) es mayor que el relleno
  del panel (`SCREEN_PAD = 12`). Si el panel se abriera pegado al borde superior o
  izquierdo del área de trabajo, el rectángulo nativo podría sobresalir ~4 px del
  monitor y la comprobación del humo fallaría aunque el panel visible esté dentro. En
  la corrida por omisión la barra queda abajo y centrada, el chat se despliega hacia
  arriba y no se espera ese caso; si aparece, es el margen de sombra, no una ventana
  en la principal.
- La tarjeta de ajustes conserva la semántica de «dejar una franja visible» de
  `clamp_position`. Con una configuración temporal (los humos) se centra en la
  secundaria; con un `voice.json` real que tenga `settings_x`/`settings_y` en la
  principal, la tarjeta podría quedar a caballo entre ambas. No ocurre en los humos
  (usan `APPDATA` temporal), pero es el límite conocido de esa función.
- Si alguien define `EZAGENT_TEST_MONITOR=secondary` en su entorno y abre la app real,
  la app también se moverá a la secundaria: es el contrato pedido por el brief, pero
  conviene no dejar la variable exportada en la sesión de la persona.
