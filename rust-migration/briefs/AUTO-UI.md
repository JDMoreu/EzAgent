# AUTO-UI — Modo autónomo, fase UI: fila en la vista rápida, marca en la barra y parar desde la fila (10.19)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- **Especificación: `docs/designs/modo-autonomo-elefante-pez-dorado.md`, secciones 9.8 y 10.19 (manda la 10).** Funcionalidad nueva aprobada por el usuario.
- HUD: `rust/crates/ez-hud/src/paint/peek.rs` (`PeekItem`, estados y textos), `rust/crates/ez-hud/src/win/peek/**` (controlador y ventana; **cuidado con los locks: lee el comentario de `apply` y `pointer_row` — ya hubo interbloqueos aquí**), `rust/crates/ez-hud/src/model/drop*` y `paint/marks*` (órbita de actividad); la app: `rust/crates/ez-app/src/app/activity/**` (`refresh_activity`, `open_activity`, `dismiss_activity`) y `rust/crates/ez-app/src/app/autonomous/**` (estado y `stop` del bucle; informe `rust-migration/reports/AUTO-A3-r1.md`)

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/paint/peek* y rust/crates/ez-hud/src/win/peek/** (estado nuevo y su dibujo)
- rust/crates/ez-hud/src/paint/marks* / model/drop* solo para la marca discreta de la órbita
- rust/crates/ez-app/src/app/activity/** y lo mínimo en rust/crates/ez-app/src/app/autonomous/** (exponer el resumen para la fila y parar desde la fila)
- pruebas nuevas en esos crates y rust-migration/reports/AUTO-UI-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_peek.jsonl
- parity/fixtures/hud_bar.jsonl

## Notas específicas
- Implementa 10.19: (1) un estado nuevo `autonomous` en `PeekItem`, con texto «Autónomo · ronda {n} · {estado}» (en pausa, el motivo); `refresh_activity` añade esa fila fija desde el estado del bucle mientras haya tarea en `preparing`, `running`, `verifying` o `paused`, y desaparece al terminar o detenerse. (2) Una marca discreta del color de acento en la órbita de la barra mientras la tarea está activa (no en pausa). (3) Pulsar la fila → confirmación generada por el código «¿Detengo el modo autónomo?» con el `ConfirmationManager`; «sí» llama al `stop` del bucle. El atajo de cancelar ya detiene el modo (A3).
- **Paridad:** `hud_peek_parity` y `hud_bar_parity` deben seguir en verde sin tocar sus PNG (sin tarea autónoma, el dibujo es idéntico). Añade pruebas: la fila aparece con el texto correcto en cada estado, desaparece al terminar, la marca solo con tarea activa, pulsar la fila pide confirmación y «sí» detiene. **Regla de locks:** nada de llamadas a otro componente, Herdr o confirmaciones con un lock de la barra o de la vista rápida tomado. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ez-hud -p ez-app` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/AUTO-UI-r1.md` y un commit en tu rama.
