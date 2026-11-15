# P-FIX-MICINDEX — r1

## Resultado
- Brief/tarea: P-FIX-MICINDEX (el `input_device_index` de `voice.json` debe
  significar el mismo dispositivo que en PyAudio, también en `--list-mics`)
- Estado: **completado** (con una desviación documentada en `--list-mics`: solo
  se reproduce el bloque de MME, ver «Desviaciones»)

### El problema (el que midió P-FIX-MICSENS)
El `voice.json` real del usuario guarda `"input_device_index": 4` y ningún
nombre. En PortAudio/MME el 4 es **una salida** (`Altavoces (TA82)`), así que
Python lo descarta y elige el micrófono automáticamente
(`Micrófono (Webcam AC410)`). En cpal/WASAPI el 4 es `Micrófono (TA82)` —el
micrófono de la barra de sonido, pegado a sus altavoces—, así que Rust abría el
dispositivo equivocado y el eco del TTS disparaba el barge-in.

### Qué se hizo
1. **Numeración de PortAudio.** `capture/portaudio.rs` reproduce el bloque de
   MME tal como lo enumera `pa_win_wmme.c`, con `winmm` (`waveIn*`/`waveOut*`):
   el mapeador de entrada (`Asignador de sonido Microsoft - Input`), las
   entradas de winmm, el mapeador de salida y las salidas. Se copian también sus
   reglas: nombres de `szPname` (cortados a 31 caracteres), sufijo ` - Input` /
   ` - Output` para los mapeadores, máximo de canales del endpoint (PortAudio le
   pregunta al filtro KS; aquí se usa el que ve cpal para el mismo nombre) y
   `DetectDefaultSampleRate` (los bits `WAVE_FORMAT_4M16`/`4S16` de `dwFormats`
   dan 44 100 sin consultar; si no, se sondea `waveInOpen(WAVE_FORMAT_QUERY)` en
   el orden de PortAudio: 44 100, 48 000, 32 000, 24 000, 22 050, 88 200, 96 000,
   192 000, 16 000, 12 000, 11 025, 9 600, 8 000).
2. **Índice → nombre → cpal.** `capture/choice.rs` traduce un índice de
   PortAudio al dispositivo de cpal por su nombre: prefijo y sin distinguir
   mayúsculas (`names_match`), porque los nombres de MME vienen cortados a 31
   caracteres. El «mapeador de sonido» es el dispositivo por omisión del
   sistema. Una **salida**, un índice que no existe o el de otra API de host no
   resuelven y caen a la elección automática, exactamente como Python.
3. **Elección automática igual que Python.** `choose_input_device` es el mismo
   algoritmo de antes (micrófono → webcam → índice menor, sonda de lectura de
   5 ms, nombres descartados) pero ahora recibe la lista de entradas de MME con
   sus índices de PortAudio, así que elige el mismo dispositivo que Python (en
   esta máquina, el Webcam = índice 2).
4. **`MicCapture`** guarda los dos índices: `device_index()` devuelve el de
   **PortAudio** (el que se guarda, se compara y entiende la app de Python) y el
   de cpal se queda interno para abrir el stream. `device_name()` es el nombre
   de PortAudio, como `self.mic.device_name` del Python.
5. **`--list-mics`** imprime la lista de entradas de PortAudio con el mismo
   formato de línea del Python (`{:>3}  {:<45} {:>6} Hz  {} canales`).

### Comprobación real (obligatoria)
`voice.json` real del usuario: `"input_device_index": 4`, sin
`input_device_name`.

Python (`.venv`, `MicCapture(device_index=4)`):

```
Micrófono: Micrófono (Webcam AC410) (índice 2, 44100 Hz)
```

Rust, **la app real** (`ezagent-voice --config <copia del voice.json real>`;
copia solo para apagar `auto_start_whisper`, el índice es el mismo):

```
2026-11-15T21:53:07.814443Z  INFO ez_app::app::core: Micrófono: Micrófono (Webcam AC410) (índice 2, 48000 Hz)
```

Mismo micrófono y mismo índice de PortAudio. La única diferencia es la tasa:
Python abre el stream por MME a 44 100 Hz (MME remuestrea) y Rust captura a la
tasa del endpoint WASAPI (48 000 Hz), que es lo que ya hacía y lo que la
medición de P-FIX-MICSENS dio por bueno (el nivel por bloque coincide).

Casos de borde comprobados en la misma corrida (ejemplo temporal, borrado):

| Entrada | Python | Rust |
|---|---|---|
| `input_device_index: 4` (salida en MME) | `Micrófono (Webcam AC410)` (índice 2) | `Micrófono (Webcam AC410)` (índice 2) |
| `4` + `input_device_name: Micrófono (Webcam AC410)` | — | `Micrófono (Webcam AC410)` (índice 2) |
| `5` (otra salida de MME) | — | `Micrófono (Webcam AC410)` (índice 2) |
| `21` (WASAPI de PortAudio) | `Micrófono (Webcam AC410)` (índice 21) | `Micrófono (Webcam AC410)` (índice 2, automático) |
| `--mic 4 --write-config` | — | `{"input_device_index": 4, "input_device_name": null}` |

### `--list-mics`: comparación de los dos binarios
Salida de Rust (3 líneas) contra las 3 primeras de Python: **idénticas byte a
byte** (mismos índices, nombres, tasas, canales y formato):

```
  0  Asignador de sonido Microsoft - Input          44100 Hz  2 canales
  1  Micrófono (TA82)                               44100 Hz  1 canales
  2  Micrófono (Webcam AC410)                       44100 Hz  2 canales
```

Python imprime 11 líneas más (8, 9, 10, 20, 21, 22, 23, 24, 28, 31, 33): son
los **mismos micrófonos** repetidos por DirectSound, WASAPI y WDM-KS, cada API
con su propio índice. Rust solo reproduce MME (ver «Desviaciones»); la
comparación está en `cli_parity` y exige igualdad en las líneas que Rust
enumera, que son el principio de la lista del Python.

## Cambios
- `rust/crates/ez-audio/src/capture/portaudio.rs` (nuevo): bloque de MME de
  PortAudio con `winmm` (mapeadores, nombres, canales, tasa) y `names_match`
  (prefijo sin distinguir mayúsculas para los nombres cortados a 31).
- `rust/crates/ez-audio/src/capture/choice.rs` (nuevo): `choose_mic` (nombre →
  índice → automático), `choose_input_device`, `pick_input_device[_named]`,
  `find_by_name`, `resolve_in` (índice de PortAudio → cpal), el índice por
  omisión y la sonda `_read_timing`.
- `rust/crates/ez-audio/src/capture.rs`: aloja la enumeración
  (`list_portaudio_devices`, `input_devices`, `list_input_devices`) y los
  dispositivos de cpal (`HostDevice`); `InputDevice.index` es ahora el índice de
  PortAudio. Se movió la elección a `choice.rs` (el archivo ya pasaba de 600
  líneas).
- `rust/crates/ez-audio/src/capture/mic.rs`: `new_named` usa `choose_mic` y
  guarda índice de PortAudio + índice de cpal; `stream` abre el de cpal. Las
  pruebas se movieron a `capture/mic/tests.rs` (el archivo quedaba en 594
  líneas).
- `rust/crates/ez-audio/src/capture/mic/live.rs`: `portaudio_index` junto a
  `device_index`; `device_index()` (público) devuelve el de PortAudio.
- `rust/crates/ez-audio/Cargo.toml`: declara `windows = { workspace = true }`
  (dependencia que ya estaba en `[workspace.dependencies]`; no se añadió nada
  nuevo ni se tocó la versión). `rust/Cargo.lock` solo gana esa arista.
- `rust/crates/ez-app/tests/cli_parity.rs`: `compare_list_mics` compara
  `--list-mics` de los dos binarios.
- `rust/crates/ez-app/src/app/menu_wiring/tests/more.rs`: la prueba del selector
  comprueba que `voice.json` guarda el índice de PortAudio **y**
  `input_device_name`.

No hizo falta cambiar `main/lists.rs`, `core/callbacks.rs` ni el resto de
`menu_wiring`: todos leen `list_input_devices()` o `mic.device_index()`, que ya
hablan la numeración de PortAudio.

## Decisiones
- `// DECISION` en `portaudio.rs`: solo se reproduce el bloque de MME. Las otras
  tres APIs de host necesitan enumerar sus endpoints (features de `windows` que
  el worktree no declara) y sus índices no se pueden resolver sin adivinar; un
  índice de esas APIs cae a la elección automática, igual que en Python con un
  índice que no sirve.
- `// DECISION` en `choice.rs` (`default_input_index`): PortAudio pregunta a
  `winmm` por el dispositivo preferido y cae al mapeador; aquí se usa el nombre
  del micrófono que elige cpal y, si no aparece, el mapeador. En esta máquina
  coincide (índice 1, el TA82).
- La sonda `_read_timing` pide la tasa de PortAudio y, si el endpoint de cpal no
  la acepta, usa la suya con `input_config` (WASAPI no remuestrea como MME). Sin
  esto, todos los candidatos fallaban la sonda a 44 100 y la elección caía al
  dispositivo por omisión.
- `find_by_name` compara exacto, luego sin mayúsculas y al final por prefijo:
  así sigue funcionando un `input_device_name` guardado por la versión anterior
  (nombre de cpal, sin cortar) y el nombre cortado de MME.

## Verificación
Desde `rust/`:

- `cargo fmt --all` y `cargo fmt --all --check`: ok.
- `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio`: 77 lib + `audio_parity`, `turn_parity`,
  `wake_parity`, `shared_environment` — todo en verde.
- `cargo test -p ez-app`: 294 lib + `app_text_parity` + `cli_parity` — en verde.
- `cargo test -p ez-app --features replay --test replay_parity`: 3 passed
  (antes hay que `cargo build -p ez-fakes --bins`; sin ese binario falla por
  entorno, no por este cambio).
- Comprobación real en hardware: la de arriba (app real + `MicCapture` de
  Python con el `voice.json` del usuario).
- `--list-mics` de los dos binarios: las 3 líneas de MME idénticas.

Tamaños: `capture.rs` 424, `choice.rs` 392, `portaudio.rs` 300, `mic.rs` 450,
`mic/live.rs` 229, `mic/tests.rs` 143 (todos < 600).

## Fixtures y pruebas
- Fixtures consumidos: ninguno (el brief dice que no hay paridad en esta fase).
- Filas de `parity/test-map.csv`: ninguna nueva.
- Pruebas nuevas (ez-audio): `capture::portaudio` 5 (nombre cortado ↔ nombre
  completo, orden del mapeador, sufijo del mapeador, bit de 44 100 sin sonda,
  bits por canales) y `capture::choice` 9 (`an_output_index_falls_back_to_the_automatic_choice`,
  `a_truncated_name_matches_the_same_microphone`,
  `a_portaudio_index_resolves_to_the_device_with_the_same_name`,
  `the_sound_mapper_follows_the_system_default_device` y las 5 de la elección
  que se conservan).
- Pruebas nuevas (ez-app): `compare_list_mics` en `cli_parity` y la comprobación
  de `input_device_name` en `apply_settings_changes_the_input_device`.

## Desviaciones del brief
- **`--list-mics` imprime solo el bloque de MME** (3 líneas aquí) en vez de las
  14 del Python. Reproducir DirectSound, WASAPI y WDM-KS exige enumerar sus
  endpoints con features de `windows` que el worktree no declara
  (`Win32_Media_Audio_DirectSound`, `Win32_Media_Audio_Endpoints`, y SetupAPI/KS
  para WDM-KS) y, sin ellas, cualquier índice ≥ 8 habría que adivinarlo —que es
  justo el fallo que esta tarea corrige—. La comparación de `cli_parity` exige
  igualdad en las líneas que Rust enumera (el principio de la lista de Python).
- **`main.rs` no se tocó** (no está en la lista de archivos del brief): `--mic N`
  guarda el índice en numeración de PortAudio (lo que entiende Python) y limpia
  el nombre anterior para que la opción explícita mande, pero no escribe
  `input_device_name`; eso solo lo hace el selector del menú. Escribirlo también
  con `--mic` son tres líneas en `main.rs` y un `portaudio_name(index)` público en
  `ez-audio`.
- Se declara `windows` en el `Cargo.toml` de `ez-audio` (dependencia del
  workspace, sin features nuevas). Sin ella no hay `winmm`.
- `capture.rs` y `mic.rs` se partieron en submódulos para no pasar de 600 líneas
  (permitido por el brief: `capture/**`).

## Pendiente o riesgos
- **Un índice de otra API de host no resuelve.** Si el usuario eligió el
  micrófono en el menú de Python y quedó un índice de DirectSound, WASAPI o
  WDM-KS (≥ 8 en esta máquina), Rust elige el micrófono automático (hoy el
  Webcam, que es el mismo que elegiría Python en la mayoría de los casos, pero no
  siempre: el Realtek del índice 23 no se abre). Con `input_device_name` guardado
  el problema desaparece, y el menú de Rust ya lo escribe.
- **La app de Python no conoce `input_device_name`**: si el usuario alterna las
  dos apps, Python vuelve a guardar solo el índice y el nombre se pierde (la
  elección sigue siendo por índice, como antes). El índice que escribe Rust ya
  es el de Python, así que la lectura es correcta en ambos sentidos.
- **La tasa del registro difiere** (`48000 Hz` en Rust, `44100 Hz` en Python):
  es la tasa real de captura de cada uno, no cambia el dispositivo. El texto de
  la línea es el mismo.
- Los nombres de MME pueden coincidir por prefijo con otro dispositivo de nombre
  más largo (`Micrófono (TA82)` y `Micrófono (TA82) 2`): se elige el primero de
  la enumeración. No hay ninguno así en esta máquina.
- El bloque de MME se reenumera en cada `MicCapture::new` y en cada apertura del
  menú (winmm es barato); la sonda de lectura sigue abriendo cada candidato como
  en Python, así que cambiar de micrófono tarda lo mismo que allí.
