# P-FIX-MICSENS — El micrófono de la app Rust es demasiado sensible: cualquier ruido corta la respuesta (barge-in) y en Python no
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/audio.py COMPLETO: `MicCapture` (apertura del dispositivo, tasa, canales, conversión a PCM16 mono 16 kHz, `stream`), `Segmenter`, `BargeInDetector` y cómo se calcula el umbral (piso de ruido, `noise_multiplier`, `barge_in_rms`, `barge_in_secs`, `min_rms`), y cómo `is_speaking`/eco del TTS afectan al barge-in
- src/ezagent/voice/app.py: `_call_worker`, `_is_speaking`, `audio_barge_in` en `_handle_event`, y lo que se registra en «VAD: …»
- Rust: `rust/crates/ez-audio/src/capture*` (captura con cpal, conversión de formato y de canales, remuestreo, `stream`, `live.rs` de P-FIX-MICLOCK), `ez-audio/src/barge_in*`, `ez-audio/src/segmenter*`; `rust/crates/ez-app/src/app/call.rs` (`is_speaking`, el callback de barge-in) y la respuesta al evento `AudioBargeIn`

## Archivos que se tocan (solo estos)
- rust/crates/ez-audio/src/** y rust/crates/ez-app/src/app/** (solo lo relacionado con captura, VAD y barge-in)
- pruebas nuevas en esos crates y un ejemplo temporal de medición (no commiteado)
- rust-migration/reports/P-FIX-MICSENS-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/audio.jsonl

## Notas específicas
- **Síntoma real (reportado por el usuario):** con la app Rust el micrófono «parece tener mucha sensibilidad»: con cualquier ruido el modelo entiende que lo interrumpe y deja de responder. Con la app Python, mismo micrófono (`Micrófono (Webcam AC410)`, 48 000 Hz) y misma configuración (`%APPDATA%/EzAgent/voice.json`), no pasa. Las pruebas de paridad del `BargeInDetector` y del `Segmenter` (`audio.jsonl`) pasan, y los escenarios de reproducción 05 (barge-in) y 06 (eco descartado) también, así que la lógica con bloques idénticos es igual: **la diferencia probable está en lo que entra** (nivel de las muestras tras convertir el formato del dispositivo, canales, remuestreo de 48 kHz a 16 kHz, ganancia) **o en los valores efectivos** (umbral derivado del piso de ruido, `barge_in_rms` efectivo, `is_speaking` durante el TTS, eco).
- **Mide en real:** escribe un ejemplo temporal que abra el mismo micrófono con la captura de Rust y, a la vez o justo después, con el `MicCapture` de Python (`.venv`), durante ~10 s de ruido ambiente y ~5 s de voz, y compara: RMS por bloque, piso de ruido estimado, umbral efectivo de barge-in, tasa y número de canales del dispositivo y del bloque entregado, y cuántas veces dispararía el barge-in cada uno. Compara también la línea «VAD: …» que registra cada versión al arrancar con la configuración real. Encuentra la causa, corrígela en Rust para que se comporte como Python y explica la evidencia en el informe (tabla Rust vs Python).
- Revisa también que P-FIX-MICLOCK (que acaba de pasar los ajustes a atómicos y dejó `end_probe` siempre `Some`) no cambiara ningún valor por defecto ni la lectura de `barge_in_rms or …`.
- `audio_parity`, `turn_parity`, `wake_parity` y `cargo test -p ez-audio -p ez-app` en verde; `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min; escenarios 05 y 06 sobre todo).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-MICSENS-r1.md` y un commit en tu rama.
