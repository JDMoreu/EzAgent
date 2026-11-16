# AUTO-A2 — r1

## Resultado

- Brief/tarea: AUTO-A2 (modo autónomo, fase A2: `ez-herdr::permission`)
- Estado: **completado**
- Fuente de la especificación: `docs/designs/modo-autonomo-elefante-pez-dorado.md` §9.4 (y §9.1 para ubicación/regla de hilos). No hay equivalente en el Python: funcionalidad nueva; no se tocó `src/`.

## Cambios

- `rust/crates/ez-herdr/src/lib.rs`: añade la línea `pub mod permission;`.
- `rust/crates/ez-herdr/src/permission.rs` (nuevo, 524 líneas): `DialogKind`, `Dialog`, `Decision`, `classify`, `denied`, `decide`, más las piezas puras del antirrebote y la auditoría (`APPROVE_SETTLE_S`, `FINGERPRINT_LINES`, `SCREEN_EXCERPT_CHARS`, `screen_fingerprint`, `screen_excerpt`). Sin hilos, sin locks y sin llamadas a Herdr.
- `rust/crates/ez-herdr/src/permission/tests.rs` (nuevo): 8 pruebas unitarias de las piezas internas.
- `rust/crates/ez-herdr/tests/permission.rs` (nuevo): 18 pruebas de integración con pantallas reales y con la lista de denegación.

### API portada de §9.4

- `classify(screen: &str) -> Dialog` con `Dialog { kind: Exec|Edit|Trust|Other|None, command, folder, options: Vec<(tecla, texto)>, approve_once }`.
- `denied(command: &str) -> Option<&'static str>` con los motivos en español.
- `decide(dialog: &Dialog, in_scope: bool, task_folder: Option<&Path>) -> Decision` con `Decision::{Approve(key), Ask(motivo), Ignore}`.

Reglas de «Aprobar» aplicadas en `decide`: alcance, `kind ∈ {Exec, Edit}`, `Trust` solo con `folder` dentro de la carpeta de la tarea, `approve_once` único y `denied` vacío. Todo lo demás responde `Ask(motivo)`.

## Pantallas reales usadas en las pruebas (y de dónde salió cada una)

- **Claude Code, comando Bash:** captura de `measuredrun.com/claude-code-permission-prompt-manual-mode/` (Claude Code 2.1.x; `Bash command` + `Do you want to proceed?` + 4 opciones) y captura de `wmedia.es/en/tips/claude-code-keeps-asking-permission-chained-commands` (formato con descripción antes del comando y `Yes, and don't ask again for npm run…`).
- **Claude Code, edición de archivo:** captura de `measuredrun.com` (`Do you want to create hello.txt?` con `Yes, and switch to accept edits…`).
- **Claude Code, confiar en carpeta:** captura de `paolocostanzo.github.io/trusting-a-repo-means-trusting-its-mcp-servers/` (Claude Code 2.1.283: `Quick safety check` con `❯ No, exit` / `Yes, I trust this folder`) y variante numerada de `github.com/anthropics/claude-code/issues/28506` (`❯ 1. Yes, proceed` / `2. No, exit`). También se cubre el texto de `_BLOCKING_DIALOG` del Python.
- **Codex, comando:** captura de `xsavikx.github.io/agent-signals/corpus/codex/approval-v0-164-0.html` (`Would you like to run the following command?` + `$ printf 'hello' > note.txt` + `Yes, proceed (y)`).
- **Codex, edición:** snapshots publicados en el commit `25a2e15` de `openai/codex` (`Would you like to make the following edits?` + `README.md (+2 -0)` + `Yes, proceed`).
- **OpenCode v2, comando y edición:** capturas reales de `github.com/xSAVIKx/agent-signals` → `corpus/opencode2/screens/permission-shell-v2-2-5.screen` y `permission-edit-v2-2-5.screen` (OpenCode 2.2.5: `△ Permission required`, `$ comando`, `→ Edit archivo`, botones `Allow once / Always allow / Reject` sin teclas).
- **OpenCode numerado:** variante con `❯ 1. Allow once / 2. Always allow / 3. Reject`, documentada en `turboai.dev/docs/opencode/permissions` y en los issues `anomalyco/opencode#33123` y `#7954` (las teclas 1/2/3 se añadieron a ese diálogo).
- **Diálogo en español:** sintetizado con los textos del diseño (`Permiso requerido`, `Sí, permitir una vez`, `Permitir siempre`, `No, volver atrás`); no había una captura pública en español.

No se pudo ver ningún CLI en vivo sin riesgo desde este worktree, así que todos los casos salen de las capturas/documentación citadas.

## Fixtures y pruebas de paridad

- Fixtures consumidos: **ninguno** (el brief indica «(ninguno)»).
- Filas de `parity/test-map.csv` portadas: **ninguna** (funcionalidad nueva sin Python equivalente).
- En esta fase no se escribieron pruebas de paridad; sí 8 pruebas unitarias `#[cfg(test)]` y 18 de integración.

## Decisiones (`// DECISION` en el código)

1. **`decide` recibe solo tres argumentos** (la firma exacta de §9.4), así que el antirrebote no se aplica dentro. El módulo expone `screen_fingerprint` (últimas 20 líneas no vacías) y `APPROVE_SETTLE_S = 1.5` para que el bucle A3 los use: copia la huella, espera 1,5 s sin cambios y no repite la misma. El módulo sigue siendo puro.
2. **`auto mode` / `modo automático` se excluye de `approve_once`** además de la lista de §9.4. No es una aprobación «para esta vez» y saltaría la lista de denegación y la auditoría (§5 del diseño). Sin esto, Claude Code nunca podría aprobar una vez porque su tercer opción también empieza por «Yes».
3. **Opciones solo de líneas numeradas o con flecha**, como dice §9.4. Los botones sin numerar de OpenCode 2.2.5 (`Allow once / Always allow / Reject`) no dan una tecla deducible: `approve_once = None` y se pregunta. Es la rama segura de «si no se puede deducir con seguridad, se pregunta».
4. **La tecla mostrada entre paréntesis gana al número** (`Yes, proceed (y)` → `y`): es el atajo que el propio Codex anuncia.
5. **Extracción del comando:** se prefiere la línea que la lista de denegación reconoce y, si no, la primera línea con forma de comando. Las capturas de Claude muestran descripción y comando en órdenes distintos; así un formato nuevo del TUI no puede ocultar `rm -rf /` detrás de una descripción.
6. **`denied` sin contexto de proyecto:** la firma es `denied(command)` y no recibe la carpeta de la tarea, así que `.env`, `.ssh`, `.aws` y `credential` se deniegan siempre que aparezcan. Es más estricto que «fuera del proyecto» de §9.4: el resultado es pausar y preguntar, nunca aprobar de más.
7. **La huella ignora líneas vacías** para que el relleno del TUI no cambie el antirrebote.
8. **Bordes de caja** (`│ ┃ | ║`) se recortan antes de leer opciones/comandos, porque OpenCode v2 y Claude los dibujan en cada línea.
9. **`command` de un diálogo `Edit` es el archivo destino** (`.env`, `README.md`), para que la lista de credenciales también cubra ediciones.
10. **Comparación de carpetas** por componentes, insensible a mayúsculas solo en Windows (donde el sistema de archivos lo es); `~` no se expande (si no coincide, se pregunta).

## Verificación

Ejecutado desde `rust/`:

- `cargo fmt --all` → limpio (`--check` sin diferencias).
- `cargo clippy -p ez-herdr --all-targets -- -D warnings` → limpio.
- `cargo test -p ez-herdr` → **170 passed, 0 failed** (147 unitarias, de las cuales 8 nuevas; 18 de integración nuevas; 5 de paridad existentes).
- `permission.rs`: 524 líneas (límite 600); ninguna función supera las 60 (la más larga es la tabla de datos de `deny_rules`).
- Sin dependencias nuevas: se usó `regex`, que `ez-herdr` ya declaraba. No se tocó ningún `Cargo.toml`.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `println!`, `unsafe` ni `.unwrap()`; borrados y subprocesos ajenos al módulo.

## Pendiente o riesgos

- El antirrebote completo (1,5 s de pantalla estable + no repetir huella) vive en el bucle de A3; este módulo solo entrega la huella y la constante porque `decide` es puro y su firma no tiene un parámetro para «ya aprobado».
- `rm -rf /tmp/..` (ruta que resuelve a la raíz con `..`) no coincide con la lista literal; queda marcado con `// shortcut:` en `deny_rules`. Igual pasa con rutas entre comillas o con variables no expandidas dentro del comando (`"$HOME"`).
- `.env`, `.ssh`, `.aws` y `credential` se deniegan incluso dentro del proyecto (la firma no recibe la carpeta); se pausa y pregunta, nunca se aprueba de más.
- Un TUI nuevo sin números, flechas ni los textos cubiertos cae en `Other`/`Ask`; es seguro pero obliga a la persona a responder.
- No hay equivalente Python, así que no se añadió nada a `rust-migration/python-quirks.md`.
