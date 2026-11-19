# ECO-6b — La ventana en árbol (como Herdr): espacios, agentes, subagentes y terminales

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §15.6 y
§15.9 (mandan), los briefs integrados `ECO-5b`, `ECO-7` y `ECO-6c` y sus informes, y los
comandos Tauri reales de `src/commands.rs`.** Esta versión del encargo sustituye a la
anterior de ECO-6b (columna fija de peces).

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/**` (sobre todo `ui/**`; en Rust solo lo necesario para la traza,
  la autocomprobación y el modo demostración).

## La ventana (§15.6)
1. **Izquierda: árbol plegable.** Espacios → agentes principales (nombre, `agente · modelo`,
   título de su tarea) → subagentes anidados por padre, sin límite de niveles. Cada nodo:
   punto de estado (`working` acento, `idle` tenue, `done` bien, `blocked` aviso, `error`
   o falló error), id y papel. Clic en un nodo: abre o enfoca su terminal. Abajo:
   «+ Agente principal» (en el espacio seleccionado) y «+ Espacio». Se refresca con
   `hub://changed` sin perder lo plegado ni la selección.
2. **Centro: pestañas de terminales.** Cada pestaña es un PTY con
   `[hc, "attach", dirección]` (en `--demo`, PowerShell como hoy). Cerrar una pestaña solo
   cierra la vista, no el agente. Botón «Dividir» para ver dos pestañas lado a lado (una
   división como máximo). Margen interior de 8 px; sin la barra blanca de abajo a la derecha.
3. **Barra de entrada** (debajo de las pestañas): escribe al agente de la pestaña activa
   con `agent_send` (texto crudo). Nota de voz igual que hoy; «Llamar» deshabilitado con
   título «Disponible pronto».
4. **Derecha: panel plegable** con «Traza» y «Tarjetas» de la tarea del agente principal
   seleccionado (o del agente principal del subagente seleccionado). Textos de la traza
   por tipo de evento (en `ui/js/texts.js`; un solo lugar):
   `task_proposed` «Tarea propuesta: {título}»; `task_approved` «Tarea aprobada»;
   `team_proposed` «Equipo propuesto»; `team_approved` «Equipo aprobado»;
   `round_started` «Ronda {n}»; `fish_launched` «{Papel} {id} en marcha ({agente} · {modelo})»;
   `fish_reported` «{id}: {pasa|no pasa} ({b} bloqueantes, {m} mayores)»;
   `fish_blocked` «{id} espera una respuesta ({motivo})»; `fish_failed` «{id} falló: {motivo}»;
   `verification` «Verificación: {aprobados}/{total} criterios»;
   `round_closed` «Ronda {n} limpia» o «Ronda {n} no limpia: {motivos}»;
   `paused` «En pausa: {motivo}»; `resumed` «Reanudada»; `task_closed` «Tarea terminada»;
   `task_stopped` «Tarea detenida»; `integrated` «Rama integrada»;
   `rejected` «Rechazado ({regla}): {mensaje}»; cualquier otro: su tipo tal cual.
   Tarjetas con sus botones: tarea y equipo «Aprobar» / «Rechazar» (comentario
   obligatorio); pausa «Reanudar» / «Dar más margen» / «Detener»; integración
   «Integrar en {rama}» / «Dejar en su rama». Si el árbitro rechaza, el mensaje aparece en
   la tarjeta.
5. **Formularios.** «+ Espacio»: nombre y carpeta. «+ Agente principal»: nombre, agente
   (de `inventory`), modelo (lista del agente o texto libre si está vacía) y esfuerzo
   opcional.
6. **Franja de estado** arriba si `runtime_status` está degradado, con el motivo.
7. **Nombres (§15.9):** ningún texto visible (tampoco los de respaldo del HTML) dice
   «elefante», «pez» o «peces».

## Modo demostración y autocomprobación
`--demo` arma un árbol de ejemplo sin hc: un espacio con dos agentes principales; el
primero con tres subagentes, uno de ellos con un hijo; el segundo sin subagentes; seis
eventos de traza y una tarjeta de equipo. Abre de entrada la pestaña del primer agente
principal y la de un subagente. `--selfcheck` informa
`{errores, nodos, pestañas, terminales_con_datos, eventos, tarjetas, textos_prohibidos}` y
sale con 0 si `errores` está vacío, `nodos` = 7 (1 espacio + 2 principales + 4
subagentes), `pestañas` = 2, `terminales_con_datos` = 2, `eventos` = 6, `tarjetas` = 1 y
`textos_prohibidos` = 0 (cuenta las apariciones de «elefante», «pez» y «peces» en el texto
visible del documento).

## Pruebas
Rust: traducción de cada tipo de evento a su texto; evaluación de la autocomprobación.
**Ejecuta** `--demo --selfcheck` y pega el JSON y el código de salida en el informe.

## Criterios de aceptación
fmt (`-p ez-hub`), clippy y test de `ez-hub` en verde y autocomprobación con 0. Informe en
`rust-migration/reports/ECO-6b-r1.md`.
