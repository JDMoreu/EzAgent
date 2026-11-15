# Errores de Python reproducidos a propósito

Regla de fidelidad (plan §0): si al portar se descubre un error en Python, **no
se corrige**: se reproduce tal cual y se anota aquí. La decisión de arreglarlo
se toma después de la auditoría final.

Formato de cada entrada:

```
## <id>
- Archivo y líneas: <ruta>:<desde>-<hasta>
- Qué hace mal: <efecto observable>
- Cómo se reproduce en Rust: <dónde queda el mismo comportamiento>
- Recomendación posterior: <qué habría que cambiar y qué fixtures tocaría>
```

Sin entradas todavía: las tareas de F2–F6 agregan las que encuentren.

## Hallazgos de F1-09 (TTS, `kokoro_onnx`)

Reproducidos tal cual en el oráculo (`parity/fixtures/tts.jsonl`); Rust devuelve `Err` donde Python lanza.

- `kokoro_onnx/sliding.py:130`: `edges[end - first + 1]` queda fuera del arreglo en la última ventana; `synthesize` siempre lanza `IndexError`.
- `kokoro_onnx/sliding.py:41`: `token_edges` con duraciones que suman 0 divide por cero y convierte NaN a `int64` (`-9223372036854775808`).
- `kokoro_onnx/pauses.py:38-39`: `pauses.insert` con menos de una trama (240 muestras a 24 kHz) lanza `ValueError` en `loudness.max()`.
- `kokoro_onnx/pauses.py:97`: con `timing.end` negativo usa el piso de Python (índice negativo); Rust satura a `usize`.
- `kokoro_onnx/__init__.py:105-106`: `_load_vocab({})` lanza `KeyError: 'vocab'` (un dict vacío no equivale a `None`).
- `kokoro_onnx/chunker.py:41`: `split_phonemes("a", max_length=0)` lanza `ValueError`; Rust devuelve `[]`.
- `np.mean` de 3 o más estilos suma por pares; la media secuencial de Rust puede diferir en el último bit (tolerancia `f32_rel = 1e-5`).
## Q-F6-04-01
- Archivo y líneas: `src/ezagent/voice/app.py:1692-1696`
- Qué hace mal: `_set_state` llama a `cfg.save_key("hud_hidden", False)` sin
  `try/except`; un fallo de disco (por ejemplo, `voice.json` de solo lectura)
  propaga `OSError` desde cualquier cambio de estado (`_hang_up`,
  `_start_call`, `_finish_turn`…) y puede dejar el hilo de la interfaz a medias.
- Cómo se reproduce en Rust: `states.rs::set_state` guarda la clave y, si falla,
  registra `No pude guardar hud_hidden: …` con `log`; el cambio de estado sigue
  porque `set_state` no devuelve `Result`.
- Recomendación posterior: envolver `save_key` en `try/except OSError` en Python
  (el estado ya cambió en memoria) y decidir si el puerto debe exponer el fallo
  a quien llama; no afecta a fixtures de paridad.


## Q-P-AGENT-A-01
- Archivo y líneas: `src/ezagent/agent/system_prompt.py:186-189`
- Qué hace mal: `_os_description()` usa `platform.system()` y `platform.release()`
  del equipo donde corre; el texto `{{os}}` y, por tanto, el prompt completo
  cambian de una máquina a otra (`Windows 11` en el oráculo). Los fixtures de
  `system_prompt` solo son deterministas en ese host.
- Cómo se reproduce en Rust: `system_prompt.rs` fija `DEFAULT_OS = "Windows 11"`
  y expone `set_os_description` para que la aplicación ponga la versión real; la
  prueba de paridad no cambia ese valor y compara el resto byte a byte.
- Recomendación posterior: inyectar la descripción del SO desde la configuración
  (o fijarla en el generador de fixtures) y regenerar `system_prompt.jsonl` en un
  host declarado; afecta a `system_prompt/build/*` y `system_prompt/parts/*`.

## Q-P-SWARM-01
- Archivo y líneas: `src/ezagent/swarm/incidents.py:79-82` (regex `_CLOCK_12`) y `:119-123` (`parse_reset`)
- Qué hace mal: la hora de 12 h acepta `(\d{2})` como minutos sin validar el rango;
  `parse_reset("resets 7:60pm", ahora)` hace `now.replace(minute=60)` y lanza
  `ValueError: minute must be in 0..59`. En producción eso ocurre dentro de
  `Recovery.inspect`, así que un texto así en la pantalla de un agente tumba la
  pasada del supervisor en vez de ignorarse.
- Cómo se reproduce en Rust: `incidents/patterns.rs::parse_reset` valida con
  `and_hms_opt` y, si la hora no existe, devuelve `0.0` (DECISION ya anotada en
  ese módulo): no propaga el pánico. Ningún caso de `parity/fixtures/swarm.jsonl`
  usa minutos fuera de rango, así que la paridad no cambia.
- Recomendación posterior: limitar los minutos en `_CLOCK_12` a `[0-5]\d` (o
  envolver el `replace` en `try/except ValueError` y devolver 0). Tocaría
  regenerar `swarm/incidents/parse-reset/*` solo si se añaden casos con `:60`.
