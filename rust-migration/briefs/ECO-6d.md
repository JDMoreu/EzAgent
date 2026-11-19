# ECO-6d — Ventana en árbol: correcciones visuales

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §15.6 y
el brief `ECO-6b` (ya integrado).** En paralelo, ECO-8 toca `src/runtime/engine.rs`,
`src/runtime/mod.rs` y `examples/`: no los toques.

## Lo que vio el agente principal del desarrollo (captura de `--demo`, 1416×939)
1. **Traza:** cada evento se parte palabra por palabra en una columna muy angosta
   («Desarrollador / dev-r1- / 1 en / marcha …»), aunque el panel mide ~330 px.
2. **Pestañas:** muestran el nombre del papel («Desarrollador»); con dos desarrolladores
   no se distinguen.
3. **Terminal:** queda una franja negra de unos 10 px entre el final de la terminal y el
   borde inferior de su área.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/ui/**`, `rust/crates/ez-hub/src/selfcheck.rs` y lo mínimo del modo
  demostración.

## Cambios
1. Traza: cada evento es un bloque a todo el ancho del panel; el texto se ajusta por
   palabras al ancho disponible. Encima del texto, la hora (`HH:MM:SS`) en tenue y pequeño.
2. Pestañas: etiqueta = id del nodo (`dev-r1-1`) para subagentes y el nombre para agentes
   principales; `title` con «{papel} · {agente} · {modelo}».
3. Terminal: la terminal llena su área (ajuste con el complemento *fit* tras el primer
   pintado, al cargar las fuentes y en cada cambio de tamaño); sin franja.
4. Bajo el nombre de cada agente principal, el título de su tarea en tenue (si hay).
5. Autocomprobación: añade al informe `traza_ancho_min` (el `clientWidth` mínimo de los
   textos de la traza, en px), `panel_ancho` (ancho del panel de traza),
   `etiquetas_pestañas` (lista) y `banda_terminal_px` (alto del área de la terminal menos
   el alto de la pantalla de xterm). Sale con 0 solo si además
   `traza_ancho_min ≥ 0,8 × panel_ancho − 40`, las etiquetas son `["Arquitecto", "dev-r1-1"]`
   (o los ids reales del demo) y `banda_terminal_px` < alto de una celda.

## Criterios de aceptación
fmt (`-p ez-hub`), clippy y test de `ez-hub` en verde y `--demo --selfcheck` con 0 (pega el
JSON). Informe en `rust-migration/reports/ECO-6d-r1.md`.
