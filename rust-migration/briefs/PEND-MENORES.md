# PEND-MENORES — Pendientes menores: nombres en voice_control, guardado de configuración fuera del hilo de pintura y confianza al abrir miembros del equipo autónomo
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- Informes con el detalle de cada pendiente: `rust-migration/reports/P-IMP-NAMES-r1.md` (voice_control `close_and_watch` no resuelve nombres), `rust-migration/reports/P-FIX-LOCKAUDIT-r1.md` («`HudConfig::save`/`remember` escriben a disco con `cfg.write()` tomado y `save_bar_position` corre en el hilo de pintura»), `rust-migration/reports/AUTO-B-r1.md` (el diálogo de confianza al abrir un miembro del equipo no se aprueba: el camino de 10.9 solo está cableado al ejecutor único)
- Código: `rust/crates/ez-agent/src/tools_herdr/voice*` y `tools_herdr/names*` (resolvedor), `rust/crates/ez-app/src/app/core/hud_bridge.rs` (`HudConfig`), `rust/crates/ez-hud/src/win/drop/ui/render.rs` (`save_bar_position`), `rust/crates/ez-app/src/app/autonomous/{team,approve}.rs` y `docs/designs/modo-autonomo-elefante-pez-dorado.md` §10.2 y §10.9

## Archivos que se tocan (solo estos)
- rust/crates/ez-agent/src/tools_herdr/** (voice_control con el resolvedor)
- rust/crates/ez-app/src/app/core/hud_bridge.rs y lo mínimo en rust/crates/ez-hud/src/win/drop/** para que la escritura a disco no ocurra en el hilo de pintura ni con un lock tomado
- rust/crates/ez-app/src/app/autonomous/** y lo mínimo en rust/crates/ez-swarm/src/** para aprobar la confianza de los miembros
- pruebas nuevas y rust-migration/reports/PEND-MENORES-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- (1) `voice_control` con `action=close_and_watch` (y cualquier otra acción de voz que reciba una terminal por nombre) usa el mismo resolvedor y la misma confirmación «¿Te refieres a…?» que `delegate_to_agent`; cambia la firma de `handle_voice` lo mínimo para recibir el puerto o los candidatos. (2) `HudConfig::save`/`remember`: la escritura a disco va a un hilo escritor (cola con el último valor por clave, sin perder el orden ni escrituras) y ningún lock se retiene durante la escritura; `save_bar_position` deja de escribir desde el hilo de pintura. Prueba: un guardado lento (doble que tarde 2 s) no retrasa `render_frame`. (3) Modo equipo: cuando un miembro del enjambre autónomo se abre y su CLI muestra el diálogo de confianza en la carpeta, el bucle lo trata con la regla de 10.2/10.9 (aprobar solo si la carpeta canonicalizada es `workdir` o está dentro; si no, pausar) usando `ez_herdr::permission` y la transacción de `approve.rs`, igual que el ejecutor único. Prueba con un `HerdrPort` falso y la pantalla real de confianza de Claude de `rust/crates/ez-herdr/tests/permission/`.
- Regla: ningún lock retenido durante una operación larga, una llamada a otro componente o una operación de ventana. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` en verde y `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/PEND-MENORES-r1.md` y un commit en tu rama.
