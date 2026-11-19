# Informe ECO-R1 — Migración a papeles abiertos, etapas y nombres neutros

## Resultado
- Brief/tarea: ECO-R1
- Estado: completado

## Cambios
- `rust/crates/ez-team/src/role_text.rs` (nuevo): lector de papeles con cabecera TOML entre `+++`, con errores que nombran el archivo; `builtin_roles()` con los cinco incluidos (`include_str!`). Pruebas de cabecera válida, campo faltante, `tipo` inválido, verificador que delega, id inválido y propiedades de los incluidos.
- `rust/crates/ez-team/src/launch.rs`: `render_instructions(def, ctx)` (común + cuerpo + sección de delegación, 13 marcadores), `first_line(role_name, instrucciones)` y `main_first_line()` con los textos exactos de §15.9; fuera `elephant_first_line`.
- `rust/crates/ez-team/src/team.rs`: `[[papel]]` con `etapa` y `delega_en`; `parse`, `validate` y `card` reciben `&[RoleDef]`; validación de §15.3 (ejecutor y verificador, modelo de verificador distinto del de ejecutores, aviso de familia, `delega_en`); tarjeta con ` · etapa {n}`.
- `rust/crates/ez-team/src/lib.rs`: exporta los tipos nuevos y `builtin_roles`; retirado `Role`.
- `rust/crates/ez-arbiter/src/**`: `Role` → `RoleId` en tipos, estado, rondas, peces y pruebas; `Command::TeamPropose` gana `roles` y `Task` los congela; reglas por propiedad (`is_verifier`, `writes_tests`, `mutations`, `first_round_only`); orden por etapas (§15.4) con espera de etapas menores, bloqueo tras verificador iniciado y ronda solo de verificación; limpia según §15.4; prefijo `RoleId::short()`; `EZ_ROLE=sub`; textos con «agente principal» y «subagente».
- `rust/crates/ez-arbiter/tests/arbiter/open_roles.rs` (nuevo): papel propio `security_auditor` (verificador, etapa 4) bloquea la etapa 5 y su `fail` deja la ronda sucia.
- `rust/crates/ez-ctl/src/cli.rs`: `ez delegate <papel> --brief MD` (protocolo `agent.delegate`, args `{role, brief}`), `ez roles` (protocolo `roles`, args `{}`), ayuda y mensajes neutros.
- `rust/crates/ez-ctl/src/server.rs`: un `wait` libera su hueco solo después de responder, para que el siguiente no entre mientras el anterior sigue en curso.
- `rust/crates/ez-ctl/tests/waits.rs`: la prueba usa un único cliente HTTP compartido; sin subir el tope de 8 s.
- `rust/crates/ez-hub/src/runtime/**`: tipos nuevos, `agent.delegate`, `roles` (cinco líneas: id · nombre · tipo · descripción), `EZ_ROLE=main`, instrucciones desde `RoleDef` y primer mensaje del agente principal.
- `rust/crates/ez-hub/src/texts.rs`, `selfcheck.rs`, `commands.rs` y `ui/js/texts.js`: «Agente principal · {agente}», pestaña «Subagentes», etiqueta «Agente principal», avisos neutros; `HEADER_PREFIX = "Agente principal · "`.
- `rust/crates/ez-hub/tests/neutral_names.rs` (nuevo): recorre textos visibles de ez-arbiter, ez-ctl, ez-team y la vista y falla si aparece «elefante», «pez» o «peces» como palabra completa.

## Decisiones
- `// DECISION` (ez-team/team.rs): `cantidad > 1` solo en papeles ejecutores, para conservar «los verificadores informan una sola vez»; el mensaje de esfuerzo usa el nombre humano del perfil; el mismo modelo ya es error y no se repite como aviso; `×{cantidad}` solo se imprime si es mayor que 1.
- En la ronda 1 se exige informe `pass` de los papeles `first_round_only` de etapas anteriores, no solo que hayan informado: conserva el comportamiento anterior de las pruebas aunque §15.4 diga «informaron».
- `round_close` sigue exigiendo los informes de `qa` y `qa_validator` por id (R9), mientras que la condición de ronda limpia es property-based (§15.4).
- `HubRuntime` resuelve el `RoleDef` de las instrucciones desde `builtin_roles()`: las bibliotecas de papeles y equipos con papeles propios en la ventana son ECO-4c/ECO-6c, fuera de este encargo.
- Causa de la prueba inestable de esperas: cada `post` creaba un `reqwest::blocking::Client` nuevo (con su propio runtime e hilo); con 32 en paralelo, una o dos peticiones llegaban tarde y `entered` no alcanzaba 32 en 8 s. No era una espera perdida en el servidor. Se comparte un cliente en la bancada, se conserva el tope de 8 s y se corrige además el orden de liberación del hueco en el servidor.

## Desviaciones del brief o del diseño
- Ninguna de alcance. Se tocaron solo los cuatro crates permitidos; los contratos (`roles.rs`, `contract.rs`, `roles/*.md`, `skill.md`) quedaron intactos. `Cargo.lock` no cambió.
- Las firmas de `ez_team::team::parse/validate/card` y de `launch::render_instructions/first_line` cambian porque el brief introduce `RoleDef` y `InstructionContext`; todos los llamadores se actualizaron.

## Verificación
- `cargo fmt -p ez-team -p ez-arbiter -p ez-ctl -p ez-hub --check`: exit 0.
- `cargo clippy -p ez-team -p ez-arbiter -p ez-ctl -p ez-hub --all-targets -- -D warnings`: `Finished dev profile`, exit 0.
- `cargo test -p ez-team -p ez-arbiter -p ez-ctl -p ez-hub`: 238 pruebas, 0 fallos, 0 ignoradas (incluye 2 doctests y `neutral_names`).
- Prueba de esperas: 10 corridas seguidas de `cargo test -p ez-ctl --test waits thirty_two_waits_do_not_block_report_and_the_next_wait_is_503` en verde (antes: 4 de 10 fallaban con «solo entraron 30/31 esperas»).
- `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck <ruta>`: exit 0, informe con `cabecera: "Agente principal · PowerShell"`, `passed=true`.
- Ningún `.rs` supera 600 líneas; sin `todo!`, `unimplemented!`, `#[ignore]`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas en código nuevo.

## Pendiente o riesgos
- La tarjeta de demostración de `hub.rs` mantiene su formato antiguo (sin ` · etapa {n}`); la tarjeta real la genera `ez-team::team::card`, que sí incluye la etapa. Ajustar la demo si se quiere paridad visual total.
- `ez roles` responde hoy solo los cinco papeles incluidos; la biblioteca de la persona y los papeles de tarea llegan en ECO-4c/ECO-6c.
- La delegación real (`delega_en`, `parent`, `borradores`) queda implementada en el contrato y las instrucciones, pero su ejecución en la ventana es ECO-2d/ECO-6c.
- `RoleId::short()` recorta ids propios a 10 caracteres (`security_auditor` → `security_a`); es el comportamiento fijado en el contrato.
