# ECO-6b — Integración, parte 2: la vista conectada (elefante, peces, traza y tarjetas)

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` (manda
§14), los briefs integrados `ECO-2c`, `ECO-5b`, `ECO-6a` y `ECO-7`, y la API real de
`ez_hub::runtime` y de los eventos del árbitro (`EventKind`, `Card`) antes de empezar.**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/**`. Puedes añadir `src/runtime/timeline.rs`; el resto del módulo
  `runtime` solo para enchufarlo.

## Qué construir
1. **Traza y tarjetas desde el árbitro.** Hilo `ez-hub-arbiter-events` suscrito al
   árbitro; arranca leyendo `snapshot(space)` y descarta eventos con `seq` ≤ `last_seq`.
   Por espacio guarda los últimos 500 eventos de traza y las tarjetas abiertas.
   `hub_state(space)` devuelve `{fish, timeline, cards}` reales en modo normal (en `--demo`,
   lo de siempre). Textos de la traza (en `ui/js/texts.js` o generados en Rust, un solo
   lugar), por tipo:
   - `task_proposed` «Tarea propuesta: {título}»; `task_approved` «Tarea aprobada»;
     `team_proposed` «Equipo propuesto»; `team_approved` «Equipo aprobado»;
   - `round_started` «Ronda {n}»; `fish_launched` «{Papel} {pez} en marcha ({agente} · {modelo})»;
   - `fish_reported` «{pez}: {pasa|no pasa} ({b} bloqueantes, {m} mayores)»;
     `fish_blocked` «{pez} espera una respuesta ({motivo})»; `fish_failed` «{pez} falló: {motivo}»;
   - `verification` «Verificación: {aprobados}/{total} criterios»;
     `round_closed` «Ronda {n} limpia» o «Ronda {n} no limpia: {motivos}»;
   - `paused` «En pausa: {motivo}»; `resumed` «Reanudada»; `task_closed` «Tarea terminada»;
     `task_stopped` «Tarea detenida»; `rejected` «Rechazado ({regla}): {mensaje}».
   Cada tipo con su clase CSS (color del tema: bien, aviso, error, acento, tenue).
2. **Tarjetas.** Botones por tipo: tarea y equipo → «Aprobar» y «Rechazar» (comentario
   obligatorio); pausa → «Reanudar», «Dar más margen» y «Detener». Llaman a
   `card_action {card, action, comment}` → `arbiter.card_action`. Si el árbitro rechaza,
   el mensaje aparece en la tarjeta.
3. **Elefante.** Al elegir un espacio en modo normal: `space_elephant` → dirección →
   vista PTY con `[ruta de hc, "attach", dirección]`. Si falla, el mensaje en el panel.
   La barra usa `elephant_send` (texto crudo, sin pegado entre corchetes). La nota de voz
   sigue igual.
4. **Peces.** `space_fish` y `hub://changed`. Cada tarjeta de pez abre su PTY con
   `hc attach <dirección>` solo mientras la pestaña «Peces» está visible y la cierra al
   ocultarse. Chip de estado con el estado de hc.
5. **Nuevo espacio** con el inventario: comando `inventory` (del runtime) → el formulario
   ofrece agente (agentes disponibles), modelo (lista del agente; campo de texto si está
   vacía) y esfuerzo (opcional); nombre y carpeta como antes. Al crear: guardar y
   `space_elephant`.
6. **Estado del runtime.** Si `runtime_status` es degradado, una franja arriba con el motivo.
7. **Detalles visuales** que vio el elefante: margen interior de 8 px en las terminales y
   la barra blanca de abajo a la derecha en la terminal del elefante (quitar).

## Pruebas
Rust: traducción de cada `EventKind` a su texto y clase; descarte por `seq`; tarjetas que
se abren y se cierran; acciones válidas e inválidas por tipo (con el árbitro real y dobles
de terminales, como en ECO-6a). `--demo --selfcheck` debe seguir saliendo con 0.

## Criterios de aceptación
fmt (`-p ez-hub`), clippy y test de `ez-hub` en verde y la autocomprobación con 0. Informe
en `rust-migration/reports/ECO-6b-r1.md`.
