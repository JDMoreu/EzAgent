# HD-2 — ez-herdr: lanzar un agente como hijo de otro y ubicarlo en Herdr

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§2, §3,
§8).** En paralelo, HD-3 añade `ez-herdr/src/driver/choice.rs`: no toques `driver*`.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-herdr/src/herdr/actions.rs`: **solo** los campos nuevos de `RunAgent`, el
  desvío a la ruta nueva en `open_launch` (y lo mínimo para pasar los datos) y el
  `--name` de `launch_background` (ver «Corrección incluida»).
- Nuevo `rust/crates/ez-herdr/src/herdr/child.rs` (+ `child/…` si hace falta) y sus pruebas
  (`rust/crates/ez-herdr/src/herdr/tests/child.rs` o `child/tests.rs`).
- `rust/crates/ez-herdr/src/herdr.rs`: **solo** la línea `mod child;` y los `pub use`.
- Literales `RunAgent { … }` de otros crates que dejen de compilar: **solo** añadir
  `..RunAgent::default()` o los campos nuevos con su valor por omisión.

## Contrato (nombres exactos)
```rust
/// Dónde se ve un subagente en Herdr (§8 del diseño).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement { #[default] Tree, Tab, Right, Down }
impl Placement {
    /// "tree" | "tab" | "right" | "down" (sin distinguir mayúsculas); otro → None.
    pub fn parse(text: &str) -> Option<Placement>;
    pub fn as_str(self) -> &'static str;
}
/// Nombre válido para hc: minúsculas, `[a-z0-9_.-]`, empieza por letra o número,
/// máx. 32; lo demás se vuelve `-` (vocales con tilde → sin tilde, «ñ» → «n»), sin `-`
/// repetidos ni al borde; vacío → "agente". hc rechaza además los nombres con forma de
/// dirección (`t7`, `t9n`: `^t[0-9][0-9a-z]*$`): a esos se les antepone `a-`.
pub fn hc_slug(text: &str) -> String;
```
`RunAgent` gana (todos con valor por omisión `None`/`false`/`Tree`):
`parent: Option<&'a str>`, `hc_name: Option<&'a str>`, `labels: Option<Map<String, Value>>`
(valores texto), `show: Placement`, `notify: bool` (avisos de hc al padre),
`outlive_parent: bool`.

## Comportamiento
1. **Con `parent`** (no vacío) el lanzamiento va siempre por la ruta nueva, ignore
   `background`: `hc run --json --bg --parent <p> --name <hc_name o hc_slug(name)> --cwd
   <carpeta> [--no-notify si !notify] [--outlive-parent] --label agent=<tipo>` + una
   `--label k=v` por cada etiqueta (orden alfabético) `-- <argv>`; con entorno o
   `path_prepend`, `<argv>` es la misma envoltura que usa `launch_background`. **Nunca**
   `--no-parent` ni `tab.new`. Sin `parent`, todo sigue exactamente como hoy.
2. **Ubicación** (`show`), tras obtener la dirección y antes de `finish_run`:
   - `Tree`: no se llama a `herdr-app`.
   - `Tab`: `herdr-app do terminal.keep --terminal <t> --place tab`.
   - `Right`/`Down`: `herdr-app do pane.focus --terminal <padre>` y luego `terminal.keep
     --terminal <t> --place right|down`. Si `pane.focus` falla (el padre no tiene panel) →
     `Tab` con el aviso exacto «El agente padre no tiene panel; abrí el subagente en una
     pestaña.».
   - Si `terminal.keep` responde con un error que contiene «no running terminal», se
     reintenta cada 0,5 s hasta 15 s (Herdr tarda en ver la terminal nueva; verificado en
     vivo). Cualquier otro error, o agotar los reintentos → queda en el árbol con el aviso
     «Herdr no pudo ubicar el subagente (<motivo>); quedó en el árbol bajo su agente.».
     La ubicación **nunca** hace fallar el lanzamiento.
3. Tras ubicar, `hc get <t>` y el resultado (`base` de `launch_base`) añade `parent` (el
   real), `name` (el de hc), `placed` (`Placement::as_str` de lo que quedó), `labels` y,
   si `terminal.keep` respondió, `tab`/`pane`. Si el `parent` real no es el pedido,
   aviso «El padre de <t> es <real>, no <pedido>.».
4. Después, `finish_run` como siempre (espera al programa, fija lo pendiente y pega la
   tarea). Los avisos de la ubicación se suman a `warnings`.

## Pruebas (runtime falso que registra argv y responde lo preparado)
- argv de `hc run` con padre: `--parent`, `--name`, `--cwd`, `--no-notify` según `notify`,
  `--outlive-parent`, etiquetas ordenadas, sin `--no-parent` ni `tab.new`; envoltura de
  entorno cuando el perfil pone entorno (OpenCode).
- `Tree` no llama a `herdr-app`; `Tab` llama a `terminal.keep --place tab`; reintento:
  dos respuestas «no running terminal» y luego éxito → `placed = tab`; `Right` llama
  `pane.focus` y luego `keep --place right`; `pane.focus` con error → `tab` y el aviso;
  `herdr-app` con otro error → `tree` y el aviso, lanzamiento `ok`.
- El resultado trae `parent`, `name`, `placed`, `labels`.
- Sin `parent`: las pruebas existentes de lanzamiento siguen en verde sin cambios.
- `hc_slug` y `Placement::parse`.
- **Corrección incluida** (verificada en vivo por el elefante): el lanzamiento en segundo
  plano de hoy (`launch_background`) pasa a hc `--name "claude · voz"` y hc lo rechaza
  (`{"error":{"kind":"invalid_params","message":"bad name \"claude · voz\": use 1-32 of
  a-z 0-9 - _ . starting with a letter or digit, not shaped like an id (t7)"}}`). Pásale
  `hc_slug(label)` (la etiqueta legible no cambia) y añade su prueba.
- Nombre ocupado: hc responde `{"error":{"kind":"name_taken","message":"a live terminal is
  already named \"x\"", …}}` y sale con código 1. La ruta con padre devuelve ese caso como
  error con `"name_taken": true` en el mapa (HD-4 reintenta con otro número).

## Criterios de aceptación
fmt, clippy y test de `ez-herdr` (y de los crates que hayas tenido que tocar por los
literales) en verde. Informe en `rust-migration/reports/HD-2-r1.md`.
