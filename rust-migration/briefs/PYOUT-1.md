# PYOUT-1 — La versión Rust deja de depender de Python: DLL propias por descargas y pruebas sin Python en vivo
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- Decisión del usuario: «la versión de Python ya no debería existir, ahora solo debería ser la de Rust». Esta tarea es el paso 1 (Rust independiente); el paso 2 (borrar la app Python) lo hace otra tarea después. **No borres nada de Python en esta tarea.**
- Dependencias actuales de Rust con Python: `rust/run-dev.cmd` y `EzAgent.bat` (`:rust_env`) toman `onnxruntime.dll` de `.venv/Lib/site-packages/onnxruntime/capi/` (onnxruntime **1.31.1**) y `espeak-ng.dll` + `espeak-ng-data` de `.venv/Lib/site-packages/espeakng_loader/` (espeakng_loader **0.2.4**); `packaging/build-rust.ps1` las copia del mismo sitio; la búsqueda en Rust: `rust/crates/ez-tts/src/kokoro/session.rs` y `rust/crates/ez-audio/src/turn/session.rs` (`ORT_DYLIB_PATH` o junto al exe), `rust/crates/ez-tts/src/phonemes.rs` (`EZAGENT_ESPEAK_*`, carpeta de datos, junto al exe). El manifiesto de descargas: `rust/crates/ez-core/src/downloads.rs` (incrustado) y su origen Python `src/ezagent/core/downloads.py` + el JSON de manifiesto que use.
- Pruebas Rust que ejecutan Python en vivo: `rust/crates/ez-app/tests/cli_parity.rs` (corre `.venv/Scripts/python.exe` para `--help`, `--list-agents`, `--list-mics`, `--write-config`), `rust/crates/ez-app/tests/eval_context_parity.rs` (corre `evals/provider.py`), `rust/crates/ez-app/src/bin/ezagent_compat*` y `parity/compat/` (lado Python), `parity/robustness/run_robustness.py compare` (motor Python), `rust/crates/ez-agent/tests/tools_skills_parity.rs` (revisa si ejecuta un intérprete o solo usa una ruta de texto).

## Archivos que se tocan (solo estos)
- rust/crates/ez-core/src/downloads.rs (+ su manifiesto) para añadir los recursos `onnxruntime` y `espeak-ng` (DLL + `espeak-ng-data`)
- rust/crates/ez-tts/src/** y rust/crates/ez-audio/src/turn/** solo para buscar las DLL también en la carpeta de datos de descargas (después de `ORT_DYLIB_PATH`/`EZAGENT_ESPEAK_*` y de la carpeta del ejecutable)
- rust/run-dev.cmd, EzAgent.bat (`:rust_env` sin `.venv`), packaging/build-rust.ps1 (las DLL salen de las descargas, no del `.venv`)
- rust/crates/ez-app/tests/cli_parity.rs, rust/crates/ez-app/tests/eval_context_parity.rs y datos nuevos en `parity/frozen/**` (salidas de Python congeladas)
- rust-migration/reports/PYOUT-1-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- (1) **DLL por descargas.** Añade al manifiesto de descargas (el incrustado de Rust; si el manifiesto es un JSON compartido con Python, el JSON) los recursos `onnxruntime` (la versión **1.31.1** para Windows x64, del paquete oficial de Microsoft en GitHub Releases `microsoft/onnxruntime` o NuGet `Microsoft.ML.OnnxRuntime`; elige el que dé `onnxruntime.dll` y `onnxruntime_providers_shared.dll` de esa versión) y `espeak-ng` (la `espeak-ng.dll` y `espeak-ng-data` de la rueda `espeakng_loader` **0.2.4** de PyPI, que es un zip). Descárgalos de verdad, **calcula el SHA-256** del archivo descargado y escríbelo en el manifiesto (nunca inventes un hash ni una URL: si no encuentras una URL oficial estable, detente y explícalo en el informe). `ezagent-voice downloads --all` los instala; la app los encuentra en la carpeta de datos sin variables de entorno. Comprueba que la DLL descargada es **idéntica** (mismo SHA-256) a la del `.venv` o, si no lo es, que la versión coincide; `tts_parity` y `turn_parity` (o como se llamen las pruebas de Kokoro y Smart Turn) deben pasar usando la DLL descargada.
- (2) **Sin `.venv` en el arranque.** `run-dev.cmd`, `EzAgent.bat` y `build-rust.ps1` ya no leen `.venv`: si faltan las DLL, dicen que se ejecute `ezagent-voice downloads --all` (o lo ejecutan en `install`). (3) **Pruebas sin Python en vivo.** Ejecuta ahora el Python (aún existe) para congelar en `parity/frozen/` sus salidas: `--help`, `--write-config` (y lo que compare `cli_parity` que no dependa del equipo), y la salida de `provider.py` en modo `context` para los casos de `eval_context_parity` (normaliza la hora). Cambia esas pruebas para comparar contra lo congelado; `--list-mics`/`--list-agents` dependen del equipo: compáralos contra el formato con dobles (ya hay pruebas de formato) y elimina la parte en vivo. `ezagent_compat`, `parity/compat` y `run_robustness.py compare` dependen de la app Python: no los cambies ahora (los retira el paso 2), solo lista en el informe qué dependería de Python. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` en verde **desde tu worktree, que no tiene `.venv`, y sin `ORT_DYLIB_PATH` ni `EZAGENT_ESPEAK_*`** en el entorno, para demostrar que Rust ya no lo necesita. **Nunca** renombres, borres ni modifiques `C:/Users/JDMoreu/Documents/EzAgent/.venv`: lo usan otros procesos en marcha (puedes usarlo solo para leer y para ejecutar el Python al congelar salidas).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/PYOUT-1-r1.md` y un commit en tu rama.
