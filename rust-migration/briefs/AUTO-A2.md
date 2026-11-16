# AUTO-A2 — Modo autónomo, fase A2: clasificador de diálogos de permiso y lista de denegación (ez-herdr::permission)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- **La especificación es `docs/designs/modo-autonomo-elefante-pez-dorado.md`: léela completa; esta tarea implementa la sección 9.4** (y respeta 9.1 y la regla de hilos). No hay equivalente en Python: es una funcionalidad nueva aprobada por el usuario.
- Referencias de pantallas y teclas: `src/ezagent/voice/agent_session.py` (`_BLOCKING_DIALOG`, `_KEY`, `blocking_dialog`), `rust/crates/ez-herdr/src/session*` (su port), `rust/crates/ez-herdr/src/herdr/actions.rs` (`keys_agent`) y `rust/crates/ez-fakes/src/fake_agent.rs` (pantallas guionizadas de agentes)

## Archivos que se tocan (solo estos)
- rust/crates/ez-herdr/src/permission.rs (nuevo; o `permission.rs` + `permission/*.rs` si pasa de 600 líneas) y la línea `pub mod permission;` en `rust/crates/ez-herdr/src/lib.rs`
- rust/crates/ez-herdr/tests/permission.rs (nuevo) y, si hace falta, ejemplos de pantalla en `rust/crates/ez-herdr/tests/permission/*.txt`
- rust-migration/reports/AUTO-A2-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- Implementa exactamente la API de 9.4: `classify(screen) -> Dialog`, `denied(command) -> Option<&'static str>` y una función `decide(dialog, in_scope, task_folder) -> Decision` con `Decision::{Approve(key), Ask(motivo), Ignore}` que aplique las reglas de «Aprobar» de 9.4 (la comprobación de alcance y la del antirrebote reciben los datos ya calculados: este módulo es puro, sin hilos ni Herdr).
- **Pantallas reales:** construye casos de prueba con el texto que muestran Claude Code, OpenCode y Codex al pedir permiso de ejecutar un comando y de editar un archivo, y el diálogo de «confiar en la carpeta». Si no puedes verlos en vivo sin riesgo, búscalos en la documentación pública de cada CLI y deja en el informe de dónde salió cada ejemplo. Casos obligatorios: opción «Yes» vs «Yes, and don't ask again» (se elige solo la primera), opciones con flecha `❯`, numeradas `1.`/`2.`, «Allow once»/«Always allow», diálogos en español; comandos de la lista de denegación con variantes (`rm -rf /`, `rm -rf ~`, `Remove-Item -Recurse C:\`, `git push -f origin main`, `curl … | sh`, `iwr … | iex`, `shutdown /s`); comandos inocuos que no deben denegarse (`rm -rf ./target`, `git push origin feature`, `cargo test`).
- Sin dependencias nuevas (usa `regex`/`fancy-regex` del workspace si ya están en `ez-herdr`; si no, no las añadas: dilo en el informe y usa coincidencias de texto). `cargo fmt`, `cargo clippy -p ez-herdr --all-targets -- -D warnings`, `cargo test -p ez-herdr` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/AUTO-A2-r1.md` y un commit en tu rama.
