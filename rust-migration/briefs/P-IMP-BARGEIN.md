# P-IMP-BARGEIN — Interrumpir con voz normal: umbral de barge-in que descuenta el eco de lo que la app está diciendo
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/audio.py: `BargeInDetector` (410-452: umbral fijo `threshold` = `barge_in_rms` 0,008, `secs` 0,25, `BARGE_IN_MAX_GAP`, devuelve la racha para no perder el principio de la frase) y su uso en `MicCapture.stream` (575-606: solo mientras `is_speaking()`); el `Segmenter` y su piso de ruido
- src/ezagent/voice/app.py: `_is_speaking`, el evento `audio_barge_in` (corta el TTS y sigue escuchando) y cómo la racha del barge-in acaba en la frase que se transcribe
- Rust: `rust/crates/ez-audio/src/barge_in*`, `ez-audio/src/capture*` (`stream`, `live.rs` con los ajustes atómicos de P-FIX-MICLOCK), `ez-audio/src/segmenter*`; `rust/crates/ez-tts/src/speaker*` (lo que se reproduce y cuándo); `rust/crates/ez-app/src/app/call.rs` (`is_speaking`, `on_barge_in`) y el manejo de `AppEvent::AudioBargeIn`; informe `rust-migration/reports/P-FIX-MICSENS-r1.md` (medición real: con la webcam la voz normal del usuario da RMS p50≈0,0014 / p90≈0,0085-0,011 por bloque de 30 ms, el ruido de sala cruza 0,008 en el 8-28 % de los bloques)

## Archivos que se tocan (solo estos)
- rust/crates/ez-audio/src/** (detector nuevo junto al actual; `unsafe` solo si ya estaba permitido)
- rust/crates/ez-tts/src/speaker* (solo para publicar la envolvente de lo que se reproduce)
- rust/crates/ez-app/src/app/** (solo cableado: pasar la referencia del altavoz a la captura, leer el ajuste nuevo, registrar el diagnóstico)
- rust/crates/ez-app/src/replay/** solo si hace falta fijar el modo clásico en el arnés
- pruebas nuevas en esos crates, `rust-migration/python-quirks.md` (añadir al final) y rust-migration/reports/P-IMP-BARGEIN-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/audio.jsonl

## Notas específicas
- **Pedido del usuario (app Rust, webcam como micrófono):** «a veces tengo que alzar mucho la voz para que entienda que lo quiero interrumpir y escuche lo que le tengo que decir». Causa probable: el barge-in usa un **umbral fijo** (0,008) que la voz normal del usuario apenas alcanza en ese micrófono, mientras que el ruido de sala y el **eco del propio TTS** viven en la misma franja; con un umbral fijo no se pueden separar. Esto es una **mejora deliberada respecto al Python** pedida por el usuario: documéntala en `python-quirks.md` y con `// DECISION`.
- **Qué construir (sin dependencias nuevas):** un modo de barge-in **adaptativo con referencia de eco**, por defecto en la app (`barge_in_mode = "adaptive"` en `voice.json`; `"fixed"` = comportamiento del Python con `barge_in_rms`). (1) El altavoz (`ez-tts`) publica la envolvente RMS de lo que está sonando, por bloques y con marca de tiempo de reproducción, en una estructura compartida **sin locks largos** (atómicos o un `Mutex` de microsegundos; nada de retener locks durante la reproducción o la captura: llevamos tres interbloqueos esta semana). (2) Mientras la app habla, la captura toma por cada bloque el nivel del micrófono y el máximo de la referencia en una ventana de unos 300-400 ms (tolera el retardo altavoz→micrófono). (3) Estima el acople del eco `c` de forma robusta (por ejemplo, percentil alto de `mic/ref` en bloques donde casi seguro solo hay eco, como el principio de cada frase del TTS), con límites razonables y valor inicial conservador; persistirlo entre sesiones es opcional. (4) Umbral por bloque = máx(piso de ruido × margen, `c` × referencia × margen de eco, mínimo absoluto) y se mantiene la regla de duración (`barge_in_secs`) y de huecos (`BARGE_IN_MAX_GAP`), devolviendo la racha para no perder el principio de la frase. Sin referencia disponible (por ejemplo, el arnés de reproducción, cuyo altavoz no suena), se usa el modo clásico, de modo que **las trazas doradas no cambian**.
- **Que después escuche lo que dices:** comprueba en el código y con una prueba que, tras el barge-in, el TTS se corta y la frase del usuario que sigue se transcribe **desde su principio** (la racha y el pre-roll llegan al segmentador), sin perder las primeras palabras.
- **Diagnóstico para afinar con uso real:** registra, por cada frase que dice la app, una línea `INFO` corta con el nivel máximo del micrófono, el eco estimado, el umbral efectivo y si hubo barge-in (sin datos sensibles). Así el usuario puede mandarnos su log y ajustamos márgenes con datos.
- **No reproduzcas sonido por los altavoces del usuario** (es de noche en su zona); valida con señales sintéticas y con los WAV de `parity/replay/audio/` mezclados con un eco simulado (referencia = un WAV de voz del TTS; eco = referencia × acople con retardo de 50-250 ms + ruido). Pruebas mínimas: solo eco (acople 0,3-0,8) → no dispara; eco + voz del usuario a nivel moderado (por debajo del 0,008 fijo) → dispara en ≤ ~0,3 s; ruido constante o golpes cortos → no dispara; con el modo `fixed` el comportamiento es idéntico al actual.
- `audio_parity`, `turn_parity`, `wake_parity`, `cargo test -p ez-audio -p ez-tts -p ez-app` y `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min; los escenarios 05 y 06 son de barge-in y eco). `cargo clippy --workspace --all-targets -- -D warnings`.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-IMP-BARGEIN-r1.md` y un commit en tu rama.
