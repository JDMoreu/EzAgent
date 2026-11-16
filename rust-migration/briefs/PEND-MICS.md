# PEND-MICS — --list-mics y el índice de micrófono idénticos a PyAudio con todas las APIs de host (MME, DirectSound, WASAPI, WDM-KS)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- `src/ezagent/voice/audio.py` (`list_input_devices`) y `src/ezagent/voice/app.py` (`--list-mics`): con el `.venv`, `C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe -m ezagent.voice.app --list-mics` da la lista de referencia (14 líneas en esta máquina: MME 0-2, y los mismos micrófonos por DirectSound, WASAPI y WDM-KS con sus índices).
- Rust: `rust/crates/ez-audio/src/capture/portaudio.rs` (hoy reproduce solo el bloque de MME con `winmm`), `capture/choice.rs`, `rust/crates/ez-app/src/app/main/lists.rs`, `rust/crates/ez-app/tests/cli_parity.rs` (`compare_list_mics`), informe `rust-migration/reports/P-FIX-MICINDEX-r1.md`; el código fuente de PortAudio (`pa_win_wmme.c`, `pa_win_ds.c`, `pa_win_wasapi.c`, `pa_win_wdmks.c`) como referencia del orden y los nombres

## Archivos que se tocan (solo estos)
- rust/crates/ez-audio/src/capture/portaudio.rs y rust/crates/ez-audio/src/capture/portaudio/** (aquí `unsafe` está permitido con `// SAFETY:`; usa el crate `windows` del workspace y sus features ya activas; si hace falta una feature nueva de `windows`, añádela en `rust/Cargo.toml` y dilo)
- rust/crates/ez-audio/src/capture/choice.rs, rust/crates/ez-app/tests/cli_parity.rs
- rust-migration/reports/PEND-MICS-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- Reproduce el **espacio de índices global de PortAudio en Windows** tal como lo da PyAudio: el orden de las APIs de host y, dentro de cada una, el orden, el nombre, la tasa por defecto y los canales de cada dispositivo de entrada. Comprueba en esta máquina el orden real de PyAudio (`get_host_api_info_by_index`, `get_device_info_by_index`). `--list-mics` debe imprimir **exactamente** las mismas líneas que el Python, y `cli_parity` debe comparar la lista completa (no solo el bloque MME). La traducción índice → dispositivo de cpal sigue funcionando para cualquier índice (de cualquier API) que corresponda a una entrada; una salida o un índice inválido → elección automática, como el Python.
- Si alguna API (probablemente WDM-KS) no se puede reproducir con fidelidad sin una dependencia nueva, reproduce las demás, documenta exactamente qué difiere y por qué en el informe, y deja la prueba comparando lo que sí coincide sin ocultar la diferencia. Pruebas unitarias con enumeraciones falsas y la comparación real con el Python. `cargo fmt`, `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings`, `cargo test -p ez-audio -p ez-app` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/PEND-MICS-r1.md` y un commit en tu rama.
