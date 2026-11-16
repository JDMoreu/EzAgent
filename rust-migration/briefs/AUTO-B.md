# AUTO-B — Modo autónomo, fase B: bucle multiagente sobre el enjambre (10.18) y modelo del verificador
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- **Especificación: `docs/designs/modo-autonomo-elefante-pez-dorado.md`, sección 10.18 (y 9.7; manda la 10).** Funcionalidad nueva aprobada por el usuario; no hay Python de referencia.
- Lo ya hecho que debes USAR: `rust/crates/ez-agent/src/autonomous/**` (A1/A1b: `TeamSpec`, estado con papel `member`, herramientas, textos) y `rust/crates/ez-app/src/app/autonomous/**` (A3: el bucle, la aprobación como transacción, la verificación con `verificacion-K`, la corrección, los topes y la pausa); lee los informes `rust-migration/reports/AUTO-A1b-r1.md` y `AUTO-A3-r1.md`, sobre todo su «Pendiente o riesgos».
- El enjambre: `rust/crates/ez-swarm/src/**` (servicio, tablero, supervisor y su lanzamiento con `swarm_args`, `incidents`, `recovery`), `rust/crates/ez-agent/src/tools_swarm*` (arranque de un enjambre y normalización de papeles con alias), `rust/crates/ez-core/src/config/defaults.rs` (`swarm_args`: Codex `--yolo`, Claude `--allowedTools Bash(ezs *)`)

## Archivos que se tocan (solo estos)
- rust/crates/ez-app/src/app/autonomous/team.rs (nuevo; y submódulos si hace falta) y lo mínimo en el resto de `rust/crates/ez-app/src/app/autonomous/**`
- rust/crates/ez-agent/src/autonomous/** (solo: `TeamSpec`/`StartSpec` con `verifier_agent` y `verifier_model`, `CONFIRM_TEAM`, y que `start_autonomous_team` deje de confirmar y fallar)
- rust/crates/ez-swarm/src/** (cambios mínimos y aditivos: lanzar miembros sin los argumentos que saltan permisos cuando el enjambre es autónomo, crear tareas `fix`, silenciar la recuperación en modo autónomo)
- pruebas nuevas en esos crates y rust-migration/reports/AUTO-B-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- Implementa 10.18 tal cual. (1) `start_autonomous_team` arranca un enjambre con `goal = task`, los papeles normalizados con los alias de `tools_swarm`, `workdir`, y sus miembros entran en `scope` con papel `member` y `edit_dirs=[workdir]`. (2) **Los miembros se lanzan SIN** `--yolo` ni `--dangerously-skip-permissions` (se quitan de `swarm_args` solo para este enjambre; `--allowedTools Bash(ezs *)` de Claude se conserva): sus permisos pasan por la aprobación como transacción de A3. (3) Cuando el tablero no tiene tareas pendientes ni en curso, se lanza la verificación de A3 (verificador aparte, en terminal nueva, con `verifier_agent`/`verifier_model`). (4) Si la ronda no es limpia: una tarea `fix` por hallazgo bloqueante o mayor, título «Corregir hallazgo {k}» y cuerpo con la ruta de `correccion-N.md`, asignada a `dev`, si no al primer papel que no sea `plan` ni `qa`, y si no hay ninguno → pausa. (5) En modo autónomo, los incidentes del enjambre pausan el bucle y **no** se lanzan las preguntas de recuperación del enjambre (una sola pregunta, la del modo autónomo). (6) `CONFIRM_TEAM` enumera papeles, agentes, modelos, criterios, carpeta y topes; `start_autonomous_team` ya no confirma para luego fallar.
- Añade `verifier_model` también al modo A (`StartSpec`) y úsalo al lanzar el verificador (pendiente de A3).
- Pruebas con dobles (enjambre en memoria o el servicio real sobre un directorio temporal, `HerdrPort` falso): arranque de equipo, que los miembros no llevan `--yolo`, verificación al vaciarse el tablero, tareas `fix` asignadas al papel correcto (y pausa si no hay quién), incidente → pausa sin preguntas del enjambre, confirmación con los papeles. Regla: ningún lock retenido durante una llamada a Herdr, al enjambre o al disco. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ez-app -p ez-agent -p ez-swarm` en verde y `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min; los escenarios 18-20 son de enjambre y no deben cambiar).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/AUTO-B-r1.md` y un commit en tu rama.
