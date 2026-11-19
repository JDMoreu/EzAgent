# DX-1d — r1 · La app se cae al abrir el chat: blindaje de pánicos en callbacks de Windows

> Encargo: **DX-1d** (`rust-migration/briefs/DX-1d.md` + `DX-1d-relevo.md`).
> Rama `work/DX-1d` (desde `work/DX-1b`, con `work/DX-2` fusionado para reproducir y
> para que el humo final sea la combinación que irá a la app de la persona).
>
> Nada de la app que la persona tiene abierta se tocó: no se cerró, no se le
> inyectó nada, no se le adjuntó un depurador, no se ejecutó ningún agente real,
> no se tocó su ventana de Herdr ni su `whisper-server`, y **no se leyó su
> historial** (`%APPDATA%\EzAgent\history`). Solo se leyeron las claves de
> interfaz de su `voice.json` (tema, forma, escala y tamaño del chat; sin
> secretos) para que el humo use la misma apariencia.

## Resultado
- Brief/tarea: DX-1d (la app se cae al abrir el chat, `0xc0000409`)
- Estado: **completado** (con una salvedad honesta: el *disparador* exacto del
  pánico original no se pudo reproducir; el *desenlace* fatal sí, y está
  blindado y cubierto por pruebas — ver §2 y §6)

## Cambios

### El blindaje (lo que impide que vuelva a matar la app)

- `rust/crates/ez-core/src/panics.rs` (nuevo): hook permanente de pánicos y
  `catch_panic`. El hook escribe `[fecha] PÁNICO: mensaje (archivo:línea:columna)`
  **de forma síncrona** en el archivo diario del registro antes de abortar
  (el `tracing` de la app es asíncrono y se pierde al abortar). `catch_panic`
  atrapa un pánico en un borde FFI y lo registra.
- `rust/crates/ez-core/src/lib.rs`: expone `panics`.
- `rust/crates/ez-app/src/app/main.rs`: `init_logging_in` instala el hook en la
  misma carpeta del registro (`%APPDATA%\EzAgent\logs`, o `EZAGENT_LOGS_DIR`)
  **antes** de crear el logger.
- `rust/crates/ez-hud/src/win/wndproc.rs`: el WndProc único de las ventanas
  *layered* (barra, tarjeta/menú, chat, vista rápida) ejecuta el despacho dentro
  de `catch_panic`; si algo entra en pánico, se registra y el mensaje sigue a
  `DefWindowProcW`.
- `rust/crates/ez-hud/src/win/wheel.rs`: el gancho global `WH_MOUSE_LL` también
  atrapa el pánico.
- `rust/crates/ez-app/src/tray.rs`: el WndProc de la bandeja, igual.
- `rust/crates/ez-audio/src/capture/portaudio/dsound.rs`: el callback de
  enumeración de DirectSound, igual.
- `rust/crates/ez-hud/src/win/ui_loop.rs` no necesita el blindaje: su WndProc
  solo llama a `DefWindowProcW` (no ejecuta código Rust que pueda entrar en
  pánico).

### Los arreglos de causa (lo que ya no debería entrar en pánico)

- `rust/crates/ez-hud/src/win/chat/window.rs`: si la maquetación de un mensaje
  entra en pánico, **la vista vuelve al panel**. Antes (DX-1b) la vista se
  tomaba para pintar sin el candado y se perdía al desenrollar: un pánico dejaba
  el chat muerto en 1×1 para siempre aunque la app siguiera viva. Además se
  guarda una capa de contenido vacía con la firma del fallo para no repetir el
  mismo pánico en cada cuadro.
- `rust/crates/ez-hud/src/model/chat/footer.rs`: `place_caret` ya no indexa una
  lista de líneas vacía. Un clic dentro del campo antes del primer cuadro de
  contenido (la vista se abre sin maquetar desde DX-1b) entraba en pánico desde
  el `WndProc`.
- `rust/crates/ez-hud/src/win/chat/compose/raster.rs` y `compose.rs`: el parche
  del cursor recorta contra la **capa de contenido** y no solo contra el lienzo.
  La prueba nueva de regresión encontró que con una capa de contenido de 1×1
  `composite_caret` indexaba fuera de rango (pánico de cuadro).
- `rust/crates/ez-core/src/panics.rs`: el archivo diario se elige por fecha de
  modificación **y nombre** (dos archivos escritos en el mismo tic podían
  desempatar mal) y la prueba que lo cubre comparaba mal el sufijo.

### Diagnóstico y pruebas

- `rust/crates/ez-hud/examples/panic_boundary.rs` (nuevo): reproduce el
  desenlace fatal de forma determinista (un `WindowHooks::pointer_down` que
  entra en pánico + un `WM_LBUTTONDOWN` real) y demuestra el blindaje.
- `rust/crates/ez-app/examples/diag_chat_open.rs`: casos por tema (incluye
  `light`, el que resuelve el tema propio de la persona) y tamaño 434×704.
- `rust/crates/ez-app/examples/diag_chat_smoke.rs`: humo de la app real con la
  secuencia de la persona (llamada → clic real del engranaje → 3 s → clic real
  del chat → ratón sobre el panel → 10 s). Sin `SendInput`: los clics son
  mensajes a ventanas propias, así que no se mueve el ratón de la persona.
  Modos: `chat`, `sequence`, `real`, `monkey`, `fuzz`.
- `rust/crates/ez-hud/src/win/chat/window/tests.rs` y
  `rust/crates/ez-hud/src/model/chat/footer/tests.rs`: pruebas de regresión de
  los dos arreglos.
- Fusión de `work/DX-2` en la rama (lo pide la nota de relevo), sin conflictos.

## Decisiones

- **`# DECISION` el hook escribe con `std::fs`, no con `tracing`.** El registro
  de la app usa `tracing_appender::non_blocking`; al abortar, el búfer se
  pierde. La línea del pánico tiene que estar en disco *antes* del aborto.
- **`# DECISION` `install_hook_in(dir)` recibe la carpeta.** Así las pruebas y
  los ejemplos usan una temporal y no el registro de la persona.
- **`# DECISION` el `catch_panic` no duplica la línea.** El hook ya la escribe;
  el borde solo añade el `tracing::error!`.
- **`# DECISION` tras un fallo de maquetación se guarda una capa de 1×1 con la
  firma.** Sin eso, cada cuadro volvía a maquetar y a entrar en pánico; con
  eso, el panel queda vivo (contenido vacío) y no se repite el error.
- **`# DECISION` la vista se restaura solo si el panel no se cerró ni instaló
  otra vista mientras se pintaba.** Si se cerró, la vista se descarta.
- **`# DECISION` el humo abre el chat con mensajes de ratón reales a la barra**
  (`WM_MOUSEMOVE`/`WM_LBUTTONDOWN`/`WM_LBUTTONUP`) en vez de llamar a
  `hud_action_ui`. Usa `set_buttons("always")` (pensado para pruebas) porque los
  botones de la barra solo se muestran con el cursor físico encima y el humo no
  mueve el ratón de la persona.
- **`# DECISION` no se lanza el `ezagent-voice.exe` real con micrófono.** Para no
  tocar el audio de la persona ni arrancar un segundo `whisper-server` (riesgo de
  huérfano, DX-3), el humo monta la misma `VoiceApp` con micrófono silencioso y
  ventanas nativas reales.

## Verificación

- `cargo fmt -p ez-core -p ez-hud -p ez-app -p ez-audio -- --check` → limpio.
- `cargo clippy --release -p ez-core -p ez-hud -p ez-app -p ez-chat -p ez-audio --all-targets -- -D warnings` → **exit 0** (los únicos avisos son del `freetype-sys` vendorizado, ya existentes).
- `cargo test --release -p ez-core` → **67 unitarias + integraciones en verde**.
- `cargo test --release -p ez-hud` → **567 unitarias + integraciones en verde** (incluye `hud_chat_parity`; `parity/thresholds.toml` no se tocó).
- `cargo test --release -p ez-app` → **359 unitarias + integraciones en verde**.
- `cargo test --release -p ez-chat` → **83 unitarias + integraciones en verde**.
- `cargo test --release -p ez-audio` → **110 unitarias + integraciones en verde**.
- Humo final (DX-1b + DX-2, la combinación que irá a la persona): chat abierto y
  vivo 10 s, **pico de RSS 246,5 MB** (muy por debajo del tope de 1 GB).
- Ninguna prueba ni medición pasó de 1 GB.

## 1. Qué pasó (evidencia del incidente)

Del registro de aplicaciones de Windows (evento 1000 del 19/11/2026 13:52:15):

```text
Nombre de la aplicación con errores: ezagent-voice.exe, versión: 0.2.0.0
Módulo con errores: ezagent-voice.exe
Código de excepción: 0xc0000409
Desplazamiento con errores: 0x0000000000be3f2e
Datos de excepción: 0000000000000007
```

`0xc0000409` + subcódigo **7** = `FAST_FAIL_FATAL_APP_EXIT`: es el `abort()`
que ejecuta el runtime de Rust, no una violación de acceso ni un desbordamiento
de pila.

El ejecutable que se cayó se compiló a las **13:49:11** desde `7ce0bbb`
(`DX-1b + DX-2` en `eco/main`) y se **sobrescribió a las 13:54** al revertir
DX-1b; no queda copia ni volcado.

Para ubicar el desplazamiento se recompiló `7ce0bbb` limpio (release, por
defecto) en un *worktree* temporal y se simbolizó con
`rust-migration/tools/orquestacion/symbolize.py`:

```text
0xbe3f2e -> std::panicking::panic_with_hook  [/rustc/.../library\std\src\panicking.rs]
0xf276d0 -> mainCRTStartup                   (control: el PDB es el correcto)
```

O sea: el proceso murió **dentro del runtime de pánicos**, no en el código del
chat. Eso encaja con un pánico que intenta desenrollarse a través de un
`extern "system"` (WndProc, gancho de rueda o bandeja): Rust no puede
propagarlo, imprime `panic in a function that cannot unwind` y aborta con
`0xc0000409`. Es exactamente el desenlace que reproduce §2.

## 2. Reproducción antes/después

### 2.1 La secuencia de la persona (el disparador no se reprodujo)

Arnés: `ez-app/examples/diag_chat_smoke.rs`, modo `real`.

- `VoiceApp` real: barra y panel del chat con sus ventanas nativas y su
  `WndProc`; micrófono de prueba (silencio) y altavoz mudo.
- `APPDATA` temporal: no se toca el historial, la configuración ni el registro
  de la persona.
- Apariencia de la persona: `theme = custom` (base `light`), `ui_layout = pill`,
  `ui_scale = 0.85`, chat recordado **434×704**, `chat_voice = true`.
- Historial sintético de **35 mensajes**: markdown, encabezados, negrita,
  cursiva, código, listas anidadas, cita, tabla, enlace, emojis (incluido un
  ZWJ y una bandera), palabra de 2 000 caracteres sin espacios, mensajes
  vacíos, mensaje de sistema y una imagen que no existe.
- Secuencia: **llamada iniciada** → clic real en el engranaje de la barra
  (`WndProc` → `HUD: panel`) → 3 s → clic real en el botón del chat
  (`WndProc` → `HUD: chat`) → movimientos y clics de ratón dentro del panel
  recién abierto → 10 s de vida.

Resultado, **igual antes (`7ce0bbb`) y después (esta rama)**:

```text
llamada iniciada
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en gear de la barra (20,12)
clic del panel enviado
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en chat de la barra (56,12)
chat abierto: true
monkey: 1 ventana(s) del chat recibieron ratón
RESULT chat_open=true alive_secs=10
EXIT=0
```

**El pánico original no se reprodujo** con historial sintético, ni con la
secuencia exacta, ni con el tema y el tamaño de la persona, ni con el menú
abierto y la llamada activa. Es la misma conclusión a la que llegó el relevo
anterior con DX-1b solo. El disparador concreto queda **sin confirmar** (§6).

### 2.2 El desenlace fatal (esto sí, determinista)

Arnés: `ez-hud/examples/panic_boundary.rs`. Crea una ventana *layered* real con
un `WindowHooks` que entra en pánico en `pointer_down` y le entrega un
`WM_LBUTTONDOWN` real (reintentando hasta que el primer fotograma está
presentado).

**Antes (`7ce0bbb`, sin blindaje):**

```text
thread 'ezagent-dx1d-panic' panicked at crates\ez-hud\examples\panic_boundary.rs:33:9:
DX-1d: pánico de prueba dentro de un callback de Windows
thread 'ezagent-dx1d-panic' panicked at ...\library\core\src\panicking.rs:225:5:
panic in a function that cannot unwind
...
  17: ... core::panicking::panic_cannot_unwind
...
thread caused non-unwinding panic. aborting.
EXIT -1073740791  0xC0000409      ← el mismo código que la persona
(stdout vacío: no llegó a imprimir «VIVO»)
```

Con el hook copiado en ese árbol para poder ver el mensaje, el registro deja:

```text
[2026-11-19 16:40:02.567] PÁNICO: DX-1d: pánico de prueba dentro de un callback de Windows (crates\ez-hud\examples\panic_boundary.rs:33:9)
[2026-11-19 16:40:02.569] PÁNICO: panic in a function that cannot unwind (.../core/src/panicking.rs:225:5)
```

**Después (esta rama):**

```text
[2026-11-19 16:39:49.823] PÁNICO: DX-1d: pánico de prueba dentro de un callback de Windows (crates\ez-hud\examples\panic_boundary.rs:33:9)
VIVO: el pánico quedó atrapado en el callback
EXIT=0
```

La app ya no muere: el pánico se registra con mensaje y ubicación y la ventana
sigue respondiendo.

## 3. La prueba de humo (regla nueva de interfaz)

La combinación que irá a la app de la persona es **DX-1b + DX-2** (la rama
fusiona `work/DX-2`). El humo monta la app real con micrófono silencioso
(no hay micrófono real), `APPDATA` temporal y la apariencia de la persona:

```powershell
cd rust
cargo run --release -p ez-app --example diag_chat_smoke -- chat   # abre el chat por el atajo
cargo run --release -p ez-app --example diag_chat_smoke -- real   # llamada + clic panel + 3 s + clic chat
```

```text
=== chat (humo corto) ===
tema: light · escala: 1 · layout: pill
chat abierto: true
RESULT chat_open=true alive_secs=10
EXIT=0

=== real (secuencia exacta con llamada y panel) ===
tema: light · escala: 1 · layout: pill
llamada iniciada
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en gear de la barra (20,12)
clic del panel enviado
barra: cliente 262x53 bs 0.85 centro Some("pill")
clic en chat de la barra (56,12)
chat abierto: true
monkey: 1 ventana(s) del chat recibieron ratón
RESULT chat_open=true alive_secs=10
EXIT=0
```

Pico de memoria del proceso: **246,5 MB** (medido con
`Get-Process … PeakWorkingSet64` durante toda la corrida).

No se lanzó el `ezagent-voice.exe` real con micrófono: para no tocar el audio
de la persona ni arrancar un segundo `whisper-server` (DX-3 documentó los
huérfanos), el humo usa la misma `VoiceApp` con `MicCapture::silent` y un
`SpeakerApi` mudo. Las ventanas (barra, tarjeta del menú y chat) sí son las
nativas y el camino del clic es el real.

## 4. Las pruebas de regresión

| prueba | archivo | qué fija |
|---|---|---|
| `a_panicking_layout_keeps_the_view_and_the_panel_recovers` | `ez-hud/src/win/chat/window/tests.rs` | un pánico en la maquetación no pierde la vista: el cuadro siguiente se pinta (con el código de DX-1b esta prueba falla) |
| `placing_the_caret_before_the_first_frame_does_not_panic` | `ez-hud/src/model/chat/footer/tests.rs` | un clic en el campo antes del primer cuadro de contenido no indexa líneas vacías |
| `panics::tests::*` | `ez-core/src/panics.rs` | el hook escribe mensaje y ubicación; el archivo diario se reutiliza; `catch_panic` no mata el proceso |
| `panic_boundary` (ejemplo manual) | `ez-hud/examples/panic_boundary.rs` | antes: `0xC0000409`; después: vivo y con la línea en el registro |

La prueba del panel encontró además el índice fuera de rango de
`composite_caret` con una capa de contenido pequeña, que quedó corregido en
§Cambios.

## 5. Qué cambia en el comportamiento de la app

- Un pánico en cualquier callback de Windows (WndProc de las ventanas
  *layered*, gancho de rueda, bandeja o enumeración de DirectSound) **ya no
  cierra la app**: se registra con mensaje y ubicación y la app sigue.
- El registro queda en `%APPDATA%\EzAgent\logs\ezagent.log.<fecha>` (o en
  `EZAGENT_LOGS_DIR`), con la línea `PÁNICO: …` escrita de forma síncrona antes
  de cualquier aborto.
- Si la maquetación de un mensaje falla (por ejemplo, una paleta incompleta o
  un markdown raro), el chat muestra el último cuadro y sigue vivo; antes la
  vista se perdía y el panel quedaba congelado en 1×1.
- Un clic en el campo de texto antes del primer cuadro de contenido ya no entra
  en pánico (se ignora hasta que hay líneas).

## 6. Lo que no se pudo confirmar y riesgos

- **El disparador exacto del pánico original no se reprodujo.** El historial
  sintético de 35 mensajes con todo lo que el brief enumera (markdown, código,
  listas, enlaces, emojis, texto largo, mensajes vacíos, imágenes, tema propio,
  escala 0,85, chat 434×704, menú abierto y llamada activa) no lo dispara ni en
  `7ce0bbb` ni en esta rama; el fallo necesita probablemente la sesión real
  (micrófono/STT/estado del agente) o una condición transitoria que no se puede
  sintetizar. **Con el hook instalado, si vuelve a ocurrir, el registro dirá el
  mensaje y el `archivo:línea:columna` exactos**, que es justo lo que faltó esta
  vez.
- El blindaje convierte un fallo fatal en un fallo parcial: la operación que
  entró en pánico puede no completarse (por ejemplo, un clic que no se atiende).
  Es preferible a cerrar la app; queda registrado.
- `catch_panic` usa `AssertUnwindSafe`: si el pánico ocurre a mitad de una
  mutación, el estado compartido puede quedar inconsistente. `parking_lot` no
  envenena los candados y la alternativa era abortar.
- La capa de contenido vacía tras un fallo de maquetación deja el área de
  contenido en blanco hasta que cambie la firma. Es el precio de no repetir el
  pánico en cada cuadro.
- El recorte nuevo del parche del cursor solo actúa cuando el cursor se sale de
  la capa de contenido; la paridad visual con los PNG dorados no cambió
  (`thresholds.toml` intacto).
- El humo no ejercita el micrófono real ni whisper; la parte de audio de la
  app no se validó en vivo (no se puede sin tocar la de la persona).

## 7. Anexo: comandos

```powershell
cd rust
cargo fmt -p ez-core -p ez-hud -p ez-app -p ez-audio -- --check
cargo clippy --release -p ez-core -p ez-hud -p ez-app -p ez-chat -p ez-audio --all-targets -- -D warnings
cargo test --release -p ez-core
cargo test --release -p ez-hud
cargo test --release -p ez-app
cargo test --release -p ez-chat
cargo test --release -p ez-audio

# reproducción del desenlace
cargo run --release -p ez-hud --example panic_boundary

# humo de la app real
cargo run --release -p ez-app --example diag_chat_smoke -- chat
cargo run --release -p ez-app --example diag_chat_smoke -- real

# diagnóstico del contenido con el tema de la persona
cargo run --release -p ez-app --example diag_chat_open
```

La comprobación de símbolos del ejecutable caído:

```powershell
# worktree temporal en 7ce0bbb, compilación limpia en release
cargo build --release -p ez-app --bin ezagent-voice
python -I rust-migration\tools\orquestacion\symbolize.py `
    <exe_de_7ce0bbb> <exe_de_7ce0bbb> 0xbe3f2e
# -> std::panicking::panic_with_hook
```
