# ECO-2d — `ez-arbiter`: varios agentes principales, worktrees y delegación

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §14 y §15
(manda §15) y el informe de ECO-R1.** En paralelo, otro pez hace ECO-4c en `ez-team`: no
lo toques.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-arbiter/**`

## Cambios
1. **Agentes principales (§15.1).** `pub struct ElephantKey { pub space: String, pub elephant: String }`
   (id `e` + 8 hex) y `Caller::Elephant(ElephantKey)`. **Una tarea activa por agente
   principal**, no por espacio. Cola del ejecutor secuencial: una por agente principal.
2. **Worktree por agente principal (§15.1).**
   - `pub fn create_main_agent(&self, space: &str, name: &str) -> Result<MainAgent, Rejection>`:
     crea `git -C <carpeta> worktree add -b ez/<id> <carpeta>.ez-wt\<id>` desde el HEAD
     actual y devuelve `MainAgent { key, name, worktree, branch, base_branch }`
     (`base_branch` = rama actual de la carpeta, `git rev-parse --abbrev-ref HEAD`). Se
     guarda en `agentes.json` dentro de `data_dir`; `Arbiter::new` lo recarga.
   - Todas las reglas, rutas (`inside`), huellas, verificación y lanzamientos usan el
     **worktree** del agente principal como carpeta.
   - `pub fn remove_main_agent(&self, key: &ElephantKey)`: solo sin tarea activa y con el
     worktree limpio; `git worktree remove` (sin `--force`); si no, rechazo con el motivo.
   - `pub fn main_agents(&self, space: &str) -> Vec<MainAgent>`.
3. **Tarjeta de integración (§15.1).** Al cerrar una tarea (`done`) se abre una tarjeta
   `Integrate` con acciones `merge` y `keep`. `merge`: en la carpeta del espacio,
   `git merge --no-ff ez/<id> -m "EzAgent: integra «{título}»"`; si falla, `git merge --abort`
   y la tarjeta queda abierta con «No pude integrar: {motivo}. No cambié nada.»; si va bien,
   evento `integrated`. `keep`: cierra la tarjeta.
4. **Delegación (§15.5).** `FishKey` gana `elephant` y `parent: Option<String>`. Un
   subagente cuyo papel `can_delegate` puede usar `agent.delegate`, `fish.send` y `wait`
   sobre **sus propios hijos**: solo papeles de su `delegates_to`, dentro de la `count` del
   equipo (contando todos los vivos de ese papel) y sin pasar `profundidad_maxima`
   (`[validacion]`, 1–5, por omisión 3; profundidad 1 = hijos del agente principal). Sus
   hijos van en su misma ronda y no siguen el orden de etapas (heredan la del padre). Un
   subagente con hijos solo puede informar cuando todos informaron. `wait` de un subagente
   devuelve las novedades de sus hijos (cola por subagente). Etiquetas hc nuevas
   `ez_elephant` y `ez_parent`.
5. **Eventos** con `elephant` además de `space`; `snapshot` por agente principal.

## Pruebas
Con git real: dos agentes principales en el mismo espacio, cada uno con su worktree y su
tarea, sin verse los cambios (la huella de uno no detecta los del otro); `remove_main_agent`
con y sin tarea; integración que funciona y que choca (deja todo igual); delegación: papel
que no delega → rechazo; papel fuera de `delega_en` → rechazo; tope de cantidad y de
profundidad; padre que informa antes que sus hijos → rechazo; `wait` del padre recibe los
informes de sus hijos; recarga de `agentes.json`.

## Criterios de aceptación
fmt (`-p ez-arbiter`), clippy y test de `ez-arbiter` en verde. Informe en
`rust-migration/reports/ECO-2d-r1.md`.
