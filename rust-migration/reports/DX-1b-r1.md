# DX-1b — r1 · Arreglo del congelamiento del chat (fase A del diagnóstico DX-1)

> Encargo: **DX-1b**, la fase A del diagnóstico `DX-1-r1.md`. Se implementan los pasos
> 1, 3, 4, 5, 6, 7 y 8 del §4 del diagnóstico. **La virtualización (paso 2) no entra en
> esta fase**, por eso queda como pendiente (ver §6).
>
> Base: `work/DX-1` (`2619815`). Rama: `work/DX-1b`. Nada de la app de voz que la
> persona tiene abierta se tocó: no se cerró, no se le inyectó nada, no se le adjuntó un
> depurador, no se ejecutó ningún agente real y no se tocó la ventana de Herdr.

## Resultado
- Brief/tarea: DX-1b (arreglar el congelamiento del chat)
- Estado: completado
- Base: `work/DX-1` (`2619815`), crates `ez-hud`, `ez-app` y `ez-chat` (solo `version()`).

## Cambios

### `ez-hud` — el panel del chat

- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: `prepare_view` ya no maqueta el
  historial en el hilo que abre (paso 1); solo configura la vista. El primer cuadro de
  contenido lo pinta el hilo de pintura.
- `rust/crates/ez-hud/src/model/chat/view/entries.rs`: la caché de maquetación cambia de
  «400 entradas» a un **tope por bytes (32 MB de teselas)** con **expulsión LRU real** (la
  menos usada recientemente y no visible), la clave deja de clonar `msg.text` (longitud +
  hash FNV-1a de 64 bits + hash del tema + ancho) y **la altura de cada mensaje se cachea
  aparte de su tesela** (paso 3).
- `rust/crates/ez-hud/src/model/chat/view/draw.rs`: `draw_messages` solo **maqueta lo
  visible**; el resto solo aporta su altura cacheada (paso 3).
- `rust/crates/ez-hud/src/model/chat/view.rs`: contadores `layouts()` y
  `content_renders()` para poder medir el coste de un cuadro (paso 3/4).
- `rust/crates/ez-hud/src/win/chat/compose.rs`: la firma se parte en dos: `ContentSig`
  (el texto de los mensajes) y `Sig` (parpadeo, tic del orbe y geometría). El cursor se
  pinta **aparte**, como parche (paso 4).
- `rust/crates/ez-hud/src/win/chat/compose/raster.rs`: `composite_masked` y
  `composite_caret` (pintar el parche del cursor **excluyendo exactamente** su
  rectángulo) para que la paridad visual no cambie (paso 4).
- `rust/crates/ez-hud/src/model/chat/footer.rs`: el cursor se dibuja en su propia capa,
  con `caret_rect`/`caret_patch` (paso 4).
- `rust/crates/ez-hud/src/win/chat/window.rs`: `ChatPanel::render` **no pinta con el
  candado del panel tomado**: la capa de contenido se dibuja con la vista fuera del
  estado y solo se vuelve a tomar el candado para guardar el resultado (paso 5).
- `rust/crates/ez-hud/src/model/chat/view/types.rs` y
  `rust/crates/ez-hud/src/model/chat/view/test_support.rs`: el trait `ChatModel` gana
  `fn version(&self) -> i64`; la firma del panel lee la versión **sin copiar el
  historial** (paso 6).
- `rust/crates/ez-hud/src/win/chat/window/chrome.rs`: los tiempos de cuadro del chat se
  anotan con `FrameStats` (p50/p95/máx por minuto) y llegan al registro (paso 7).
- `rust/crates/ez-hud/src/win/layered.rs` y `rust/crates/ez-hud/src/win/layered/run.rs`:
  `LayeredWindow` expone `interval()`/`set_interval()` (atómico) y el hilo de la ventana
  presenta **a la cadencia que pide el panel** en vez de a 60 fps fijos (paso 8).
- `rust/crates/ez-hud/src/win/chat/window/chrome.rs`: `ChatWindow::set_interval` propaga
  la cadencia a la ventana *layered* (paso 8).
- `rust/crates/ez-hud/tests/chat_frame_cost.rs` (nuevo): las pruebas de regresión, rápidas
  y acotadas.
- `rust/crates/ez-hud/src/win/chat/window/tests.rs`: `a_slow_frame_does_not_block_the_input`.
- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: `opening_the_chat_does_not_lay_out_the_history`.

### `ez-app` — el rastro y el modelo

- `rust/crates/ez-app/src/app/main.rs`: `init_logging` escribe **también en un archivo
  con rotación diaria** (`%APPDATA%\EzAgent\logs\ezagent.log.<fecha>`, `tracing-appender`
  con `non_blocking` y el guard vivo) y conserva **siete días** (paso 7). La carpeta es un
  parámetro (`init_logging_in`) para que la prueba use una temporal.
- `rust/crates/ez-app/src/app/chat_wiring/model.rs`: `ChatModel::version` (paso 6).
- `rust/crates/ez-app/tests/log_rotation.rs` (nuevo): el registro existe en la carpeta
  pedida y la poda borra los registros viejos (y solo esos).
- `rust/crates/ez-app/examples/diag_chat_history_cost.rs` (nuevo): la medición larga,
  **fuera de `tests/`**.
- `rust/crates/ez-app/tests/diag_chat_history_cost.rs` (borrado): la medición ya no es
  una prueba de `cargo test`.

### `ez-chat`

- `rust/crates/ez-chat/src/session.rs`: `ChatModel::version` (paso 6). Nada más se tocó
  de este crate.

## Decisiones

- **`# DECISION` el alto se cachea aparte de la tesela.** El alto total de la
  conversación necesita el alto de cada mensaje, pero la tesela (la imagen) solo hace
  falta para lo que se ve. Sin separar las dos cosas, el tope por bytes seguiría
  expulsando y re-maquetando en cada cuadro, y la prueba de 450 mensajes no podría
  pasar. (`ez-hud/src/model/chat/view/entries.rs`)
- **`# DECISION` el cursor se pinta como parche aparte, con exclusión exacta de su
  rectángulo.** Es lo que permite que el parpadeo no invalide la capa de contenido sin
  cambiar un solo píxel de la paridad visual. (`ez-hud/src/win/chat/compose/raster.rs`)
- **`# DECISION` durante el render de la capa de contenido la vista sale del estado.**
  Es lo que hace posible pintar sin el candado del panel (paso 5). El coste: durante un
  repintado de contenido (raro y barato después del resto de los arreglos) la entrada
  **devuelve el evento en vez de esperar**, así que un clic en ese instante se puede
  perder. (`ez-hud/src/win/chat/window.rs`)
- **`# DECISION` `anim_tick` sale de la firma del cuadro.** La capa de contenido no lo
  usa (el orbe se pinta encima); dejarlo solo invalidaba la caché de contenido sin
  motivo. (`ez-hud/src/win/chat/compose.rs`)
- **`# DECISION` la poda del registro solo toca lo que el registro crea.** Se borran
  únicamente los archivos que empiezan por `ezagent.log` y con más de siete días; nada
  más de la carpeta. (`ez-app/src/app/main.rs`)
- **`# DECISION` el guard del `non_blocking` se olvida a propósito** (`std::mem::forget`):
  el archivo deja de recibir el registro en cuanto se suelta el guard, y la app vive
  más que el `main`. (`ez-app/src/app/main.rs`)
- **`# DECISION` la medición larga sale de `tests/`.** Vive en
  `ez-app/examples/diag_chat_history_cost.rs` porque su coste depende de la máquina; las
  pruebas de regresión son las rápidas de `ez-hud/tests/chat_frame_cost.rs`.

## Verificación

- `cargo test --release -p ez-hud` → **565 unitarias + todos los objetivos de integración
  en verde** (incluye `hud_chat_parity`, que compara los PNG dorados).
- `cargo test --release -p ez-app` → **349 unitarias + integraciones en verde**
  (incluye `log_rotation`).
- `cargo test --release -p ez-chat` → **83 unitarias + integraciones en verde**.
- `cargo fmt -p ez-hud -p ez-app -p ez-chat -- --check` → limpio (exit 0).
- `cargo clippy --release -p ez-hud -p ez-app -p ez-chat --all-targets -- -D warnings` →
  **sin avisos en estos crates** (exit 0; los únicos avisos son del `freetype-sys`
  vendorizado).
- Paridad visual: **sin relajar umbrales**. `parity/thresholds.toml` no se tocó y
  `hud_chat_parity` sigue en verde: el parche del cursor es idéntico píxel a píxel al
  oráculo.
- Ninguna prueba ni medición pasó de 1 GB de RSS (la medición larga lleva el corte de
  1 GB de `GetProcessMemoryInfo`).

---

## 1. Qué hace cada paso y qué se ve

| paso | qué cambió | efecto observable |
|---|---|---|
| 1 | `prepare_view` no maqueta el historial | abrir el chat **no bloquea el hilo principal** de la app: el maquetado lo hace el hilo de pintura |
| 3 | caché por bytes + LRU real + alto aparte de la tesela | con 450 mensajes el segundo cuadro **no vuelve a maquetar** nada; con 1 000 el cuadro pasa de 53 227 ms a 7,3 ms |
| 4 | `ContentSig` separada de `Sig`, cursor como parche | en reposo el contenido se repinta **0 veces por segundo** (antes 2, por el parpadeo) |
| 5 | el cuadro se pinta sin el candado del panel | un cuadro de 400 ms **no bloquea la entrada**: el clic se responde en < 100 ms |
| 6 | `ChatModel::version()` | la firma del panel **no copia el historial** (una copia menos por cuadro) |
| 7 | registro a archivo con rotación diaria + `FrameStats` del chat | un congelamiento deja rastro en `%APPDATA%\EzAgent\logs\ezagent.log.<fecha>` |
| 8 | la ventana *layered* presenta a la cadencia del panel | con el chat abierto la ventana deja de presentar a 60 fps fijos |

## 2. Medición antes/después

Mismo arnés, misma máquina, misma compilación (`release`). El «antes» es la base
`work/DX-1` (`2619815`, sin los arreglos de este brief) y el «después» es esta rama.

```text
== antes (base 2619815) ==                 == después (work/DX-1b) ==
  msgs  abre (frío)  2º render  3er render      msgs  abre (frío)  2º render  3er render    snapshot   RSS MB
    10     747.35 ms    7.12 ms    6.26 ms          10     739.03 ms    6.84 ms    7.47 ms     0.001 ms       35
   100    6933.98 ms    6.32 ms    6.02 ms         100    7434.60 ms    7.14 ms    7.45 ms     0.008 ms       35
  1000   68082.60 ms        —         —           1000   63539.03 ms    7.32 ms    8.12 ms     0.105 ms       37
                                              (el «antes» se cortó a 1 126 MB de RSS)
```

| medida | antes | después | |
|---|---|---|---|
| render en frío, 10 mensajes | 747,35 ms | 739,03 ms | = |
| render en frío, 100 mensajes | 6 933,98 ms | 7 434,60 ms | = (sigue siendo O(historial): ver §6) |
| render en frío, 1 000 mensajes | 68 082,60 ms | 63 539,03 ms | = |
| **2.º render, 1 000 mensajes** | **no medido** (la medición se cortó a 1 GB) | **7,32 ms** | el cliff de los 400 mensajes no existe |
| **RSS con 100 mensajes** | **288 MB** | **35 MB** | ×8,2 menos |
| **RSS con 1 000 mensajes** | **1 126 MB → corte** | **37 MB** | la medición ya no se corta |
| snapshot con 1 000 mensajes | no medido | 0,105 ms | |
| fotograma completo, 10 mensajes | 3,24 ms (RSS 54 MB) | 2,77 ms (RSS 55 MB) | = |
| fotograma completo, 100 mensajes | 2,93 ms (RSS 305 MB) | 2,99 ms (RSS 55 MB) | RSS ×5,5 menos |
| fotograma siguiente, 100 mensajes | 5,92 ms | 5,76 ms | = |
| cuadro con 1 000 mensajes (cliff de 401, `DX-1-r1.md` §3.2) | 53 227 ms | 7,3 ms | ×7 300 menos |
| repintados del contenido en reposo | 2 por segundo (el parpadeo invalidaba la firma) | **0 por segundo** | |
| clic durante un cuadro de 400 ms | bloqueado por el candado del panel | **< 100 ms** | |

## 3. Pruebas de regresión (rápidas y acotadas)

| prueba | qué demuestra |
|---|---|
| `ez-hud` `win::chat::window::lifecycle::tests::opening_the_chat_does_not_lay_out_the_history` | abrir el chat no maqueta nada en el hilo que abre (el contador `layouts()` de la vista sigue en 0; el primer cuadro sí maqueta) |
| `ez-hud` `win::chat::window::tests::a_slow_frame_does_not_block_the_input` | un cuadro lento (400 ms) no bloquea la entrada: `pointer_down` responde en < 100 ms |
| `ez-hud` `tests/chat_frame_cost.rs::the_layout_cache_does_not_empty_itself_past_the_old_cap` | con 450 mensajes (más que el tope antiguo) el segundo cuadro no vuelve a maquetar y es mucho más barato que el primero |
| `ez-hud` `tests/chat_frame_cost.rs::the_content_is_not_repainted_at_rest` | en reposo, con el cursor parpadeando, el contenido se repinta **1 vez** en el primer cuadro y **0** en el segundo (antes: 2 por segundo) |
| `ez-hud` `tests/chat_frame_cost.rs::the_frame_signature_does_not_copy_the_history` | la composición copia el historial exactamente tantas veces como el dibujo: la firma no lo pide |
| `ez-app` `tests/log_rotation.rs::the_log_is_written_to_a_file_in_the_given_folder` | el registro llega a un archivo de la carpeta pedida (carpeta temporal) |
| `ez-app` `tests/log_rotation.rs::the_old_logs_are_pruned_and_the_others_are_kept` | la poda borra el registro viejo y **solo** los archivos `ezagent.log*` |

## 4. La paridad visual no cambió

`parity/thresholds.toml` **no se tocó** y `hud_chat_parity` sigue en verde: el cursor se
pinta ahora como parche, pero el rectángulo del cursor se excluye **exactamente** del
contenido (misma aritmética que antes), así que la imagen final es idéntica a la del
oráculo. El caso de la paridad con `now = 0.0` (la primera fase del morph, sin parpadeo)
también sigue en verde.

## 5. Qué cambió en el comportamiento de la app

- Abrir el chat **no congela la app**: el maquetado del historial lo hace el hilo de
  pintura, no el hilo principal. La interfaz sigue respondiendo.
- Con el chat abierto **en reposo** no se repinta el historial: el parpadeo del cursor
  ya no invalida la capa de contenido (antes lo repintaba entero dos veces por segundo).
- La ventana del chat **deja de presentar a 60 fps fijos** y presenta a la cadencia que
  pide el panel (30 fps en reposo, 60 fps mientras algo se mueve).
- Un cuadro lento **no bloquea el clic ni el teclado**: la entrada se sirve durante el
  repintado.
- Un congelamiento **deja rastro**: `%APPDATA%\EzAgent\logs\ezagent.log.<fecha>` (rotación
  diaria, siete días) con los tiempos de cuadro del chat (p50/p95/máx por minuto).

## 6. Lo que queda fuera (la virtualización, paso 2 del diagnóstico)

**El render en frío sigue siendo proporcional al historial**: con 1 000 mensajes son
63,5 s (antes 68,1 s). La razón es que la vista necesita el **alto de cada mensaje**
para saber el alto total de la conversación, y ese alto se mide con el maquetado
completo (`build_entry`, que además rasteriza la tesela). Es exactamente el paso 2 del
diagnóstico —«calcular el alto sin rasterizar y maquetar solo lo que se ve»—, que este
brief excluye explícitamente.

Consecuencia práctica: con el historial que tenía la persona (33 mensajes, ~3 s de
maquetado) **el chat tarda unos segundos en mostrar su contenido la primera vez**, pero
la app **ya no se congela**: ese trabajo está en el hilo de pintura y el resto de la
interfaz responde. Con 1 000 mensajes el panel sigue tardando un minuto en llenarse; el
cuadro siguiente, en cambio, es de 7 ms.

Lo que sí se arregló de ese camino: la **memoria** (1 126 MB → 37 MB) y el **cliff de los
400 mensajes** (53 227 ms → 7,3 ms por cuadro).

## 7. Riesgos y notas

- **La entrada durante un repintado de contenido** puede perderse: la vista sale del
  estado mientras se pinta y la entrada devuelve el evento en vez de esperar. Es una
  ventana de milisegundos y solo durante un repintado de contenido (que ya no ocurre en
  reposo).
- **La tesela sigue siendo RGBA `f32`** (16 B por píxel). Lo que se hizo fue **guardar
  solo las teselas que caben en 32 MB** (las visibles), no convertir el formato a `u8`
  premultiplicado (div4 de la memoria). Era la otra mitad del paso 3 del diagnóstico.
- **La poda del registro borra archivos** de `%APPDATA%\EzAgent\logs`: solo los que
  empiezan por `ezagent.log` y con más de siete días. Es el comportamiento pedido
  («conservando 7 días»), pero es una excepción consciente a la regla de no borrar
  fuera de carpetas temporales: se limita al directorio de registro propio de la app y a
  los archivos que el propio registro crea.
- **La firma del contenido y la del cuadro** se calculan en dos momentos distintos: si
  entre la firma y la composición cambia el texto (el usuario escribe), ese cuadro
  repinta el contenido bajo el candado. Es un caso puntual.
- Durante el **morph**, el cursor ahora parpadea en vez de quedar congelado en la
  primera fase. La paridad con `now = 0.0` sigue en verde.
- **No se comprobó en la app en marcha**: la app de voz que la persona tiene abierta no
  se tocó (ni se cerró ni se le inyectó nada). La evidencia es la medición y las
  pruebas.

## 8. Anexo: comandos

```powershell
cd rust
cargo build --release -p ez-hud -p ez-app -p ez-chat --tests
cargo test --release -p ez-hud
cargo test --release -p ez-app
cargo test --release -p ez-chat
cargo fmt -p ez-hud -p ez-app -p ez-chat -- --check
cargo clippy --release -p ez-hud -p ez-app -p ez-chat --all-targets -- -D warnings

# medición (una sola vez, con el corte de 1 GB del arnés)
cargo run --release -p ez-app --example diag_chat_history_cost
```

La medición del «antes» se hizo en la base con el arnés todavía en `tests/`:

```powershell
cargo test --release -p ez-app --test diag_chat_history_cost -- --nocapture --test-threads=1
```
