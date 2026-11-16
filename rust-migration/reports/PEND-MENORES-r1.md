# PEND-MENORES — Nombres en `voice_control`, guardado de configuración fuera del hilo de pintura y confianza al abrir miembros (ronda 1)

## Resultado
- Brief/tarea: PEND-MENORES
- Estado: completado

Los tres pendientes quedan cerrados:

1. `voice_control` con `close_and_watch` pasa su `address` por el mismo
   resolvedor de nombres y la misma confirmación «¿Te refieres a…?» que
   `delegate_to_agent`.
2. `HudConfig::save`/`save_soon` ya no escriben a disco en el hilo que llama:
   encolan en un hilo escritor (`ezagent-config-save`) con el último valor por
   clave y sin retener ningún lock durante la escritura. `save_bar_position`
   solo pide el guardado desde el hilo de pintura.
3. Un miembro del equipo autónomo que se abre con el diálogo de confianza de su
   CLI conserva su terminal y el bucle lo aprueba con la transacción de
   `approve.rs` y la regla 10.2 (solo si la carpeta está dentro de `workdir`; si
   no, pausa).

## Cambios

- `rust/crates/ez-agent/src/tools_herdr/voice.rs`: `handle_voice` recibe el
  `HerdrPort` (firma mínima) y construye un `Names`; `close_and_watch` resuelve
  el `address` dictado con `Names::pick` (`NameKind::Terminal`) y la nota del
  nombre usado viaja en `name_note`. La dirección ya resuelta de la sesión
  (`last_address`) se usa tal cual.
- `rust/crates/ez-agent/src/tools_herdr/voice/tests.rs`: puerto falso
  `NamesPort` (solo devuelve terminales vivas), envoltorio `voice(...)` para las
  pruebas que no prueban nombres y tres pruebas nuevas: «TES» → «TEST» con
  confirmación sí, rechazo sin respuesta (lista «Terminales abiertas: «TEST»») y
  `last_address` sin resolver.
- `rust/crates/ez-agent/src/tools_herdr/builder.rs`: pasa el adaptador a
  `handle_voice`.
- `rust/crates/ez-agent/tests/tools_herdr_parity.rs`: una línea; el arnés de
  paridad pasa su puerto falso (sin candidatos, así que las salidas del Python
  no cambian).
- `rust/crates/ez-app/src/app/core/hud_bridge.rs`: `SaveQueue` (cola con
  coalescencia del último valor por clave, por ruta y clave, en orden de
  llegada) y su hilo `ezagent-config-save`; `HudConfig::save`/`save_soon`
  actualizan memoria con un cerrojo corto y encolan; `remember` sigue siendo
  solo memoria. Tres pruebas: escritura lenta que no bloquea al que llama,
  último valor por clave en orden y guardado real que conserva claves ajenas.
- `rust/crates/ez-hud/src/win/drop/ui/traits.rs`: `DropConfig::save_soon` (por
  omisión delega en `save`), documentado como no bloqueante.
- `rust/crates/ez-hud/src/win/drop/ui.rs`: `save_config_soon`.
- `rust/crates/ez-hud/src/win/drop/ui/appearance.rs`: `save_bar_position` usa
  `save_config_soon` (hilo de pintura) en vez de `save`.
- `rust/crates/ez-hud/src/win/drop/ui/tests/save.rs` (nuevo) y `.../tests.rs`:
  prueba con un doble cuyo `save` tarda 2 s: `render_frame` termina en menos de
  1 s y pide los cuatro valores por `save_soon`.
- `rust/crates/ez-app/src/app/autonomous/approve.rs`: `approve_launch` devuelve
  `Approval` y aplica 10.2/10.9 (Trust solo dentro de `workdir`; `Ask` registra
  la denegación y pausa); `approve_trust` lo reutiliza.
- `rust/crates/ez-app/src/app/autonomous/team.rs`: `TeamMember` lleva el estado
  del tablero; al cubrir a un miembro (y mientras siga `blocked`) se llama a
  `approve_launch`; una pausa corta el tic.
- `rust/crates/ez-app/src/app/autonomous/tests/team.rs`: `set_member_state` en
  el enjambre falso y dos pruebas con la pantalla real de Claude
  (`claude_trust_numbered.txt`): dentro de `workdir` se aprueba (`1`), fuera se
  pausa con el motivo.
- `rust/crates/ez-swarm/src/supervisor/launch.rs`: si `run_agent` vuelve con
  `ok=false` pero hay terminal y la pantalla muestra un diálogo, el miembro se
  guarda con su `address` y estado `blocked` (y su aviso inicial pendiente) en
  vez de marcarse `gone`; sólo se anuncia el fallo cuando no hay diálogo.
- `rust/crates/ez-swarm/src/supervisor/tests.rs` y `.../tests/launch.rs`: las
  pruebas de lanzamiento se mueven al submódulo `launch` (el archivo pasaba de
  600 líneas) y se añade la de la terminal que queda esperando la confianza.

## Decisiones

- `// DECISION` (`hud_bridge.rs`): un solo escritor por proceso (hay un HUD y un
  `voice.json`); así `HudConfig` conserva sus dos campos públicos y no hay que
  tocar `init.rs` ni las construcciones de las pruebas.
- `// DECISION` (`traits.rs`): `save_soon` por omisión llama a `save`, para no
  romper adaptadores que ya son baratos; el HUD de la app lo sobrescribe con la
  cola.
- `// DECISION` (`launch.rs` de ez-swarm): la seguridad la decide el bucle con la
  transacción de 10.2/10.9; el supervisor solo reconoce que hay un diálogo
  (`ez_herdr::permission::classify`) y conserva la terminal.
- `// DECISION` (`team.rs`): se reintenta la aprobación mientras el tablero dé
  al miembro por `blocked`, porque el diálogo puede tardar un tic en dibujarse.
- `approve_trust` ahora registra la denegación (`deny`/`deny` de auditoría) en
  vez de devolver `false` en silencio; el resultado visible del ejecutor único
  no cambia (`launch_failed` → pausa).
- `voice_control` no resuelve `last_address` (ya es una dirección real); solo el
  `address` explícito del modelo.

## Verificación

- `cargo fmt --all`: limpio.
- `cargo clippy --workspace --all-targets -- -D warnings`: limpio (solo los
  avisos de `vendor/freetype-sys`, ajenos a esta tarea).
- `cargo test --workspace`: **todo en verde, 0 fallos** (bibliotecas,
  integración y doc-tests; destacan `ez-app` 325 de biblioteca, `ez-hud` 509,
  `ez-agent` 251, `ez-swarm` 120 y los 172 de la paridad de `ez-agent`).
- `cargo test -p ez-app --features replay --test replay_parity`: PENDIENTE_REPLAY
- Pruebas nuevas: 3 de `voice_control` con nombres (10 en total en el módulo), 1
  de `render_frame` con guardado lento (509 en `ez-hud`), 3 de la cola de
  guardado (6 en `hud_bridge`), 2 del equipo con la pantalla real de Claude (10
  en `autonomous::tests::team`) y 1 del supervisor que conserva la terminal (21
  en `supervisor::tests`).

### Fixtures consumidos

- `rust/crates/ez-herdr/tests/permission/claude_trust_numbered.txt`: pantalla
  real de confianza de Claude, en las dos pruebas del equipo (ruta sustituida
  por `workdir` en la del caso aprobado) y en la del supervisor. 3 usos, 0
  fixtures de paridad (`parity/fixtures/` no tiene dominio de esta tarea).

### Pruebas portadas (`parity/test-map.csv`)

- Ninguna fila: los tres pendientes son mejoras sobre el Python acordadas por el
  usuario, sin fixtures de paridad; `test-map.csv` no tiene entradas de
  `tools_herdr.py` ni de la configuración del HUD.

## Pendiente o riesgos

- La cola de guardado no se vacía al cerrar el proceso: si la app termina en el
  mismo milisegundo que un guardado, esa última escritura se pierde (antes se
  perdía igual si el proceso moría escribiendo). No hay hook de cierre que la
  espere porque el escritor es del proceso, no del HUD.
- El estado `blocked` de un miembro con diálogo lo publica el supervisor; si
  `hc` no lo reportara como bloqueado, el bucle igualmente lo reintenta por el
  estado del tablero. Un diálogo que no sea de confianza (exec/edit) en el
  arranque de un miembro lo aprueba `approve_blocked` (10.7/10.8) si Herdr lo
  marca `blocked`; si no, el miembro se queda esperando y la tarea pausará por
  plazo.
- El resumen de la política (`describe_fn`) de `voice_control` sigue mostrando
  la dirección tal cual (comportamiento del Python): la resolución ocurre dentro
  del manejador.
- `HudConfig::remember` sigue sin escribir a disco (lo hace `save`/`save_soon`);
  si algún adaptador futuro usara solo `remember` para persistir, no guardaría.
- El escritor de la cola no toma el lock de `cfg` (la regla del brief), así que
  un «Guardar» de Ajustes de la app (que escribe `voice.json` entero con
  `cfg.read().save(...)`) puede intercalarse con una escritura de clave y
  dejar el archivo con los valores anteriores de esas claves. Antes el lock de
  `cfg` serializaba las dos escrituras; el riesgo es pequeño (el guardado de la
  cola son milisegundos y el de Ajustes es un clic explícito) pero real. Cerrarlo
  exige pasar también el guardado completo por la cola (fuera del alcance de
  este brief) o un lock de archivo.
- No se añadieron entradas a `rust-migration/python-quirks.md` (no está en la
  lista de archivos de la tarea): las tres son desviaciones deliberadas pedidas
  en el brief, documentadas aquí; conviene apuntarlas allí si se quiere el
  historial completo.
