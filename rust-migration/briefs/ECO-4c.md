# ECO-4c — `ez-team`: papeles abiertos (biblioteca y papeles de la tarea)

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §15 y el
informe de ECO-R1 (`rust-migration/reports/ECO-R1-r1.md`).** En paralelo, otro pez hace
ECO-2d en `ez-arbiter`: no lo toques.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-team/**`, salvo los contratos `src/roles.rs`, `src/contract.rs` y
  `src/roles/*.md`.

## Cambios
1. **Cargador de papeles.**
   `pub fn load_roles(task_dir: Option<&Path>, library_dir: &Path) -> Result<Vec<RoleDef>, TeamError>`:
   lee `*.md` de la carpeta de la tarea (`origin = Task`), de la biblioteca
   (`Library`) y los incluidos (`Builtin`). Con el mismo id gana el de mayor prioridad
   (tarea > biblioteca > incluido). Un archivo inválido es un error que nombra el archivo
   y el motivo (no se ignora en silencio). Carpeta inexistente = sin papeles de ese origen.
   Los archivos que empiezan por `_` no son papeles.
   `pub fn library_dir() -> PathBuf` = `%APPDATA%\EzAgent\hub\papeles` (con
   `ez_core::dirs::app_data_dir()`).
2. **Texto de `ez roles`.** `pub fn roles_text(roles: &[RoleDef]) -> String`: una línea por
   papel, ordenados por origen y luego por id:
   `- {id} · {nombre} · {ejecutor|verificador}{ · delega}{ · escribe pruebas}{ · mutaciones} ({incluido|biblioteca|tarea}): {descripcion}`.
   Y `roles_json(roles)` con los mismos datos.
3. **Validación del equipo (§15.3) con cualquier lista de papeles.** Además de lo que ya
   hace: cada papel del equipo existe en la lista; al menos un ejecutor y un verificador;
   un verificador no usa el modelo de **ningún** ejecutor (error) y se avisa si comparte
   familia habiendo otra disponible; `delega_en` solo en papeles que pueden delegar y solo
   con papeles del equipo; `cantidad` y `etapa` ≥ 1.
4. **Tarjeta del equipo.** Si el equipo usa papeles con `origin = Task`, añade al final:
   `Papeles nuevos de esta tarea:` y una línea por papel
   `- {id} ({nombre}, {ejecutor|verificador}): {descripcion}`.
5. **Pendiente de la verificación anterior:** una prueba que detecte quitar los
   verificadores del aviso de familia (la mutación que quedó sin detectar en ECO-4b).

## Pruebas
Cargador con las tres fuentes y precedencia; archivo inválido en la biblioteca → error con
su nombre; `_algo.md` ignorado; `roles_text` exacto para un conjunto de ejemplo; cada regla
nueva de validación; tarjeta con papeles nuevos de la tarea.

## Criterios de aceptación
fmt (`-p ez-team`), clippy y test de `ez-team` en verde. Informe en
`rust-migration/reports/ECO-4c-r1.md`.
