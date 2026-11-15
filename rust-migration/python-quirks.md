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
## Hallazgos de P-AGENT-B (traspasos, tools_d, archivos y núcleo)

### Q-PAGENTB-01
- Archivo y líneas: `src/ezagent/agent/tools_fs.py:426-431` (`move_path`), mismo patrón en `copy_path:361-366`.
- Qué hace mal: el `kind` del resultado se calcula **después** de mover (`shutil.move` u `os.replace`), cuando `src` ya no existe; mover una carpeta informa `"kind": "file"` en vez de `"folder"`. Los casos `tools_fs/call/move-folder`, `move-folder-empty` y `move-folder-overwrite` del oráculo lo fijan así.
- Cómo se reproduce en Rust: `tools_fs/handlers.rs::transfer` evalúa `kind_of(&src)` después de `move_into_place`/`record_move`, con el comentario del quirk; `copy_path` conserva `"folder"` porque el origen sigue existiendo.
- Recomendación posterior: calcular `kind` antes de mover en Python (`kind = "folder" if src.is_dir() else "file"` al entrar) y regenerar `parity/fixtures/tools_fs.jsonl` (3 casos cambian de `file` a `folder`).
## Q-P-LLM-01
- Archivo y líneas: `src/ezagent/voice/llm.py:139-147` (`_status_of`)
- Qué hace mal: busca el código como subcadena (`f" {code}"` o `f"{code}:"`),
  así que clasifica como 429 cualquier texto que contenga « 429» como prefijo de
  otro número: `friendly_llm_error(Exception("Error 4290: raro"), …)` responde
  «me limitó por demasiadas peticiones» aunque el código sea 4290.
- Cómo se reproduce en Rust: `ez-llm/src/errors.rs::status_of` usa la misma
  comprobación por subcadena; el caso `llm_sse/friendly_llm_error/0026` lo fija.
- Recomendación posterior: extraer el código con un límite de palabra
  (`(?:^|\D)(401|403|404|413|429)(?:\D|$)`) y regenerar ese fixture; es un
  cambio observable en los avisos de voz.

## Q-P-LLM-02
- Archivo y líneas: `src/ezagent/voice/llm.py:1107-1140` (`_open_stream`)
- Qué hace mal: el `for attempt in range(3)` solo hace `continue` en los
  intentos 0 y 1; el intento 2 siempre retorna o lanza. El
  `raise RuntimeError("No pude abrir el stream del proveedor.")` posterior al
  bucle es inalcanzable, así que tres fallos seguidos devuelven el último texto
  del proveedor y nunca ese mensaje genérico.
- Cómo se reproduce en Rust: `ez-llm/src/messenger/recovery.rs::open_stream`
  conserva la misma estructura y la misma rama inalcanzable; los casos
  `llm_tools/turn/0024` y `0025` fijan el texto real que sale.
- Recomendación posterior: borrar la rama muerta o convertirla en el mensaje
  final del tercer intento; no cambia ningún fixture actual.

## Hallazgos de P-AUDIO-TTS (audio, turn, wake y TTS)

### Q-PAUDIO-01
- Archivo y líneas: `kokoro_onnx/sliding.py:43` (`token_edges`)
- Qué hace mal: con `duration` vacía (o toda a cero) `frames[-1] == 0`; la división
  `samples / frames[-1]` da `inf`/`NaN`, `np.round(NaN).astype(int64)` produce el
  mínimo de `int64` y los bordes salen `-9223372036854775808` en vez de un error.
  Lo fijan `tts/sliding/token_edges/0004` a `0007` (y `0013` con una sola muestra).
- Cómo se reproduce en Rust: `kokoro::sliding::token_edges` devuelve
  `vec![i64::MIN; frames.len()]` cuando la suma es 0, con el comentario del quirk.
- Recomendación posterior: devolver un error cuando `duration.is_empty()` y
  regenerar esos cinco casos; hoy `create_timed` nunca llega ahí porque exige una
  duración válida del modelo.

### Q-PAUDIO-02
- Archivo y líneas: `kokoro_onnx/chunker.py:41` y `:79` (`_atoms`/`split_phonemes`)
- Qué hace mal: `split_phonemes(texto_no_vacío, max_length=0)` llega a
  `range(0, n, 0)` y lanza `ValueError: range() arg 3 must not be zero`; con texto
  vacío devuelve `[]`. Corrige el aviso anterior de este archivo que decía que Rust
  devolvía `[]` en ambos casos.
- Cómo se reproduce en Rust: `kokoro::chunker::split_phonemes` ahora devuelve
  `Err("range() arg 3 must not be zero")` solo con texto no vacío (caso
  `tts/chunker/split/0006`); `create` propaga el error.
- Recomendación posterior: validar `max_length >= 1` al entrar en la función y
  decidir si el error debe ser `ValueError` propio; no afecta a la síntesis normal.

### Q-PAUDIO-03
- Archivo y líneas: `kokoro_onnx/pauses.py:35` (`_quiet_frames`) y `:83` (`insert`)
- Qué hace mal: si el audio dura menos que un fotograma (10 ms), `loudness` queda
  vacío y `loudness.max()` lanza
  `zero-size array to reduction operation maximum which has no identity`
  (`tts/pauses/insert/0012`).
- Cómo se reproduce en Rust: `kokoro::pauses::insert` devuelve ese mismo texto
  como `Err` antes de calcular los fotogramas.
- Recomendación posterior: devolver el audio sin tocar cuando no hay fotogramas
  completos; es más útil que abortar por un clip de 4 ms.

### Q-PAUDIO-04
- Archivo y líneas: `kokoro_onnx/__init__.py:148-152` (`_style_for`)
- Qué hace mal (comportamiento no evidente, no un fallo): con `length == 0`,
  `min(0, len(voice)) - 1 == -1` e indexa la **última** fila del estilo; el caso
  `tts/voices/style_for/0001` lo fija con `index: -1`.
- Cómo se reproduce en Rust: `kokoro::style_for` trata `0` como la última fila y
  devuelve ese `index` en la prueba; con `tokens.len() >= 1` nunca se usa.
- Recomendación posterior: documentar el `-1` o tratar `0` como error; afecta a
  cualquier llamada con un lote sin tokens.

### Q-PAUDIO-05
- Archivo y líneas: `src/ezagent/voice/turn.py:84-89` (`prepare`)
- Qué hace mal: una `rate` negativa produce `length = int(round(n * 16000 / rate))`
  negativo, pero `np.linspace(0, n - 1, max(1, length))` lo convierte en **una**
  muestra y `np.interp` la toma del primer valor; el resultado no es un
  remuestreo válido, sino un clip de 1 muestra (`turn/prepare/0102`).
- Cómo se reproduce en Rust: `turn::prepare` acepta `rate: i32` y
  `resample::resampled_len` devuelve 0 para longitudes negativas, que
  `resample_linear` eleva a 1; el mismo clip de una muestra.
- Recomendación posterior: validar `rate > 0` en `prepare` y documentar el rango
  aceptado; en la práctica la captura siempre usa tasas positivas.
