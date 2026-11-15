# P-FIX-MICINDEX — El índice de micrófono de voice.json debe significar el mismo dispositivo que en Python (numeración de PortAudio), y --list-mics igual
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/audio.py: `list_input_devices`, la elección del dispositivo en `MicCapture.__init__`/`_open` (índice preferido, comprobación de que responde, elección automática), y `src/ezagent/voice/app.py` (`--list-mics`, `--mic`, el selector de micrófono del menú y lo que se guarda en `input_device_index`)
- PyAudio/PortAudio en el `.venv` (`C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe -I -c "import pyaudio; p=pyaudio.PyAudio(); ..."`): enumera aquí los dispositivos, sus `hostApi`, nombres, `maxInputChannels` y `defaultSampleRate` para saber exactamente cómo numera PortAudio en Windows (todas las APIs de host en un solo espacio de índices, MME primero, nombres MME truncados a 31 caracteres…)
- Rust: `rust/crates/ez-audio/src/capture.rs` (`list_input_devices`, `pick_input_device`, `pick_input_device_named`, `choose_input_device`, `find_by_name`) y `capture/mic.rs`; `rust/crates/ez-app/src/app/main/lists.rs` (`--list-mics`), `app/core/callbacks.rs` (`mic_capture`), `app/menu_wiring/**` (selector de micrófono) e informe `rust-migration/reports/P-FIX-MICSENS-r1.md` (la medición que encontró el problema)

## Archivos que se tocan (solo estos)
- rust/crates/ez-audio/src/capture.rs y rust/crates/ez-audio/src/capture/** (aquí `unsafe` está permitido, con `// SAFETY:`; para MME usa la API `winmm` del crate `windows` del workspace si ya expone las funciones; **no** añadas dependencias nuevas sin decirlo en el informe)
- rust/crates/ez-app/src/app/main/lists.rs, rust/crates/ez-app/src/app/core/callbacks.rs, rust/crates/ez-app/src/app/menu_wiring/** (solo la elección y el listado de micrófonos)
- pruebas nuevas en esos crates
- rust-migration/reports/P-FIX-MICINDEX-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- **Problema real (medido por P-FIX-MICSENS):** el `voice.json` del usuario lo escribió la app Python con `input_device_index: 4`. En PortAudio/MME el 4 es una **salida** (`Altavoces (TA82)`), así que Python lo descarta y elige solo el micrófono `Webcam AC410`. En Rust (cpal/WASAPI) el 4 es `Micrófono (TA82)`, el micrófono de la barra de sonido, pegado a sus altavoces: oye el eco del TTS y el ruido, y el barge-in corta las respuestas. P-FIX-MICSENS añadió `input_device_name` (nombre → índice → automático), pero con un `voice.json` que solo tiene el índice (el caso del usuario y de cualquiera que venga de Python) Rust **sigue** abriendo el TA82.
- **Qué hacer:** que en Rust `input_device_index` (de `voice.json`, de `--mic N` y del selector del menú) signifique **exactamente el mismo dispositivo que en PyAudio**: reproduce la numeración de PortAudio en Windows (enumera como lo hace PortAudio; como mínimo MME, que es la API de host por defecto de PyAudio y la que ocupa los primeros índices; compruébalo contra la lista real de PyAudio en esta máquina), traduce índice de PortAudio → nombre → dispositivo de cpal (los nombres MME vienen truncados a 31 caracteres: empareja por prefijo y sin distinguir mayúsculas), y si el índice es una salida, no responde o no existe, cae a la elección automática **igual que Python**. Al guardar desde el menú o con `--mic`, escribe el índice en la numeración de PortAudio (para que la app Python lo entienda igual) y además `input_device_name`.
- **`--list-mics`** debe imprimir la misma lista que el Python (índices, nombres, tasa, canales y formato de línea): compara la salida de los dos binarios en esta máquina y añade la comparación a `rust/crates/ez-app/tests/cli_parity.rs` si existe la prueba de CLI (ejecuta el Python en el momento de la prueba solo si está el `.venv`; si no, se omite con mensaje).
- **Comprobación real obligatoria en el informe:** con el `voice.json` real del usuario (`input_device_index: 4`, sin nombre), la app Rust abre `Micrófono (Webcam AC410)` como la de Python (muéstralo con la línea de registro `Micrófono: …` de las dos apps). Pruebas unitarias con una enumeración falsa (índice de salida → automático, índice válido → mismo nombre, nombre truncado a 31 → empareja). `cargo fmt`, `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings`, `cargo test -p ez-audio -p ez-app` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-MICINDEX-r1.md` y un commit en tu rama.
