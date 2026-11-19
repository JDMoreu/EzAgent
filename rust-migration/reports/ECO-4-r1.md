# ECO-4 — `ez-team`: agentes, suscripciones, inventario, equipo y textos de los peces

## Resultado
- Brief/tarea: ECO-4
- Estado: completado

## Cambios
- `rust/crates/ez-team/src/error.rs`: `TeamError` (`Io`, `Toml`, `Invalid`) con `Display` en español y `source` para los errores de E/S y TOML.
- `rust/crates/ez-team/src/profiles.rs`: `AgentProfile`, `load_or_init` (escribe `agentes.toml` por omisión de §6 tal cual, con comentarios) y `load`.
- `rust/crates/ez-team/src/subscriptions.rs`: `Subscription` y `load` (sin archivo → vacío).
- `rust/crates/ez-team/src/family.rs`: `family` con el campo `familia` o la derivación del modelo (último `/`, hasta `-`/`.`, sin dígitos finales, `#variante` ignorado).
- `rust/crates/ez-team/src/inventory.rs`: trait `System`, `RealSystem` (sin shell y sin consola vía `CREATE_NO_WINDOW`), `detect` (PATH + PATHEXT, ruta absoluta, filtro por suscripciones, `modelos_cmd` con tope de 20 s y caché en memoria), `AvailableAgent`, `Inventory::{find,text,json}`.
- `rust/crates/ez-team/src/team.rs`: `parse` de `equipo.toml` con errores por fila, `validate` (R§6) y `card`.
- `rust/crates/ez-team/src/launch.rs`: `argv` (`{modelo}`/`{esfuerzo}` y `esfuerzo_args`), `render_instructions` (común + papel, marcadores y comprobación de sobrantes) y `first_line`.
- `rust/crates/ez-team/src/lib.rs`: módulos y reexportaciones nuevas; `contract.rs`, `roles.rs` y `roles/*.md` quedaron intactos.
- `rust/crates/ez-team/src/test_support.rs`: carpeta temporal única bajo `temp_dir()`, borrada en `Drop` (solo pruebas).

## Decisiones
- `first_line` sustituye `{papel}` por `Role::label()` («Desarrollador», «Validador de QA»): el diseño no fija cuál de los dos nombres va y la frase es para la persona/el pez, no para el protocolo. En `equipo.toml` y `ez` se sigue usando el id.
- La tarjeta muestra también los **errores** bloqueantes, después de las líneas de papel y antes de los avisos, con prefijos `Error:` y `Aviso:`; §6 solo menciona los avisos, pero sin los errores la persona no vería por qué no puede aprobar.
- `· ×{cantidad}` solo aparece cuando `cantidad > 1`: el corchete de §6 lo marca opcional y `×1` en todas las líneas sería ruido.
- El aviso de «misma familia» se omite cuando el modelo es exactamente el del desarrollador, porque ese caso ya es un error (mismo modelo).
- La caché de `modelos_cmd` se indexa por la línea de comando y solo guarda salidas correctas; un fallo cae a `modelos` y se reintenta en la siguiente detección.
- `Inventory::json` usa las claves `id`, `nombre`, `ruta`, `modelos`, `familias`: el protocolo no fija los nombres y se mantienen en español como el resto de los datos.
- El separador de listas (`PATH`, `PATHEXT`) es `;` en Windows y `:` en otros sistemas; el diseño es Windows pero las pruebas no dependen de una ruta concreta.
- Un perfil sin `argv` se ignora en `detect` (no hay nada que lanzar) en vez de entrar en pánico.

## Verificación
- `cargo fmt --all --check` → exit 0, sin salida.
- `cargo clippy -p ez-team --all-targets -- -D warnings` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.59s` (sin avisos).
- `cargo test -p ez-team` → `test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` y doc-tests `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s`.
- Contratos sin cambios: `git diff --stat -- crates/ez-team/src/roles.rs crates/ez-team/src/contract.rs crates/ez-team/src/roles/` vacío.
- Nada fuera de `rust/crates/ez-team/**` y el informe: `Cargo.toml`, `Cargo.lock` y el resto del workspace sin tocar.

## Pendiente o riesgos
- `RealSystem::run` no se prueba contra un proceso real (solo su parte de entorno/archivos); su lógica de tiempo límite se apoya en `try_wait` + hilo lector y queda para la integración con un CLI instalado.
- El diseño no fija el texto exacto de `Inventory::text` ni de las claves de `json`; si `ez-ctl` o `ez-hub` esperaban otro formato, se ajusta en la ola 2 sin cambiar la API.
