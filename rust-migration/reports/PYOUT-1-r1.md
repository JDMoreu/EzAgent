# Informe PYOUT-1 (ronda 1)

## Resultado

- Brief/tarea: PYOUT-1 — la versión Rust deja de depender de Python (DLL
  propias por descargas y pruebas sin Python en vivo).
- Estado: completado.

## Cambios

- `rust/crates/ez-core/src/manifest.toml`: manifiesto propio de la versión Rust.
  Copia literal de los 11 recursos del Python más `onnxruntime` (paquete NuGet
  `Microsoft.ML.OnnxRuntime` 1.31.1) y `espeak-ng` (rueda `espeakng_loader`
  0.2.4), con el SHA-256 calculado sobre el archivo realmente descargado.
- `rust/crates/ez-core/src/downloads.rs`: el manifiesto incrustado pasa a ser el
  de Rust (antes `include_str!` del árbol de Python, que desaparece en el paso
  siguiente). La prueba del manifiesto comprueba los 13 recursos y el `dest` /
  `strip_prefix` de los dos nuevos.
- `rust/crates/ez-tts/src/phonemes.rs`: `espeak-ng` se busca en la carpeta de
  descargas (`<datos>/espeak-ng`) además de junto al ejecutable, con el mismo
  orden que la DLL de ONNX Runtime (`EZAGENT_ESPEAK_*`, ejecutable, descargas).
- `rust/crates/ez-tts/src/phonemes/tests.rs`: prueba de que la carpeta de
  descargas de espeak-ng es `<datos>/espeak-ng` (la que crea el manifiesto).
- `rust/crates/ez-tts/src/kokoro/session.rs` y
  `rust/crates/ez-audio/src/turn/session.rs`: solo el comentario y el mensaje de
  error de `onnxruntime.dll`, que ahora indica `ezagent-voice downloads
  onnxruntime` (la búsqueda en la carpeta de datos ya existía).
- `rust/run-dev.cmd`: sin `.venv` ni `ORT_DYLIB_PATH`/`EZAGENT_ESPEAK_*`; si
  faltan las DLL en la carpeta de datos, avisa de `downloads --all` y sale.
- `EzAgent.bat`: `:rust_env` ya no lee `.venv`; comprueba las DLL de la carpeta
  de datos y, si faltan, sugiere `install`. `:install_all` ejecuta
  `ezagent-voice downloads --all` después de compilar.
- `packaging/build-rust.ps1`: las DLL y `espeak-ng-data` salen de la carpeta de
  datos de las descargas; se quitó `-SitePackages`. La licencia MIT de ONNX
  Runtime se escribe en `LICENSES` porque el paquete NuGet solo deja la
  subcarpeta de las DLL (el texto es el de Microsoft, copiado literal).
- `rust/crates/ez-app/tests/cli_parity.rs`: compara `--help` y `--write-config`
  contra `parity/frozen/cli_help.json` y `cli_write_config.json`; se quitó la
  parte en vivo de `--list-mics` y `--list-agents` (su formato ya está cubierto
  con dobles en `src/app/main/tests.rs`: `mic_row`, `agent_row`).
- `rust/crates/ez-app/tests/eval_context_parity.rs`: compara los tres casos
  contra `parity/frozen/eval_context.json`, sin ejecutar `provider.py`.
- `parity/frozen/cli_help.json`, `cli_write_config.json`, `eval_context.json`:
  salidas del Python (aún existente) congeladas; la hora del prompt y la ruta
  del `voice.json` quedan normalizadas.

## Decisiones

- Manifiesto propio en `rust/crates/ez-core/src/manifest.toml` en vez de tocar
  `src/ezagent/core/manifest.toml` (solo lectura): Rust deja de incluir un
  archivo del árbol Python, que se retira en el paso 2. Los 11 recursos viejos
  son idénticos byte a byte (`diff` de las claves del TOML).
- `onnxruntime` desde **NuGet** (`microsoft.ml.onnxruntime` 1.31.1) y no desde el
  zip de GitHub Releases: los dos son oficiales de Microsoft y dan la misma DLL,
  pero el zip trae `onnxruntime.pdb` (429 MB) y la extracción del manifiesto solo
  filtra por prefijo, así que ensuciaría la carpeta de datos. `strip_prefix =
  "runtimes/win-x64/native/"` deja solo las DLL y sus `.lib`.
- La DLL de ONNX Runtime descargada **no** es idéntica a la del `.venv`
  (compilación distinta: `1.31.1.20261023.4.41d4d61` frente a
  `1.31.1.20261023.3.6af1a93`), pero la versión es la misma (1.31.1) y las
  pruebas de Kokoro y Smart Turn pasan con ella. La `espeak-ng.dll` sí es
  idéntica (`646d387a…`).
- `--write-config` congela a partir de una configuración semilla con
  `llm_pinned` y sin `default_cwd`, para que el `voice.json` esperado no dependa
  del `.env` ni de la carpeta de ejecución. La ruta del archivo se compara como
  `{config}`.
- La búsqueda de espeak-ng queda «ejecutable y después descargas», igual que
  ONNX Runtime, como pide el brief (antes era al revés).
- Los `\r\n` del texto de `--help` y del `voice.json` congelados son los que
  escribe Python en Windows; el binario Rust los reproduce (`write_stdout`).

## Verificación

- `cargo fmt --all -- --check`: ok.
- `cargo clippy --workspace --all-targets -- -D warnings`: ok (verificado por la coordinación; ver
  abajo).
- `cargo test --workspace` desde `rust/`, **sin** `.venv` en el worktree y sin
  `ORT_DYLIB_PATH` ni `EZAGENT_ESPEAK_*` en el entorno: ok, 104 binarios en verde (verificado por la coordinación).
- `ezagent-voice downloads onnxruntime espeak-ng` descargó y verificó los dos
  recursos (159 626 394 y 9 437 292 bytes) en `%LOCALAPPDATA%\EzAgent`.
- `ezagent-voice downloads --list`: los dos nuevos aparecen «instalado».
- `ez-audio` `turn_parity`: los 27 casos de `probability` se ejecutaron con el
  modelo real y la DLL descargada (sin mensaje de omisión); 56 s.
- `ez-tts` `tts_parity`: los 297 casos, incluida la síntesis con Kokoro, con la
  DLL descargada y `espeak-ng` de la carpeta de datos; 163 s.
- `run-dev.cmd` con `EZAGENT_HOME` vacío: avisa de las DLL que faltan y sale con
  código 1 (no arranca la app a medias).
- Pruebas portadas: no hay fixtures de paridad para este dominio; los datos
  congelados consumidos son 3 (`parity/frozen/*.json`).

## Pendiente o riesgos

- Siguen dependiendo del Python (los retira el paso 2, no se tocan aquí):
  `rust/crates/ez-app/src/bin/ezagent_compat.rs` (lo maneja
  `parity/compat/run_compat.py`), el propio `parity/compat/` y
  `parity/robustness/run_robustness.py compare`. `tools_skills_parity` usa
  `C:/Python/python.exe` solo como cadena de configuración con un runner falso:
  no ejecuta nada.
- `packaging/build-rust.ps1` todavía copia `src\ezagent\voice\assets\fonts` y
  `src\ezagent\agent\default_skills` del árbol Python; no era parte de esta
  tarea (solo las DLL) y queda anotado para el paso 2.
- `parity/frozen/eval_context.json` congela el prompt de sistema de este commit:
  si otra tarea cambia `system.md`, las skills o las herramientas, habrá que
  volver a congelarlo.
- El texto congelado de `--help` incluye `\r\n` (Windows). La suite es de
  Windows; en otro sistema habría que recongelarlo.
- `whisper-vulkan` aparece «falta» en `downloads --list` de esta máquina porque
  no tiene el marcador `.installed-*`; es anterior a esta tarea (el Python dice
  lo mismo) y no afecta a las pruebas.
