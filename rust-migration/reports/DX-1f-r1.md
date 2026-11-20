# DX-1f — r1 · La animación de la barra al chat no se ve

> Encargo: **DX-1f** (`rust-migration/briefs/DX-1f.md`). Rama `work/DX-1f` desde
> `eco/main` (con DX-1b, DX-1c, DX-1d y DX-3). Nada de la app de voz de la persona
> se tocó: no se cerró, no se le inyectó nada, no se le adjuntó un depurador, no se
> ejecutó ningún agente real y no se tocó la ventana de Herdr ni su `whisper-server`.
> Todas las pruebas de humo son ventanas del propio proceso de la prueba y trabajan
> con un `APPDATA` temporal.

## Resultado
- Brief/tarea: DX-1f (la apertura del chat no se anima; el cierre sí)
- Estado: **completado**

## Cambios

- `rust/crates/ez-hud/src/win/chat/compose.rs`: `compose_morph`, la composición
  del morph, **nunca maqueta** el historial; `compose` (la de siempre) sigue
  maquetando cuando no hay capa. `paint_content` recibe `build`. El traslado de
  la firma, la caché y el pintado de contenido a `compose/content.rs` deja el
  módulo en 512 líneas (estaba en 619 antes de este brief).
- `rust/crates/ez-hud/src/win/chat/compose/content.rs` (nuevo): `ContentSig`,
  `content_signature`, `ContentCache` y `paint_content`, trasladados sin cambios
  de comportamiento (solo el parámetro `build`).
- `rust/crates/ez-hud/src/win/chat/window.rs`: el estado del morph se lee **antes**
  de `step` (`was_open`); solo se prepara el contenido fuera del candado cuando
  el panel ya estaba abierto; el morph usa `compose_morph`; `record_frame` anota
  cada cuadro en el diario; `AnimState::as_str`.
- `rust/crates/ez-hud/src/win/chat/trace.rs` (nuevo): diario de cuadros del panel
  (apagado por omisión: un `AtomicBool` por cuadro), con instante del cuadro,
  instante de entrega, fase, progreso, tamaño, alfa máxima y si llevaba la capa
  de contenido.
- `rust/crates/ez-hud/src/win/chat/mod.rs`: `pub mod trace`.
- `rust/crates/ez-hud/src/win/chat/window/tests.rs`: dos pruebas de regresión
  (`a_slow_history_does_not_eat_the_opening_animation` y
  `the_frame_that_ends_the_morph_does_not_lay_out_the_history`).
- `rust/crates/ez-app/examples/diag_chat_smoke.rs`: modo `anim` (apertura con el
  clic real de la barra, cierre con el clic real del botón de cerrar de la
  cabecera), historial normal para medir el morph (sin la palabra de 2 000
  caracteres, que se corta en O(n²) y tarda segundos), y las «décimas» del bucle
  en la salida de los tres modos de humo.
- `rust-migration/reports/DX-1f-r1.md` (nuevo): este informe.

## Decisiones

- **`// DECISION` el historial no se maqueta durante el morph.** Es lo que arregla
  el salto: el primer cuadro de la apertura maquetaba todo el historial antes de
  presentarse y, con una conversación real, el morph (0,28 s) terminaba antes de
  que el cuadro estuviera listo. (`compose.rs`, `window.rs`)
- **`// DECISION` el estado se lee antes de `step`.** `step` pasa a `Open` dentro
  del cuadro que cruza el final; ese cuadro tomaba el camino estable y maquetaba
  el historial **con el candado del panel tomado** (lo que DX-1b quitó). El
  cuadro que termina el morph sigue siendo del morph; el contenido llega en el
  siguiente, fuera del candado. (Lo encontró el humo `real`: sin esto, el bucle
  de la prueba solo avanzaba 3 décimas en 10 s.) (`window.rs`)
- **`// DECISION` el diario de cuadros está apagado por omisión.** Es diagnóstico:
  el humo lo enciende y las pruebas lo usan; en la app cuesta un `AtomicBool`.
  (`trace.rs`)
- **`// DECISION` el modo `anim` usa un historial normal.** El humo de DX-1d usa
  una palabra de 2 000 caracteres a propósito (estrés de maquetación); cortarla es
  O(n²) y en esta máquina tarda >10 s, así que el modo que mide el *morph* usa 24
  mensajes cortos. (`diag_chat_smoke.rs`)
- **`// DECISION` la capa de contenido vive en `compose/content.rs`.** El traslado
  es mecánico y deja `compose.rs` bajo el límite de 600 líneas del contrato ECO.

## Verificación

- `cargo fmt -p ez-hud -p ez-app -- --check` → limpio (exit 0).
- `cargo clippy --release -p ez-hud -p ez-app --all-targets -- -D warnings` →
  **exit 0** (los únicos avisos son del `freetype-sys` vendorizado, ya existentes).
- `cargo test --release -p ez-hud` → **573 unitarias + 20 de integración en
  verde** (`test result: ok` en los 15 objetivos; incluye `hud_chat_parity` con
  sus 221 casos y `chat_virtualization`; `parity/thresholds.toml` no se tocó).
- `cargo test --release -p ez-app` → **359 unitarias + 3 del bin + 10 de
  integración en verde**.
- `cargo run --release -p ez-app --example diag_chat_smoke -- anim` → **exit 0**
  (apertura 11 cuadros intermedios, cierre 8; antes de este brief: 0 y 8).
- `chat`, `real` y `big` en release → **exit 0**, vivos los 10 s, sin `PÁNICO` y
  con **100 décimas** cada uno; pico de RSS 91 / 173 / 91 MB.
- Ninguna prueba ni medición pasó de 1 GB (el arnés de `diag_chat_history_cost`
  conserva su corte de 1 024 MB).

---

## 1. Diagnóstico con evidencia

El síntoma es real y es el que sospechaba el brief, con un matiz: **el primer
cuadro de la apertura no se presenta tarde, se compone tarde**. La ventana solo
presenta cuando `ChatPanel::render` devuelve el fotograma, y ese primer render
hacía la maquetación completa del historial dentro de `paint_content`
(`window.rs` §2) antes de componer nada:

```text
ChatWindow::paint_loop
  └ ChatPanel::render
      ├ (1) firma del contenido + update_live
      ├ (2) paint_content → view.render_content   ← TODO el historial (≈250 ms a 1 s+)
      ├ (3) step + compose                        ← el morph ya terminó (p ≈ 1)
      └ record_frame / ventana.present
```

El cierre no pasaba por ahí: reusaba la capa de contenido que dejó el panel
abierto (`reuse`, `paint_content` no maqueta) y por eso sí se veía.

### Instrumentación (diario de cuadros)

`ez-hud/src/win/chat/trace.rs` graba, por cada cuadro que el panel entrega:

| campo | qué es |
|---|---|
| `t` | segundos desde el inicio de la animación, en el instante del cuadro |
| `listo` | segundos desde el inicio cuando el cuadro está compuesto (se entrega enseguida) |
| `p` | progreso del morph (0 = forma de la barra, 1 = panel entero) |
| tamaño | píxeles físicos de la ventana (constante durante el morph: la escala la da `p`) |
| alfa | alfa máxima del cuadro (la forma es opaca desde el primer cuadro: 255) |
| contenido | si el cuadro llevaba la capa de contenido (cabecera, mensajes, campo) |

Lo enciende el humo (`trace::enable(true)`) justo antes del clic que abre el chat
y lo lee el modo `anim`; en producción está apagado.

### 1.1 Antes (esta rama, con DX-1b + DX-1c; `eco/main` `c455fcf`)

`cargo run --release -p ez-app --example diag_chat_smoke -- anim` (esta corrida es
anterior a la columna `listo`; el salto de 0,88 ms a 252 ms **es** el tiempo que
tardó el primer cuadro en estar listo):

```text
=== cuadros de la apertura ===
fase       t(ms)      p    tamaño   alfa  contenido
opening      0.88  0.000    466x736   255  sí
opening    252.14  0.996    466x736   255  sí
opening    270.23  1.000    466x736   255  sí
=== cuadros del cierre ===
closing      0.02  1.000    466x736   255  no
closing     18.54  0.997    466x736   255  no
closing     36.15  0.976    466x736   255  no
closing     54.83  0.918    466x736   255  no
closing     71.94  0.814    466x736   255  no
closing     89.28  0.644    466x736   255  no
closing    106.77  0.405    466x736   255  no
closing    123.97  0.220    466x736   255  no
closing    141.84  0.098    466x736   255  no
closing    158.52  0.036    466x736   255  no
closing    176.32  0.007    466x736   255  no
closing    193.15  0.000    466x736   255  no
closing    210.37  0.000    466x736   255  no
apertura: 0 cuadros intermedios · primer p 0.000 · contenido a los 288.3 ms
cierre: 8 cuadros intermedios · primer cuadro abierto a los 288.3 ms
RESULT anim opening_frames=0 closing_frames=8 first_p=0.000
RSS pico (anim): 175 MB
estado: la apertura es un salto
```

Lectura: la apertura presenta **3 cuadros** (0, 0,996 y 1) en 270 ms; el
intermedio se ve 17 ms. El cierre presenta **13** (8 intermedios) en 210 ms.
La apertura es un salto.

### 1.2 Antes de DX-1b (emulado)

DX-1b quitó la llamada `view.render(monotonic())` de `prepare_view` (ver
`git log -L` de `lifecycle.rs`); esa llamada dejaba el historial maquetado antes
del morph. Para medir «la app de antes de `acacccd`» sin compilar otro árbol, se
volvió a poner esa línea (solo durante la medición) en la misma base sin arreglo:

```text
=== cuadros de la apertura ===
opening      0.14  0.000 ...   opening     34.75  0.008 ...   (16 cuadros, uno cada ~17 ms)
...
opening    278.13  1.000 ...
apertura: 11 cuadros intermedios · primer p 0.000 · contenido a los 296.4 ms
cierre: 8 cuadros intermedios · primer cuadro abierto a los 296.4 ms
RSS pico (anim): 188 MB
estado: la apertura se anima
```

Confirma el «antes» que recordaba la persona: con el historial ya maquetado, la
apertura presentaba 16 cuadros. (Es una emulación, no el binario de aquel commit;
la llamada es exactamente la que DX-1b eliminó.)

### 1.3 Después (esta rama)

```text
=== cuadros de la apertura ===
fase       t(ms)  listo(ms)      p    tamaño   alfa  contenido
opening     1.21      4.01  0.000    466x736   255  no
opening    18.00     19.91  0.001    466x736   255  no
opening    36.59     38.06  0.009    466x736   255  no
opening    55.08     56.69  0.030    466x736   255  no
opening    72.70     73.98  0.070    466x736   255  no
opening    90.29     92.68  0.134    466x736   255  no
opening   107.38    110.67  0.226    466x736   255  no
opening   124.97    128.04  0.356    466x736   255  no
opening   142.95    145.63  0.531    466x736   255  no
opening   159.79    166.96  0.683    466x736   255  no
opening   176.63    184.53  0.799    466x736   255  no
opening   193.37    199.39  0.882    466x736   255  no
opening   210.56    218.94  0.939    466x736   255  no
opening   227.41    230.86  0.973    466x736   255  no
opening   244.28    248.97  0.992    466x736   255  no
opening   262.87    267.95  0.999    466x736   255  no
=== cuadros del cierre ===
fase       t(ms)  listo(ms)      p    tamaño   alfa  contenido
closing     0.02      3.03  1.000    466x736   255  no
closing    17.46     22.23  0.997    466x736   255  no
closing    35.18     40.06  0.978    466x736   255  no
closing    52.34     61.01  0.928    466x736   255  no
closing    70.23     75.77  0.827    466x736   255  no
closing    87.50     93.65  0.665    466x736   255  no
closing   104.25    106.35  0.439    466x736   255  no
closing   121.75    124.23  0.240    466x736   255  no
closing   139.08    140.56  0.113    466x736   255  no
closing   156.49    158.48  0.041    466x736   255  no
closing   173.94    175.22  0.009    466x736   255  no
closing   191.07    196.52  0.000    466x736   255  no
closing   208.24    209.78  0.000    466x736   255  no
apertura: 11 cuadros intermedios · primer p 0.000 · contenido a los 298.5 ms
cierre: 8 cuadros intermedios · primer cuadro abierto a los 281.0 ms
RESULT anim opening_frames=11 closing_frames=8 first_p=0.000
RSS pico (anim): 187 MB
estado: la apertura se anima
```

### 1.4 Tabla comparativa

| medida | antes (DX-1b + DX-1c) | antes de DX-1b (emulado) | después |
|---|---|---|---|
| cuadros de la apertura | **3** | 16 | **16** |
| cuadros intermedios (0,02 < p < 0,98) | **0** | 11 | **11** |
| primer `p` | 0,000 | 0,000 | 0,000 |
| segundo cuadro | 252 ms, p 0,996 | 35 ms, p 0,008 | 18 ms, p 0,001 |
| último cuadro del morph | 270 ms | 278 ms | 263 ms |
| cuadros del cierre | 13 | 13 | 12 |
| intermedios del cierre | 8 | 8 | 8 |
| el contenido llega | 288 ms | 296 ms | 299 ms |
| pico de RSS (`anim`) | 175 MB | 188 MB | 187 MB |

La duración de cada animación es la del modelo (`MORPH_SECS` 0,28 s de apertura,
`CLOSE_SECS` 0,20 s de cierre): no se tocaron. La apertura, con 40 % más de
tiempo, presenta **más** cuadros intermedios que el cierre (11 frente a 8).

### 1.5 Un segundo fallo, encontrado por el humo

Al aplicar el primer arreglo, el humo `real` dejó de avanzar: solo 3 décimas en
10 s y sin llegar al `monkey` de la décima 5. La causa: el cuadro que **termina**
el morph también tomaba el camino estable (`step` muta el estado a `Open` dentro
de ese cuadro), así que el historial se maquetaba **con el candado del panel
tomado** y `chat_is_open()`/la entrada esperaban la maquetación completa. Se
arregla leyendo el estado antes de `step` (`was_open`); el cuadro que cruza el
final sigue siendo del morph y el contenido llega en el siguiente, fuera del
candado. Medido: el bucle del humo pasa de **3** a **100** décimas y vuelve a
imprimir `monkey: 1 ventana(s)`.

Ese detalle muestra que el candado importa: con el historial de 320 mensajes del
modo `big`, la maquetación bajo el candado bloqueaba la entrada varios segundos.

## 2. El arreglo

1. **Durante el morph no se maqueta.** `ChatPanel::render` solo prepara la capa
   de contenido (con la vista fuera del candado) cuando el panel **ya estaba
   abierto** al empezar el cuadro. El morph usa `compose_morph`, que pinta la
   forma y, si ya existe, la capa de contenido cacheada.
2. **El contenido aparece cuando llega.** El primer cuadro del panel abierto
   maqueta el historial en el hilo de pintura (`ezagent-chat-paint`), fuera del
   candado del panel; el cuadro se presenta al terminar. Con 24 mensajes el
   contenido aparece a los ~300 ms (el morph dura 280 ms); con el historial real
   de la persona (33 mensajes) el orden de magnitud es el mismo que midió DX-1c
   (~6,9 ms por mensaje rico).
3. **No se deshace DX-1b ni DX-1d.** La maquetación sigue en el hilo de pintura y
   fuera del candado; el `catch_unwind` de `paint_content` (la vista vuelve al
   panel y se guarda una capa de 1×1 con la firma) sigue igual.

## 3. Prueba automática

### 3.1 Humo `anim`

```text
cargo run --release -p ez-app --example diag_chat_smoke -- anim
```

- Abre el chat con un **clic real a la barra** (mismo camino que `real`:
  `WM_MOUSEMOVE`/`WM_LBUTTONDOWN`/`WM_LBUTTONUP` → `WndProc` → `HUD: chat`).
- Espera al primer cuadro con contenido (hasta 8 s) y luego **cierra con un clic
  real** en el botón de cerrar de la cabecera (la barra no sirve para cerrar: con
  el chat abierto `pointer_down` ignora el clic por `form_hidden`).
- Compara las dos animaciones y **falla (exit 1)** si:
  - el primer cuadro de la apertura empieza con `p >= 0.5`;
  - la apertura presenta menos cuadros intermedios que el cierre (mínimo 2),
    contados por el instante de **entrega** dentro de `MORPH_SECS`/`CLOSE_SECS`;
  - el cierre no presenta al menos 2;
  - el contenido no llega nunca;
  - el pico de RSS pasa de 1 GB o hay una línea `PÁNICO`.
- Antes del arreglo (con la versión final del contador): `apertura: 0 cuadros
  intermedios`, `cierre: 8`, `estado: la apertura es un salto`, exit 1.
  Después: `apertura: 11`, `cierre: 8`, `estado: la apertura se anima`, exit 0.

### 3.2 Pruebas de unidad (ez-hud)

| prueba | qué fija |
|---|---|
| `win::chat::window::tests::a_slow_history_does_not_eat_the_opening_animation` | un historial que tarda 300 ms en cada copia no se maqueta en el primer cuadro de la apertura: el cuadro tarda <150 ms, el morph sigue en `Opening` con `p < 0,5` y el contenido llega en el cuadro estable. **Antes fallaba**: `el primer cuadro de la apertura tardó 1.1029563s: maquetó el historial` |
| `win::chat::window::tests::the_frame_that_ends_the_morph_does_not_lay_out_the_history` | el cuadro que `step` pasa a `Open` no maqueta (`layouts() == 0`); el siguiente sí (`layouts() > 0`) |
| `win::chat::trace::tests::*` (3) | el diario apagado no graba; conserva el orden y se limpia; se detiene en el tope |

## 4. Humo (release)

```text
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real
cargo run --release -p ez-app --example diag_chat_smoke -- big
```

```text
=== chat ===
tema: light · escala: 1 · layout: pill
chat abierto: true
RSS pico: 91 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== real ===
tema: light · escala: 1 · layout: pill
llamada iniciada
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en gear de la barra (20,12)
clic del panel enviado
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en chat de la barra (56,12)
chat abierto: true
monkey: 1 ventana(s) del chat recibieron ratón
RSS pico: 173 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== big (320 mensajes) ===
historial: 320 mensajes sintéticos
tema: light · escala: 1 · layout: pill
chat abierto: true
rueda: 120 muescas de 32767 en 1 ventana(s)
desplazamiento: hasta arriba
vivo tras arriba: true
rueda: 120 muescas de -32767 en 1 ventana(s)
desplazamiento: hasta abajo
vivo tras abajo: true
RSS pico: 91 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10
```

Los tres terminan con exit 0, con 100 décimas de vida y sin `PÁNICO`. Pico de
memoria: 91 / 173 / 91 MB (`anim`: 187 MB), muy por debajo del tope de 1 GB.

## Pendiente o riesgos

- **El contenido aparece después del morph, no durante.** Es el intercambio que
  autoriza el brief («anima el marco/fondo»): con 24 mensajes llega a los ~300 ms
  (el morph acaba a los 280 ms) y la primera vez que se abre el chat se ve la
  forma durante un instante sin el historial. La alternativa (maquetar y animar a
  la vez) exige mover la maquetación a un hilo aparte y no entra en este brief.
- **El humo `chat`/`real`/`big` no ve el contenido del historial rico en 10 s.**
  La palabra de 2 000 caracteres del humo de DX-1d se corta en O(n²)
  (`ez-gfx/src/text/chat.rs::split_word`) y la maquetación tarda >10 s en esta
  máquina. Es anterior a este brief y queda fuera de alcance (es `ez-gfx`, no
  `ez-hud`): lo que sí se arregló es que ya no bloquea la entrada ni el latido
  del bucle (100 décimas en los tres modos; antes de este arreglo, `real` solo
  avanzaba 3).
- **La comparación `apertura >= cierre` puede apretar en una máquina muy
  cargada**: el reloj de la animación es real y, si el sistema roba CPU, se
  presentan menos cuadros en las dos. Medido aquí: 11 frente a 8 y 11 frente a 7
  en dos corridas; el margen es del 40 % (la apertura dura 0,28 s y el cierre
  0,20 s).
- **El diario graba el instante en que el cuadro se entrega al hilo de la
  ventana**, no el `UpdateLayeredWindow` exacto; la ventana presenta en su
  siguiente turno (≤ 16 ms). Para comparar apertura y cierre es suficiente porque
  las dos pasan por el mismo camino.
- **`diag_chat_smoke.rs` sigue por encima de 600 líneas** (1 071; ya tenía 784
  antes de este brief) y `window/tests.rs` (762; ya tenía 649). Son ejemplos y
  pruebas; partirlos obligaría a crear archivos que el brief no lista.
- **No se comprobó en la app en marcha**: la app de voz que la persona tiene
  abierta no se tocó. La evidencia es el diario de cuadros y las pruebas.

## Anexo: comandos

```powershell
cd rust
cargo fmt -p ez-hud -p ez-app -- --check
cargo clippy --release -p ez-hud -p ez-app --all-targets -- -D warnings
cargo test --release -p ez-hud
cargo test --release -p ez-app

cargo build --release -p ez-app --example diag_chat_smoke
cargo run --release -p ez-app --example diag_chat_smoke -- anim
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real
cargo run --release -p ez-app --example diag_chat_smoke -- big
```

Commits de la rama para este brief:

```text
25eea67 DX-1f: la apertura del chat se anima (no maqueta el historial durante el morph)
80910ba DX-1f: el cuadro que termina el morph no maqueta bajo el candado
4ebf28b DX-1f: el diario anota cuando el cuadro esta listo y el humo lo imprime
2a1bdcf DX-1f: la capa de contenido vive en compose/content.rs
```
