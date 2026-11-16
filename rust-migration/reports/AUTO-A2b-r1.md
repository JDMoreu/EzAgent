# AUTO-A2b — r1

## Resultado

- Brief/tarea: AUTO-A2b (`ez-herdr::permission` a la versión 2 del diseño)
- Estado: **completado**
- Fuente de la especificación: `docs/designs/modo-autonomo-elefante-pez-dorado.md` §10.7, §10.8, §10.2 y §10.15 (donde difieren de §9, manda §10). No hay equivalente en Python: funcionalidad nueva; no se tocó `src/`.
- Solo partes puras: sin hilos, sin locks, sin Herdr y sin guardar pantallas.

## Cambios

- `rust/crates/ez-herdr/src/permission.rs` (162 líneas): tipos públicos (`Dialog` con `path`, `approve_once: Option<Vec<String>>`, `Decision::Approve(Vec<String>)`), `Role`, `Scope` y `decide`, que aplica §10.7/§10.8. Reexporta la API desde los submódulos.
- `rust/crates/ez-herdr/src/permission/dialogs.rs` (516): lectura de la pantalla (tipo, comando, ruta, carpeta), filtro por palabra completa de §10.7, secuencias de teclas (número, atajo `(y)` o flechas + `enter`) y `classify`. Sustituye el contenido anterior de `permission.rs`.
- `rust/crates/ez-herdr/src/permission/deny.rs` (279): lista de denegación ampliada de §10.8 (git destructivo, `.git`, publicar, prune, `del /s` sobre unidad), rutas siempre denegadas (`denied_path`) y comprobación de borrados recursivos contra las carpetas del papel (`deletes_outside`, `path_allowed`).
- `rust/crates/ez-herdr/src/permission/transaction.rs` (66): partes puras de la transacción de §10.7: `screen_fingerprint`, `same_dialog` (S1/S2) y `approval_granted` (S3: diálogo fuera y sin concesión permanente).
- `rust/crates/ez-herdr/src/permission/audit.rs` (208): `audit_summary(dialog, decision)` sin texto de pantalla y SHA-256 propio (ver Decisiones).
- `rust/crates/ez-herdr/src/permission/tests.rs` (341): 21 pruebas unitarias.
- `rust/crates/ez-herdr/tests/permission.rs` (466): 27 pruebas de integración con pantallas reales.
- `rust/crates/ez-herdr/tests/permission/*.txt` (13 archivos): pantallas de Claude Code, OpenCode v2, Codex y español, extraídas a ficheros para que la prueba se lea.
- `rust/crates/ez-herdr/src/lib.rs`: sin cambios; `pub mod permission;` ya existía.

### API

- `classify(screen) -> Dialog` con `kind`, `command` (solo `Exec`), `path` (nuevo, solo `Edit`), `folder` (`Trust`), `options` y `approve_once`.
- `denied(command) -> Option<&'static str>` y `denied_path(path) -> Option<&'static str>` (nuevo).
- `decide(dialog: &Dialog, scope: &Scope) -> Decision`, con `Scope { in_scope, role, workdir, edit_dirs }`; `edit_dirs` y `workdir` llegan canónicos desde A3.
- `same_dialog(s1, s2) -> bool` y `approval_granted(s3) -> Result<(), &'static str>` (nuevas).
- `audit_summary(dialog, decision) -> String` (nueva). Se eliminaron `screen_excerpt` y `SCREEN_EXCERPT_CHARS`.

## Decisiones (`// DECISION` en el código)

1. **SHA-256 propio.** `sha2` está en `[workspace.dependencies]` pero no es dependencia de `ez-herdr` y el brief prohíbe tocar `Cargo.toml`; `ez-core` tampoco expone un hash de texto. Se implementó SHA-256 en `permission/audit.rs` (sin dependencias) y se probó contra los vectores estándar de `""`, `"abc"` y el mensaje de 56 bytes. Impacto: ninguno fuera del crate.
2. **`auto mode` sigue excluido** además del filtro literal de §10.7 (decisión 2 de r1). Sin esto, todo diálogo de Claude Code tendría dos opciones «sí» y no se aprobaría nunca.
3. **El atajo mostrado gana al número** (`Yes, proceed (y)` → `["y"]`, decisión 4 de r1). Es una tecla que el propio CLI anuncia; el número solo se usa si no hay atajo.
4. **`enter` extra solo para opciones numeradas sin atajo** cuando la pantalla pide confirmar (`Press enter to confirm`, `enter confirm`). Las listas con flecha siempre terminan en `enter`; añadirlo otra vez enviaría un Enter al prompt del agente.
5. **`Trust` ignora el filtro de permanencia** (la confianza es permanente por naturaleza) y queda acotado por §10.2; un `Verifier` nunca aprueba `Trust`.
6. **Rutas de `Edit`**: relativas se resuelven contra `workdir`; la resuelta debe estar dentro de algún `edit_dir` (para `executor`/`member` A3 pasa `[workdir]`, para `verifier` `[verificacion-K]`). `.env*` e `id_rsa*` se comparan por prefijo de componente, como el glob de §10.8; `.pem`/`.key` por sufijo.
7. **Borrados recursivos** (`rm -r…`, `rd /s`, `del /s`, `Remove-Item -Recurse`): si la ruta lleva `~`, `$` o `%` no se puede comprobar sin shell → `Ask`; si es la carpeta permitida o una que la contiene → `Ask`; si queda fuera → `Ask`. Dentro se aprueba.
8. **Auditoría en una línea JSON** (la bitácora es JSONL) con solo `kind`, `option`, `command`, `command_hash`, `path` y `decision`. La dirección de la terminal la añade A3, que es quien la conoce; `audit_summary` no la recibe.
9. **Menús con flecha sin números**: se absorben las líneas no marcadas que siguen a la resaltada (el diálogo de confianza de Claude lista así el «sí»). Queda un `shortcut:` si la opción marcada no es la primera del menú: el conteo de flechas podría quedarse corto y S3 lo detecta (el diálogo seguiría en pantalla) → pausa.
10. **Reglas de git con `\bgit\b[^\n]*\bverbo\b`** para cubrir `git -C <ruta> reset --hard` y `sudo git …`. Puede denegar de más una frase que contenga esas palabras; el resultado es pausar, nunca aprobar.
11. **S3 vacía no es éxito**: si la lectura posterior queda en blanco (fallo de Herdr), `approval_granted` devuelve error en vez de dar el diálogo por desaparecido.

## Pantallas de las pruebas

Mismas fuentes que r1 (capturas públicas citadas en `rust-migration/reports/AUTO-A2-r1.md`):

- Claude Code Bash, «don't ask again» y creación de archivo: capturas de `measuredrun.com` y `wmedia.es`.
- Claude Code confiar en carpeta: `paolocostanzo.github.io` y el issue `anthropics/claude-code#28506`.
- Codex comando y edición: corpus `agent-signals` y snapshots del commit `25a2e15` de `openai/codex`.
- OpenCode v2 comando y edición: capturas reales de `xSAVIKx/agent-signals` (`permission-shell-v2-2-5`, `permission-edit-v2-2-5`); variante numerada según `turboai.dev` e issues `anomalyco/opencode#33123` y `#7954`.
- Español: sintetizado con los textos del diseño (no hay captura pública).
- Nuevo caso obligatorio: `❯ 1. Yes, and don't ask again / 2. Yes` elige la opción 2 (`["2"]`).
- Nuevo caso sin números: el diálogo de confianza con `❯ No, exit` / `Yes, I trust this folder` produce `["down", "enter"]`.

No se pudo ver ningún CLI en vivo sin riesgo desde este worktree.

## Fixtures y pruebas de paridad

- Fixtures consumidos: **ninguno** (el brief indica «(ninguno)»).
- Filas de `parity/test-map.csv` portadas: **ninguna** (funcionalidad nueva sin Python equivalente).
- Pruebas nuevas: 21 unitarias (`#[cfg(test)]`) y 27 de integración.

## Verificación

Ejecutado desde `rust/`:

- `cargo fmt --all` → limpio (`--check` sin diferencias).
- `cargo clippy -p ez-herdr --all-targets -- -D warnings` → limpio.
- `cargo test -p ez-herdr` → **192 passed, 0 failed** (160 unitarias, 21 de ellas de `permission`; 27 de integración de `permission`, 9 casos nuevos y el resto adaptados a la API v2; el resto son las que ya existían).
- Límites: `permission.rs` 162 líneas, `dialogs.rs` 516, `deny.rs` 279, `audit.rs` 208, `tests.rs` 341, `tests/permission.rs` 466; todas por debajo de 600. Ninguna función pasa de 60 líneas (las tablas de patrones y la de SHA-256 son datos).
- Sin dependencias nuevas ni cambios en `Cargo.toml`.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `unsafe` ni `.unwrap()` fuera de pruebas; sin shell ni subprocesos.

## Pendiente o riesgos

- **`approve_keys`: no existía.** No hay ningún uso en el crate (grep de `approve_keys` solo aparece en el diseño y en el informe del pez dorado A); no hubo nada que eliminar. La secuencia se deduce solo de la pantalla.
- **Paso 3 de §10.7 (contador `generation`)**: es de A3 (atómico del bucle); este módulo entrega las partes puras S1/S2 (`same_dialog`) y S3 (`approval_granted`).
- **Flechas en menús que resaltan una opción del medio**: pueden faltar flechas; S3 pausa al ver el diálogo todavía en pantalla.
- **Detección de concesión permanente en S3**: es heurística por texto («always», «siempre», «don't ask again», «for this session»). Una línea normal del agente con «always» dentro de la ventana de 2 s pausaría; se prefiere pausar a aprobar de más.
- **Comandos ofuscados** (base64, concatenación de variables) no los ve `denied`, que compara texto literal; el borrado recursivo con `$HOME`/`~` sí se pausa porque no se puede comprobar sin shell.
- **La auditoría no lleva la dirección** (la añade A3) ni la carpeta del diálogo `Trust` (§10.15 pide la ruta editada).
- Sin equivalente Python, así que no se añadió nada a `rust-migration/python-quirks.md`.
