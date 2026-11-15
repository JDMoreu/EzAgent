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


## Q-P-AGENT-C-01
- Archivo y líneas: `src/ezagent/agent/tools_herdr.py:467-470`
- Qué hace mal: `_delegate_summary` reutiliza la variable `shown` para las teclas
  y para el destino; la confirmación de `keys` sale como «Pulsar enter esc en
  enter esc.» (las teclas otra vez como nombre del agente).
- Cómo se reproduce en Rust: `tools_herdr/summaries.rs::keys_summary` repite el
  mismo texto; fixture `tools_herdr/summary/delegate/014-keys`.
- Recomendación posterior: usar una variable distinta para el destino
  (`shown(address, label_for)`); tocaría regenerar los fixtures de ese resumen.

## Q-P-AGENT-C-02
- Archivo y líneas: `src/ezagent/agent/tools_herdr.py:904-910`
- Qué hace mal: `_handle_voice` con `close` evalúa `_user_text(callbacks)` dos
  veces (una en `_wants_notify` y otra en `remember_task`); un callback con
  efectos laterales se invoca dos veces.
- Cómo se reproduce en Rust: `tools_herdr/voice.rs::close_action` consulta el
  texto otra vez dentro del aviso para clonar la secuencia de llamadas
  (`tools_herdr/voice/002-close-notify`).
- Recomendación posterior: guardar el texto en una variable local; no cambia
  el resultado visible, solo el número de llamadas al callback.

## Q-P-AGENT-C-03
- Archivo y líneas: `src/ezagent/agent/tools_swarm.py:188-198`
- Qué hace mal: si el encargo supera `MAX_GOAL` y la ruta de contexto también
  supera `MAX_REF`, `_goal` devuelve el encargo entero; el tablero lo recorta a
  600 caracteres y la nota «[Encargo completo…]» desaparece sin aviso.
- Cómo se reproduce en Rust: `tools_swarm.rs::goal` devuelve el original en ese
  caso y `SwarmService::start` lo recorta; fixture
  `tools_swarm/call/start-goal-long-path`.
- Recomendación posterior: recortar antes de decidir o acortar la ruta; si se
  corrige, hay que regenerar el tablero de ese fixture.
