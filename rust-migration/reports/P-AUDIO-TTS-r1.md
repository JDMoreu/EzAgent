## Resultado
- Brief/tarea: P-AUDIO-TTS
- Estado: completado (con una omisión justificada: los 27 casos de `turn/probability`)

## Cambios
- `rust/crates/ez-audio/tests/audio_parity.rs` y `audio_parity/segmenter.rs`: prueba de paridad de los 265 casos de `audio.jsonl` (WAV, `pcm_rms`, `pcm_is_weak`, `pcm_to_wav`, remuestreo, barge-in y `Segmenter`), con reloj congelado y sondas guionizadas.
- `rust/crates/ez-audio/tests/turn_parity.rs`: 60 casos de `turn.jsonl` (`hz_to_mel`, `mel_to_hz`, `log_mel`, `prepare`); los 27 de `probability` se omiten con aviso (ver Pendiente).
- `rust/crates/ez-audio/tests/wake_parity.rs`: los 264 casos de `wake.jsonl`.
- `rust/crates/ez-tts/tests/tts_parity.rs` + `tts_parity/{support,text_cases,audio_cases,voices_cases,kokoro_cases}.rs`: los 297 casos de `tts.jsonl`, con comparación de PCM por longitud exacta y correlación ≥ 0,999, y `sha256` para estilos.
- `rust/crates/ez-audio/src/segmenter.rs`: reloj inyectable y `SegmenterState`/`state()` (DECISION), `EndProbe` recibe el estado (el oráculo lee `_silence_run`), `Probe::scripted` para sondas pendientes.
- `rust/crates/ez-audio/src/resample.rs`: `resampled_len`/`resample_linear` devuelven `Result` y aceptan tasas `i32` (división por cero y tasas negativas del Python).
- `rust/crates/ez-audio/src/capture.rs`: `pcm_to_wav_checked` reproduce el error de `wave.setframerate`; `pcm_is_weak` compara el pico como el Python (float32 contra 0.005 float64).
- `rust/crates/ez-audio/src/turn/features.rs`, `turn.rs`: `log_mel`/`prepare` devuelven `Result` y reproducen los errores de numpy (reflect vacío, `spec.max()` vacío, byte impar, tasa 0).
- `rust/crates/ez-tts/src/kokoro.rs`: `prepare()` y `model()` públicos, `style_for(0)` = última fila, `split_phonemes` con `?`.
- `rust/crates/ez-tts/src/kokoro/chunker.rs`: `split_phonemes` devuelve `Result` (`range() arg 3 must not be zero`).
- `rust/crates/ez-tts/src/kokoro/sliding.rs`: `token_edges` con total 0 da `i64::MIN`; `synthesize` con tokens vacíos da `list index out of range`; `quietest` devuelve el `IndexError` de numpy en vez de entrar en pánico.
- `rust/crates/ez-tts/src/kokoro/pauses.rs`: `insert` devuelve `Result` (audio más corto que un fotograma).
- `rust/crates/ez-tts/src/kokoro/tokenizer.rs`: `tokenize` acepta límites `i64` (el caso `-1` del fixture), `load_vocab`/`parse_config_vocab` públicos, `KeyError` como `'vocab'` y `OSError` como `[Errno 2] No such file or directory: '…'`.
- `rust/crates/ez-tts/src/kokoro/session.rs`: `resolve_providers` respeta `ONNX_PROVIDER` (como el Python) en vez de devolver siempre CPU.
- `rust/crates/ez-tts/src/phonemes/ffi.rs`, `phonemes.rs`: selección de voces por `espeak_ListVoices` (necesaria para `en-gb`/`fr-fr`) y mensaje público `language "xx" is not supported by the espeak backend`.
- `rust/crates/ez-tts/src/voices.rs`: `style()` con el error de `NpzFile` (`'x is not a file in the archive'`).
- `rust/crates/ez-tts/src/speaker.rs`: adaptación a `resample_linear` con `Result`/`i32`.
- `rust-migration/python-quirks.md`: hallazgos Q-PAUDIO-01 a 05.

## Pruebas por dominio
- `audio`: 265/265 casos. 264 pasaron en la primera comparación correcta; 1 defecto de Rust corregido: `Probe::done` de la sonda pendiente consumía una consulta de más (caso `wav-voice_dora_probe_pending`). No hubo más defectos de comportamiento en `audio`.
- `turn`: 60/60 casos ejecutados (87 en el archivo). `hz_to_mel`, `mel_to_hz`, `log_mel` y `prepare` pasaron a la primera. Los 27 de `probability` se omiten: `ez-audio` no declara `ort` y este brief no puede tocar `Cargo.toml`; se validaron sus entradas (WAV, muestras, tasa y umbral).
- `wake`: 264/264 a la primera.
- `tts`: 192/192 casos ejecutables sin DLL a la primera (tras corregir los defectos listados); con `ORT_DYLIB_PATH` y `EZAGENT_ESPEAK_*` apuntando al `.venv` de Python, 297/297 (incluidos los 94 `create_timed`, 2 `create_stream` y 1 `artifacts`). Sin esas variables, 105 se omiten con mensaje explícito y se cuentan aparte.

## Decisiones
- # DECISION en `audio_parity.rs`: `pcm_is_weak` recibe 0 en vez de una tasa negativa (mismo `frames` que el Python) y `pcm_to_wav` usa la variante `_checked` para las tasas no positivas.
- # DECISION en `segmenter.rs`: reloj inyectable y `SegmenterState` como costura de prueba; la aplicación no los usa.
- # DECISION en `turn_parity.rs`: los 27 casos de `probability` se validan solo en su entrada.
- # DECISION en el top de `tts_parity.rs` y en `text_cases.rs`: comparaciones sin `serde_json` (no es dependencia del crate); entradas `null`/`int` de `normalize`/`phonemize` se aceptan como caso consumido porque la API Rust no las admite; el vocabulario con una clave de dos caracteres solo se compara por tokens (`HashMap<char, i64>`).
- # DECISION en `support.rs`: `serde_json` sin `float_roundtrip` puede desviarse 1 ulp al leer un decimal; las comparaciones numéricas usan tolerancia (f32/f64 según el dominio).
- Desviación del brief: los `tests/*_parity.rs` se partieron en subdirectorios `tests/<dominio>_parity/` para no superar 600 líneas por archivo (todos los `.rs` quedan ≤ 600). No se tocó ningún `Cargo.toml` ni archivo de `parity/`.

## Verificación
- `cargo fmt --all --check`: ok.
- `cargo clippy -p ez-audio --all-targets -- -D warnings`: ok.
- `cargo clippy -p ez-tts --all-targets -- -D warnings`: ok.
- `cargo clippy --workspace --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio`: 66 lib + 1 `audio_parity` (265 casos) + 1 `turn_parity` (60) + 1 `wake_parity` (264), todo en verde.
- `cargo test -p ez-tts`: 55 lib + 1 `tts_parity` (192 sin DLL / 297 con DLL), todo en verde.
- `cargo test --workspace --no-run`: compila.
- Fixtures consumidos: `audio.jsonl` 265, `turn.jsonl` 87 (60 ejecutados + 27 omitidos por `ort`), `wake.jsonl` 264, `tts.jsonl` 297 (192 sin DLL + 105 omitidos con aviso; 297 con DLL y espeak del `.venv`). `parity/test-map.csv` no tiene filas propias de esta tarea; las filas `tests/test_audio.py::*` siguen en `pendiente`.

## Pendiente o riesgos
- `turn/probability` (27 casos): falta `ort` en `ez-audio`; no se puede añadir la dependencia en este brief. Cuando exista, comparar la probabilidad con `f32_rel = 1e-5` y el modelo `smart-turn-v3.2-cpu.onnx` ya instalado.
- `tts` sin DLL: en una máquina sin `onnxruntime.dll`/espeak-ng, 105 casos se omiten (mensaje explícito). Para el gate hay que exportar `ORT_DYLIB_PATH` y `EZAGENT_ESPEAK_LIBRARY`/`EZAGENT_ESPEAK_DATA_PATH` (los del `.venv` sirven).
- `tts.artifacts` no compara `packages` ni `espeak.loader` (son versiones de paquetes de Python), y `serde_json` sin `float_roundtrip` puede desviar 1 ulp los decimales del fixture (cubierto por tolerancia).
- El PCM de `create_timed` se compara con correlación (0,999) y longitud exacta; un cambio de hilos de ONNX Runtime podría alterar los últimos bits y `create_stream` compara `sha256` exacto, así que es el caso más sensible a la versión del runtime.
