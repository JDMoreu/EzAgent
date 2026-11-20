# DX-1h — r1 · La prueba de la sombra única falla en depuración

> Encargo: **DX-1h** (`rust-migration/briefs/DX-1h.md`), sobre la entrega de DX-1f
> y DX-1g. Rama `work/DX-1h`. Nada de la app de voz de la persona se tocó: no se
> cerró, no se le inyectó nada, no se le adjuntó un depurador, no se ejecutó ningún
> agente real y no se tocó la ventana de Herdr ni su `whisper-server`. Los humos son
> ventanas del propio proceso de la prueba, con un `APPDATA` temporal
> (`diag_chat_smoke.rs`), y solo se compiló y probó `ez-hud` (más `ez-app` para el
> humo), siempre en primer plano y con un solo `cargo` a la vez.

## Resultado
- Brief/tarea: DX-1h (la prueba de la sombra única falla en debug y pasa en release)
- Estado: **completado**

## Cambios

- `rust/crates/ez-hud/src/win/chat/window/tests.rs`: en
  `the_panel_paints_one_soft_shadow`, el cuadro que se mide tras redimensionar
  vuelve a dejar el panel en `Open` antes de pedirlo. El bucle anterior lo dejaba
  en `Closing` y ese cuadro salía a mitad del cierre (sin sombra), así que el
  resultado dependía del reloj real. La prueba no cambia lo que exige.
- `rust-migration/reports/DX-1h-r1.md` (nuevo): este informe.

No se tocó el producto: el fallo era de la prueba (ver §1), no de `ez-hud`.

## Decisiones

- **`// DECISION` el cuadro del redimensionado se mide con el panel abierto.**
  Es lo que hace determinista la prueba: el muestreo del cierre ya tiene su
  propio cuadro, y el redimensionado declara medir «el mismo panel en una ventana
  más alta». (`window/tests.rs`)
- **No se cambia el apagado de la sombra durante el morph.** El progreso apaga la
  sombra por debajo de `p = 0,6` (`shadow_k = smoothstep((p − 0,6)/0,4)`) y el
  oráculo Python hace exactamente lo mismo (`chat_paint.py:119`): es el diseño,
  no un fallo del producto. Cambiarlo habría roto la paridad.

## Verificación

- `cargo fmt -p ez-hud -- --check` → limpio (exit 0).
- `cargo clippy -p ez-hud --all-targets -- -D warnings` (debug) → **exit 0**.
- `cargo clippy --release -p ez-hud --all-targets -- -D warnings` → **exit 0**.
- `cargo test -p ez-hud` (debug) → **573 unitarias + 20 de integración en verde**
  (todos los objetivos con `test result: ok`; antes de este brief la unitaria de
  la sombra fallaba siempre).
- `cargo test --release -p ez-hud` → **573 unitarias + 20 de integración en verde**.
- `the_panel_paints_one_soft_shadow` en debug, cinco corridas seguidas → **5/5 en
  verde**.
- Con una segunda llamada a `shadow_under` (temporal): la prueba falla en **debug
  y en release** con `the shadow is composed more than once (Open, peak 68)`;
  retirada la llamada, verde en los dos modos.
- Humo en release: `chat` y `anim` → **exit 0**; `chat` vivo los 10 s, 100
  décimas, RSS pico 95 MB; `anim` con 11 cuadros intermedios en la apertura y 8 en
  el cierre, contenido a los 301 ms, RSS pico 190 MB, sin `PÁNICO`.
- Ninguna prueba ni medición pasó de 1 GB de memoria.

---

## 1. Diagnóstico con evidencia

### 1.1 Reproducción

```text
cargo test -p ez-hud --lib the_panel_paints_one_soft_shadow
→ FAILED … the shadow is stepped, not soft (Open):
  [0, 0, …, 0, 255, 255, …]   (16 ceros y el cuerpo desde x = 16)
```

El mensaje dice `(Open)` y no lo era: la aserción que falla es la **cuarta**, la
del redimensionado (`assert_one_soft_shadow(&frame, 366, AnimState::Open)`), que
recibe `AnimState::Open` solo como rótulo. El perfil que imprime es correcto para
el cuadro que estaba midiendo; el cuadro no era el que la prueba decía.

### 1.2 Qué cuadro falla

Con una traza temporal en `paint_shape` (retirada después), los cuatro cuadros de
la prueba en **debug** componen con estos valores:

| # | cuadro | `p` | `shadow_k` | rect (`y1`) |
|---|---|---|---|---|
| 1 | `Open` (quieto) | 1,000 | 1,000 | 656,0 |
| 2 | `Opening` | 0,9988 | 0,9995 | 656,0 |
| 3 | `Closing` (muestra del bucle) | 0,9375 | 0,9344 | 656,0 |
| 4 | **redimensionado** | **0,2295** | **0** | **669,8** |

El bucle de la prueba termina en `Closing` (con `t0` a 0,05 s del reloj real) y
**no lo reinicia** antes de redimensionar y pedir el cuarto cuadro. Ese cuadro se
compone **a mitad del cierre**: `rect` es la forma crecida un 23 % y el morph ya
apagó la sombra. En **release** el mismo cuadro sale con `p = 0,9247` y
`shadow_k = 0,907`, así que sí lleva sombra.

### 1.3 Por qué debug sí y release no

El reloj de la animación es real. Entre fijar `t0 = monotonic() − 0,05` (muestra
del cierre) y el `monotonic()` del cuarto cuadro pasan:

| | deriva | `p` del cierre en el cuadro 4 | sombra |
|---|---|---|---|
| debug | **≈ 73 ms** (0,1229 s en vez de 0,05) | 0,2295 | no (`shadow_k = 0`) |
| release | **≈ 3 ms** (0,0532 s) | 0,9247 | sí (`shadow_k = 0,907`) |

La deriva es el coste de componer el cuadro del cierre y de `window.resize()` en
debug. No hay ninguna diferencia de producto entre perfiles: el mismo binario de
prueba, con los mismos cuadros, da `p` distinto porque el cierre siguió
avanzando por el reloj de pared.

Comprobación de que el cuadro del cierre no depende de la composición: `render`
lee `now` al principio y `step` usa ese mismo instante; el trabajo caro
(maquetación y composición) ocurre después y no mueve el `p` del cuadro. La única
deriva posible es el hueco entre fijar `t0` y el inicio del cuadro, que es de
microsegundos. La del cuadro 4 existía porque entre ambos había **otro cuadro y
un redimensionado**.

### 1.4 El producto sí pinta la sombra

En el cuadro 1 (el `Open` de verdad), justo después de `shadow_under`, el lienzo
tiene:

```text
[0, 0, 0, 0, 1, 1, 3, 5, 7, 10, 13, 17, 21, 26, 31, 37, 40, 40, 40, 40]
 x=0 …                                      x=15  cuerpo
```

y el `Frame` que recibe la prueba tiene exactamente el mismo perfil, en debug y en
release. O sea: la sombra se pinta, no se pierde en la conversión ni en la caché.
Los cuadros 2 y 3 tienen perfiles suaves equivalentes (el 3, un poco más claro
porque `shadow_k = 0,934`).

El único cuadro sin sombra es el 4 de debug, con `p = 0,2295`. Ahí la sombra se
apaga **a propósito**: `shadow_k = smoothstep((p − 0,6)/0,4)` en la rama `fly`
(`compose/shapes.rs::shape_geometry`), y el oráculo Python tiene la misma línea
(`src/ezagent/voice/hud/chat_paint.py:119`) y la misma caída en la rama de
fundido (`shadow_k = p`, línea 126). Con la forma encogida hacia la barra, el
diseño no dibuja sombra.

**Conclusión:** artefacto de la prueba (heredaba el estado `Closing` de la
iteración anterior), no un fallo del producto.

## 2. El arreglo

Antes de redimensionar, la prueba vuelve a dejar el panel en `Open`:

```rust
{
    let mut inner = panel.state.lock();
    inner.anim = Anim { state: AnimState::Open, t0: 0.0, p: 1.0 };
}
window.resize(472, 732);
let frame = panel.render();
assert_one_soft_shadow(&frame, 366, AnimState::Open);
```

El cuadro del redimensionado pasa a ser un cuadro estable (`p = 1`) y deja de
depender del tiempo que haya tardado el cierre. La prueba sigue midiendo lo mismo:
alfa no decreciente hacia el cuerpo, pico ≤ 40 y al menos 5 niveles, en `Open`,
`Opening`, `Closing` y tras redimensionar.

## 3. La prueba sigue sirviendo

Con una segunda llamada a `shadow_under` en `paint_shape` (temporal, retirada
después):

```text
debug   → FAILED: the shadow is composed more than once (Open, peak 68)
release → FAILED: the shadow is composed more than once (Open, peak 68)
          [0, 0, 0, 0, 1, 3, 6, 9, 14, 20, 26, 33, 41, 49, 58, 68, 255, …]
```

Es el mismo resultado que documentó DX-1g: dos composiciones dan `peak 68` y la
prueba las detecta en los dos perfiles, así que no se aflojó nada.

## 4. Humo (release)

```text
=== chat ===
tema: light · escala: 1 · layout: pill
chat abierto: true
RSS pico: 95 MB (décimas: 100)
registro temporal: sin PÁNICO
RESULT chat_open=true alive_secs=10

=== anim ===
apertura: 11 cuadros intermedios · primer p 0.000 · contenido a los 301.1 ms
cierre: 8 cuadros intermedios · primer cuadro abierto a los 284.1 ms
RESULT anim opening_frames=11 closing_frames=8 first_p=0.000 content_ms=301.1
RSS pico (anim): 190 MB
estado: la apertura se anima
registro temporal: sin PÁNICO
```

Los dos con exit 0 y por debajo de 1 GB.

## Pendiente o riesgos

- **`window/tests.rs` sigue por encima de 600 líneas** (867; ya tenía 854 antes de
  este brief). Es un archivo de pruebas y partirlo es un movimiento mecánico de
  varios cientos de líneas que no pide este brief; queda anotado, como en DX-1f.
- **Queda una dependencia mínima del reloj real**: cada muestra fija su `t0` justo
  antes de su cuadro y `render` lee `now` al empezar, así que la deriva es de
  microsegundos. La más apretada es la del cierre: se toma a 50 ms de 200 y deja
  de tener sombra a los ~93 ms, o sea que un parón del hilo de más de ~40 ms entre
  fijar `t0` y el inicio del cuadro la haría fallar. No hay trabajo entre esas dos
  llamadas y es la exposición que ya tenía la prueba; el fallo sistemático de
  debug era otro (un cuadro intermedio por medio) y está cerrado.
- **El apagado de la sombra por debajo de `p = 0,6` es diseño** (Rust y Python a
  la vez). Si el elefante quiere sombra durante todo el cierre, hay que cambiarlo
  en los dos lados y regenerar los PNG de paridad del morph; queda fuera de este
  brief.
- **No se comprobó en la app en marcha**: la app de voz de la persona no se tocó.
  La evidencia son la traza de los cuatro cuadros, las pruebas y los humos.

## Anexo: comandos

```powershell
cd rust
cargo fmt -p ez-hud -- --check
cargo clippy -p ez-hud --all-targets -- -D warnings
cargo clippy --release -p ez-hud --all-targets -- -D warnings
cargo test -p ez-hud
cargo test --release -p ez-hud

cargo build --release -p ez-app --example diag_chat_smoke
./target/release/examples/diag_chat_smoke.exe chat
./target/release/examples/diag_chat_smoke.exe anim
```

Commits de la rama para este brief:

```text
df990ca DX-1h: la prueba de la sombra mide el redimensionado con el panel abierto
```
