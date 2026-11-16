# AUTO-A1 — Modo autónomo, fase A1: estado, artefactos, veredicto, textos y herramientas con sus guardas (ez-agent::autonomous)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- **La especificación es `docs/designs/modo-autonomo-elefante-pez-dorado.md`: léela completa; esta tarea implementa 9.2, 9.3 y 9.6** (y respeta 9.1 y la regla de hilos). Es funcionalidad nueva aprobada por el usuario.
- Rust existente: `rust/crates/ez-agent/src/registry*` (cómo se declaran herramientas, esquemas y niveles de riesgo), `rust/crates/ez-agent/src/runtime/` (`state.rs`: `session.tainted`; `execute.rs`; `tools.rs`: el gate de confirmación), `rust/crates/ez-agent/src/confirm.rs` (`ConfirmationManager::ask`), `rust/crates/ez-agent/src/tools_swarm*` y `tools_herdr/` (modelo de grupo de herramientas), `rust/crates/ez-agent/src/audit.rs`, `rust/crates/ez-core/src/paths*` (carpeta de datos)

## Archivos que se tocan (solo estos)
- rust/crates/ez-agent/src/autonomous/ (nuevo: `mod.rs`, `state.rs`, `artifacts.rs`, `verdict.rs`, `texts.rs`, `tools.rs`) y la línea `pub mod autonomous;` en `rust/crates/ez-agent/src/lib.rs`
- el registro del nuevo grupo de herramientas donde se registran los demás grupos (cambio mínimo y aditivo)
- pruebas nuevas en `rust/crates/ez-agent/src/autonomous/tests*.rs` o `rust/crates/ez-agent/tests/autonomous.rs`
- rust-migration/reports/AUTO-A1-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- **Este crate no ejecuta el bucle** (eso es A3 en `ez-app`). Aquí va: el modelo `AutonomousState` con las transiciones de 9.5 como funciones puras (`on_executor_finished`, `on_verdict`, `on_incident`, `on_deadline`, `pause`, `resume`, `stop`) que devuelven la acción siguiente (`Action::{LaunchVerifier, SendCorrection(path), Pause(motivo), Done, …}`) sin tocar Herdr; los artefactos de 9.3 (crear carpeta, escribir `encargo.md`, `criterios.md`, `estado.json` atómico, `bitacora.jsonl`, leer y validar `ronda-N/veredicto.json`, escribir `correccion-N.md`, detección de churn con similitud ≥ 0,85); los textos de 9.6 (literales); y las 4 herramientas de 9.2 con sus esquemas y **las cuatro guardas de activación** (contaminado, `AUTHORIZE_RE` sobre el último mensaje del usuario, una sola tarea a la vez, confirmación única con el texto `CONFIRM`).
- Las herramientas hablan con el bucle por un trait `AutonomousHost` (inyectado al construir el grupo) con `start(spec) -> Result<id>`, `status() -> Option<Summary>`, `resume()`, `stop(reason)`; en esta fase, un doble en pruebas. Para la guarda 2 necesitas el último mensaje del usuario: si el runtime no lo expone a las herramientas, añade el mínimo aditivo (y dilo en el informe).
- Pruebas: transiciones completas (dos rondas limpias → `Done`; ronda sucia → corrección; churn → pausa; topes; JSON de veredicto inválido → reintento y luego pausa), artefactos en un directorio temporal, y las cuatro guardas (cada una rechaza; las cuatro pasan + «sí» → `start`). `cargo fmt`, `cargo clippy -p ez-agent --all-targets -- -D warnings`, `cargo test -p ez-agent` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/AUTO-A1-r1.md` y un commit en tu rama.
