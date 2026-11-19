# Informe ECO-6c-r1 — Motor de la ventana: varios agentes principales, árbol, papeles y delegación

## Resultado
- Brief/tarea: ECO-6c (ola 2; rama `work/ECO-6c`, copia de `eco/main`)
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/src/protocol.rs` (y 4 archivos de pruebas): `Caller::Elephant { space, elephant }` y `Caller::Fish { space, task, elephant, fish, attempt, parent }` para que el `Handler` arme `ElephantKey`/`FishKey` sin adivinar; pruebas adaptadas.
- `rust/crates/ez-hub/src/runtime/spaces.rs`: `espacios.json` se lee solo con `{id, nombre, carpeta}`; el `elefante` del formato viejo se conserva como sugerencia; `principales.json` guarda `{elefante → {agente, modelo, esfuerzo}}`; `elefantes.json` pasa a estar indexado por id de agente principal.
- `rust/crates/ez-hub/src/runtime/launcher.rs`: `launch_main` lanza en el worktree con `EZ_ROLE=main` y etiquetas `ez_elephant`/`ez_role=main`; `caller_from_labels` construye el `Caller::Fish` completo (elefante y padre); el escritor de instrucciones carga los papeles del worktree.
- `rust/crates/ez-hub/src/runtime/handler.rs`: traduce ambos `Caller` a las claves de ECO-2d; `agent.delegate` de un subagente → `Command::AgentDelegate` (del elefante → `FishNew`); `roles` y `team.propose` cargan `load_roles(Some(<worktree>/.ez/borrador/papeles), library_dir())` y propagan el error si un papel es inválido; `inventory` completa los modelos vacíos de `codex` con `discover_codex_models()` (doble inyectable); `card_action` y `worktree` añadidos a `ArbiterApi`.
- `rust/crates/ez-hub/src/runtime/events.rs`: `fish_key` arma la `FishKey` completa desde las etiquetas (`ez_elephant`, `ez_parent`); ya no exige `ez_role == fish` (ahora es el id del papel).
- `rust/crates/ez-hub/src/runtime/tree.rs` (nuevo): árbol por `ez_parent`, con nombres de papel, dirección hc y terminales «sin papel» colgadas por `parent` de hc.
- `rust/crates/ez-hub/src/runtime/engine.rs` (nuevo): `HubRuntime` crea/remueve agentes principales, Guarda tokens por id, `space_tree`, `node_address`, `agent_send`, `hub_state` (traza y tarjetas crudas del árbitro) y `card_action` (incluye `merge`/`keep`). Recolector de eventos del árbitro en hilo propio, sin candado durante la espera.
- `rust/crates/ez-hub/src/runtime/types.rs`: tipos serializables `TreeNode`, `SpaceTree` y `MainAgentInfo`.
- `rust/crates/ez-hub/src/runtime_commands.rs` (nuevo): comandos Tauri `main_agent_create`, `main_agent_remove`, `space_tree`, `node_address`, `agent_send`, `hub_state {space, elephant}`, `card_action`, `roles_list` e `inventory`; `space_create {nombre, carpeta}` y `spaces_list` quedan en `commands.rs`.
- `rust/crates/ez-hub/src/space.rs`: el archivo `espacios.json` escrito contiene solo id, nombre y carpeta; al leer se acepta el campo viejo `elefante` con un valor por omisión para no romper la vista de demostración.
- `rust/crates/ez-hub/src/lib.rs`: registra los diez comandos nuevos; separación de `commands.rs` (557 líneas), `runtime/mod.rs` (246) y `runtime/engine.rs` (402) para cumplir el límite de 600.
- `rust/crates/ez-hub/src/runtime/tests/**`: dobles y git real para dos agentes principales con dos worktrees y dos tokens, árbol anidado con terminal «sin papel`, `agent.delegate` de un pez con padre, papel de `.ez/borrador/papeles/` en `roles` y en `team.propose`, `inventory` con modelos de Codex descubiertos y lectura del `espacios.json` viejo.

## Decisiones
- `spaces.rs:67`: el campo `elefante` del formato viejo no se guarda en el espacio; se conserva en memoria como sugerencia del formulario.
- `spaces.rs`: los tokens se guardan por id de agente principal en `elefantes.json`; al arrancar se resuelve el espacio buscando el id en `agentes.json` del árbitro.
- `launcher.rs:278`: los papeles de la tarea se cargan desde `<worktree>/.ez/borrador/papeles` para que sus instrucciones usen el cuerpo definido por la persona.
- `launcher.rs:295`: las etiquetas del árbitro son la fuente de identidad del pez; si falta alguna se usan valores vacíos y el árbitro rechaza la acción.
- `engine.rs (collect_arbiter_events)`: se conservan los últimos 500 eventos de traza en memoria para que `hub_state` pueda servirlos sin crecimiento ilimitado.
- `commands.rs:157`: `space_create` acepta `agente` como parámetro opcional para no romper la vista anterior; la elección real se hace con `main_agent_create`. ECO-6b puede dejar de enviarlo.
- `space.rs`: `espacios.json` se escribe sin `elefante`; si el archivo viejo lo trae, se acepta y se rellena un elefante por omisión solo en memoria.
- `tree.rs`: el anidamiento usa `ez_parent` (id del pez), no la dirección hc; las terminales «sin papel» sí se cuelgan por el `parent` de hc.
- `hub_state` en demostración devuelve el `HubState` anterior; en modo normal devuelve los eventos crudos del árbitro (ECO-6b los convierte en texto).
- `roles_list` acepta `space`/`elephant` opcionales (superset del comando sin argumentos del brief) para poder resolver papeles de la tarea de cada agente principal.

## Desviaciones del brief o del diseño
- Ninguna de alcance: solo se tocaron `rust/crates/ez-hub/src/**` y `rust/crates/ez-ctl/src/**` más sus pruebas; no se tocó `ui/**`, el `Cargo.toml` raíz ni otros crates. `Cargo.lock` no cambió.
- Se mantuvieron los comandos heredados `space_elephant`, `space_fish`, `elephant_send` y `view_*` y `space.rs::Space` con `elefante` en memoria porque la vista actual (ECO-6b la reescribe) los necesita para `--demo --selfcheck`.
- `Caller::Elephant` de `ez-ctl` es una variante con campos (según el brief) mientras que la del árbitro es tupla; el `Handler` traduce entre ambas.

## Verificación
- `cargo fmt -p ez-hub -p ez-ctl --check`: exit 0 (`FMT_OK`).
- `cargo clippy -p ez-hub -p ez-ctl --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2.14s`, exit 0, sin avisos.
- `cargo test -p ez-hub -p ez-ctl`: **111 passed; 0 failed; 0 ignored** (26 de `ez-ctl`: 4 + 7 + 6 + 7 + 2; 85 de `ez-hub`: 84 de biblioteca + 1 de `neutral_names`).
- `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck <ruta>`: exit **0**, informe `{"errores":[],"cabecera":"Agente principal · PowerShell","terminales":3,"peces":2,"eventos":5,"tarjetas":1,"formulario_abierto":false}`.
- Límites: ningún `.rs` del cambio supera 600 líneas (máximo 557 en `commands.rs`) ni función nueva supera 60; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas; ningún candado retenido durante E/S, subproceso o espera.

## Pendiente o riesgos
- La traza de `hub_state` se reconstruye de los eventos vistos desde el arranque; los eventos anteriores al arranque del hilo recolector no aparecen. Es suficiente para el estado «por ahora» del brief.
- `hub_state` en modo normal y `roles_list`/`inventory` devuelven datos crudos: la conversión a texto y tarjetas con botones es trabajo de ECO-6b.
- No se probó contra un `hc` real (las pruebas usan dobles de `Terminals`, según el alcance). El arranque normal sigue dependiendo de `hc.exe` y de `ez.exe` junto a la ventana.
- `main_agent_create` no revierte el registro en `agentes.json` si falla la escritura de `principales.json` o el lanzamiento; la ruta de error devuelve el motivo al llamador.
- `space_create` con `agente` distinto de `opencode` ya no guarda esa elección en `espacios.json`: debe usarse `main_agent_create` para fijar agente, modelo y esfuerzo.
