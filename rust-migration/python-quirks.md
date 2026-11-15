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

## Q-P-CHAT-01
- Archivo y líneas: `parity/gen/_common.py:56-59` (generación de `chat_trace`)
- Qué hace mal: el generador fija `TZ=UTC` y llama a `time.tzset()`, pero en
  Windows ese método no existe (`hasattr(time, "tzset") == False`), así que
  `time.localtime` siguió usando la zona de la máquina que generó los casos. Los
  30 casos de `stamp` implican un desfase fijo de −18000 s (UTC−5), no 0: por
  ejemplo `stamp/001` entra `t = 1768496400.0` y espera «15 ene 2026 · 12:00:00».
- Cómo se reproduce en Rust: `trace::stamp_with_offset(t, zona)` recibe el
  desfase deducido del propio fixture (la prueba comprueba que sea el mismo en
  todos los casos); `stamp`/`stamp_for` siguen usando la zona local real.
- Recomendación posterior: generar los textos con una zona explícita
  (`datetime.fromtimestamp(t, timezone(timedelta(hours=-5)))`) o fijar la zona
  del host de generación; afecta a `parity/fixtures/chat_trace.jsonl` y a su
  generador.

## Q-P-CHAT-02
- Archivo y líneas: `parity/gen/chat_store.py:1038-1046` (`_ContextRun.finish`)
- Qué hace mal: el escenario escribe `seed` en el disco, pero el `input` del
  fixture solo incluye `store`, `holder`, `setup`, `exports` y `ops`. El caso
  `chat_store/contexts/rebuild` no se puede reproducir desde su entrada: su
  `output.file` empieza con una línea «guardado» que nunca se declara.
- Cómo se reproduce en Rust: `chat_store_parity.rs` recompone el único seed
  afectado con el texto literal de `parity/gen/chat_store.py:1112`.
- Recomendación posterior: añadir `"seed": spec.get("seed")` al `input` y
  regenerar `parity/fixtures/chat_store.jsonl`.

## Q-P-CHAT-03
- Archivo y líneas: `src/ezagent/voice/chat/store.py:135-167` (`append`)
- Qué hace mal: el parámetro `source` está declarado `Source` (`Literal`), pero
  `append` no lo valida: guarda cualquier texto en el JSONL. Solo `_ingest`
  (línea 108-113) normaliza una fuente desconocida a `"chat"` al recargar, así
  que el archivo y la memoria pueden discrepar hasta el siguiente arranque.
- Cómo se reproduce en Rust: `store::Source` tiene la variante `Other(String)`;
  `ChatStore::append` la persiste tal cual y `ingest` la vuelve a `Chat`.
  `chat_store/append-coercions` depende de esta laxitud (`source: "otro"`).
- Recomendación posterior: validar `source` en `append` o tiparlo como `str` y
  normalizarlo al guardar; afecta a `chat_store/append-coercions`.

## Q-P-CHAT-04
- Archivo y líneas: `src/ezagent/voice/chat/store.py:342-345` (`recent`)
- Qué hace mal: `self._entries[-max(0, limit) :]` con `limit <= 0` usa
  `-0 == 0`, así que devuelve **todo** el historial en vez de nada. Un llamador
  que pida «los últimos 0 mensajes» recibe la conversación completa.
- Cómo se reproduce en Rust: `ChatStore::recent` devuelve todas las entradas
  cuando `limit <= 0`; `chat_store/recent` fija ese comportamiento con
  `limit: 0` y `limit: -1`.
- Recomendación posterior: devolver `[]` con `limit <= 0` y regenerar
  `chat_store/recent`; no hay otro consumidor que dependa del valor actual.
