# DX-1g — r1 · Una sola sombra en la ventana del chat

> Encargo: **DX-1g** (`rust-migration/briefs/DX-1g.md`), sobre la entrega de DX-1f.
> Rama `work/DX-1f`. Nada de la app de voz de la persona se tocó: no se cerró, no se
> le inyectó nada, no se le adjuntó un depurador, no se ejecutó ningún agente real y
> no se tocó su ventana de Herdr ni su `whisper-server`. Los PNG del chat salen del
> propio proceso de la prueba (ventana *layered* real) con `APPDATA` temporal.

## Resultado
- Brief/tarea: DX-1g (la ventana del chat tiene varias sombras; debe ser una)
- Estado: **completado**

## Cambios

- `rust/crates/ez-hud/src/win/chat/compose/shapes.rs`: `shadow_under` pinta **una**
  sombra suave con `rounded_shadow` (la caída del diseño) en una sola pasada, con el
  mismo alcance (12 px) y el mismo pico de alfa que dejaban las tres capas. Fuera el
  bucle de tres bandas.

- `rust/crates/ez-hud/src/win/chat/compose/raster.rs`: se retira `shade_frame`
  (quedó sin uso) y su prueba.

- `rust/crates/ez-hud/src/win/chat/window/tests.rs`:
  `the_panel_paints_one_soft_shadow` mide el borde en el panel quieto, en la
  apertura, en el cierre y tras redimensionar: el alfa crece sin escalones hacia el
  cuerpo, hay al menos 5 niveles (una caída suave, no tres bandas planas) y el pico
  nunca pasa de 40/255 (dos composiciones darían ~80).

- `src/ezagent/voice/hud/chat_paint.py`: `_shadow_under` hace la misma pasada única
  con `rounded_shadow`; `_shade_frame` se retira (sin uso). El oráculo Python cambia
  a la vez que el producto para que la paridad mida el diseño nuevo.

- `parity/golden/hud/chat/*.png` (6): regenerados con el generador de siempre
  (`parity/gen/hud_chat.py` sobre el Python cambiado). `parity/thresholds.toml` **no**
  se tocó. Solo cambian los casos con sombra visible: `morph-open-light-75/100`,
  `morph-open-dark-100`, `morph-open-circle-100`, `morph-close-dark-75` y
  `morph-open-reduced-light-50`.

- `rust-migration/reports/DX-1g/`: los PNG de evidencia (ver §1).

- `rust-migration/reports/DX-1g-r1.md` (nuevo): este informe.

## Decisiones

- **`// DECISION` una sola pasada con `rounded_shadow`.** Es la sombra suave que el
  diseño ya tenía (`chat_geometry.rounded_shadow`: «una sola pasada»); el chat era el
  único sitio que seguía usando tres bandas concéntricas, y al sumarse se ven como
  varias sombras apiladas. (`shapes.rs`, `chat_paint.py`)
- **`// DECISION` el pico conserva el alfa de las tres capas.** Con `k` = opacidad del
  morph, el pico es `1 − (1 − 0,055·k)³`: el borde más oscuro mide lo mismo que antes
  (40/255 con el panel abierto) y lo que cambia es la caída, que pasa de tres
  escalones a un degradado. Así el «alfa del borde» no cambia y la sonda de la prueba
  puede exigir «no más de una sombra».
- **`// DECISION` el oráculo Python se cambia y los PNG del morph del chat se
  regeneran.** La paridad del chat compara contra el PNG del Python; si solo cambiara
  el Rust, `hud_chat_parity` fallaría con los umbrales intactos. El generador es el
  mismo (`parity/gen/hud_chat.py`) y la regeneración es determinista: antes de tocar
  nada se regeneró con el Python original y los 12 PNG salieron idénticos a los del
  repo. `thresholds.toml` no se tocó.
- **`// DECISION` `shade_frame` se retira** en los dos lados: tras la pasada única no
  queda ningún uso (la prueba que lo cubría se va con él).

## Verificación

- `cargo fmt -p ez-hud -p ez-app -- --check` → limpio (exit 0).
- `cargo clippy --release -p ez-hud -p ez-app --all-targets -- -D warnings` →
  **exit 0** (los únicos avisos son del `freetype-sys` vendorizado).
- `cargo test --release -p ez-hud` → **573 unitarias + 20 de integración en verde**;
  incluye `hud_chat_parity` (`parity/thresholds.toml` intacto) y
  `the_panel_paints_one_soft_shadow`.
- `cargo test --release -p ez-app` → **359 unitarias + 3 del bin + 10 de integración
  en verde**.
- Humo en release: `chat`, `real`, `big` y `anim` → **exit 0**, vivos los 10 s, 100
  décimas, sin `PÁNICO`; pico de RSS 95 / 169 / 96 / 182 MB (tope 1 GB).
  `anim` sigue en verde (11 cuadros intermedios en la apertura, 8 en el cierre).
- La prueba de la sombra **falla si se compone dos veces**: se comprobó metiendo una
  segunda llamada a `shadow_under` (temporal): `peak 68` y la prueba en rojo; después
  se retiró.

---

## 1. Diagnóstico con evidencia

### 1.1 Qué capas dibujan sombra y cuántas veces

En el panel del chat **solo una** función dibuja sombra: `paint_shape`
(`compose/shapes.rs`) llama a `shadow_under` **una vez por cuadro compuesto**. Ni la
capa de contenido (`render_content`: cabecera, mensajes, campo) ni el parche del
cursor de DX-1d dibujan sombra. `stable` reusa el fotograma base (con su sombra) y
solo le compone encima el orbe y la línea de estado.

Lo que había dentro de esa única llamada eran **tres pasadas concéntricas**
(`for step in range(3)` en el Python, `for step in 0..3` en el Rust), cada una una
banda plana de alfa; al superponerse dejaban tres escalones visibles: eso es lo que
se ve como «varias sombras una sobre otra».

No se encontró ninguna composición duplicada:
- perfil del borde del panel quieto, de la apertura y del cierre: idéntico
  (14 / 27 / 40 en tres bandas de 4 px);
- mismo perfil en la ventana real de la prueba (`cuadro-*.png`) que en el PNG del
  oráculo;
- el máximo semitransparente es **40/255**, no 80: la sombra no se pinta dos veces.

### 1.2 Antes de DX-1b: era igual

`git log` de `ez-hud/src/win/chat/compose/shapes.rs`: el archivo no cambia desde
`ea697dd` (P-FIX-CHATMORPH), que es **anterior a `acacccd`** (DX-1b)
(`git merge-base --is-ancestor ea697dd acacccd^` → sí). El PNG del oráculo del chat
(`morph-open-light-100.png`) tampoco cambia desde `5310189` (F1-10c), también
anterior. O sea: la app de antes de DX-1b tenía exactamente las mismas tres bandas;
DX-1b no introdujo la sombra apilada.

### 1.3 Los PNG

En `rust-migration/reports/DX-1g/`:

| archivo | qué es |
|---|---|
| `antes-dx1b.png` | el cuadro del oráculo Python (472×672) de antes de DX-1b (`5310189`), con las tres bandas |
| `eco-main/chat-abierto.png` · `eco-main/morph-medio.png` | la ventana real del chat (466×736, tema propio, escala 0,85 del humo) **antes** del arreglo |
| `arreglo/chat-abierto.png` · `arreglo/morph-medio.png` | la misma ventana **después** del arreglo |
| `esquina-antes-dx1b.png` · `esquina-eco-main.png` · `esquina-arreglo.png` | la esquina superior izquierda ampliada ×4 |

La app real no se tocó: los PNG de `eco-main`/`arreglo` salen de los cuadros que el
propio proceso de humo entrega a su ventana *layered* (volcado temporal con
`EZAGENT_CHAT_PNG`, retirado después).

### 1.4 El alfa del borde

Perfil del borde izquierdo a la altura del centro, desde fuera hacia el cuerpo
(`x = 0..16`, y luego el cuerpo a 255), en el PNG del oráculo (el mismo valor que en
la ventana real):

| `x` | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **antes** | 0 | 0 | 0 | 14 | 14 | 14 | 14 | 14 | 27 | 27 | 27 | 27 | 40 | 40 | 40 | 40 | 255 |
| **después** | 0 | 0 | 0 | 0 | 1 | 1 | 3 | 5 | 7 | 10 | 13 | 17 | 21 | 26 | 31 | 37 | 255 |

- El **pico** es el mismo (40/255): no se suma nada.
- El **número de niveles** del borde pasa de 3 bandas planas (14, 27, 40; 9 940 +
  8 078 + 8 850 px cada una) a un degradado de 12 valores; en el cuadro completo hay
  25 516 px semitransparentes frente a 27 464, todos por debajo de 40.
- El alcance es el mismo (12 px de margen + el desplazamiento de 5 px hacia abajo).

### 1.5 Por qué se veía así

`_shadow_under` (Python) y su port (`shadow_under`, Rust) dibujaban tres sombras
concéntricas de 4 px, cada una con alfa plano, para simular una caída suave sin
desenfoque. A 1:1 el conjunto se lee como una sombra con escalones; en la esquina
ampliada se ven las tres bandas. El diseño tiene desde F2-12 una sombra suave de una
pasada (`rounded_shadow`), pero el chat nunca la usó.

## 2. El arreglo

`shadow_under` ahora:

1. calcula el alcance (`reach = round(16 · escala · 0,8)`, 12 px a escala 1) y el
   desplazamiento (`round(escala · 5)`) igual que antes;
2. pide `rounded_shadow(ancho, alto, radio, reach)` —la caída `(1 − d/reach)²`, con
   caché de 4 entradas— y la compone **una vez** con `shade_rows`, con ganancia
   `peak = 1 − (1 − k·0,10)³`;
3. el cuerpo se pinta encima como siempre y tapa el centro de la sombra.

Es la misma sombra en la apertura animada (DX-1f), el cierre, el redimensionado y con
el chat quieto: los cuatro estados pasan el mismo `paint_shape` y la prueba nueva los
recorre.

## 3. Pruebas

### 3.1 Unidad (`ez-hud`)

`win::chat::window::tests::the_panel_paints_one_soft_shadow`: con el panel a
440×640 (ventana 472×672), mide el borde izquierdo en `Open`, `Opening` (p≈0,98),
`Closing` (p≈0,89) y tras redimensionar a 472×732, y exige:

- alfa no decreciente hacia el cuerpo (sin escalón);
- pico ≤ 40 (una sola sombra; dos darían ~80);
- ≥ 5 niveles en el borde (una caída suave, no tres bandas planas).

Comprobación de que la prueba sirve: con una segunda llamada a `shadow_under`
(temporal) el perfil da `peak 68` y la prueba falla; retirada la llamada, verde.

### 3.2 Paridad

`hud_chat_parity` → **1 pasado**, con `parity/thresholds.toml` sin tocar. Los 6 PNG
del morph del chat se regeneraron con el Python cambiado; la regeneración es
determinista (se comprobó antes del cambio: los 12 PNG salían idénticos a los del
repo).

## 4. Humo (release)

```text
=== chat ===
chat abierto: true
RSS pico: 95 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== real ===
llamada iniciada · clic en gear · clic en chat
chat abierto: true
monkey: 1 ventana(s) del chat recibieron ratón
RSS pico: 169 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== big (320 mensajes) ===
chat abierto: true
rueda: 120 muescas de 32767 · hasta arriba · vivo tras arriba: true
rueda: 120 muescas de -32767 · hasta abajo · vivo tras abajo: true
RSS pico: 96 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== anim ===
apertura: 11 cuadros intermedios · primer p 0.000 · contenido a los 297.6 ms
cierre: 8 cuadros intermedios · primer cuadro abierto a los 280.6 ms
RESULT anim opening_frames=11 closing_frames=8 first_p=0.000
RSS pico (anim): 182 MB
estado: la apertura se anima
registro temporal: sin PÁNICO
```

Los cuatro con exit 0; ninguno pasa de 1 GB.

## Pendiente o riesgos

- **El oráculo cambia de diseño.** Los 6 PNG del morph del chat ahora tienen la
  sombra suave; el Python de referencia se cambió a la vez para que sigan siendo el
  mismo diseño. Si el elefante prefiere mantener el oráculo viejo, hay que revertir
  los PNG y el `_shadow_under` del Python junto con el Rust (el cambio es simétrico).
- **El pico de la sombra no cambia (40/255)**, pero el reparto sí: el borde es algo
  más claro en los primeros píxeles y más oscuro cerca del cuerpo. Es lo que hace que
  se lea como una sola sombra.
- **`rounded_shadow` tiene caché de 4 entradas** (FIFO): redimensionar en ráfaga
  puede recalcularla; el coste es de una pasada por tamaño nuevo, no por cuadro.
- **Los PNG de `eco-main/` tienen el panel vacío**: el historial rico del humo de
  DX-1d tarda >10 s en maquetarse en esta máquina (palabra de 2 000 caracteres,
  `split_word` O(n²) en `ez-gfx`), así que el cuadro capturado es la forma sin
  contenido. La sombra no depende del contenido.
- **La app de la persona no se abrió**; la evidencia son los cuadros del proceso de
  prueba y la paridad.

## Anexo: comandos

```powershell
cd rust
cargo fmt -p ez-hud -p ez-app -- --check
cargo clippy --release -p ez-hud -p ez-app --all-targets -- -D warnings
cargo test --release -p ez-hud
cargo test --release -p ez-app
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real
cargo run --release -p ez-app --example diag_chat_smoke -- big
cargo run --release -p ez-app --example diag_chat_smoke -- anim

# PNG del oráculo del morph del chat (con el Python cambiado): se llama
# parity.gen.hud_chat.morph_cases con un FixtureFile de descarte; escribe los
# PNG en parity/golden/hud/chat/ y no toca los fixtures ni el manifiesto.
```
