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

## Hallazgos de P-HUD-A (geometría, modelo y pintura de la barra)

### Q-PHUDA-01
- Archivo y líneas: `src/ezagent/voice/hud/drop.py:111-120` (`emerge_order`), `188-195` (`form_center`, `row_center_y`).
- Qué hace mal: `clean_dock` y `layout_size` toleran una disposición desconocida (caen a la píldora), pero `emerge_order`, `form_center`, `row_center_y` y `slots` indexan `_SIDES[(layout, dock)]` y lanzan `KeyError` con el mismo valor que acaban de normalizar; el oráculo fija `('wide', 'sides')` en 24 casos.
- Cómo se reproduce en Rust: la API pública de `model::drop::geometry` cae a la píldora (decisión documentada); la prueba de paridad devuelve el texto del `KeyError` para `layout == "wide"` en esas tres funciones.
- Recomendación posterior: decidir una sola política (o validar la disposición al entrar, o caer siempre al valor por defecto) y regenerar los 24 casos de `hud_drop_model/geometry/{emerge-order,form-center,row-center}/wide-*`.

### Q-PHUDA-02
- Archivo y líneas: `src/ezagent/voice/hud/anim.py:134-144` (`lerp_rgb`).
- Qué hace mal: con `k` fuera de `0..1` devuelve enteros fuera de `0..255` (p. ej. `-4` o `306`); el contrato de color de la UI asume `u8` y los llama solo con `0..1`, pero el oráculo comprueba los extremos.
- Cómo se reproduce en Rust: `anim::lerp_rgb` recorta a `0..255` para el dibujo; `anim::lerp_rgb_raw` reproduce los enteros sin recortar y es lo que usa la prueba de paridad.
- Recomendación posterior: documentar el rango de `k` en Python o recortar allí también; no afecta a ningún llamador actual.

### Q-PHUDA-03
- Archivo y líneas: `src/ezagent/voice/hud/chat_geometry.py:116` (`_coverage`) y `157` (`_shadow`) junto a `drop.py:332-359` (`_MASKS`).
- Qué hace mal: las máscaras se cachean por `round(escala·100)` / `round(radio, 3)`, así que el primer uso de una clave fija la máscara para escalas distintas que redondean igual; el mismo fotograma cambia hasta 1 px según el orden de llamadas previo. Los PNG dorados dependen de ese orden (los cuadros quietos se generaron antes que las transiciones).
- Cómo se reproduce en Rust: `paint::drop::fluid::form_mask` usa la misma clave; la prueba de `hud_bar` ordena los casos como `parity/gen/hud_bar.py` (quietos primero; después emerger, silenciar y llamar por tema) para poblar la caché en el mismo orden.
- Recomendación posterior: redondear la escala antes de construir la máscara (`scale = round(scale, 2)`) para que caché y geometría coincidan siempre; regenerar los PNG.

### Q-PHUDA-04
- Archivo y líneas: `src/ezagent/voice/hud/theme_accent.py:32-39` (`parse_hex`).
- Qué hace mal: `str(value or "")` convierte `None` en `""` y `123` en `"123"`, de modo que acepta entradas que no son cadenas sin avisar; el oráculo incluye ambos casos.
- Cómo se reproduce en Rust: `theme::parse_hex` recibe `&str`; la prueba aplica la misma coerción (`Value::Null` → `""`, número → su texto) antes de llamar.
- Recomendación posterior: tipar la entrada como `str` en la firma o rechazar no-cadenas explícitamente; no cambia ninguna salida actual.

## Hallazgos de P-HERDR (adaptador de Herdr, tareas y métricas)

### Q-PHERDR-01
- Archivo y líneas: `parity/gen/agent_driver.py:2495-2547` (`watcher/loop/*`) y `:2549-2583` (`watcher/deliver/*`).
- Qué hace mal: la acción de cada caso solo **lee** el estado (`{"delays": stop.delays, …}` y `{"seen": seen}`); nunca llama a `Watcher._loop` ni a `Watcher._deliver`. Los 5 casos de `loop` y los 3 de `deliver` del oráculo afirman listas vacías y no prueban nada.
- Cómo se reproduce en Rust: el arnés de `agent_driver` (pendiente en P-HERDR) debe devolver esas estructuras vacías sin ejecutar el bucle, o el generador debe llamar a la función y regenerarse. En esta ronda no se portó `agent_driver`.
- Recomendación posterior: cambiar la acción del generador por `watcher._loop()` / `watcher._deliver()` (con el `_StopAfter` y la cola ya preparados) y regenerar `parity/fixtures/agent_driver.jsonl`.

### Q-PHERDR-02
- Archivo y líneas: `src/ezagent/voice/herdr.py:440` (`return text[-limit:]`).
- Qué hace mal: con `limit=0`, `text[-0:]` es `text[0:]`, es decir, **no recorta**; el caso `herdr/read_agent/007-limit-zero` fija que se devuelve la pantalla completa. Es contraintuitivo pero está fijado por el oráculo.
- Cómo se reproduce en Rust: `ez_core::pyvalue::tail_chars` devuelve el texto completo cuando `count` es 0, igual que Python; no se corrigió.
- Recomendación posterior: si se quiere que `0` signifique «nada», documentarlo y regenerar el fixture; hoy `read_agent` nunca pasa 0 porque el valor por defecto es `max_read_lines`.

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
## Hallazgos de P-DRIVER (lanzamiento y sesión de agentes)

### Q-PDRIVER-01
- Archivo y líneas: `src/ezagent/voice/agent_prelaunch.py:134-137` (`discover_codex_models`).
- Qué hace mal: `for item in model.get("supported_reasoning_levels") or []` itera el valor tal cual. Si es una cadena, los «niveles» son sus caracteres (`"xy"` → `["x", "y"]`); si fuera un número lanzaría `TypeError` sin capturar (el `except` solo cubre `OSError`/`ValueError` de la lectura).
- Cómo se reproduce en Rust: `prelaunch::discover_codex_models` itera los caracteres de una cadena y deja vacío cualquier otro tipo que no sea lista; el caso `agent_driver/prelaunch/discover_codex_models/009-levels-not-list` lo fija.
- Recomendación posterior: exigir lista (`if not isinstance(levels, list): levels = []`) y regenerar el fixture.

### Q-PDRIVER-02
- Archivo y líneas: `src/ezagent/voice/watcher.py:118-127` (`Watcher._set_health`).
- Qué hace mal: la firma promete `bool` pero no valida nada; el generador llama `_set_health(None)` en `agent_driver/watcher/health/003-same` y el atributo `healthy` queda en `None` (y `on_health(None)` se invoca). Después, `self.healthy` ya no es booleano.
- Cómo se reproduce en Rust: `Watcher::set_health(Value)`/`set_health_value` guardan el valor tal cual (incluido `null`) y `healthy()` lo interpreta con `truthy`; el camino de producción sigue recibiendo `bool`.
- Recomendación posterior: coaccionar a `bool` en `_set_health` (`healthy = bool(healthy)`) o tipar el parámetro y regenerar el caso.

### Q-PDRIVER-03
- Archivo y líneas: `parity/gen/agent_driver.py:1476-1487` y `src/ezagent/voice/agent_session.py:374-391` (`finish_launch`).
- Qué hace mal: el doble de `_apply_in_session` devuelve la **misma** lista de avisos que `finish_launch` muta al agregar el aviso del envío; como `_capture` serializa el `input` después de ejecutar la acción, el oráculo guarda en `input.outcome.apply[1]` el aviso que en realidad añadió el envío (casos `finish_launch/006-task-warning` y `007-task-error`). Reejecutar el caso con ese `input` duplicaría el aviso.
- Cómo se reproduce en Rust: el arnés de `agent_driver` quita de `apply` el aviso final si coincide con el que producirá el envío, para partir del valor original; la prueba compara la salida del oráculo.
- Recomendación posterior: capturar el `input` antes de la acción o copiar `spec["apply"]` (tuplas y listas) en el generador; regenerar esos dos casos.

### Q-PDRIVER-04
- Archivo y líneas: `parity/gen/agent_driver.py:1610-1618` (`AgentDriver._text_list`).
- Qué hace mal: el caso `text_list/009` pasa una **tupla** `("a", "b")`; `json.dumps` la serializa como lista, así que el `input` del fixture no distingue una tupla de una lista y el Python devuelve `[]` (solo acepta `list`). No hay forma de reproducirlo solo con el JSON de entrada.
- Cómo se reproduce en Rust: el arnés detecta ese caso y devuelve `[]`; el resto de los casos usa `driver::text_list` sin cambios.
- Recomendación posterior: marcar el tipo en la entrada (p. ej. `{"tuple": [...]}`) o eliminar ese caso del generador; regenerar el fixture.
## Hallazgos de F1-12c (arnés de reproducción, escenarios 16-25)

### Q-F112C-01
- Archivo y líneas: `src/ezagent/swarm/incidents.py:166-175` (`describe`) y `178-188` (`question`).
- Qué hace mal: `when()` devuelve «las 7:10 p. m.» (con punto final), `describe` añade «. Su trabajo pendiente: …» y `question` vuelve a añadir el punto de la frase. El aviso de un incidente de límite de uso queda con doble punto: «… hasta las 7:10 p. m.. Su trabajo pendiente: T1.».
- Cómo se reproduce en Rust: el escenario 19 conserva el texto literal del Python en la locución y en la pregunta; la traza dorada fija el doble punto.
- Recomendación posterior: quitar el punto final de `when()` o unir las frases sin duplicarlo; regenerar el escenario 19 cuando se decida.

## Hallazgos de P-GFX-TEXT (texto del HUD)

### Q-PGFXTEXT-01
- Archivo y líneas: `src/ezagent/voice/hud/text.py:99` (`ImageFont.truetype`) y `pyproject.toml` (versión de Pillow).
- Qué hace mal (dependencia no declarada): la máscara depende del FreeType que empaquete la rueda de Pillow, que no está fijado por el proyecto. El `.venv` trae Pillow 13.0.0 con FreeType 2.14.3; `freetype-sys 0.23.0` (lo único que admite el workspace) compila FreeType 2.13.2. El intérprete de 2.13.2 desplaza y engorda los acentos compuestos: U+00B4 a 13 px sale `(3×2, top 10, [0,164,83] [48,202,2])` frente a `(3×2, top 9, [17,192,49] [10,15,0])` de Pillow, y `"Automático"` mide 10 px de alto en vez de 9.
- Cómo se reproduce en Rust: `ez-gfx::text` produce esas mismas máscaras de 2.13.2; `tests/text_mask_parity.rs` sondea U+00B4 para distinguir los dos rasterizadores. Con 2.14.3 las 181 máscaras de la tabla coinciden byte a byte.
- Recomendación posterior: fijar la versión de Pillow/FreeType con la que se generan los PNG dorados y actualizar `freetype-sys` a una versión con FreeType ≥ 2.14.0; mientras tanto, cualquier prueba de texto byte a byte queda atada a 2.13.2.

## Hallazgos de P-HUD-D (cuadros del menú)

### Q-PHUDD-01
- Archivo y líneas: `src/ezagent/voice/hud/menu.py:3801-3866` (`Menu._draw_scroll`).
- Qué hace mal (comportamiento no evidente): el degradado inferior y el pulgar del scroll se pintan sobre `buf` después de componer las bandas y no se recortan al alto animado (`_height.value`); usan `footer_top` del layout completo. Al cerrar el menú, entre ~100 y ~350 ms quedan visibles unas filas del degradado por debajo del borde retraído (p. ej. `anim-close-dark-100@200`: el cuerpo mide 460 px pero el PNG conserva píxeles hasta `y=537`).
- Cómo se reproduce en Rust: `paint::menu::advanced::scroll::draw_scroll` pinta igual, fuera de las bandas y sin recorte por altura; `hud_menu_parity` compara el PNG completo y lo fija.
- Recomendación posterior: recortar `_draw_scroll`/`draw_scroll` a `min(footer_top, H_ISLAND + altura)` si se quiere que el cierre no deje ese resto; regenerar los PNG dorados de `anim-close-*` al cambiarlo.
## Q-P-REPLAY-01
- Archivo y líneas: `src/ezagent/voice/app.py:1791-1808` (`_speak_same_turn`) y `:1761-1769` (`_new_tts_stop`), alcanzado desde `:1810-1829` (`_show_confirmation`).
- Qué hace mal: la pregunta de confirmación se dice con `_speak_same_turn` para no invalidar el turno, pero esa función llama a `_new_tts_stop`, que activa el `stop_event` del turno en curso. En cuanto el usuario responde, la herramienta se ejecuta y después `_answer_worker` ve el stop activo, cancela el turno y tira la respuesta final del LLM (`chat.cancel` con texto vacío). La operación sí ocurre; lo que no se oye ni se ve es el texto que el modelo había preparado para después de la confirmación.
- Cómo se reproduce en Rust: los escenarios `10a`, `10b`, `11`, `12`, `13a` y `13b` esperan `assistant: []` y `llm_requests` = una petición más que turnos visibles; el `tool.result` y la auditoría demuestran que la herramienta se ejecutó. No hay fixture de Rust para este flujo (es el arnés de reproducción).
- Recomendación posterior: separar el evento de parada del TTS de la pregunta del `stop_event` del turno (por ejemplo, un `stop` propio como el de `_speak_busy`) y, al terminar de responder, dejar que `_answer_worker` continúe con el siguiente turno del LLM. Si se corrige, hay que actualizar los seis escenarios de confirmación y sus aserciones `assistant`/`llm_requests`.
## Hallazgos de P-HUD-E (cuadros del chat: texto enriquecido)

### Q-PHUDE-01
- Archivo y líneas: `src/ezagent/voice/chat/render_base.py:171-210` (`draw_text_at`) sobre Pillow 13.0.0.
- Qué hace mal (dependencia): `ImageDraw.text` sobre un `RGBA` usa `bitmap_alpha_blend` de `Draw.c`, que **no** es «source-over» en alfa directo. Con el destino transparente copia el color de la tinta y solo escala la cobertura por el alfa de la tinta; con cualquier otro destino mezcla los cuatro canales como si el fondo estuviera premultiplicado: `(src*mask + dst*(255-mask) + 127) / 255`. Componer con `Image.alpha_composite` da hasta 28 niveles de diferencia sobre el fondo del chip de código.
- Cómo se reproduce en Rust: `paint::chat::base::blend_mask` aplica esa fórmula canal a canal (el caso «destino con alfa 0» copia la tinta), y `text_blend_matches_pillow` la fija contra los valores medidos.
- Recomendación: ninguna en el Python; es comportamiento de Pillow que el puerto debe copiar. Si algún día se cambia de motor de dibujo, revisar esta mezcla antes que nada.

### Q-PHUDE-02
- Archivo y líneas: `src/ezagent/voice/chat/render_base.py:196-204` (la cursiva) sobre Pillow 13.0.0.
- Qué hace mal (dependencia): `Image.transform(size, AFFINE, (1, 0.2, -0.2*ascent, 0, 1, 0), BICUBIC)` no usa el mismo filtro que `Image.resize`: el muestreo es en el **centro** del píxel de destino (`y + 0.5`) y el núcleo cúbico (`|t|³ - 2t² + 1` para `|t|<1`, `-t³ + 5t² - 8t + 4` para el resto) **no se renormaliza**, así que a medio píxel la cobertura suma 1,25 (159+159 de 255).
- Cómo se reproduce en Rust: `paint::chat::base::shear_rows`/`cubic_kernel` replican ese muestreo y truncan el acumulado; `the_italic_shear_matches_pillow_affine_bicubic` fija una máscara 10x3 contra la transformación real.
- Recomendación: ninguna en el Python; al portar cualquier otra `Image.transform` del HUD, no reutilizar el resampleo de `Image.resize`.

### Q-PHUDE-03
- Archivo y líneas: `src/ezagent/voice/chat/render_base.py:185` (`ImageDraw.text((px, py), ..., anchor="ls")`).
- Qué hace mal (dependencia): el ancla «ls» pega la máscara del **bbox del contorno** (no la tinta recortada), que puede llevar una fila o columna vacía delante (`*` y los acentos), y redondea primero el ancla (`ctx.px`) para luego desplazar por un número entero de píxeles físicos. Un puerto que ancle la tinta recortada se desvía 1 px en las palabras que empiezan por un glifo con «bearing» (`ñ`, `*`, `Abrir`) y al sumar el redondeo de la línea base.
- Cómo se reproduce en Rust: `paint::chat::base::{ink_bearing, ink_top}` recuperan el «bearing» y la primera fila de tinta comparando anchos/altos contra un glifo sonda (`T`, `Á`), y el ancla se redondea antes de aplicar el desplazamiento entero; `text_offset_matches_pillow_pen_and_baseline` fija los valores medidos.
- Recomendación: ninguna en el Python. Cuando `ez-gfx` exponga el bbox del contorno (`ink_left`/`ink_top` de `draw_text`), sustituir las sondas por esas métricas.

## Q-P-REPLAY-02
- Archivo y líneas: `parity/replay/harness_ext.py:257-270` (`_hud`).
- Qué hace mal: llama `callback()` sin argumentos para toda acción que no sea `chat_send`, `chat_switch` o `chat_delete`; `on_chat_mic(muted)` y `on_chat_confirm(yes)` exigen un booleano, así que `hud: {action: chat_mic}` o `hud: {action: chat_confirm}` fallan con `TypeError` en Python. Ningún escenario 16-25 usa esas dos acciones.
- Cómo se reproduce en Rust: `replay::steps_c::Hud` sí acepta `chat_mic` y `chat_confirm` con `muted`/`yes` del paso (y conserva el error `el HUD no expone on_<acción>` para lo demás). Es una desviación deliberada del arnés para poder probar el mute y la confirmación; no cambia ninguna traza dorada.
- Recomendación posterior: si se usan esas acciones, arreglar `_hud` en el Python pasando el argumento que pide cada callback (o marcar el paso como no soportado en ambos arneses).

## Q-P-REPLAY-03
- Archivo y líneas: `parity/replay/harness_ext.py:317-341` (`_drained`).
- Qué hace mal: el plazo solo se comprueba cuando la app ya está quieta pero no lleva 0,3 s estable; si nunca queda quieta, el bucle no vence y el escenario muere por el plazo global, no por el mensaje «la app no quedó quieta». En Rust `Driver::poll_drained` reproduce la misma rama (el mensaje sale solo en ese caso).
- Cómo se reproduce en Rust: `replay::driver::Driver::poll_drained`; los escenarios 16-25 terminan dentro del plazo global.
- Recomendación posterior: comprobar el plazo también cuando no está quieta, si se quiere un diagnóstico más preciso.

## Q-P-FIX-SIDEBAR-01 — Ajustes: la sección anterior se queda marcada al cambiar de sección
- **Archivo:** `src/ezagent/voice/hud/drop.py:2110-2111` (`_sidebar_hover` solo se actualiza al mover **con el botón pulsado**).
- **Efecto en Python:** tras pulsar otra sección, la anterior conserva el fondo de hover y se ven dos filas marcadas (comprobado píxel a píxel contra el `.venv`).
- **En Rust:** **desviación deliberada** pedida por el usuario: el hover de la barra lateral se refresca en cada movimiento (`card_input.rs::track_sidebar_hover`) y se limpia al abrir la tarjeta. Solo la sección actual queda marcada. No revertir por paridad.

## Q-P-IMP-NAMES-01 — Los nombres dictados por voz llegan a Herdr sin comparar con lo abierto
- **Archivo:** `src/ezagent/agent/tools_herdr.py:794-883` (`_handle_delegate` pasa `address`, `space`, `agent` e `input_from` tal cual llegan del modelo) y `src/ezagent/voice/herdr.py:290-316` (`list_agents` nunca se usa para comparar nombres).
- **Efecto en Python:** un nombre mal transcrito se entrega a Herdr y este responde `no space "TES"`; la herramienta devuelve ese error sin los nombres disponibles y el asistente tiene que preguntar a ciegas. Caso real del log: el usuario dijo el espacio «TEST» y Whisper transcribió «TES», «TESS» y «TETS»; también «OpenCobe» por «OpenCode» y «Deedseed» por «DeepSeek».
- **En Rust:** **desviación deliberada** pedida por el usuario. `ez_herdr::herdr::names` normaliza (minúsculas, sin tildes ni signos, espacios colapsados), calcula una clave fonética pensada para nombres ingleses dichos en español (dobles letras, b/v, c/k/q, s/z, d final → t, «ss» → «s») y puntúa con Damerau-Levenshtein, prefijo/contenido y esa clave; los umbrales son los de `names.rs` y los fijan sus pruebas. `ez_agent::tools_herdr::names::Names::pick` usa la coincidencia exacta sin decir nada, acepta la muy probable y lo cuenta en el resultado («usé el espacio «TEST»»), pregunta la dudosa con el `ConfirmationManager` ya existente («¿Te refieres al espacio «TEST»?») y, con «no» o sin respuesta, devuelve un error que lista los nombres vivos. `spaces`, `terminals` y `agents` se resuelven en todas las acciones de Herdr (`get`, `read`, `send`, `run`, `open_terminal`, `focus`, `kill`, `watch`, `configure`, `keys`, `guide` y `input_from`). El lanzamiento previo de cadenas (`app/activity/tasks.rs`) solo usa coincidencias seguras y, con duda, no abre nada: lo resolverá la herramienta.
- **Recomendación posterior:** si el Python sigue vivo, portar allí el mismo resolvedor para que las dos versiones pregunten igual; los umbrales conviene reajustarlos con más transcripciones reales. El texto del error de agente desconocido de `run`/`guide` cambia de «Agente desconocido: …» a «No encontré el agente «…». Agentes disponibles: …» (es la misma mejora: lista para preguntar).
