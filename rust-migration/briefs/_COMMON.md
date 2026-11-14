# Reglas comunes de esta fase («código primero, pruebas después»)

Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
Lee ese archivo completo antes de empezar. Este documento añade lo aprendido en
las tareas ya portadas (F2-03 … F3-08) y manda sobre él en lo que contradiga.

## Alcance

- Tu worktree es una copia de `rust/main`. Trabaja **solo** en los archivos que
  lista tu brief. Si necesitas tocar otro, **no lo toques**: resuélvelo dentro de
  tu crate y anótalo en el informe.
- **No añadas dependencias** ni cambies ningún `Cargo.toml`. Solo existen las de
  `[workspace.dependencies]` y las que ya declara tu crate (incluido `ez-core`).
- El Python (`src/ezagent/…`) es la especificación y es de solo lectura. Léelo
  completo. Porta cada función pública, rama, valor por defecto y texto
  (español neutro latinoamericano, copiado literal, hasta la tilde).
  Si el Python tiene un error, **repróducelo** y anótalo en el informe.
- En esta fase **no se escriben pruebas de paridad contra `parity/fixtures`**
  (se hacen después). Sí escribe pruebas unitarias propias (`#[cfg(test)]`) que
  comprueben lo esencial de cada módulo. Los fixtures de tu dominio existen en
  `parity/fixtures/<dominio>.jsonl`: puedes leerlos para entender el formato
  exacto de entradas y salidas, y para tus pruebas unitarias.

## Calidad (la puerta que voy a pasar al validar tu entrega)

Desde `rust/` debe quedar todo limpio:

```
cargo fmt --all
cargo clippy -p <tu-crate> --all-targets -- -D warnings
cargo test -p <tu-crate>
```

Patrones **prohibidos en el código que añadas** (la puerta los rechaza):
`todo!`, `unimplemented!`, `#[ignore]`, la palabra `TODO`, `println!` (usa
`tracing`; solo `ezs` y `ezagent_voice` pueden imprimir), `remove_dir_all`,
`"cmd"`, `"/c"`, `-Command`, `unsafe` fuera de los módulos que permite
AGENTS.rust.md §5, y `.unwrap()` fuera de `tests/` (usa `?` o
`.expect("motivo")`). Ningún `.rs` supera **600 líneas** (cuéntalas **después**
de `cargo fmt`, que expande) y ninguna función ~60 líneas: parte en submódulos
(`modulo/parte.rs`, declarados en el `modulo.rs`) o en funciones auxiliares.

Seguridad: sin `shell`, ni `cmd /c`, ni `powershell -Command`; subprocesos con
`std::process::Command` y argumentos en lista y entorno explícito. Borrar solo a
la Papelera (`trash::delete`) o archivos temporales propios (`remove_file`).
Las claves de API nunca van a logs.

Concurrencia: sin async/tokio; hilos (`std::thread`), `crossbeam-channel`,
`parking_lot` si tu crate lo declara, o `std::sync`. Mismos nombres de hilo que
Python. Nada bloquea el hilo de UI ni los de audio.

## Lo que ya existe y debes reutilizar (no lo reescribas)

`ez_core` (crate de la base compartida):
- `pyjson::{dumps, dumps_with, DumpOptions}`: `json.dumps` byte a byte.
  `DumpOptions::utf8()` = `ensure_ascii=False`; `.with_indent(2)`; `compact_utf8()`.
  `float_repr` = `repr(float)`.
- `pyloads::loads(text)`: `json.loads` con los mensajes de error de CPython.
- `pyvalue`: `py_strip/py_lstrip/py_rstrip`, `py_split`, `collapse_spaces`,
  `py_splitlines`, `head_chars/tail_chars/char_len` (cortes por carácter, nunca
  por bytes), `truthy` (= `bool(x)`), `python_str` (= `str(x)`), `python_repr`,
  `python_int`, `int_value`, `format_g` (= `:g`), `py_eq`, `casefold`,
  `fold_accents`, `strip_accents`, `text_or_empty`.
- `pytime`: `timestamp()`, `isoformat_local/utc/millis_utc`, `date_local`,
  `now_local_iso_seconds()`, `local_iso_seconds(secs)`, `json_seconds`.
- `paths::{resolve_safe, resolve, expand, fnmatch}`, `dirs::*` (carpetas de la
  app), `agent_config::AgentConfig`, `config::VoiceConfig`,
  `redact::redact`, `speech_text::*`, `llm_providers::*`, `downloads::*`.

`ez_agent` (si tu crate lo necesita): `registry::{ToolSpec, ToolResult, ToolInfo,
ToolRegistry}`. `ToolResult::err(msg)` = `ToolResult(ok=False, error=msg)` del
Python (outcome `completed`); `ToolResult::failed(msg)` es solo para fallos del
propio registro. `tools_d::run_captured` ejecuta subprocesos con timeout sin shell.

Trampas de Python → Rust que ya mordieron (AGENTS.rust.md §9 las lista todas):
`round()` redondea a par (`round_ties_even`); `len(s)`/cortes cuentan puntos de
código; `str.strip()` quita también `\x1c`-`\x1f`; `dict` conserva el orden
(usa `serde_json::Map` con `preserve_order` o `IndexMap`); `json.dumps` usa
separadores `", "` y `": "`; `Path.write_text` escribe CRLF en Windows;
`open("r")` traduce `\r\n` a `\n`; los errores de `OSError` salen con el texto de
Windows (aproxima con el de `std::io::Error`).

## Si algo no está especificado

Elige la opción más simple y segura, márcala con `// DECISION: <motivo>` y
menciónala en el informe. Nunca inventes alcance ni APIs externas: si dudas de un
flag de un CLI, hazlo configurable.

## Entrega

1. Cuando `fmt`, `clippy` y `test` de tu crate estén limpios: `git add -A` y
   **un commit** en tu rama con el mensaje `<ID>: <qué>`.
2. Escribe `rust-migration/reports/<ID>-r1.md` con el formato de AGENTS.md §6
   (Resultado / Cambios / Decisiones / Verificación / Pendiente o riesgos) y
   haz el commit incluyéndolo.
3. Termina con un resumen corto: estado (completado | parcial | bloqueado) y,
   si quedó algo sin portar, **qué función exacta** y por qué.

No empieces ninguna otra tarea. Si te quedas sin presupuesto, entrega lo
terminado y marca el resto como `parcial` en el informe.
