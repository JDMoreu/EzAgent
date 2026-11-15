# P-APP — Paridad de `ez-app` (textos y comandos rápidos)

## Resultado
- Brief/tarea: P-APP
- Estado: completado (169/169 casos de `app_text` y la CLI en verde)

## Cambios
- `rust/crates/ez-app/src/app/core.rs`: `hotkey_key` partía primero por espacios
  (`py_split`) y luego por `+`; Python solo parte por `+` y recorta cada trozo.
  «ctrl alt» daba `ctrl+alt` en vez de `ctrl alt`.
- `rust/crates/ez-app/tests/app_text_parity.rs`: paridad de los 169 casos de
  `parity/fixtures/app_text.jsonl` (6 funciones), con un doble de `FastHost` y
  la frase de pausa «oye ez» del generador.
- `rust/crates/ez-app/tests/cli_parity.rs`: paridad de `--help`,
  `--list-agents` y `--write-config` del binario `ezagent-voice` contra el
  Python del proyecto, ejecutado en el momento de la prueba.
- `rust-migration/reports/P-APP-r1.md`: este informe.

## Por dominio

| Dominio | Casos | Verde | Defectos de Rust corregidos |
|---|---:|---:|---|
| `app_text` | 169 | 169 | 1: `hotkey_key` separaba por espacios antes de `+` |

Ningún caso queda fallando. No se modificó ningún fixture.

Desglose de `app_text`: `_Sentences.feed` 29 (28 + 1 error de tipo),
`_Sentences.flush` 20, `_strip_courtesy` 26 (24 + 2 errores),
`_is_filler_sentence` 29 (28 + 1 error), `_hotkey_key` 23 y `_fast_command`
42 (40 + 2 errores). Los 6 casos de error son `AttributeError`/`TypeError` de
Python con argumentos que la firma Rust no admite (`None`, `5`): se consumen
comprobando que la entrada no es texto, como en
`ez-agent/tests/handoff_packet_parity.rs`.

## Fixtures consumidos
- `parity/fixtures/app_text.jsonl` — 169 casos

Total: 169 casos, con `assert_eq!(cases_run, cases_in_file)` y
`assert!(cases_in_file >= 169)`.

## Pruebas portadas (`parity/test-map.csv`)
No hay filas de `app_text` en el mapa; las de la CLI tampoco están registradas.
La cobertura equivalente queda en los dos `tests/*_parity.rs` de arriba.

## Decisiones
- La prueba de CLI lanza el Python con `-s -c` y fija `sys.argv[0]` al nombre
  del binario Rust: `argparse` deriva de ahí el `prog` y el ancho de `usage:`,
  y sin eso la única diferencia sería el nombre del programa. Se añade
  `PYTHONIOENCODING=utf-8` para que la tubería use UTF-8 (sin consola, Python
  usa la codificación del sistema). `-I` no sirve aquí porque ignora esa
  variable.
- `--write-config <tmp>` del brief se ejecuta como
  `--config <tmp> --write-config`: en Python `--write-config` es un flag sin
  argumento, y el destino se da con `--config`. Se comparan salida, código y
  el `voice.json` escrito.
- El doble de `FastHost` de la prueba deja `label_for` vacío, usa
  `last_summary`, `tasks` y `close_action` del caso y ejecuta el trabajador de
  agentes en el acto (`agents_report` o `AGENTS_UNAVAILABLE` si el oráculo trae
  `"raise"`), igual que `_FakeHerdr`/`_start_worker` del generador.
- `_hotkey_key` admite `None` y números en Python (`str(spec or "")`): la prueba
  los convierte con `truthy`/`python_str` antes de llamar a `hotkey_key`.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-app`: 287 pruebas en verde (285 unitarias + 2 de paridad).
- pytest/ruff de Python: no aplica (Python de solo lectura).
- Métricas: 169/169 casos de `app_text`; 168 al primer intento y 1 defecto
  detectado leyendo el código antes de ejecutar la prueba.

## Pendiente o riesgos
- `parity/fixtures/wake.jsonl` (264 casos de `ez-audio::wake`) no se consume:
  la lista de dominios del brief asigna solo `app_text`, y el arreglo que haría
  falta para los dos casos de tasa negativa (`utterance_secs`/`worth_transcribing`
  con `rate: i32` en vez de `u32`) está en `ez-audio/src/wake.rs`, fuera de los
  archivos que puedo tocar. Si se quiere paridad de `wake` en una ronda
  siguiente, hay que ampliar el alcance a `ez-audio` (la función y el llamador
  en `ez-app/src/app/pause.rs`).
- `--list-agents` compara contra Herdr en vivo: si un agente cambia de estado
  entre las dos ejecuciones (milisegundos), la prueba puede fallar sin que el
  binario esté mal. En esta máquina pasó dos veces seguidas.
