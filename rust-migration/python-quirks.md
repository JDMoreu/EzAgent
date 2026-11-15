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
