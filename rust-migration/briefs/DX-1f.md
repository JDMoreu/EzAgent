# DX-1f — La animación de la barra al chat no se ve (la de vuelta sí)

**Lee `AGENTS.md`, `rust-migration/reports/DX-1-r1.md`, `DX-1b-r1.md`, `DX-1d-r1.md` y
`DX-1c-r1.md`.** Rama `work/DX-1f` desde `work/DX-1c` (virtualización del chat, que se
fusiona en `eco/main` antes que esto).

## Lo que ve la persona (19-nov noche, app de `eco/main` con DX-1b + DX-1d)
Al abrir el chat desde la barra (el «hub») **no hay animación**: el chat aparece de golpe.
Al cerrar el chat y volver a la barra **sí** se ve la animación. Antes de DX-1b la apertura
estaba animada (compruébalo: `git log` de `ez-hud/src/win/chat/` y la app de antes de
`acacccd`).

## Sospecha (compruébala, no la des por cierta)
DX-1b cambió la apertura: la vista se abre **sin maquetar**, el contenido se pinta fuera del
candado y la ventana presenta «a la cadencia del panel»; DX-1d deja una capa de 1×1 si la
maquetación falla. Si los cuadros de la animación de apertura se presentan antes de que
exista la primera capa de contenido (o la ventana se muestra cuando la animación ya
terminó), la apertura se ve como un salto.

## Qué se hace
1. **Diagnóstico con evidencia**: registra, durante la apertura y el cierre, cada cuadro
   presentado de la ventana del chat (marca de tiempo, tamaño o escala, alfa, si había capa
   de contenido). Compara apertura y cierre, y la apertura de antes de DX-1b. Pon las
   tablas en el informe.
2. **Arreglo**: la apertura debe animarse igual que el cierre (mismo número de cuadros
   intermedios y duración), sin volver a maquetar en el hilo de la interfaz (no deshagas
   DX-1b: el chat no puede congelar la app) y sin pánicos (DX-1d). Si el contenido aún no
   está, anima el marco/fondo y deja que el contenido aparezca cuando llegue.
3. **Prueba automática**: un modo de `ez-app/examples/diag_chat_smoke.rs` (p. ej. `anim`)
   que abra y cierre el chat con clics reales a la barra (como `real`) y compruebe que la
   apertura presenta al menos N cuadros intermedios (N el que tenga el cierre) dentro de la
   duración de la animación; falla si la apertura es un salto. Y una prueba de unidad del
   estado de la animación si el fallo está en el modelo.
4. Humo `chat`, `real` y `big` en release, como siempre: vivos y sin `PÁNICO`.

## Archivos
`ez-hud/src/win/chat/*`, `ez-hud/src/model/chat/*` solo lo de la animación,
`ez-app/examples/diag_chat_smoke.rs` y sus pruebas. Nada de `ez-stt` ni `ez-herdr`.

## Entrega
fmt, clippy y test release de `ez-hud` y `ez-app` en verde (ojo: si fallan pruebas de
medidas de texto, prueba `cargo clean --release -p freetype-sys`, hubo una caché vieja);
tablas antes/después y la salida del modo `anim`. Informe `rust-migration/reports/DX-1f-r1.md`,
commit en `work/DX-1f`. Compila en primer plano. No toques la app de la persona.
