# AGENTS.rust.md — contrato de los trabajadores de la migración a Rust

Todo brief empieza con: **«Excepción a AGENTS.md: rama de migración a Rust;
aplica `rust-migration/AGENTS.rust.md`.»** Este archivo sustituye a `AGENTS.md`
mientras dure la migración. Lo que no contradiga aquí sigue vigente.

Fuente de verdad: el código Python del tag `py-baseline`. Si el Python y este
documento discrepan, gana el Python.

## 1. Alcance de la sesión

- Una tarea por sesión, solo los archivos del brief.
- Son de **solo lectura**: `src/`, `tests/`, `parity/fixtures/`,
  `parity/golden/`, `parity/thresholds.toml`, `parity/test-map.csv`,
  `rust-migration/tasks.yaml`.
- No toques `Cargo.toml` salvo que el brief diga que eres el dueño de una
  dependencia nativa (solo `F2-10` para `freetype-rs` y `F4-04` para `ort`).
- Nada de refactors, renombrados ni «mejoras» fuera del brief.

## 2. El Python es la especificación

Lee primero, completo, el Python que indica el brief y los fixtures de tu
dominio. Porta cada función pública, cada rama, cada valor por defecto, cada
texto y cada mensaje de error. Prohibido `todo!()`, `unimplemented!()`,
`// TODO`, `#[ignore]` y pruebas comentadas.

Si el Python tiene un error: **no lo corrijas**. Repróducelo igual y anótalo en
`rust-migration/python-quirks.md` (una entrada por hallazgo, con archivo, línea,
efecto y recomendación).

## 3. Textos

Se copian literalmente del Python, en español neutro latinoamericano. No se
reescribe ningún texto, ni siquiera una tilde.

## 4. Límites

- Módulo `.rs` ≤ 600 líneas; función ≤ 60 líneas (`clippy.toml`: 60), salvo
  tablas de datos.
- Diff ≤ ~300 líneas por paso; `cargo test` al terminar cada paso.
- `cargo fmt` y `cargo clippy --workspace --all-targets -- -D warnings` limpios.

## 5. Seguridad (no negociable)

- Nada de `cmd /c`, `powershell -Command` con texto interpolado ni ningún
  shell. `std::process::Command` solo con los binarios declarados y argumentos
  en lista.
- Borrar solo en directorios temporales propios; lo del usuario va a la
  Papelera (`trash`).
- `unsafe` solo en `ez-hud::win`, `ez-audio::capture`, `ez-tts::phonemes`,
  `ez-app::{tray, hotkeys}`, cada bloque con `// SAFETY: <por qué es válido>`.
- Claves de API nunca en logs ni en auditoría.
- El servidor del enjambre solo escucha en `127.0.0.1` y exige token por sesión.

## 6. Logs y concurrencia

- `tracing`; nada de `println!` salvo la salida de `ezs` y de las opciones de
  línea de comandos que hoy imprimen.
- Nada bloquea el hilo de UI ni los de audio. Estado compartido con lock o
  canal. Mismos nombres de hilo que Python (`ezagent-turn`, `ezagent-speech`,
  `ezagent-models`…). Sin async/tokio: `reqwest::blocking` en hilos de trabajo.

## 7. Dependencias

Solo las de `[workspace.dependencies]`. Si hace falta otra, detente y
explícalo en el informe.

## 8. Si algo no está especificado

Elige la opción más simple y segura, márcala con `// DECISION: <motivo>` y
menciónala en el informe.

## 9. Trampas de Python a Rust

| Python | Rust correcto |
|---|---|
| `round(x)` redondea a par (2.5 → 2) | `f64::round_ties_even`, nunca `round` |
| `int(x)` trunca; `//` y `%` usan piso (también con negativos) | `trunc`, `div_euclid`/`rem_euclid` o `floor` explícito según el caso |
| `len(s)`, cortes `s[:280]` cuentan puntos de código | `chars().count()`, cortar por `char_indices`, nunca por bytes |
| `str.lower()`, `casefold()`, `unicodedata.normalize` | `to_lowercase()` (verificar con fixtures), `unicode-normalization` |
| `re` admite lookahead/lookbehind y referencias | `regex`; si el patrón las usa, `fancy-regex`. Cada regex con su fixture |
| `dict` conserva el orden de inserción | `IndexMap` |
| `json.dumps` (`", "`, `": "`, `ensure_ascii=True`, `1e-05`) | `ez_core::pyjson` |
| `datetime.isoformat()`, `time.time()` | `ez_core::pytime` |
| numpy calcula en float32 si el array es float32 | `f32` donde Python es `float32`; `f64` donde es `float64` |
| `sorted(key=...)` es estable | `sort_by` (estable); flotantes con `total_cmp` |
| Excepciones que Python convierte en texto | `Result` + el mismo texto final |
| `str.strip()` quita también `\x0b\x0c` y espacios Unicode | `trim_matches(char::is_whitespace)` no basta: porta el conjunto exacto |
| `f"{x:g}"`, `str(float)` | `ez_core::pyjson::float_repr` |

## 10. Informe final

Formato de `AGENTS.md` §6, en `rust-migration/reports/<ID>-r<ronda>.md`, más:

- lista de fixtures consumidos y número de casos,
- lista de pruebas portadas (filas de `parity/test-map.csv`),
- `// DECISION` tomadas,
- cualquier desviación del brief.
