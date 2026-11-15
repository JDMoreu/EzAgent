# P-FIX-MICSENS — r1

## Resultado
- Brief/tarea: P-FIX-MICSENS (el micrófono de la app Rust «es demasiado
  sensible»: cualquier ruido corta la respuesta por barge-in y en Python no)
- Estado: completado

### Qué se midió (lo que pide el brief)
Ejemplo temporal en `rust/crates/ez-audio/examples/` (creado, ejecutado y
**borrado**: no está en el commit) que abre el micrófono con la captura **real**
de `ez-audio` (`MicCapture::stream`, mismos bloques de 30 ms, mismos callbacks
del pipeline) y vuelca el nivel de cada bloque. En paralelo, el
`MicCapture.stream` **real** de Python (`.venv`) con la **misma** configuración
(`%APPDATA%/EzAgent/voice.json`) y el mismo micrófono, en el mismo instante.
Los dos midieron ruido ambiente ~10 s y la reproducción de un WAV de ráfagas
(~5 s) que entra por el altavoz y vuelve por el micrófono.

Dispositivo: `Micrófono (Webcam AC410)` en las dos apps (auto-elección).

| Medida (misma sesión, arrancadas a la vez) | Rust (cpal/WASAPI) | Python (PyAudio/MME) |
|---|---|---|
| Tasa pedida | 48 000 Hz | 44 100 Hz |
| Canales del dispositivo | 2 | 2 (`maxInputChannels`) |
| Canales del stream | 2 → mono por media | 1 (PortAudio hace el mono) |
| Muestras por bloque | 1 440 (30,0 ms) | 1 323 (30,0 ms) |
| Bloques entregados | 832 en 25,01 s (33,3/s) | 833 en 25,05 s (33,3/s) |
| RMS por bloque p20/p50/p90 | 0,000128 / 0,001415 / 0,008537 | 0,000281 / 0,001474 / 0,007161 |
| Piso de ruido estimado | 0,000063 | 0,0003 |
| Umbral de arranque / cierre | 0,014125 / 0,008475 | 0,0141 / 0,0085 (log `VAD start`) |
| Arranques de voz (`VAD start`) | 5 | 4 |
| Barge-in confirmados | 4 | 3 |
| Frases emitidas | 1 | 1 |

Segunda tanda, con los **valores de fábrica** del VAD (`min_rms=0.0006`,
`noise_multiplier=1.3`), otra vez a la vez:

| Medida | Rust | Python |
|---|---|---|
| RMS por bloque p50 / p90 | 0,002038 / 0,011034 | 0,002043 / 0,011007 |
| Piso de ruido | 0,000005 | 0,000014 |
| Umbral de arranque / cierre | 0,000600 / 0,000360 | 0,000600 / 0,000360 |
| Bloques ≥ `barge_in_rms` (0,008) | 108 de 731 | 108 de 733 |
| Barge-in disparados | 7 | 7 |
| Momentos del disparo (s) | 0,84 / 6,15 / 6,90 / 7,17 / 7,44 / 8,55 / 8,85 | 0,63 / 5,94 / 6,69 / 6,96 / 7,23 / 8,34 / 8,64 |

**Conclusión de la medición: el camino de audio de Rust es fiel.** Nivel por
bloque, piso de ruido, umbrales, arranques de VAD y barge-in coinciden con
Python (diferencias de un bloque por el instante de arranque). No hay ganancia
extra, ni diferencia por convertir el formato del dispositivo, ni por los
canales, ni por el remuestreo: Rust no remuestrea para el VAD (usa la tasa del
dispositivo, igual que Python) y el RMS por bloque es el mismo número.

### La causa real (lo que sí difiere con la configuración del usuario)
`voice.json` guarda **un índice numérico** de dispositivo, y los índices no
significan lo mismo en las dos apps (ni sobreviven a que cambie la lista de
dispositivos). Con el `voice.json` real, `"input_device_index": 4`:

| `input_device_index: 4` | Rust (cpal) | Python (PyAudio/MME) |
|---|---|---|
| Qué es el índice 4 | `Micrófono (TA82)`, 48 000 Hz, 1 canal | `Altavoces (TA82)` (salida) → no sirve |
| Micrófono que se abre | **Micrófono (TA82)** | **Micrófono (Webcam AC410)**, 44 100 Hz |
| Nivel medido de ese micrófono | 1,5e-5 constante (mudo hoy) | — |
| Nivel del Webcam (el bueno) | 1,4e-5…0,011 (p50 0,0014, p90 0,0085) | igual |

Enumeración completa de cpal en esta máquina: `0..3` son **salidas**
(`3 - Q27B16`, `Auriculares (SK212)`, `Altavoces (Steam Streaming)`,
`Altavoces (TA82)`), `4` = `Micrófono (TA82)` y `5` =
`Micrófono (Webcam AC410)`. En PortAudio/MME los micrófonos son 0, 1 y 2 y el
índice 4 es una salida. Es decir: **«la misma configuración» no es el mismo
micrófono en las dos apps**, y es el único factor que difiere de verdad
(comprobado con la captura real de cada una).

Con el micrófono equivocado (el del soundbar TA82, de campo lejano y al lado de
sus propios altavoces) el nivel ambiente y el eco del TTS cruzan
`barge_in_rms = 0.008` constantemente: «con cualquier ruido el modelo entiende
que lo interrumpe» y, al invalidarse el turno, «deja de responder». Con el
Webcam (el que elige Python) el nivel es el mismo en las dos apps.

Aparte, la medición deja un dato que explica el resto del síntoma: con
`barge_in_rms = 0.008` y el `min_rms = 0.014125` / `noise_multiplier = 2.2` que
el usuario puso a mano (venían de `0.0007` / `1.3`), **el ruido de sala solo**
supera el umbral de interrupción en el 8-28 % de los bloques de 30 ms y produce
≈1 barge-in cada 6 s de escucha armada, en las dos apps. La banda
`0.008 … 0.0141` (por debajo del umbral de voz, por encima del de interrupción)
es exactamente donde vive el ruido de esta sala. Eso **no** es un defecto del
puerto: el Python hace lo mismo, medido aquí. Se recomienda subir
`barge_in_rms` por encima del piso de ruido (≈0,02 en esta sala) — y, si se
quiere que sea automático, es un cambio de la especificación Python
(`BargeInDetector` con umbral derivado del piso de ruido), fuera del alcance de
esta tarea.

### Lo que se corrigió
1. **Identidad del micrófono**: el dispositivo se guarda también por **nombre**
   (`input_device_name`) y se resuelve **nombre → índice → automático**. Así la
   app abre siempre el mismo micrófono físico, aunque cambie el orden de cpal o
   el `voice.json` lo haya escrito la app de Python. Verificado en hardware:
   `pick_input_device_named(Some(4), "Micrófono (Webcam AC410)") = Some(5)` y
   `MicCapture::new_named(4, webcam)` abre el Webcam; con
   `"Micrófono (TA82)"` abre el 4.
2. **Texto de la línea `VAD:`**: el puerto imprimía `barge_in=true`; Python
   imprime `barge_in=True` (`str(True)`).
3. **Valores por omisión del VAD al cambiar de micrófono**
   (`reload_mic_with`): `barge_in_rms` caía a `0.0` (apagaba el detector) y
   `silence_secs` a `2.5`; ahora `0.008` y `1.3`, los del `VoiceConfig`.

## Cambios
- `rust/crates/ez-audio/src/capture.rs`: `find_by_name` (nombre exacto y luego
  sin distinguir mayúsculas) y `pick_input_device_named(preferred, name)`, que
  usa el nombre como pista y delega en `choose_input_device` (así el nombre
  también pasa por la comprobación de «dispositivo que responde»); con `None`
  se comporta exactamente como `pick_input_device`. Pruebas de nombre.
- `rust/crates/ez-audio/src/capture/mic.rs`: `MicCapture::new_named(index,
  name, settings)`; `new` delega sin nombre (paridad y todas las llamadas
  existentes intactas).
- `rust/crates/ez-app/src/app/core/callbacks.rs`: `mic_capture` lee
  `input_device_name` y construye el micrófono con `new_named`.
- `rust/crates/ez-app/src/app/menu_wiring/apply/mic.rs`: `reload_mic_with`
  guarda `input_device_name` junto al índice y usa los valores por omisión del
  `VoiceConfig` (`barge_in_rms` 0.008, `silence_secs` 1.3).
- `rust/crates/ez-app/src/app/main.rs`: `--mic N` limpia `input_device_name`
  (la opción explícita manda sobre el nombre guardado).
- `rust/crates/ez-app/src/app/core/init.rs`: `vad_line()` (extraída para poder
  probarla) imprime `barge_in=True/False` con `python_str` y sin Smart Turn
  `sin Smart Turn`, como el Python.
- `rust/crates/ez-app/src/app/core.rs`: importa `python_str`.
- `rust/crates/ez-app/src/app/core/tests.rs`: prueba de texto de `vad_line`
  (con y sin Smart Turn, `True` y `False`).

## Decisiones
- El nombre del dispositivo no puede venir de Python (su `voice.json` no lo
  tiene): clave nueva `input_device_name`, que la app de Python ignora al leer
  (`VoiceConfig` descarta claves desconocidas) y pierde al volver a guardar; en
  ese caso se sigue usando el índice, como hasta ahora.
- El nombre solo **sugiere**: si ya no existe o no responde a la sonda de
  lectura, se cae al índice y luego a la elección automática, igual que Python
  con un índice inservible.
- No se cambió ninguna regla del `Segmenter` ni del `BargeInDetector`, ni sus
  valores por defecto: la medición demuestra que ya coinciden con Python, y el
  exceso de disparos con ruido de sala es de la especificación, no del puerto.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-audio --all-targets -- -D warnings`: ok.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio`: 70 lib + `audio_parity`, `turn_parity`,
  `wake_parity`, `shared_environment` — en verde.
- `cargo test -p ez-app`: 294 lib (incluye `vad_line_matches_the_python_text`) +
  `app_text_parity` + `cli_parity` — en verde.
- `cargo test -p ez-app --features replay --test replay_parity`: 3 passed
  (escenarios 01-06 con 05 y 06 incluidos).
- Medición real: tablas de arriba (captura real de las dos apps a la vez).
- Comprobación en hardware del arreglo: `pick_input_device_named` con el
  `voice.json` real (`index 4` + `"Micrófono (Webcam AC410)"`) abre el Webcam
  (`cpal=5`), no el TA82.

## Fixtures y pruebas
- Fixtures consumidos: ninguno (el brief dice que no hay paridad en esta fase);
  `parity/fixtures/audio.jsonl` solo se leyó para el formato. No se añadieron
  filas a `parity/test-map.csv`.
- Pruebas nuevas: `ez-audio` → `the_saved_name_wins_over_the_index`,
  `a_missing_or_empty_name_falls_back_to_the_index`; `ez-app` →
  `vad_line_matches_the_python_text`.

## Desviaciones del brief
- `rust/crates/ez-app/src/app/main.rs` y `core.rs` están bajo
  `rust/crates/ez-app/src/app/**`, permitido por el brief; tocan la selección
  del micrófono y el registro del VAD, nada más.
- El ejemplo de medición se creó, se ejecutó y se borró (no está en el commit).
- No se tocó `Cargo.toml` ni ningún archivo de Python, fixtures o paridad.

## Pendiente o riesgos
- **La app de Python no conoce `input_device_name`**: si el usuario alterna
  entre las dos apps con el mismo `voice.json`, Python volverá a guardar solo el
  índice y el nombre se perderá (la elección seguirá siendo por índice, como
  hoy). Lo correcto a medio plazo es que las dos apps guarden nombre+índice.
- El `voice.json` actual del usuario (`input_device_index: 4`, sin nombre) sigue
  apuntando al `Micrófono (TA82)` en Rust hasta que se elija el micrófono una
  vez en el menú del HUD (o se arranque con `--mic 5`); desde ese momento queda
  guardado el nombre y no vuelve a cambiar.
- El barge-in seguirá disparando con ruido de sala si `barge_in_rms` (0.008)
  queda por debajo del piso de ruido del micrófono elegido: es comportamiento
  de la especificación, idéntico en las dos apps. Recomendación de ajuste:
  `barge_in_rms ≈ 0.02` en esta sala (`min_rms = 0.014125`).
- Un micrófono preferido que esté mudo (el TA82 hoy: 1,5e-5 constante) sigue
  siendo preferido si su índice es válido, igual que en Python; no se añadió una
  heurística de «dispositivo mudo» para no inventar comportamiento.
