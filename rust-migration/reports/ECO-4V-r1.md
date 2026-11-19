# ECO-4V — verificación independiente de `ez-team`

## Resultado
- Brief/tarea: ECO-4V (ECO-4)
- Estado: completado

## Veredicto
fail

## Hallazgos

### Bloqueante
- Ninguno.

### Mayor
- `rust/crates/ez-team/src/team.rs:121-134` y `:395-406`: la regla exige error cuando `qa` **o** `qa_validator` usa el modelo del desarrollador, pero la prueba solo cubre `qa`. Al mutar el bucle a `[Role::Qa]`, las 38 pruebas y el doc-test siguieron pasando. Propuesta: añadir una prueba que ponga el modelo de `qa_validator` igual al de `developer` y compruebe un error con «mismo modelo».
- `rust/crates/ez-team/src/inventory.rs:15-16` y `:421-434`: el tope de 20 s de `modelos_cmd` no está comprobado; `FakeSystem::run` ignora el parámetro `timeout`. Cambiar `MODELS_TIMEOUT` de 20 s a 1 s no falló ninguna prueba. Propuesta: registrar el `timeout` recibido en el doble y afirmar que es `MODELS_TIMEOUT`, además de comprobar el comportamiento con un proceso local que se exceda.
- `rust/crates/ez-team/src/profiles.rs:12-32` y `:88-109`: la prueba del archivo por omisión no compara el contenido exacto con el diseño. Cambiar `familia = "openai"` por `familia = "familia-mutada"` no falló ninguna prueba. Propuesta: afirmar que el archivo escrito es byte a byte el bloque TOML de §6.
- `rust/crates/ez-team/src/inventory.rs:198`: una suscripción declarada con `agentes = []` se trata como «sin suscripciones» y hace disponibles todos los agentes. La regla dice que un agente debe estar nombrado por alguna suscripción; solo cuentan todos cuando no hay suscripciones declaradas. Propuesta: usar `subs.is_empty()` en la comprobación y añadir una prueba con una suscripción vacía.
- `rust/crates/ez-team/src/inventory.rs:93`: tras matar el proceso, `reader.join()` puede esperar indefinidamente si un descendiente conserva abierto el pipe de stdout; el tope de 20 s no cubre esa espera. Propuesta: comunicar la salida por canal con vencimiento y devolver `None` al vencer, sin `join()` sin límite.

### Menor
- `rust/crates/ez-team/src/inventory.rs:177-180` y `:252-254`: la caché process-wide se indexa por la línea de comando, no por agente, aunque el brief pide guardar la salida «por agente». Propuesta: indexar por `(profile.id, command)` y probar dos perfiles con el mismo comando y salidas distintas.
- `rust/crates/ez-team/src/team.rs:204-209`: la tarjeta añade líneas `Error:` que el formato exacto de §6 no incluye; §6 solo pide las líneas de papel y luego los avisos. Propuesta: quitar los errores de `card` y dejar que el llamador los muestre por separado, o confirmar la extensión del formato antes de mantenerla.
- `rust/crates/ez-team/src/profiles.rs:12`, `team.rs:185`, `inventory.rs:158` y `launch.rs:110`: el informe del desarrollador declara varias decisiones por omisión del brief, pero el código solo marca una con `// DECISION:`, como exige `_COMMON-ECO.md`. Propuesta: añadir el marcador junto a cada elección (valores por omisión, errores en la tarjeta, claves de JSON y `Role::label` en el primer mensaje).

## Mutaciones
- `rust/crates/ez-team/src/profiles.rs`: `familia = "openai"` → `familia = "familia-mutada"`; detectada: no.
- `rust/crates/ez-team/src/family.rs`: eliminar el recorte de dígitos finales en `derive`; detectada: sí (`design_examples`, `text_and_json_describe_the_inventory`).
- `rust/crates/ez-team/src/inventory.rs`: invertir el filtro de suscripciones (`declared.is_empty()` → `!declared.is_empty()`); detectada: sí (7 pruebas de inventario).
- `rust/crates/ez-team/src/inventory.rs`: `MODELS_TIMEOUT` de 20 s → 1 s; detectada: no.
- `rust/crates/ez-team/src/inventory.rs`: eliminar `cache.insert(key, models.clone())`; detectada: sí (`models_cmd_output_is_parsed_and_cached`).
- `rust/crates/ez-team/src/team.rs`: quitar `Role::QaValidator` del bucle de error por modelo compartido; detectada: no.
- `rust/crates/ez-team/src/team.rs`: desplazar el límite de la tarjeta de `count > 1` a `count > 2`; detectada: sí (`card_has_the_exact_design_format`).
- `rust/crates/ez-team/src/launch.rs`: exigir `qa_report` a `Role::Qa` en vez de `Role::QaValidator`; detectada: sí (2 pruebas de instrucciones).

Las ocho fallas se metieron una a la vez y cada una se deshizo con `git checkout -- <archivo>` antes de la siguiente.

## Verificación
- `cargo fmt --all --check`: exit 0, sin salida.
- `cargo clippy -p ez-team --all-targets -- -D warnings`: ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.97s``, sin avisos.
- `cargo test -p ez-team`: `test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`; doc-tests `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s`.
- Reglas comunes: no hay `shell`, `unsafe`, `unwrap` fuera de pruebas, impresión directa ni locks durante E/S; ningún `.rs` supera 600 líneas ni ninguna función 60.
- Alcance: los contratos siguen sin cambios y la rama no contiene modificaciones distintas de los entregables de ECO-4 y este informe.
