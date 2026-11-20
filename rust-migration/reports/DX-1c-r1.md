# DX-1c — r1 · Fase B del chat: maquetar solo lo que se ve

> Encargo: **DX-1c**, la fase B del diagnóstico `DX-1-r1.md` §4 paso 2.
> mcode dejó la implementación a medias en `work/DX-1c` (`888b91e` con el brief y
> `ee02fd7` con la virtualización, sin informe ni mediciones). Este relevo la retoma
> empezando por fusionar `eco/main` (que ya trae DX-1b + DX-1d), la revisa, añade la
> prueba de humo con historial grande y mide antes/después.
>
> Base del «antes»: `eco/main` `13623bb`. Rama: `work/DX-1c`. La fusión fue limpia
> (`bf0b745`); no hubo conflictos y los arreglos de DX-1d quedaron intactos: hook de
> pánicos, `catch_unwind` en `ChatPanel::render`, `catch_panic` en el `WndProc`,
> `place_caret` con líneas vacías y `composite_caret` recortado contra el contenido.
> La app de la persona (PID 19576) y su `whisper-server` no se tocaron; no se ejecutó
> ningún agente real ni se tocó la ventana de Herdr.

## Resultado
- Brief/tarea: DX-1c (fase B del chat: virtualización)
- Estado: completado

## Cambios

### Heredado de mcode (revisado; solo formato, un doc y un `mut` de más)

- `rust/crates/ez-hud/src/model/chat/view/measure.rs` (nuevo): alto de cada mensaje
  **sin pintarlo**. Repite la aritmética del camino de pintura (nota, burbuja del
  usuario, tarjeta del asistente) y mide los bloques con `measure_blocks`. Incluye la
  prueba `the_measured_height_is_the_height_of_the_painted_message`.
- `rust/crates/ez-hud/src/paint/chat/render.rs`: `render_blocks` se parte en
  `stack_blocks` (coloca y mide) + `render_blocks` (pinta) + `measure_blocks` (solo
  mide). Los dos caminos comparten `stack_blocks`: el alto medido es el pintado.
- `rust/crates/ez-hud/src/model/chat/view/draw.rs`: `draw_messages` mide el alto de
  todo el historial (barato) pero **solo maqueta y pinta el viewport ± una pantalla**
  (`laid_out_top`/`laid_out_bottom`); los mensajes del margen se dejan en la caché
  para que el desplazamiento los encuentre hechos. Las zonas clicables y la copia se
  registran **solo para lo visible**.
- `rust/crates/ez-hud/src/model/chat/view/entries.rs`: `height_of` usa
  `measure_entry` en vez de construir la tesela; `is_rich` se comparte entre medir y
  pintar; el alto de una respuesta en streaming respeta el mismo throttle de 50 ms
  que la tesela.
- `rust/crates/ez-hud/src/model/chat/view.rs`: declara el módulo `measure`.
- `rust/crates/ez-hud/tests/chat_virtualization.rs` (nuevo): las tres pruebas de la
  fase B (solo se maqueta lo visible; el primer cuadro no paga el historial; al subir
  se maqueta lo que entra).

### Añadido en este relevo

- `rust/crates/ez-app/examples/diag_chat_smoke.rs`: modo `big` (320 mensajes
  sintéticos con markdown, código, emojis, mensajes vacíos y palabras sin espacios)
  que abre el chat por el atajo, espera al primer cuadro, **sube hasta arriba y baja
  hasta abajo con la rueda** (`WM_MOUSEWHEEL` a la ventana del chat de este proceso,
  sin `SendInput` ni tocar ventanas ajenas), comprueba que sigue vivo, mide el pico
  de RSS y lee el registro temporal: falla si hay una línea `PÁNICO` o si el pico
  pasa de 1 GB. También imprime `RSS pico` y `registro temporal: sin PÁNICO` en los
  modos `chat` y `real`.
- `rust/crates/ez-hud/src/model/chat/view/measure.rs` y
  `rust/crates/ez-hud/tests/chat_virtualization.rs`: `cargo fmt` y un `mut` que
  clippy rechazaba; el comentario de coste dice «unas diez veces más barato» (lo
  medido), no «mil veces».

## Decisiones

- **`# DECISION` el alto se mide, no se pinta (DX-1c).** El desplazamiento total
  necesita el alto de todos los mensajes, pero la tesela solo hace falta para lo que
  se ve; medir cuesta ~9 veces menos que pintar y no asigna la tesela.
  (`view/entries.rs`, `view/measure.rs`)
- **`# DECISION` el margen es una pantalla arriba y otra abajo.** Así el
  desplazamiento encuentra las teselas ya hechas y no se ve un hueco al entrar.
  (`view/draw.rs`)
- **`# DECISION` medir y pintar comparten `stack_blocks`.** Si el alto medido no
  fuera el pintado, los mensajes se colocarían solapados o con huecos; la prueba
  compara los dos números para usuario, asistente y nota, con encabezado, lista,
  cita, código y tabla. (`paint/chat/render.rs`, `view/measure.rs`)
- **`# DECISION` el humo grande usa 320 mensajes y la rueda real.** El historial
  mínimo pedido es 300; la rueda se entrega a la ventana de este proceso con
  `PostMessage`, no con `SendInput`, para no mover el ratón de la persona ni tocar su
  app. (`ez-app/examples/diag_chat_smoke.rs`)
- **`# DECISION` la paridad no se adaptó.** `hud_chat_parity` sigue en verde con los
  mismos 221 casos y `parity/thresholds.toml` sin tocar: el caso mayor tiene 12
  mensajes y el alto medido coincide con el pintado, así que los píxeles visibles no
  cambian. (`ez-hud/tests/hud_chat_parity.rs`)

## Verificación

### Pruebas, formato y lint

- `cargo test --release -p ez-hud` → **568 unitarias + 20 de integración, todas en
  verde** (`test result: ok` en los 15 objetivos; incluye `hud_chat_parity` con sus
  221 casos y `chat_virtualization` con sus 3).
- `cargo test --release -p ez-app` → **359 unitarias + 10 de integración, todas en
  verde** (incluye `log_rotation`).
- `cargo fmt -p ez-hud -p ez-app -p ez-chat -- --check` → limpio (exit 0).
- `cargo clippy --release -p ez-hud -p ez-app -p ez-chat --all-targets -- -D warnings`
  → sin avisos en estos crates (exit 0; los únicos avisos son los 14 ya conocidos del
  `freetype-sys` vendorizado).
- `hud_chat_parity` → **1 pasado**; `parity/thresholds.toml` y el test de paridad no
  aparecen en el diff contra `eco/main`.

### Contador de maquetas (prueba `chat_virtualization`)

Medido en la prueba con mensajes cortos y ventana de 440×600:

| caso | maquetas del primer cuadro |
|---|---|
| 10 mensajes | **10** |
| 1 000 mensajes | **11** |
| 1 000, tras subir hasta arriba | **11 → 22** (solo entra lo nuevo) |

El primer cuadro con 1 000 mensajes cortos tarda **849 ms**; con 10, **68 ms** (el
caso de 10 incluye el arranque de fuentes; el coste marginal es ~0,8 ms por mensaje).
Antes, cada mensaje costaba ~64 ms de maquetado y pintado (tabla siguiente).

### Medición antes/después (`diag_chat_history_cost`)

Mismo arnés, misma máquina y `release`. El «antes» es `eco/main` `13623bb` (DX-1b +
DX-1d) en un worktree aparte; el «después» es esta rama. El arnés corta a 1 GB de
RSS (`GetProcessMemoryInfo`).

**Antes (`eco/main` `13623bb`):**

```text
== contenido según el historial ==
  msgs  abre (frío)    2º render   3er render     snapshot   RSS MB
    10     711.40 ms       6.28 ms       6.05 ms      0.002 ms       34
   100    5974.00 ms       6.46 ms       6.33 ms      0.008 ms       34
  1000   63983.47 ms       7.92 ms       6.67 ms      0.077 ms       36
el contenido cuesta 63983.5 ms con 1000 mensajes y 5974.0 ms con 100
== caché de maquetación ==
   10 mensajes: fotograma siguiente     5.93 ms
  100 mensajes: fotograma siguiente     6.51 ms
con la caché completa (100 < 401) el fotograma posterior cuesta 6.51 ms
== fotograma completo ==
ventana del panel: 472×672 px
   10 mensajes: fotograma completo     4.14 ms (RSS 54 MB)
  100 mensajes: fotograma completo     3.67 ms (RSS 54 MB)
```

**Después (`work/DX-1c`):**

```text
== contenido según el historial ==
  msgs  abre (frío)    2º render   3er render     snapshot   RSS MB
    10     402.24 ms       5.84 ms       5.90 ms      0.001 ms       17
   100     911.90 ms       5.93 ms       5.94 ms      0.007 ms       17
  1000    6927.66 ms       7.75 ms       6.77 ms      0.076 ms       18
el contenido cuesta 6927.7 ms con 1000 mensajes y 911.9 ms con 100
== caché de maquetación ==
   10 mensajes: fotograma siguiente     6.01 ms
  100 mensajes: fotograma siguiente     5.80 ms
con la caché completa (100 < 401) el fotograma posterior cuesta 5.80 ms
== fotograma completo ==
ventana del panel: 472×672 px
   10 mensajes: fotograma completo     3.34 ms (RSS 37 MB)
  100 mensajes: fotograma completo     2.78 ms (RSS 37 MB)
```

| medida | antes | después | cambio |
|---|---|---|---|
| primer render, 10 mensajes | 711,40 ms | 402,24 ms | ×1,8 menos |
| primer render, 100 mensajes | 5 974,00 ms | 911,90 ms | **×6,6 menos** |
| primer render, 1 000 mensajes | 63 983,47 ms | 6 927,66 ms | **×9,2 menos** |
| primer render con 1 000 mensajes **cortos** | no medido | 849 ms | — |
| 2.º render, 1 000 mensajes | 7,92 ms | 7,75 ms | igual (caché caliente) |
| RSS de la vista, 1 000 mensajes | 36 MB | 18 MB | ×2 menos |
| fotograma completo, 100 mensajes | 3,67 ms (RSS 54 MB) | 2,78 ms (RSS 37 MB) | = / RSS ×1,5 menos |
| snapshot, 1 000 mensajes | 0,077 ms | 0,076 ms | igual |

Lectura: el primer cuadro ya no pinta el historial; el coste que queda es **medir**
(parsear y medir el texto de todos los mensajes), que sigue siendo lineal pero ~9
veces más barato y no asigna teselas. Extrapolando los 33 mensajes de la persona:
33 × 6,9 ms ≈ 0,23 s de medición (más el arranque), frente a 33 × 64 ms ≈ 2,1 s de
pintado en la base; DX-1b ya había movido ese trabajo al hilo de pintura, así que
la app no se congelaba, pero el contenido tardaba en aparecer.

### Prueba de humo de la app real (regla nueva de interfaz)

```powershell
cd rust
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real
cargo run --release -p ez-app --example diag_chat_smoke -- big
```

```text
=== chat ===
tema: light · escala: 1 · layout: pill
chat abierto: true
RSS pico: 72 MB
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
RSS pico: 147 MB
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
RSS pico: 74 MB
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10
```

Los tres modos terminan con exit 0. Ninguna prueba ni medición pasó de 1 GB: el pico
mayor fue **147 MB** (`real`), y el arnés de la medición lleva su propio corte de
1 024 MB (no se activó en ninguna corrida de esta tabla).

## Pendiente o riesgos

- **Medir sigue siendo O(historial).** No se pinta nada fuera de pantalla, pero el
  alto de cada mensaje se mide una vez: con 1 000 mensajes ricos son ~6,9 s (antes
  ~64 s) y con 1 000 cortos ~0,85 s. Para que el primer cuadro fuese constante
  habría que estimar el alto de lo que no se ve o cachear anchos de texto en
  `ez-gfx`; ninguna de las dos cosas está en el brief.
- **Los bloques con imagen o documento se cargan al medir.** `layout_image` llama a
  `load_image` y `layout_document` lee la vista previa del archivo; con un historial
  lleno de imágenes el primer cuadro seguirá dependiendo del disco. Los historiales
  medidos y las pruebas usan solo texto.
- **El alto medido duplica la aritmética del pintado.** La prueba compara los dos
  números con un host de bloques variados; si en el futuro cambia una fórmula de
  `entries.rs`/`measure.rs` sin la otra, la prueba debe cubrir el caso nuevo.
- **`big` no comprueba la posición exacta del desplazamiento**, solo que la app sigue
  viva y sin pánicos tras las muescas; la prueba de `layouts()` (11 → 22 al subir) es
  la que fija que lo que entra se maqueta.
- **La tesela sigue siendo RGBA `f32`** (16 B/píxel); lo que cambió es que solo se
  guardan las visibles + margen (32 MB de tope). Convertirla a `u8` era la otra mitad
  del paso 3 del diagnóstico y no entra en DX-1c.

## Anexo: comandos

```powershell
# fusión y verificación
git merge eco/main
cd rust
cargo test --release -p ez-hud
cargo test --release -p ez-app
cargo fmt -p ez-hud -p ez-app -p ez-chat -- --check
cargo clippy --release -p ez-hud -p ez-app -p ez-chat --all-targets -- -D warnings

# medición antes/después (release)
cargo run --release -p ez-app --example diag_chat_history_cost

# humo
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real
cargo run --release -p ez-app --example diag_chat_smoke -- big
```

Nota de método: la primera medición del «después» se hizo compartiendo el
`CARGO_TARGET_DIR` con el worktree de la base y enlazó el `ez-hud` de la base
(63,1 s con 1 000 mensajes). Se reconstruyó `ez-hud`/`ez-app` en esta rama y se
repitió: la tabla de arriba es la corrida con el artefacto correcto (RSS 17–18 MB
frente a 34–36 MB de la base, y los 6,9 s). El worktree temporal del «antes» se
retiró al terminar.
