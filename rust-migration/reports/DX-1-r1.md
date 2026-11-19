# DX-1 — r1 · Diagnóstico: el chat congela la app de voz

> Encargo: **solo diagnóstico y propuesta**. No se cambió código del producto; lo único
> que se añadió a la rama son las pruebas de medición propias
> (`rust/crates/ez-app/tests/diag_chat_history_cost.rs`) y este informe.

## Resultado
- Brief/tarea: DX-1 (la app de voz se congela al abrir el chat)
- Estado: completado
- Base: `eco/main` (`work/DX-1`), la app de voz `ez-app` + `ez-hud` + `ez-chat` sin cambios.

## Cambios
- `rust/crates/ez-app/tests/diag_chat_history_cost.rs` (nuevo): prueba de medición
  propia del coste del panel del chat según el tamaño del historial, con el parser
  real de `ez-chat` y la composición real de `ez-hud`. Incluye un **corte de 1 GB de
  RSS** para no vaciar la máquina. No toca código del producto.
- `rust-migration/reports/DX-1-r1.md` (nuevo): este informe.

## Verificación
- `cargo test --release -p ez-app --test diag_chat_history_cost` → **3 pruebas en
  verde** (`test result: ok`), una por vez, con `--test-threads=1`.
- `cargo fmt -p ez-app -- --check` → limpio (exit 0).
- `cargo clippy --release -p ez-app --all-targets -- -D warnings` → **sin avisos**
  (exit 0).
- Nada de la app en marcha se tocó: no se cerró `ezagent-voice.exe`, no se le inyectó
  nada, no se le adjuntó un depurador, no se ejecutó ningún agente real y no se tocó la
  ventana de Herdr.

---

## 1. Resumen

El panel del chat **recalcula su contenido entero, en cada cuadro que "algo cambia", y
ese trabajo se hace con el candado del panel tomado**. El coste de un cuadro es
proporcional a **todo el historial**, no a lo que se ve. Tres disparadores
independientes invalidan la caché de contenido:

| disparador | dónde | frecuencia |
|---|---|---|
| parpadeo del cursor | `compose.rs:91` (`blink`) | 2 Hz, incluso en reposo |
| tic de la animación del orbe | `compose.rs:105` (`anim_tick`) | 30 Hz mientras el modelo contesta |
| versión del modelo | `compose.rs:93` (`version`) | por fragmento de texto en streaming |

Y al **abrir** el chat, el historial entero se maqueta de una tirada, **en el hilo que
abre** (`prepare_view` → `view.render()`, `win/chat/window/lifecycle.rs:113`). Con el
atajo ese hilo es el **principal de la app**, que es el mismo que corre `UiLoop::run`
(`app/startup.rs:180-186`): mientras maqueta, no se atienden atajos, ni el menú, ni los
eventos, ni los clics de la barra.

Medido (§3): **maquetar y rasterizar un mensaje cuesta ~89 ms**. Abrir el chat con
33 mensajes bloquea la app ~3 s; con 100, ~9 s; con 1 000, ~88 s.

Además, la **caché de maquetación está topeada en 401 entradas y expulsa la más
antigua**: a partir de 402 mensajes, *cada cuadro* vuelve a maquetar el historial
entero (y a releer y decodificar las imágenes del disco). Medido: el cuadro pasa de
**8,9 ms** (401 mensajes) a **53 227 ms** (1 000 mensajes): ×5 950.

El efecto es que un cuadro del chat puede tardar medio segundo o más. Como ese cuadro
se pinta **con el candado `PanelState` tomado** (`win/chat/window.rs:332`), el hilo de
la ventana del chat (el que atiende ratón y teclado), el hilo de la barra y el hilo
principal de la app (el bucle de la interfaz, `UiLoop`) se quedan **esperando el mismo
candado**: la app deja de responder a clics, minimizar, silenciar y escribir — mientras
el audio, que vive en otros hilos, sigue escuchando. De ahí el síntoma exacto que
describe la persona.

Ordenadas por probabilidad:

1. **Abrir el chat maqueta el historial entero, en el hilo principal de la app**
   (~89 ms por mensaje) — confianza **alta**, medido (§3.1). Es la causa directa de
   «al abrir el chat se congela».
2. **La caché de maquetación se vacía sola por encima de 401 mensajes** — confianza
   **alta**, cliff medido (§3.2). Es la causa de «empeora cuando la app lleva mucho
   tiempo abierta»: la conversación crece y el coste crece con ella.
3. **El cuadro entero se pinta con el candado del panel tomado** — confianza **alta**
   (es el multiplicador que convierte un cuadro lento en "app congelada").
4. **La capa de contenido se invalida a 2–30 Hz por el parpadeo del cursor y el tic del
   orbe** — confianza **alta** (código), aunque con un historial corto el efecto es
   pequeño (8 ms).
5. **Impuesto constante: cada ventana *layered* presenta a 60 fps** — confianza **media**
   (medido en vivo: 36 % de un núcleo por ventana; §3.3).
6. **Descartado**: inundación de `UiLoop::post`, auto-bloqueo en `poll`/`teardown`,
   fuga de objetos GDI/USER.

---

## 2. El camino de un cuadro (con números de línea)

```
cromo.rs:241  ChatWindow::paint_loop          (hilo "ezagent-chat-paint")
  └ window.rs:328  ChatPanel::render()
      ├ window.rs:332   self.state.lock()      ← candado TOMADO durante TODO el cuadro
      ├ window.rs:361   update_live()          → ui.chat_pose() → bar.lock()
      ├ window.rs:383   compose::stable()
      │   ├ compose.rs:351  signature(view, now, ticking=true)
      │   │   └ compose.rs:93   view.model.snapshot().0   ← COPIA TODO EL HISTORIAL
      │   ├ compose.rs:355  compose(scene, 1.0, false, window, content)
      │   │   └ compose.rs:418  signature(view, now, ticking=true)
      │   │       └ compose.rs:424  view.render(now)      ← REPINTA TODOS LOS MENSAJES
      │   │           └ view.rs:264  draw_messages()
      │   │               └ draw.rs:23-24  messages.iter().map(|m| self.entry(m, now))
      │   │                   └ entries.rs:99  build_entry()  ← maquetación + imágenes
      │   └ compose.rs:358  base.clone()                  ← clona el fotograma entero
      └ window.rs:386  compose::compose(...)   (fuera de `Open`: el morph)
```

Cada fila de ese diagrama tiene su propio multiplicador, y todos actúan sobre **el
historial completo**:

- **`signature()` copia el historial entero para leer un número.**
  `compose.rs:93` hace `view.model.snapshot().0`; `snapshot()` devuelve
  `(i64, Vec<ChatMessage>)` con **todo el texto clonado**
  (`ez-chat/src/session.rs:379-391` → `inner.messages.clone()`), y el adaptador de la
  app vuelve a convertir cada mensaje (`ez-app/src/app/chat_wiring/model.rs:33-48`).
  Se hace **dos veces por cuadro** (una en `stable`, otra en `paint_content`).

- **`draw_messages` maqueta *todos* los mensajes, no solo los visibles.**
  `draw.rs:23-24` construye un `Entry` por cada mensaje del historial antes de
  comprobar qué queda en el viewport (`draw.rs:66` solo descarta el *pintado*, el
  trabajo ya está hecho).

- **`entry()` clona el texto de cada mensaje en la clave de caché, en cada cuadro.**
  `entries.rs:80-86`: `CacheKey { text: msg.text.clone(), ... }` → una copia de todo el
  texto del historial por cuadro, solo para comparar.

- **La caché de maquetación está topeada en 401 y expulsa el id más pequeño**
  (`entries.rs:100-106`), es decir, **el mensaje más antiguo**. Como `draw_messages`
  los recorre del más antiguo al más nuevo, a partir de 402 mensajes el primer
  mensaje ya no está en la caché → se vuelve a maquetar → al insertarlo se expulsa el
  siguiente más antiguo → y así en cada cuadro. §3.2 mide el cliff.

- **Las imágenes se releen y decodifican del disco en cada remaquetación.**
  `paint/chat/render_rich/images.rs:75` → `ctx.host.load_image()` →
  `ez-app/src/app/chat_wiring/host.rs:160-176` → `std::fs::read` + `imaging::decode`.
  No hay caché de la imagen decodificada.

- **El cuadro entero se pinta con `PanelState` tomado** (`win/chat/window.rs:332`), y
  el hilo de la ventana del chat pide el mismo candado para *cualquier* entrada
  (`win/chat/window/input.rs:30, 77, 113, 126…`). Con un cuadro de 300 ms, la ventana
  está «no responde» ese tiempo.

- **Abrir el chat maqueta el historial entero, en el hilo que abre.**
  `win/chat/window/lifecycle.rs:105-114` (`prepare_view` → `view.render(monotonic())`).
  Con el atajo (`Ctrl+Alt+T`) ese hilo es el principal de la app (`events.rs:95-98` →
  `toggle_chat` → `drop/ui/appearance.rs:171` → `chat.toggle()` → `ChatPanel::open`),
  y el hilo principal es el que corre `UiLoop::run` (`app/core.rs`, `ui_loop.rs`).
  Mientras maqueta, no se atienden atajos, ni el menú, ni los eventos: **la app entera
  está congelada, pero el audio sigue**.

---

## 3. Mediciones

### 3.1 Método

Prueba propia: `rust/crates/ez-app/tests/diag_chat_history_cost.rs` (release, en esta
rama). Reproduce el camino real: el parser de markdown de `ez-chat`, el
`ChatView` de `ez-hud` y la composición del panel (`compose_cached`), con el tamaño
de panel por defecto (900×640 lógicos a escala 1,0).

```powershell
cd rust
cargo test --release -p ez-app --test diag_chat_history_cost -- --nocapture --test-threads=1
```

<!--MEDICIONES-->
**Salida (release, esta máquina con la app de la persona en marcha):**

```
  msgs  abre (frío)    2º render   3er render     snapshot   RSS MB
    10    1061.20 ms       8.16 ms       8.00 ms      0.001 ms       34
   100    9050.04 ms       8.06 ms       7.84 ms      0.014 ms      286
  1000   82183.15 ms            —            —            —     1124   (CORTADA: 1024 MB de RSS)
```

Las otras dos pruebas de la misma ejecución:

```
   10 mensajes: fotograma siguiente     6.39 ms
  100 mensajes: fotograma siguiente     8.18 ms

ventana del panel: 472×672 px
   10 mensajes: fotograma completo     3.32 ms (RSS 54 MB)
  100 mensajes: fotograma completo     3.31 ms (RSS 305 MB)
```

**Primera corrida (antes de poner el tope de 1 GB), con más tamaños:**

```
  msgs  abre (frío)    2º render   3er render     snapshot
    10     954.38 ms       8.73 ms       6.92 ms      0.002 ms
   100    8573.85 ms       8.46 ms       9.13 ms      0.014 ms
   300   26666.68 ms       8.43 ms       8.33 ms      0.033 ms
   400   36529.79 ms      11.17 ms      10.63 ms      0.060 ms
   401   36682.14 ms       9.48 ms       8.94 ms      0.057 ms
  1000   89620.52 ms   56350.48 ms   53227.72 ms      0.150 ms
```

**Episodio de memoria (anotado por petición del agente principal).** La primera
corrida incluía además el caso de 5 000 mensajes. Con ese caso el proceso de la
prueba pasó de 1 GB a **7,4 GB de RSS** a los 12,6 min de ejecución (medido con
`Get-Process`), y el agente principal lo detuvo cuando llegó a **11,4 GB**, porque
otros procesos se estaban quedando sin memoria. El harness quedó así:
`diag_chat_history_cost` ya lleva un **corte de 1 GB de RSS**
(`GetProcessMemoryInfo` sobre el proceso propio, `MEMORY_LIMIT_MB`), corta la
medición de ese tamaño y sigue con el siguiente; y **no se vuelve a lanzar el caso
de 5 000 mensajes**. La tabla de arriba sale de la corrida con el tope ya puesto
(10 / 100 / 1 000).

**Lectura de los números**

1. **Abrir el chat cuesta ~89 ms por mensaje**: 1,07 s con 10 mensajes, 8,87 s con
   100, 88,7 s con 1 000. Es una recta, y ocurre **en el hilo que abre el chat**
   (`prepare_view` → `view.render()`, `win/chat/window/lifecycle.rs:113`). Con el
   atajo ese hilo es el principal de la app: 9 s de bloqueo son 9 s de app
   congelada (y el audio sigue).
2. **Con la caché completa (≤ 401 mensajes) el cuadro siguiente cuesta ~8,5 ms**:
   el panel en reposo va bien, y por eso el defecto **no se ve con conversaciones
   cortas** (y por eso `P-FIX-CHATMORPH` lo midió con 3 mensajes y dio por bueno el
   morph).
3. **Por encima de 401 mensajes el cuadro se dispara**: 401 → 8,94 ms;
   1 000 → **53 227 ms** (× 5 950). Un solo cuadro de 53 s: la ventana no puede
   atender ni un clic.
4. **`snapshot()` no es el problema**: 0,15 ms con 1 000 mensajes. La copia del
   historial es barata **a este tamaño**; lo caro es **maquetar y rasterizar**
   (89 ms por mensaje: parsear markdown dos veces, medir el texto tres veces y
   rasterizar la tesela RGBA f32 de ~0,9 MB).
5. **La composición del panel es barata; el contenido no**: con la caché de
   contenido caliente el fotograma completo cuesta **3,3 ms** con 10 y con 100
   mensajes (mismo número). Lo que cuesta 8–90 ms es **`view.render()`**, es decir
   re-maquetar los mensajes. Por eso el arreglo tiene que ir a la maquetación y a la
   invalidación, no a la composición.
6. **La memoria es proporcional al historial**: con 100 mensajes la sola vista ya
   ocupa **286 MB de RSS** en el harness (305 MB con el fotograma completo). La tesela
   de cada mensaje es `Layer` (RGBA **f32**, 16 bytes por píxel): 404×134 px ≈ 866 KB
   por mensaje.

### 3.2 El cliff de los 400 mensajes

El tope está en `ez-hud/src/model/chat/view/entries.rs:100-106`:

```rust
let entry = Rc::new(self.build_entry(msg));
if self.cache.len() > 400 {
    // DECISION: Python evicts the oldest insertion; ids grow with the
    // conversation, so the smallest id is the closest equivalent.
    if let Some(first) = self.cache.keys().next().copied() {
        self.cache.remove(&first);
    }
}
self.cache.insert(msg.id, Cached { key, entry: Rc::clone(&entry), at: now });
```

La caché guarda como mucho 401 entradas y **expulsa el id más pequeño** (el mensaje
más antiguo). Como `draw_messages` (`draw.rs:23-24`) recorre los mensajes del más
antiguo al más nuevo, en cuanto hay 402 mensajes el primero ya no está en la caché →
se remaqueta → al insertarlo se expulsa el siguiente más antiguo → y así en **cada
cuadro**. La medición lo confirma: el cuadro pasa de 8,94 ms (401 mensajes) a
53 227 ms (1 000 mensajes).

Efectos colaterales del mismo bug:

- Cada remaquetación **relee y decodifica las imágenes del disco**
  (`paint/chat/render_rich/images.rs:75` → `ez-app/src/app/chat_wiring/host.rs:160-176`
  → `std::fs::read` + `imaging::decode`), sin caché de la imagen decodificada.
- La tesela de cada mensaje se vuelve a pintar y a convertir a `f32` premultiplicado
  (`entries.rs:392-395`), 866 MB por fotograma con 1 000 mensajes.

### 3.3 El proceso que la persona tiene abierto (solo lectura)

No se cerró, no se adjuntó ningún depurador y no se inyectó nada. Lecturas con
PowerShell (`Get-Process`, `GetGuiResources`, `EnumWindows`, `GetThreadDescription`).

```
Id            : 22432  (ezagent-voice.exe)
StartTime     : 19/11/2026 9:46:18
09:56:29  WS 467,2 MB | privado 486,5 MB | CPU 511 s   | 37 hilos | 609 handles
10:45     WS 464,1 MB |                      CPU 950 s   | 38 hilos
GDI 14 | USER 39          ← sin fuga de objetos GDI/USER
```

**El chat estaba abierto durante las lecturas** (`EnumWindows`):

```
hwnd=0x4F3042A vis=True class='ezagent-layered-window' title='EzAgent' rect=466x736+1458+308
hwnd=0x11069E vis=True class='ezagent-layered-window' title='drop'    rect=221x70+1681+972
```

**Consumo de CPU por hilo** (delta de 10 s con `GetThreadDescription`, solo los
hilos que pasan del 0,2 %):

```
  Tid   Name             CpuPct
 12012  ezagent-drop      36,1     ← hilo de la ventana de la barra
  8672  ezagent-EzAgent   35,6     ← hilo de la ventana del chat
```

Los otros 36 hilos, por debajo del 0,2 %. O sea: **con el chat abierto, la app gasta
≈ 1 núcleo entero entre los dos hilos de ventana**, y el proceso pasó de 511 s a
950 s de CPU en 7 minutos (≈ 104 % de un núcleo). La memoria, en cambio, **no crece**
(467 → 464 MB): no hay fuga; parte es el modelo de TTS en memoria
(`kokoro-v1.0.int8.onnx` 88 MB + `voices-v1.0.bin` 27 MB, con `onnxruntime.dll`
cargado) y parte las teselas del chat.

**El historial en vivo es pequeño**: `%APPDATA%\EzAgent\history\chat.jsonl` tiene 33
mensajes (9 178 caracteres), todos de la sesión 16 (la actual), en 16 sesiones. Con
33 mensajes la caché de maquetación **no** se vacía, así que **el defecto del cliff no
es el que la persona está viendo ahora mismo**; lo que sí está presente en su sesión
es el coste de apertura (~3 s para 33 mensajes, medido con la misma recta: 89 ms por
mensaje) y el gasto de CPU de los hilos de ventana.

**No hay rastro en archivo**: `ez-app/src/app/main.rs:106` (`init_logging`) escribe
**solo por stderr**; en `%APPDATA%\EzAgent\logs\` solo hay `latency.jsonl` (451
líneas) y `audit.jsonl`, que no registran tiempos de cuadro. Por eso no se puede
ver el congelamiento en los logs: hay que añadir el registro a archivo (paso 6 del
arreglo).


---

## 4. Arreglo propuesto

Ordenado por relación entre esfuerzo y efecto sobre el síntoma reportado. Cada paso
es un diff pequeño y se puede hacer por separado.

### Paso 1 — No maquetar el historial en el hilo que abre (≈ 10 líneas)

**Archivo:** `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs:105-114`.

- `prepare_view` llama a `view.render(monotonic())`: **maqueta el historial entero en
  el hilo que abre** (el principal de la app si se abre con el atajo). Medido: 89 ms
  por mensaje → 3 s con 33 mensajes, 9 s con 100.
- Con el paso 2 (virtualización) esto deja de ser un problema: solo se maqueta lo
  visible. Mientras tanto, el arreglo mínimo es **no maquetar nada aquí**: el primer
  fotograma lo pinta el hilo de pintura, que es donde debe estar ese trabajo.
- Efecto inmediato: abrir el chat deja de congelar la app entera.

### Paso 2 — No maquetar lo que no se ve (virtualización)

**Archivos:** `rust/crates/ez-hud/src/model/chat/view/draw.rs`,
`rust/crates/ez-hud/src/model/chat/view/entries.rs`.

- `draw_messages` necesita el alto de cada mensaje para saber el desplazamiento total.
  Se puede calcular el alto **sin rasterizar** (una medición de texto por bloque, sin
  pintar) y maquetar (rasterizar) solo los mensajes del viewport ± un margen.
- Efecto: el coste de un cuadro deja de depender del historial (hoy es lineal) y el
  de abrir el chat también.

### Paso 3 — Que la caché de maquetación no se vacíe sola

**Archivo:** `rust/crates/ez-hud/src/model/chat/view/entries.rs:100-106`.

- Cambiar el tope de «400 entradas» por un tope **por bytes** (p. ej. 32 MB de
  teselas) y expulsar la **más antigua no visible** (LRU real), no el id más
  pequeño. Con 402+ mensajes, la caché deja de vaciarse sola (hoy: ×5 950 de
  coste por cuadro).
- `entries.rs:80-86`: no clonar `msg.text` en la clave. Usar una huella barata
  (id + longitud + un hash de 64 bits del texto) o `Arc<str>` compartido con el modelo.
- `entries.rs:23-27`: la tesela de cada mensaje es `Layer` = RGBA **f32** (16 bytes por
  píxel). Una respuesta de 5 líneas ocupa ~0,9 MB. Con 400 mensajes, ~350 MB solo en
  teselas. Pasar a `u8` premultiplicado (4 bytes por píxel) y/o guardar solo las
  teselas visibles divide por 4 esa memoria.

### Paso 4 — Que la capa de contenido no dependa del parpadeo del cursor ni del tic
del orbe (≈ 30 líneas)

**Archivos:** `rust/crates/ez-hud/src/win/chat/compose.rs`,
`rust/crates/ez-hud/src/model/chat/view.rs` (o `footer.rs` para el cursor).

- Hoy `signature()` mezcla tres cosas: *contenido* (mensajes), *entorno* (scroll, hover,
  input) y *animación* (`blink`, `anim_tick`). La caché de contenido se invalida con
  cualquiera de ellas.
- Separar la firma en dos: `ContentSig` (lo que cambia el texto de los mensajes) y
  `FrameSig` (lo que cambia el panel entero). El cursor que parpadea se pinta en una
  capa propia o se invalida solo el rectángulo del cursor.
- Efecto: en reposo el contenido se repinta **0 veces por segundo** en vez de 2; en
  streaming, **1 vez por fragmento** en vez de 30.

### Paso 5 — No pintar el cuadro con el candado del panel tomado

**Archivo:** `rust/crates/ez-hud/src/win/chat/window.rs:328-388`.

- Tomar el candado, **copiar lo necesario** (o mover el `ChatView` a un `RwLock`
  separado), soltarlo, y pintar. Volver a tomarlo solo para guardar el resultado.
- Con esto, un cuadro lento deja de bloquear la entrada: la ventana sigue respondiendo
  (aunque tarde), que es justo lo que la persona necesita para poder cerrar el chat.

### Paso 6 — Que `signature()` no copie el historial

**Archivos:** `rust/crates/ez-hud/src/model/chat/view/types.rs` (trait `ChatModel`),
`rust/crates/ez-app/src/app/chat_wiring/model.rs`, `rust/crates/ez-chat/src/session.rs`.

- Añadir `fn version(&self) -> i64` al trait `ChatModel` y usarlo en
  `compose.rs:93` en vez de `view.model.snapshot().0`. `ez-chat` ya lo tiene:
  `session.rs:366` (`ChatModel::version`).
- Para el dibujado, que el modelo exponga una vista prestada (`&[ChatMessage]`) en vez
  de un `Vec` clonado, o al menos que `snapshot()` devuelva `Arc<[ChatMessage]>`.
- Nota: hoy esto es **lo de menor impacto** (0,15 ms con 1 000 mensajes), pero es
  gratis y evita que crezca con el historial.

### Paso 7 — Rastro en archivo, con rotación (para la próxima vez)

**Archivo:** `rust/crates/ez-app/src/app/main.rs:106` (`init_logging`).

- Hoy el registro sale **solo por stderr** (`init_logging` → `tracing_subscriber::fmt()
  .with_writer(std::io::stderr)`), y la app se arranca sin consola: **no queda rastro
  del congelamiento**. `tracing-appender` ya es dependencia de `ez-app`
  (`ez-app/Cargo.toml`), así que basta un `rolling::daily` (o `hourly`) en
  `%APPDATA%\EzAgent\logs\` con `non_blocking(true)` y el `WorkerGuard` guardado.
- Añadir al mismo archivo: el tiempo de cuadro del chat y el del panel de ajustes. Ya
  existe la infraestructura: `ez-hud/src/win/frame_stats.rs` (`FrameStats`, resumen
  p50/p95/máx por minuto). Hoy solo se alimenta de la presentación
  (`win/layered/paint.rs:166`); falta alimentar el **render** del chat, que se mide en
  `win/chat/window/chrome.rs:256-269` (`painted`) y se tira.
- Con eso, un congelamiento deja en el log: «Cuadros chat: p50 … p95 … máx …» y la
  primera línea lenta.

### Paso 8 — Presentar a la cadencia que pide el panel

**Archivos:** `rust/crates/ez-hud/src/win/chat/window/chrome.rs:154-159`,
`rust/crates/ez-hud/src/win/layered.rs`.

- `ChatWindow::set_interval` (chrome.rs:154) solo cambia el intervalo del hilo de
  *pintura*; el hilo de la *ventana* (`win/layered/run.rs:103`) sigue con
  `options.interval = 1/60` (chrome.rs:44) y presenta a 60 fps aunque el panel pida
  30. Propagar el intervalo a la ventana layered.

### Pruebas que lo cubrirían

| prueba | archivo | qué fija |
|---|---|---|
| El coste del contenido no crece con el historial | `ez-app/tests/diag_chat_history_cost.rs` (esta) | que `view.render()` con 1 000 mensajes cueste lo mismo que con 10 |
| La caché no se vacía sola | idem | que el segundo cuadro con 1 000 mensajes sea mucho más barato que el primero |
| `signature()` no llama a `snapshot()` | `ez-hud` (modelo falso que cuenta llamadas) | que la versión se lea sin copiar el historial |
| El parpadeo no invalida el contenido | `ez-hud` (conteo de renders) | que en reposo el contenido se repinte 0 veces por segundo |
| Un cuadro lento no bloquea la entrada | `ez-hud` (hilo de entrada concurrente) | que `pointer_down` responda mientras se pinta |
| El rastro a archivo | `ez-app` | que exista `%APPDATA%\EzAgent\logs\*.log` con rotación |

### Procedimiento para capturar la pila la próxima vez que se congele

Herramientas que ya están en el repo: `rust-migration/tools/orquestacion/stackdump.py`
(suspende cada hilo un instante, recorre su pila con dbghelp y lo reanuda; **solo
lectura**) y `symbolize.py` (traduce los RVA del exe sin símbolos a función y
`archivo:línea` con un exe equivalente que tenga PDB).

```powershell
# 1) Con la app congelada y el chat abierto:
python -I rust-migration\tools\orquestacion\stackdump.py <PID> 60 > pila.txt

# 2) Hilos que hay que mirar (nombres reales, sacados del proceso en vivo):
#    "ezagent-chat-paint" -> hilo que pinta el panel (render + compose)
#    "ezagent-EzAgent"    -> hilo de la ventana del chat (presenta; se bloquea en el candado)
#    "ezagent-drop-paint" -> hilo que pinta la barra
#    "ezagent-drop"       -> hilo de la ventana de la barra

# 3) Traducir los RVA del propio exe (sale como "ezagent-voice.exe+0x…"):
python -I rust-migration\tools\orquestacion\symbolize.py `
    rust\target\release\ezagent-voice.exe `
    rust\target\release\ezagent-voice-con-pdb.exe 0xRVA1 0xRVA2 …
```

**Requisito:** el exe con PDB. Hoy `[profile.release]` no lleva `debug`
(`rust/Cargo.toml:118-121`), así que hay que compilar una vez con
`$env:CARGO_PROFILE_RELEASE_DEBUG=1` (o `debug = 1` en el perfil) para que exista el
`.pdb` que `symbolize.py` necesita.

Qué mirar en la salida, en este orden:

1. Si el hilo **`ezagent-chat-paint`** está dentro de `render`/`build_entry`/`decode`
   → es el coste O(historial) (causa 1 y 3).
2. Si el hilo **`ezagent-EzAgent`** está en `WaitForSingleObject`/lock y el
   **`ezagent-chat-paint`** está en `render` → es el amplificador del candado (causa 3).
3. Si **todos** los hilos de la interfaz están en `WaitForSingleObject` y el principal
   está en `render` → es el `prepare_view` síncrono (causa 1).

---

## 5. Riesgos del arreglo y lo que no se pudo confirmar

**Riesgos**

- La virtualización (paso 2) cambia el resultado de las pruebas de paridad
  (`ez-hud/tests/hud_chat_parity*`): hay que decidir qué pasa con los mensajes fuera
  del viewport (no se maquetan → no hay zona clicable) y hacer que las pruebas de
  paridad usen un historial corto o que acepten la ventana virtualizada.
- Guardar las teselas en `u8` cambia el redondeo del premultiplicado; hay que
  verificar que los PNG dorados siguen igual (los umbrales de
  `parity/thresholds.toml` no se tocan).
- Separar la firma de contenido y la de animación puede mostrar un cursor que parpadea
  con un fotograma de retraso si no se hace bien; conviene dibujar el cursor en una
  capa aparte.
- El registro a archivo escribe en `%APPDATA%`: hay que rotar y limitar el tamaño
  (`tracing-appender` lo hace; falta decidir cuántos días se guardan).
- **Medir con historiales largos consume mucha memoria**: la primera corrida del
  harness llegó a 7,4 GB de RSS con el caso de 5 000 mensajes y el agente principal
  la detuvo al llegar a 11,4 GB. El harness ya corta a 1 GB y no repite ese caso
  (§3). Con el arreglo (virtualización + teselas en `u8`) el techo de memoria
  debería bajar de forma notable, pero conviene medirlo otra vez antes de dar el
  caso por cerrado.

**No se pudo confirmar**

- **El congelamiento exacto que vio la persona no se reprodujo**: no se puede abrir el
  chat de la app que está en marcha (no se toca) y el historial en vivo tiene 33
  mensajes (§3.3), muy por debajo del cliff de 401. Las causas 1–4 están confirmadas
  *estructuralmente* y con mediciones propias, pero no se verificó que el cuadro que
  la persona vio fuera el de la maquetación.
- **El reparto de los 464 MB de RSS**: son estables (no crecen: 467 MB a las 09:56 y
  464 MB a las 10:45), así que **no hay fuga**; parte es el modelo de TTS en memoria
  (`kokoro-v1.0.int8.onnx` 88 MB + `voices-v1.0.bin` 27 MB, con `onnxruntime.dll`
  cargado) y parte son las teselas del chat y los búferes por cuadro. Sin adjuntar un
  depurador no se puede separar el reparto exacto.
- **El coste real de `UpdateLayeredWindow`**: los hilos de ventana consumen ~36 % de un
  núcleo cada uno, pero no se pudo medir la parte que es de Windows (composición DWM)
  y la que es del bucle propio, porque `FrameStats` solo mide la presentación, no el
  desglose.
- **El papel de «muchas cosas abiertas»**: con el chat, la barra, la vista rápida y el
  panel de ajustes abiertos hay cuatro ventanas *layered*, cada una con su hilo
  presentation a 60 fps. Es plausible que esa suma sea lo que la persona llama
  «empeora con muchas cosas abiertas», pero no se midió el caso con las cuatro
  ventanas.
- La cola de `UiLoop` **no** se inunda: el único temporizador es el tick de 30 ms
  (`app/core/callbacks.rs:161-171`) y las colas son `crossbeam` ilimitadas que se
  vacían en cada tick (`app/events.rs:51-68`). No hay una tarea por token.
- `ChatPanel::poll` **no** se auto-bloquea: en Rust 2021 el candado temporal de la
  condición de un `if` se suelta **antes** del bloque, comprobado con un programa
  mínimo (`if *m.lock().unwrap() == 5 { m.lock().unwrap() }` no se cuelga). El
  `teardown` desde `poll` es seguro.

---

## 6. Anexo: comandos ejecutados y salidas

### 6.1 Archivos añadidos en esta rama

- `rust/crates/ez-app/tests/diag_chat_history_cost.rs`: prueba de medición propia
  (parser real de `ez-chat`, vista real de `ez-hud`, composición real del panel),
  con corte de 1 GB de RSS. **No toca código del producto.**

### 6.2 Compilar y medir

```powershell
cd rust
cargo build --release -p ez-hud --tests          # 18 min 33 s (máquina ocupada)
cargo build --release -p ez-app  --test diag_chat_history_cost   # 3 min 12 s

# Una prueba por vez (--test-threads=1 y --exact), como se pidió:
cargo test --release -p ez-app --test diag_chat_history_cost -- --nocapture --test-threads=1 --exact content_cost_grows_with_the_history
cargo test --release -p ez-app --test diag_chat_history_cost -- --nocapture --test-threads=1 --exact the_layout_cache_thrashes_past_400_messages
cargo test --release -p ez-app --test diag_chat_history_cost -- --nocapture --test-threads=1 --exact full_frame_cost_grows_with_the_history
```

Las tres en verde (`test result: ok`).

### 6.3 Lecturas del proceso en vivo (PowerShell, solo lectura)

```powershell
Get-Process -Id 22432 | Select-Object Id, @{n='WS_MB';e={[math]::Round($_.WorkingSet64/1MB,1)}}, CPU, @{n='Threads';e={$_.Threads.Count}}, Handles
# -> WS 467,2 MB | CPU 511 s | 37 hilos | 609 handles

# GetGuiResources (GDI=0, USER=1)
$src = @"
using System; using System.Runtime.InteropServices;
public static class Gui {
  [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr hProcess, uint uiFlags);
}
"@
Add-Type -TypeDefinition $src
$p = Get-Process -Id 22432
"GDI: " + [Gui]::GetGuiResources($p.Handle, 0) + "  USER: " + [Gui]::GetGuiResources($p.Handle, 1)
# -> GDI: 14  USER: 39

# EnumWindows: el chat estaba abierto
# -> title='EzAgent' rect=466x736+1458+308 | title='drop' rect=221x70+1681+972

# CPU por hilo (delta de 10 s) + GetThreadDescription
# -> 12012 ezagent-drop    36,1 %
# ->  8672 ezagent-EzAgent 35,6 %
```

### 6.4 Comprobación de que `poll`/`teardown` no se auto-bloquea

En Rust 2021 el candado temporal de la condición de un `if` se suelta **antes** del
bloque, así que `ChatPanel::poll` (`win/chat/window/lifecycle.rs:180`) puede llamar a
`teardown` sin quedarse con `PanelState` tomado. Comprobado con un programa mínimo:

```rust
let m = Mutex::new(5);
if *m.lock().unwrap() == 5 {
    println!("inside block: lock again -> {}", m.lock().unwrap());
}
// -> inside block: lock again -> 5   (no se cuelga)
```

### 6.5 Historial y logs de la app

```powershell
$f = "$env:APPDATA\EzAgent\history\chat.jsonl"
# 33 mensajes con rol (9178 caracteres), todos de la sesión 16; 16 sesiones en total
Get-ChildItem "$env:APPDATA\EzAgent\logs"    # solo latency.jsonl y audit.jsonl
```

