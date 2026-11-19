# ECO-6c — Motor de la ventana: varios agentes principales, árbol, papeles y delegación

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §14 y §15
(manda §15), los briefs integrados `ECO-6a`, `ECO-R1`, `ECO-4c` y `ECO-2d` y sus informes,
y la API pública real de `ez-arbiter`, `ez-team`, `ez-ctl` y `ez_hub::runtime`.** No toques
la vista (`ui/**`): es ECO-6b.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/src/**` (sobre todo `runtime/**` y `commands.rs`) y su `Cargo.toml`.
- `rust/crates/ez-ctl/src/**` y sus pruebas, solo para el cambio de `Caller` del punto 3.

## Cambios
1. **Espacios y agentes principales (§15.1).** `espacios.json` guarda solo
   `{id, nombre, carpeta}`. Los agentes principales los crea el árbitro
   (`create_main_agent`, con su worktree); la ventana guarda aparte, en
   `%APPDATA%\EzAgent\hub\principales.json`, la elección de cada uno
   `{elefante → {agente, modelo, esfuerzo}}`. Los tokens de los agentes principales se
   guardan por id de agente principal (no por espacio). Formato viejo de `espacios.json`
   con `elefante`: se lee, se descarta ese campo y su elección queda como valor sugerido
   del formulario (`// DECISION`).
2. **Comandos Tauri** (los usará ECO-6b; documenta cada uno en el código):
   - `space_create {nombre, carpeta}`, `spaces_list`.
   - `main_agent_create {space, nombre, agente, modelo, esfuerzo}` → crea el agente
     principal y lo lanza en su worktree (`EZ_ROLE=main`, primera línea
     `main_first_line()`); `main_agent_remove {space, elephant}`.
   - `space_tree {space}` → árbol: agentes principales (id, nombre, agente, modelo, título
     de su tarea, estado de su terminal) y, debajo, sus subagentes anidados por `ez_parent`
     (id, papel con su `nombre`, agente · modelo, estado, dirección hc). Las terminales de hc
     cuyo `parent` es un nodo del árbol y no tienen etiquetas `ez` aparecen como hijos
     «sin papel».
   - `node_address {space, node}` → dirección hc (lanza el agente principal si hace falta).
   - `agent_send {address, text}` → `HcClient::send` con el texto crudo (la barra escribe al
     agente de la pestaña activa, principal o subagente).
   - `hub_state {space, elephant}` → traza y tarjetas de la tarea de ese agente principal
     (por ahora la lista cruda de eventos del árbitro; ECO-6b los convierte en texto).
   - `card_action {card, action, comment}` → `arbiter.card_action` (incluye `merge` y `keep`).
   - `roles_list`, `inventory` → los mismos datos que `ez roles` y `ez inventory`.
3. **`ez-ctl`:** `Caller::Elephant { space, elephant }` y `Caller::Fish` con `elephant` y
   `parent`, para que el `Handler` arme las claves de ECO-2d sin adivinar.
4. **`Handler`:**
   - Agente principal → `Caller::Elephant(ElephantKey)`; subagente →
     `Caller::Fish(FishKey)` con `elephant` y `parent`. Los subagentes pueden usar
     `agent.delegate`, `fish.send` y `wait` (el árbitro decide si su papel lo permite).
   - `roles`: `ez_team::load_roles(Some(<worktree>/.ez/borrador/papeles), library_dir())`.
   - `team.propose`: carga los papeles igual, valida con esa lista y envía
     `TeamPropose { seats, card, roles }` con las definiciones de los papeles del equipo.
   - `inventory`: si el perfil `codex` tiene `modelos` vacío, completa sus modelos con
     `ez_herdr::prelaunch::discover_codex_models()`.
5. **Lanzador y eventos:** el lanzador usa el `cwd` y las etiquetas que da el árbitro
   (worktree, `ez_elephant`, `ez_parent`) y pone `EZ_ROLE=sub`. El hilo de eventos de hc arma
   la `FishKey` completa desde las etiquetas y avisa al árbitro como hasta ahora.

## Pruebas
Con git real, el árbitro real y dobles de `Terminals`: crear dos agentes principales en un
espacio (dos worktrees, dos tokens); `space_tree` anida por `ez_parent` y cuelga una
terminal «sin papel» por `parent`; el `Handler` traduce un `agent.delegate` de un subagente
que delega; `roles` incluye un papel de `.ez/borrador/papeles/`; `team.propose` con un papel
de la tarea llega al árbitro con su definición; `inventory` con modelos de Codex
descubiertos (doble); lectura del `espacios.json` viejo. `--demo --selfcheck` sigue en 0.

## Criterios de aceptación
fmt (`-p ez-hub -p ez-ctl`), clippy y test de `ez-hub` y `ez-ctl` en verde. Informe en
`rust-migration/reports/ECO-6c-r1.md`.
