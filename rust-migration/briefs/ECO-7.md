# ECO-7 — `ez-hub`: nota de voz en la barra del elefante

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §1 y §10,
y los briefs `ECO-5.md` y `ECO-5b.md` (la ventana ya integrada).**

## Qué es
El botón «Nota de voz» de la barra graba lo que dices, lo transcribe con whisper.cpp y deja
el texto **en la barra, sin enviarlo**, para que lo revises (como las apps de Codex o de
Claude). No es la llamada (eso es la fase 3).

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/**`. En su `Cargo.toml` puedes añadir `ez-audio` y `ez-stt` (rutas
  `../ez-audio`, `../ez-stt`). No toques otros crates.

## Reutiliza (no reescribas)
- Configuración del usuario: `ez_core::config::VoiceConfig::load(None)` (su `voice.json`:
  `whisper_url`, `input_device_index`, `input_device_name`, umbrales de VAD).
- Micrófono: cómo lo abre la app de voz en `rust/crates/ez-app/src/app/core/callbacks.rs`
  (`mic_capture`, líneas 85-110): copia esa construcción de `MicCapture::new_named` con
  `VadSettings` en un módulo de `ez-hub` (no dependas de `ez-app`).
- `MicCapture::stream(stop, hooks, None, None, barge_in_rms)` con `on_level` y
  `on_utterance`; `ez_audio::capture::pcm_to_wav`; `ez_stt::server::ensure_whisper`
  (arranca whisper-server si hace falta; guarda el `Child` y ciérralo al salir de la app);
  `ez_stt::client::transcribe(&config, &wav, None)`.

## Comportamiento
- Estados: `inactivo` → (clic) `grabando` → (clic o 5 min) `transcribiendo` → `inactivo`.
  - `grabando`: el botón dice «Detener», con un punto rojo y un medidor de nivel (evento
    `hub://voice-level` como mucho 10 veces por segundo).
  - `transcribiendo`: el botón dice «Transcribiendo…» y está deshabilitado.
  - Al terminar, el texto se inserta en la posición del cursor del área de texto, con un
    espacio de separación si hace falta, y el foco vuelve al área. No se envía.
- La grabación junta los `on_utterance` (el VAD ya quita los silencios largos) y al parar
  los concatena en un solo WAV.
- Errores, en la línea de estado de la barra (no en una alerta): sin micrófono
  «No encontré un micrófono.»; nada grabado «No escuché nada.»; whisper caído o error
  «No pude transcribir: {motivo}».
- Comandos Tauri: `voice_note_start`, `voice_note_stop` → `{texto}` o error. La captura
  corre en el hilo `ez-hub-voice-note` y la transcripción en `ez-hub-voice-stt`; nada
  bloquea el hilo de la ventana.
- En `--demo` el botón queda habilitado; la autocomprobación (`--selfcheck`) no cambia y
  debe seguir saliendo con 0.

## Diseño para probar sin hardware
`trait NoteRecorder` (empezar y parar, devuelve PCM y frecuencia) y `trait Transcriber`
(WAV → texto) con implementaciones reales y dobles. La máquina de estados de la nota es
lógica pura sobre esos traits.

## Pruebas
Máquina de estados (inicio, parada, tope de 5 min con reloj falso, doble clic, error de
micrófono, grabación vacía, error de transcripción); concatenación de varios fragmentos y
WAV resultante (cabecera y tamaño); inserción del texto en el cursor (función JS pura en
`ui/js/voice.js`, probada desde Rust no es posible: deja la lógica de inserción en una
función Rust `insert_at(texto_actual, cursor, nuevo) -> (texto, cursor)` y úsala desde la
vista por un comando, con pruebas).

## Criterios de aceptación
fmt (`-p ez-hub`), clippy y test de `ez-hub` en verde; `--demo --selfcheck` sale con 0
(pega el JSON). Informe en `rust-migration/reports/ECO-7-r1.md`.
