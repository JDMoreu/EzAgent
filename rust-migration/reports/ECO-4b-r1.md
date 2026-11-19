# ECO-4b — `ez-team`: correcciones de la revisión 1

## Resultado
- Brief/tarea: ECO-4b
- Estado: completado

## Cambios
- `rust/crates/ez-team/src/profiles.rs`: `AgentProfile` gana `env: BTreeMap<String, String>` (tabla `[agente.env]`) y `prelaunch: Option<String>` (clave `previo`); `load` rechaza cualquier `previo` distinto de `opencode_state`. Valores por omisión de §14.2: OpenCode con `--standalone`, `OPENCODE_CONFIG_CONTENT` y `previo`, Claude con `--effort`, Codex sin cambios, con comentario explicativo. Prueba del archivo por omisión byte a byte.
- `rust/crates/ez-team/src/launch.rs`: sustituye `argv()` por `LaunchCommand { argv, env, prelaunch }` y `command()`, que rellena `{modelo}`, `{esfuerzo}` y `{variante}` en argv, `esfuerzo_args` y valores de `env`; añade `elephant_first_line()` con el texto exacto de §14.2.
- `rust/crates/ez-team/src/team.rs`: `parse` rechaza `esfuerzo` presente y vacío; `validate` exige que el perfil pueda aplicar el esfuerzo (`esfuerzo_args` o marcador en `env`) con el mensaje de §14.2; `card` ya no incluye errores, solo líneas de papel y avisos; nueva prueba de `qa_validator` con el modelo del desarrollador.
- `rust/crates/ez-team/src/team/tests.rs`: pruebas de `team` movidas a submódulo para cumplir el límite de 600 líneas por `.rs` (sin cambio de comportamiento).
- `rust/crates/ez-team/src/inventory.rs`: `RealSystem::run` entrega la salida por canal con vencimiento y sin `join()` sin límite; caché indexada por `(profile.id, comando)`; una suscripción declarada con `agentes = []` no habilita agentes; el doble registra el `timeout` recibido y se comprueba el valor literal de 20 s; prueba con un proceso local lento.
- `rust/crates/ez-team/src/lib.rs`: reexporta `LaunchCommand`.
- `rust/crates/ez-team/src/family.rs`: ajusta los literales de `AgentProfile` de la documentación y las pruebas a los campos nuevos (las cuatro familias de m03 ya estaban cubiertas).

## Decisiones
- `profiles.rs`: solo `opencode_state` es válido en `previo`; rechazarlo al cargar evita una preparación silenciosa que no haría nada.
- `launch.rs`: el nombre del papel en la primera línea es `Role::label()` (nombre humano); el protocolo sigue usando `Role::id()` (declarada en ECO-4).
- `inventory.rs`: el lector de salida usa un canal para que un descendiente que retenga el pipe no bloquee al llamador; no hay `join()` sin límite.
- `inventory.rs`: tras salir el proceso se espera como máximo 1 s (y al menos 50 ms) por el canal; suficiente para un pipe normal y acotado si un descendiente lo mantiene abierto.
- `inventory.rs`: las claves JSON (`id`, `nombre`, `ruta`, `modelos`, `familias`) se mantienen en español; el protocolo no las fija (declarada en ECO-4).
- `inventory.rs`: una suscripción con `agentes = []` es una suscripción declarada y no habilita nada; solo la ausencia de suscripciones habilita todos los agentes.
- `inventory.rs`: un perfil sin `argv` se ignora en `detect` en vez de fallar (declarada en ECO-4).
- `inventory.rs`: separador de listas `;` en Windows y `:` en otros sistemas (declarada en ECO-4).
- `inventory.rs`: solo se cachean las salidas correctas; un fallo cae a `modelos` y se reintenta. El id del perfil entra en la clave para que dos agentes con el mismo comando no compartan modelos (declarada en ECO-4, ampliada por ECO-4b).
- `team.rs`: el mensaje de esfuerzo no aplicable nombra al agente con el `nombre` del perfil cuando se conoce, porque el id es ambiguo para la persona que lee la tarjeta.
- `team.rs`: el aviso de «misma familia» se omite si los modelos son idénticos, porque ese caso ya es un error (declarada en ECO-4).
- `team.rs`: `×{cantidad}` aparece solo si `cantidad > 1`; §6 lo marca opcional (declarada en ECO-4).

## Verificación
- `cargo fmt -p ez-team -- --check` → exit 0, sin salida.
- `cargo clippy -p ez-team --all-targets -- -D warnings` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.75s`, sin avisos.
- `cargo test -p ez-team` → `test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s`; doc-tests `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s`.
- Mutaciones del verificador que antes pasaban, ahora detectadas (una a la vez, revertidas con `git checkout --`):
  - `familia = "openai"` → `familia-mutada` en el archivo por omisión: falla `profiles::tests::creates_the_default_file_and_reads_it_back`.
  - `MODELS_TIMEOUT` 20 s → 1 s: falla `inventory::tests::models_cmd_uses_the_design_timeout`.
  - quitar `Role::QaValidator` del bucle de error por modelo compartido: falla `team::tests::validator_with_the_developer_model_is_an_error`.
- Límites: ningún `.rs` supera 600 líneas (máximo 371 en `team/tests.rs`) ni ninguna función 60 líneas; contratos (`roles.rs`, `contract.rs`, `roles/*.md`) sin cambios; alcance limitado a `rust/crates/ez-team/**` y este informe.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, impresión directa, `unsafe` ni `.unwrap()` fuera de pruebas; ningún lock retenido durante E/S, subproceso o espera.

## Pendiente o riesgos
- La decisión de ECO-4 de mostrar los errores en la tarjeta queda **revertida** por el punto 6 de este encargo: con errores la propuesta se rechaza al elefante y no se emite tarjeta. Si `ez-ctl` o la vista esperaban el prefijo `Error:`, hay que ajustarlos en su crate (fuera de este alcance).
- `RealSystem::run` se prueba con `ping -n 4 127.0.0.1` en Windows (solo loopback) y `sleep 2` en otros sistemas; si el ejecutable no está disponible, la prueba sigue siendo válida porque `run` devuelve `None`, pero no ejerce la rama de vencimiento.
- El diseño sigue mostrando en §6 el bloque antiguo de `agentes.toml` (con `--model`); este crate usa el bloque de §14.2 que manda la revisión 1, y no se editó el documento de diseño por estar fuera de alcance.
