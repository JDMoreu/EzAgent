# P-TURN — Smart Turn: inferencia ONNX y paridad de `turn`

## Resultado
- Brief/tarea: P-TURN
- Estado: completado (dominio `turn` 87/87). Queda un riesgo de integración con `ez-tts` fuera del alcance de archivos (ver Pendiente).

## Cambios
- `rust/crates/ez-audio/Cargo.toml`: añade `ort = { workspace = true }` (dependencia autorizada por el brief). No se añadió `ndarray` (no hizo falta).
- `rust/Cargo.lock`: la entrada de `ez-audio` gana `ort`.
- `rust/crates/ez-audio/src/turn/session.rs` (nuevo): `OrtTurnModel`, carga perezosa de `smart-turn-v3.2-cpu.onnx` con `ort` (`load-dynamic`, `ORT_DYLIB_PATH`/DLL junto al ejecutable/carpeta de datos), opciones de `SmartTurn._load` (secuencial, inter 1, intra 2, CPU), entrada `input_features` (1 x 80 x 800) y salida `logits[0]`.
- `rust/crates/ez-audio/src/turn.rs`: `load_model` abre la sesión real (con `resolve_model_path()` si la ruta viene vacía, como el Python); guarda la sesión antes del precalentado (si el precalentado falla, `probability` sigue funcionando y `ready` no se enciende, como el Python); `installed_model_path()` nuevo para consultar el modelo sin descargarlo; prueba sin backend cambiada a «modelo ausente», que ya no depende de que falte la DLL.
- `rust/crates/ez-audio/tests/turn_parity.rs`: los 27 casos `turn/probability` se ejecutan contra el modelo real y se comparan con `f32_rel = 1e-5` (probabilidad y `ended`); si faltan DLL o modelo se omiten con aviso explícito y se cuentan aparte. Añade `loaded_model`, `missing_model_reason` y `compare_probability`.
- `rust/crates/ez-app`: sin cambios. `core/parts.rs` ya resolvía la ruta y construía `SmartTurn::new`; con esto el hilo `ezagent-turn` carga el `TurnModel` real.
- `rust-migration/python-quirks.md`: sin entradas nuevas (no se encontró ningún error en `turn.py`).

## Decisiones
- `// DECISION` en `turn/session.rs`: el entorno de `ort` y la búsqueda de la DLL se repiten en vez de depender de `ez-tts`; añadir esa dependencia está fuera del brief y crearía un ciclo (`ez-tts` usa `ez-audio`).
- `installed_model_path()`: API nueva (no existe en Python) para que la prueba decida si omitir sin descargar 8,7 MB; `resolve_model_path` la reutiliza para no duplicar la comprobación de `is_installed`.
- La prueba ejecuta la comparación de `probability` dentro de la única función `#[test]` del archivo (crea un entorno de ONNX Runtime por proceso).
- Sin `parts.rs`: el cableado existente ya pasa la ruta real; el cambio necesario estaba en `ez-audio`.

## Verificación
- `cargo fmt --all --check`: ok.
- `cargo clippy -p ez-audio --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio` con `ORT_DYLIB_PATH=C:/Users/JDMoreu/Documents/EzAgent/.venv/Lib/site-packages/onnxruntime/capi/onnxruntime.dll`: 66 lib + `audio_parity` + `turn_parity` (87 casos, 50 s) + `wake_parity`, todo en verde.
- `cargo test -p ez-audio` sin `ORT_DYLIB_PATH`: verde; `turn_parity` avisa «turn: 27 casos de `probability` omitidos: no hay onnxruntime.dll (define ORT_DYLIB_PATH)».
- `cargo build -p ez-app`: ok.
- Los 27 casos `probability` corrieron con el modelo real (`sha256` del modelo instalado = `2bb02631…`, el del fixture) y la DLL del `.venv`; ninguno falló.

### Por dominio
- `turn` (`parity/fixtures/turn.jsonl`): 87/87 casos. 60 de `hz_to_mel`/`mel_to_hz`/`log_mel`/`prepare` ya pasaban; los 27 de `probability` pasaron al primer intento con la sesión real. Defectos de Rust corregidos: ninguno. Hallazgos de Python: ninguno. Casos que siguen fallando: ninguno.
- Fixtures consumidos: `turn.jsonl`, 87 casos (13 `hz_to_mel`, 13 `mel_to_hz`, 14 `log_mel`, 20 `prepare`, 27 `probability`).
- Pruebas de `parity/test-map.csv` portadas: ninguna fila nueva (la fase de pruebas de paridad es posterior). Las filas `tests/test_turn.py::*` siguen en `pendiente`; `test_smart_turn_sin_modelo_no_esta_listo_y_no_bloquea` queda cubierta de facto por `missing_model_is_never_ready`.

## Pendiente o riesgos
- **Un solo `ort::Environment` por proceso.** `ez-tts` crea el suyo (`"ezagent-tts"`) al cargar Kokoro en `VoiceApp::new`, antes de `apply_start_audio`. Si `ez-audio` crea otro, `ort` rc15 falla con «only one environment is allowed per process» y Smart Turn queda desactivado (con el aviso «Smart Turn no disponible; uso solo el silencio: …»). `Environment::current()` es `pub(crate)` en `ort`, así que no se puede reutilizar desde otro crate. Arreglo recomendado (fuera del brief): exponer el entorno desde `ez-tts::kokoro::session` (p. ej. `pub fn environment(path: &Path) -> Result<&'static Environment, String>`) o moverlo a un módulo compartido de `ez-core`, y que `ez_audio::turn::session` lo use en vez de crear el suyo. Sin eso, la app arranca con Kokoro y sin Smart Turn; el resto de la paridad no se ve afectado.
- `parts.rs` resuelve/descarga el modelo en el hilo que llama (`create_turn_detector`), como ya hacía antes de esta tarea; el Python lo hace dentro del hilo `ezagent-turn`. No se tocó por estar fuera del cambio mínimo (la ruta y el comportamiento del modelo son los de `turn.py`).
