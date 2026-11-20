# DX-7c r1 — Mantener toda la ventana del chat en la secundaria

Rama `work/DX-7`. Se corrige el píxel que el humo real encontró fuera de
`DISPLAY2`: la ventana nativa del chat terminaba en 1528 con el monitor hasta
1527. No se ejecutaron pruebas, compilaciones ni ventanas (instrucción de esta
sesión); no se tocó la app de la persona ni Herdr.

## Resultado

- Brief/tarea: DX-7c
- Estado: parcial

## Cambios

- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: la ventana del chat es
  el panel más `MARGIN` (16 px de sombra), pero el panel se calculaba con el
  área de trabajo completa. Se agregó `panel_work_area()`, que descuenta del
  área solo la diferencia entre el margen de la ventana y `SCREEN_PAD` (el
  relleno que ya aplica `panel_rect`). Así `panel + margen` cabe entero en el
  monitor, tanto al colocar como al limitar el tamaño máximo. No se tocó la
  animación de DX-1i.
- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs` (pruebas): se agregó la
  prueba de límites con el caso real del humo (`DISPLAY2` vertical, barra arriba
  del borde inferior) y otra con el panel de tamaño máximo. La primera fija la
  ventana en `(-773, 791, -307, 1527)`, dentro de `(-1080, -393, 0, 1527)`.
- `rust-migration/reports/DX-7c-r1.md`: este informe.

## Decisiones

- `// DECISION` (DX-7c): se reserva solo `margen - SCREEN_PAD`, no el margen
  completo, porque `panel_rect` ya separa el panel 12 px del borde. Con eso la
  ventana queda pegada al borde del monitor sin dejar un hueco de 16 px y el
  panel conserva su cercanía a la barra.
- `// DECISION` (DX-7c): el ajuste se hace antes de `panel_rect`, para que el
  tamaño máximo del panel también tenga en cuenta la sombra; no se recorta la
  ventana después de crear el contenido.
- La comprobación posterior del humo (`check_monitor`) y el preflight de DX-7b
  quedaron intactos: no se relajó la prueba.
- No se cambió `clamp_to_work` de arrastre/redimensionado: sigue permitiendo que
  la sombra asome, por diseño, y el caso reportado es la colocación inicial del
  humo. Si se quiere exigir contención también al arrastrar, es otro cambio.

## Verificación

No ejecutada por instrucción expresa: nada de `cargo test`, `cargo clippy`,
`cargo build`, humo ni ventanas. Solo se comprobó el formato, sin compilar:

```
rustfmt --check --edition 2021 \
  rust/crates/ez-hud/src/win/chat/window/lifecycle.rs
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

La salida del humo `chat` debe seguir mostrando
`monitor verificado: secundario · rect (-1080, -393, 0, 1527)` y, al final, la
ventana del chat dentro de ese rectángulo (se espera
`(-773, 791, -307, 1527)` si la barra conserva su posición actual). No se capturó
ninguna salida real porque no se abrieron ventanas.

## Pendiente o riesgos

- **Verificación pendiente**: fmt, clippy, tests y el humo real no se ejecutaron.
  Hay que confirmar que el chat queda en `dentro=true` y que el humo sale con
  código 0.
- El ajuste usa `ceil` sobre `margen - SCREEN_PAD * escala`; con escalas
  fraccionarias la ventana queda como máximo en el borde exacto del área de
  trabajo, nunca fuera. Si el `rcWork` de la secundaria fuera menor que su
  `rcMonitor`, el margen quedará aún más holgado.
- El arrastre y el redimensionado siguen usando `clamp_to_work`, que permite que
  la sombra asome del área de trabajo. No es el caso del humo y no se cambió por
  el alcance de DX-7c.
- La rama `work/DX-7` sigue detrás de `eco/main` por los briefs y registros de
  orquestación; conviene integrarlos antes de la entrega.
