# DX-2 r1 — Acuses cortos («ok», «listo», «bueno»): diagnóstico y propuesta

Rama: `work/DX-2` (copia de `eco/main`). Trabajo de **investigación**: no se cambia código del
producto. Evidencia de código con `archivo:línea`, mediciones del registro real y una sonda
propia ejecutada en esta rama.

## 1. Cómo funciona hoy y por qué «ok» sale larga o repetida

### 1.1 Camino de una transcripción

`event_transcribed` (`rust/crates/ez-app/src/app/events/branches.rs:49-86`) decide en este orden:

1. si hay confirmación pendiente → `answer_confirmation` (`branches.rs:59-62`);
2. `self.fast_command(&text)` (`branches.rs:69-79`): si devuelve `handled=true`, termina el turno
   sin LLM y registra `command` / `fast_command` en latencia;
3. si no → `THINKING` + `answer_worker` (`branches.rs:80-85`).

`answer_worker` (`rust/crates/ez-app/src/app/workers.rs:374-414`) crea `AnswerRun`, y
`AnswerRun::stream` llama a `messenger.ask_stream(text, AskMode::Voice)`
(`workers.rs:164-175`). El modo voz **no añade ninguna instrucción propia**: solo el modo chat
añade `CHAT_MODE_PROMPT` (`ez-llm/src/messenger/tool_loop/turn.rs:191-195`). Es decir, el único
texto que pide brevedad es el del prompt de sistema.

### 1.2 El camino rápido existente no cubre el acuse

`fast_command` (`rust/crates/ez-app/src/fast.rs:233-258`) comprueba, en orden: agradecimiento
(«gracias» → `De nada.`, `fast.rs:237-240`), saludo, listar agentes, listar tareas y, por último,
`control_command` (`fast.rs:261-306`: mute/unmute/cerrar/repetir).

`COURTESY_WORDS` (`fast.rs:21-24`) ya contiene `ok, okay, oki, bueno, listo, perfecto, dale,
gracias, … va`, pero **solo se usan para quitar cortesías delante de un atajo**
(`strip_courtesy`, `fast.rs:191-206`). Si no queda nada, `strip_courtesy` devuelve el texto original
(`fast.rs:201-203`), así que `ok` sigue siendo `ok` y no empareja ninguna orden:
`control_command` devuelve `None` y `fast_command` devuelve `FastOutcome::default()`
(`handled=false`). Un `ok` suelto **va al LLM**. La prueba existente
`fast/tests.rs:194-208` confirma(el patrón «lo que no es atajo sigue al LLM») pero no incluye
`ok` entre los casos.

### 1.3 Qué ve el LLM justo después de un aviso

El aviso se produce así:

- `handle_done_answer` / `handle_done_prompt` (`watch.rs:330-371`) llaman a
  `note_done(...)` (`watch.rs:373-380`) y después `queue_announcement(...)`.
- `queue_announcement` / `pump_announcements` (`states.rs:251-283`) hablan el texto con
  `speak(text, "announce")`. El texto del aviso **no se guarda en el chat**; `speak`
  (`states.rs:195-202`) solo adelanta el turno y lanza el TTS.
- Sí se guarda, en cambio, el contenido del agente: `Messenger::note_done`
  (`ez-llm/src/messenger/notices.rs:17-37`) añade como mensaje **`role: "user"`** un bloque de
  hasta **12 000 caracteres** de salida del agente, con el texto
  «No des el resumen hasta que lo pida» (`notices.rs:23-31`).

Al llegar «ok», `drive_turn` (`ez-llm/src/messenger/tool_loop/turn.rs:200-233`) hace
`begin_turn_history` (inyecta notas pendientes, `turn.rs:107-118`) y `normalize_history`
(`turn.rs:147-161`), que **apila «ok» como otro mensaje `user`** detrás de ese bloque. O sea: el
modelo recibe, en mensajes de usuario consecutivos, la salida larga del agente más «ok», sin
ninguna regla que diga qué hacer con un acuse.

### 1.4 Por qué se repite o se alarga

- El prompt de sistema pide brevedad genérica («una o dos frases»,
  `ez-agent/src/text/default_system_md.txt:5`) y «no des el resumen hasta que lo pida»
  (`default_system_md.txt:26`; también `execution_hint.txt:4`: «No confundas un acuse con la
  entrega»). Ante «ok» —ambiguo— el modelo interpreta que debe confirmar *qué* terminó y lo hace
  repitiendo el aviso o su resumen.
- `voice_flow_hint.txt` (inyectado siempre, `system_prompt.rs:202-207`) **solo** prohíbe
  muletillas de espera (`voice_flow_hint.txt:1-2`); no hay ninguna regla de acuses.
- El único filtro local de voz es `is_filler_sentence` (`ez-app/src/app/core.rs:184-210`), que
  calla la primera frase si es una muletilla de espera («un momento», «a ver»). No detecta
  repeticiones ni acuses, así que una respuesta larga se dice entera.
- Además, `done_question` (`watch.rs:393-420`) termina los avisos con una pregunta
  («¿Quieres que te resuma…?»), lo que refuerza que el modelo trate «ok» como una respuesta que
  hay que gestionar, no como un acuse.

### 1.5 Medición del coste actual (registro real)

Fuente: `%APPDATA%\EzAgent\logs\latency.jsonl` (451 filas: 433 `turn`, 18 `command`; modelos
`opencode` / `deepseek-v4.1-flash`). Solo se leyeron tiempos y nombres.

| Métrica | n | mediana | media | min | max |
|---|---|---|---|---|---|
| `ttfa_ms` de todos los turnos | 285 | 4437 | 6053 | 1547 | 44297 |
| `ttfa_ms` de turnos de 1 ronda | 52 | 4624 | 4485 | — | — |
| `llm_ms` de turnos de 1 ronda | 52 | 3458 | 3925 | — | — |
| `llm_ttft_ms` de turnos de 1 ronda | 52 | 2929 | 3373 | — | — |
| `tts_synth_ms` de turnos de 1 ronda | 52 | 1755 | 2440 | — | — |
| `turn_total_ms` de turnos de 1 ronda | 52 | 11112 | 13625 | — | — |
| `handled_ms` de `fast_command` | 18 | 0.0 | ~1.8 | 0.0 | 16.0 |

Conclusión: un turno corto (una sola ronda LLM) tarda **~4.6 s en sonido** y **~11 s en total**;
el acuse determinista cuesta **0–16 ms** antes del TTS. `audit.jsonl` (301 filas) solo registra
llamadas a herramientas (`delegate_to_agent` 216, `voice_control` 20, …) y no aporta al acuse.

## 2. Propuesta concreta

### 2.1 Camino rápido determinista para acuses

Punto de inserción: al **final** de `fast_command` (`fast.rs:233-258`), después de los atajos
existentes, para que «ok, mutea» siga silenciando. Código propuesto (en `fast.rs`, junto a las
demás constantes):

```rust
// DECISION: ventana configurable; 10 s cubre la pausa normal tras un aviso sin
// capturar un «ok» dicho mucho después como inicio de frase nueva.
const ACK_TOKENS: [&str; 15] = [
    "ok", "okey", "oki", "oc", "k", "va", "vale", "bueno", "listo", "perfecto",
    "dale", "genial", "gracias", "muchas", "mil",
];
const ACK_EXACT: [&str; 6] = [
    "de acuerdo", "ya esta", "muy bien", "esta bien", "entendido", "confirmado",
];
const ACK_WINDOW_SECS: f64 = 10.0; // valor por defecto de `ack_fast_window_secs`
/// Respuestas cortas y variadas, generadas por código (nunca por el LLM).
const ACK_REPLIES: [&str; 6] = [
    "Listo, quedo atento.",
    "Me avisas si necesitas algo.",
    "Entendido, aquí sigo.",
    "Anotado.",
    "Perfecto, no hay cambio.",
    "Ya, gracias.",
];

/// `true` solo si todo el texto es un acuse, sin contenido nuevo.
fn is_pure_acknowledgement(raw_key: &str) -> bool {
    if ACK_EXACT.contains(&raw_key) {
        return true;
    }
    let words: Vec<&str> = py_split(raw_key).collect();
    !words.is_empty()
        && words.len() <= 3
        && words.iter().all(|word| ACK_TOKENS.contains(word))
}
```

Extensión del trait `FastHost` (`fast.rs:135-169`), con valores por defecto para no tocar los
fakes de prueba:

```rust
/// `None` si no hay aviso/respuesta reciente o si el texto no es de voz.
fn ack_context(&self) -> Option<(f64, Option<String>)> { None }
/// Índice monótonico para rotar las respuestas (evita repetir la misma).
fn ack_index(&self) -> usize { 0 }
```

Bloque final dentro de `fast_command`:

```rust
// DECISION: se comprueba al final para que los atajos reales ganen siempre.
if let Some((seconds_since, kind)) = host.ack_context() {
    let recent = seconds_since >= 0.0 && seconds_since <= ack_window_secs(host);
    let after_outbound = matches!(kind.as_deref(), Some("announce") | Some("answer"));
    if recent && after_outbound && is_pure_acknowledgement(&raw_key) {
        info!("Atajo local: acuse corto.");
        return answer(ACK_REPLIES[host.ack_index() % ACK_REPLIES.len()]);
    }
}
control_command(&key, host).unwrap_or_default()
```

donde `ack_window_secs(host)` lee `cfg_float(&cfg, "ack_fast_window_secs", 10.0)` ( patrón ya
usado en `core/callbacks.rs:99-107` y `core/parts.rs:36-55`). No se añade dependencia.

**Estado en la app** (tres campos en `SpeechState`, `core/state.rs:220-234`, todos bajo los
cerrojos que ya usa el tipo):

```rust
pub last_outbound_kind: RwLock<Option<String>>,   // "announce" | "answer"
pub last_outbound_done_at: RwLock<f64>,           // monotonic() al terminar de hablar
pub ack_index: AtomicUsize,                       // fetch_add(1) al elegir respuesta
```

Se escriben al terminar la frase: en `event_speak_done` (`branches.rs:138-164`, donde ya está
`kind_before`) y en `event_answer_done` (`branches.rs:110-136`, donde ya se limpia
`speaking_kind`). `ack_context()` en `AppFastHost` (`settings_apply.rs:123-172`) devuelve
`(monotonic() - last_outbound_done_at, kind)` y borra el tipo después de consumirlo, para que un
segundo «ok» seguido **no** vuelva a activar el atajo. La rotación usa `ack_index.fetch_add(1)`
bajo el `AtomicUsize`, cumpliendo la regla de estado compartido de AGENTS.md.

**Cuándo NO aplica** (cae al LLM, como hoy):

- La frase trae algo más: «ok, pero cambia X», «listo, haz esto», «gracias por avisar, ¿y ahora?»
  → más de 3 palabras o alguna palabra fuera de `ACK_TOKENS`/`ACK_EXACT`.
- No hubo aviso/respuesta hablada en los últimos 10 s, o la última salida fue `confirm`,
  `busy` o un error (`speaking_kind` distinto de `announce`/`answer`).
- Hay una confirmación pendiente: se resuelve antes en `event_transcribed` (`branches.rs:59-62`).
- Un «sí» suelto: **no** está en la lista (puede ser la respuesta a la pregunta de
  `done_question`, `watch.rs:393-420`) → sigue al LLM. Igualmente, «repite», «mutea», etc., los
  gana `control_command`.
- Segundo acuse consecutivo sin nueva salida: el consumo de `last_outbound_kind` lo impide.

### 2.2 Regla en el prompt de voz

Añadir al final de `voice_flow_hint.txt` (archivo fijo inyectado por código en
`system_prompt.rs:202-207`; así alcanza también a `system.md` ya existentes, igual que
`EXECUTION_HINT`, `system_prompt.rs:31-33`):

```text
# Acuses cortos
Cuando el usuario solo acuse recibo («ok», «okey», «oc», «va», «listo», «bueno», «dale», «gracias») justo después de un aviso o de tu respuesta, contesta en una frase breve y distinta: «Listo, quedo atento.», «Me avisas si necesitas algo.», «Entendido.». No repitas el aviso, su resumen ni la tarea; no expliques lo que ya dijiste. Si el acuse trae algo más («ok, pero cambia X»), ignora la cortesía y responde solo a lo que pide.
```

(Opcional, para instalaciones nuevas: reflejar la misma frase bajo «# Estilo de voz» en
`default_system_md.txt:4-7`. No es necesario para que la regla se aplique.)

### 2.3 Cómo se mide y pruebas que lo cubren

**Medición antes/después**, con el mismo `latency.jsonl`:

- Antes (ya medido, §1.5): turnos de 1 ronda → `ttfa_ms` mediana 4624, `llm_ms` 3458,
  `turn_total_ms` 11112; `fast_command` → 0–16 ms.
- Después: registrar el acuse con `record_command("ack_fast", handled_ms)` en
  `event_transcribed` (`branches.rs:69-78`, mismo punto que `fast_command`) y comparar:
  1. `handled_ms` del acuse (objetivo: < 1 ms, sin llamada LLM);
  2. `ttfa_ms` del turno de acuse (objetivo: < 2000 ms; solo STT + síntesis de 2–6 palabras,
     frente a 4624 ms);
  3. nº de rondas LLM y longitud de la respuesta (objetivo: 0 rondas, una frase).
  La medición se repite con el mismo provider/modelo para que la comparación sea válida.

**Pruebas** (no incluidas en este informe porque no se cambia código):

1. `fast/tests.rs`: tabla con `ok, okey, oki, oc, k, va, vale, bueno, listo, perfecto, dale,
   genial, gracias, ok gracias, de acuerdo, ya está` → `handled=true` y speech ∈ `ACK_REPLIES`;
   y `ok, pero cambia X`, `listo, haz esto`, `hola mundo`, `repite`, `mutea` → `handled=false`
   o el atajo correspondiente (prioridad de `control_command`).
2. `fast/tests.rs`: `ack_context()` con `None`, con `kind="confirm"`, con `seconds_since=11`
   → `handled=false`; con `kind="announce"` y `seconds_since=1` → `handled=true`.
3. `fast/tests.rs`: 8 acuses seguidos rotan las 6 respuestas sin repetir la inmediatamente
   anterior (`ack_index`).
4. `events/tests.rs` (estilo de `transcribed_starts_the_answer_worker_for_the_current_turn`,
   `events/tests.rs:137-150` y `announcement_reopens_the_call_and_speaks`,
   `states/tests.rs:127-141`): aviso → `SpeakDone` → `Transcribed("ok")`; afirmar que
   `runtime.workers` no crece con `ezagent-answer`, que se registra `ack_fast` y que el speaker
   recibe una sola frase corta.
5. `system_prompt_parity.rs` (`ez-agent/tests/`): el prompt construido contiene el bloque
   «# Acuses cortos».

## 3. Riesgos y cómo se evitan

| Riesgo | Mitigación |
|---|---|
| Cortar una orden real | `is_pure_acknowledgement` exige que **todas** las palabras sean acuses (≤3); «ok, pero cambia X» va al LLM. El bloque se coloca al final de `fast_command`, así que mute/cerrar/repetir/listar ganan. |
| Responder «sí» a una pregunta del aviso | `done_question` (`watch.rs:393-420`) acaba en pregunta; «sí»/**no** están fuera de la lista → el LLM decide. El acuse nunca ejecuta una acción, solo habla. |
| Sonar robótico | Seis respuestas rotadas por `ack_index`, en español neutro y con longitud similar; se descartan las muletillas existentes (`is_filler_sentence`). |
| Capturar un «ok» mucho después | Ventana de 10 s configurable (`ack_fast_window_secs`) y consumo de `last_outbound_kind`. |
| Segundo acuse seguido | El consume de `last_outbound_kind` fuerza al segundo «ok» al LLM. |
| Romper pruebas de paridad Python↔Rust | El cambio es nuevo en Rust; las pruebas de paridad existentes no cubren acuses (confirmado en `fast/tests.rs:194-208`). Hay que añadir pruebas nuevas, no relajar las viejas. |
| Hilo de UI / estado compartido | Solo `AtomicUsize` y `RwLock` ya presentes en `SpeechState`; sin bloqueo de red ni `subprocess` en el camino rápido. |

## 4. Decisiones

- `# DECISION: ventana configurable; 10 s cubre la pausa normal tras un aviso sin capturar un
  «ok» dicho mucho después como inicio de frase nueva.` (config `ack_fast_window_secs`).
- `# DECISION: se comprueba al final para que los atajos reales ganen siempre.`
- `# DECISION: no se incluye «sí» en los acuses porque puede responder a la pregunta del aviso.`
- `# DECISION: la regla va en voice_flow_hint.txt (código) para alcanzar system.md ya existentes.`

Estado: completado
