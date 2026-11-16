# P-IMP-BARGEIN — r1

## Resultado
- Brief/tarea: P-IMP-BARGEIN (interrumpir con voz normal: umbral de barge-in que
  descuenta el eco de lo que la app está diciendo)
- Estado: completado

## Cambios
- `rust/crates/ez-audio/src/barge_in/adaptive.rs`: `PlaybackReference` (rejilla
  de 32 `AtomicU64` con marca de tiempo y nivel RMS cuantizado; sin locks),
  `AdaptiveSettings`, `AdaptiveBargeIn` (umbral por bloque
  `máx(piso×1,8, acople×referencia×1,25, 0,0012)`, aprendizaje del acople con la
  mediana de `mic/ref` emparejando el retardo altavoz→micrófono, resumen por
  frase) y `BargeInMode` (`fixed` | `adaptive`).
- `rust/crates/ez-audio/src/barge_in/adaptive/tests.rs`: pruebas del anillo
  atómico, de la ventana de 350 ms, de la mediana, del aprendizaje sin voz, de
  los topes del acople y del resumen.
- `rust/crates/ez-audio/src/barge_in.rs`: exporta el detector adaptativo y el
  modo; `BargeInDetector` (Python) intacto.
- `rust/crates/ez-audio/src/capture/mic.rs`: `VadSettings.barge_in_mode`;
  `CapturePipeline::with_reference`; cada bloque mientras la app habla toma el
  pico de la referencia (350 ms) y el bloque actual, calcula el umbral y lo
  aplica al `BargeInDetector` (misma regla de duración y huecos, misma racha
  reinyectada). Sin referencia → modo clásico. Línea `INFO` por frase del TTS
  con `mic_max`, `eco`, `acople`, `umbral` e `interrumpió`.
- `rust/crates/ez-audio/src/capture/mic/live.rs`: `barge_in_mode` en vivo
  (`AtomicU8`) y la referencia de reproducción compartida
  (`Mutex<Option<Arc<PlaybackReference>>>`, cerrojo de microsegundos).
- `rust/crates/ez-audio/src/capture/mic/tests.rs`: escenarios sintéticos (solo
  eco 0,3/0,5/0,8 no dispara; eco + voz moderada dispara; modo `fixed` idéntico
  al actual; clics y ruido constante no disparan; tras el barge-in la frase
  empieza por la racha reinyectada).
- `rust/crates/ez-audio/tests/barge_in_adaptive.rs`: validación con los WAV de
  `parity/replay/audio/` («Dime algo largo» como referencia, «Espera, para.»
  como voz) mezclados con eco retrasado 150 ms y ruido determinista.
- `rust/crates/ez-tts/src/speaker.rs`: el `Speaker` posee la
  `PlaybackReference` y la pasa a `play_samples`, que abre y cierra la frase
  (`begin`/`end` también al cortar). Nuevo `Speaker::reference()`.
- `rust/crates/ez-tts/src/speaker/cpal.rs`: la salida real con cpal, extraída de
  `speaker.rs` para no pasar de 600 líneas; `finish` publica la envolvente RMS
  de cada bloque de 480 muestras al ritmo real de envío al dispositivo (el
  `write` solo acumula).
- `rust/crates/ez-tts/src/speaker/tests.rs`: la referencia queda inactiva al
  terminar o cortar la frase y el número de frase avanza una vez por locución.
- `rust/crates/ez-app/src/app/core/services.rs`: `SpeakerApi::reference()`
  (por omisión `None`; el altavoz real devuelve la suya).
- `rust/crates/ez-app/src/app/core/callbacks.rs`: `mic_capture` lee
  `barge_in_mode` (por omisión `"adaptive"`; `"fixed"` = Python).
- `rust/crates/ez-app/src/app/core.rs`: reexporta `barge_in_mode` para el menú.
- `rust/crates/ez-app/src/app/menu_wiring/apply/mic.rs`: al recargar el
  micrófono conserva el modo adaptativo y `turn_quick_secs`.
- `rust/crates/ez-app/src/app/call.rs`: antes de capturar, pasa la referencia
  del altavoz al micrófono (un clon de `Arc`; sin locks durante la captura).
- `rust/crates/ez-app/src/replay/run.rs`: el arnés fija `BargeInMode::Fixed`
  (su altavoz no suena), de modo que las trazas doradas no cambian.
- `rust-migration/python-quirks.md`: hallazgo Q-PIMP-BARGEIN-01 (desviación
  deliberada pedida por el usuario).

## Decisiones
- `// DECISION` en `VadSettings::default()`: el valor de fábrica de la captura
  es `fixed` (el del Python); la app pide `adaptive` al leer `voice.json`, para
  que el arnés y cualquier uso sin configuración sigan siendo clásicos.
- El detector adaptativo solo se arma si `barge_in_rms > 0`, igual que el gate
  del Python: `barge_in_rms: 0` sigue significando «sin umbral, interrumpe al
  empezar a hablar».
- La envolvente se publica en `CpalStream::finish` (cuando el audio sale hacia
  el dispositivo), no en `write` (que solo acumula): con el sink real, publicar
  en `write` adelantaría toda la frase al instante.
- Acople por frase: mediana de `mic/ref` en la ventana 0,2-0,6 s de la frase,
  probando retardos de 2 a 8 bloques (60-240 ms) y quedándose con la mediana de
  las medianas. La mediana descarta la voz del usuario (que solo sube el
  cociente); el retardo evita que un bloque flojo tras un pico parezca acople
  bajo. Acople inicial 0,9 (conservador), límites 0,05-1,5.
- El diagnóstico se emite al cerrar la frase (cambio de frase o fin del TTS),
  no por bloque, para no inundar el log.
- `speaker.rs` se partió en `speaker/cpal.rs` al superar 600 líneas con el
  cambio; es una mudanza literal, sin cambios de comportamiento.
- No se persiste el acople entre sesiones (el brief lo deja como opcional).

## Verificación
- `cargo fmt --all -- --check`: ok.
- `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio -p ez-tts -p ez-app`: 94 + 56 + 295 pruebas, 0 fallos.
  Incluye `audio_parity` (265 casos de `parity/fixtures/audio.jsonl`, de ellos
  22 de `barge_in` y 10 de `barge_in_need`), `tts_parity`, `turn_parity` y
  `wake_parity`.
- `cargo test -p ez-audio --test barge_in_adaptive`: 2 pruebas con WAV reales.
- `cargo test -p ez-app --features replay --test replay_parity`: 3/3 ok
  (escenarios 01-25, incluidos 05 barge-in y 06 eco descartado).
- Los escenarios 07-15 fallaron una vez al ejecutar los tres grupos en paralelo
  («escenario sin terminar en 60 s» en el 11, sin relación con el audio); en la
  repetición y en la pasada final quedaron verdes.

### Métricas de la validación sintética con WAV
- Solo eco, acople 0,3 / 0,5 / 0,8: **0 barge-in** en dos frases del TTS.
- Eco 0,05 + voz moderada (p90 ≈ 0,0069, por debajo del 0,008 fijo): **barge-in
  a los 12 bloques** desde que empieza la voz (≈0,36 s, incluidas las pausas de
  «Espera, para.»); con el mismo audio y `fixed`: **0 barge-in**.
- Clics de 2 bloques a 0,2 y ruido constante de 0,005: **0 barge-in**.

## Pendiente o riesgos
- `barge_in_mode` **no se persiste** en `voice.json`: `VoiceConfig::from_json`
  descarta claves que no estén en `ez-core/src/config/defaults.rs`, y ese
  archivo queda fuera del alcance de este brief. El comportamiento por omisión
  sí es `adaptive`; para que la clave se guarde/lea hay que añadir
  `"barge_in_mode": "adaptive"` a los valores de fábrica de `ez-core` (fuera de
  alcance; anotado aquí).
- La primera frase de una sesión no se puede interrumpir durante sus primeros
  0,6 s: el acople inicial (0,9) es conservador a propósito y se aprende en esa
  misma frase. A partir de la segunda frase el umbral ya es el medido.
- El acople no se persiste entre sesiones (opcional en el brief).
- La estimación asume que el eco es lineal y que el retardo está entre 60 y
  240 ms (cubre los 50-250 ms del brief). Un dispositivo con más retardo o
  cancelación de eco propia solo perdería sensibilidad, no dispararía de más.
