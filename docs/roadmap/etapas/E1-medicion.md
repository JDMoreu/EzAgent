# E1 · Medición: banco de latencia y set dorado de intenciones

| Campo | Valor |
|---|---|
| Depende de | E0 |
| Agente sugerido | DeepSeek V4.1 Flash |
| Esfuerzo | M (3–5 días) |
| Rama | `stage/E1-medicion` |
| Hallazgos | L12, sección 7 de la auditoría; prerrequisito de todas las puertas |

## Objetivo
Poder medir, de forma reproducible y con un solo comando, (a) la latencia
de cada fase del turno con audio real grabado y (b) la precisión de
cualquier modelo LLM eligiendo herramienta y argumentos.

## Por qué
Hoy no hay ninguna cifra medida (`docs/voice-latency.md:53-55`). Sin línea
base, E2 y E3 no pueden demostrar mejora ni detectar regresiones, y la
elección de modelo (DeepSeek vs MiniMax) es una opinión.

## Dentro del alcance
1. Banco de latencia offline que usa los componentes reales.
2. Set dorado de intenciones y runner multi-proveedor.
3. Medición de los comandos rápidos en la app.
4. Línea base registrada en `docs/roadmap/metricas.md`.

## Fuera del alcance
- No optimizar nada (ni VAD, ni TTS, ni prompt).
- No cambiar herramientas ni prompt: el set dorado de esta etapa evalúa **el
  sistema actual** (14 herramientas) y además define los casos con la
  expectativa de las herramientas nuevas de E3 (ver «Doble expectativa»).
- No ejecutar evals en CI automática.

## Archivos que se tocan
| Archivo | Acción | Notas |
|---|---|---|
| `bench/__init__.py`, `bench/latency/__init__.py` | crear | Paquete |
| `bench/latency/__main__.py` | crear | CLI (ver contrato) |
| `bench/latency/runner.py` | crear | Lógica del banco (≤ 400 l.) |
| `bench/latency/fixtures/README.md` | crear | Cómo grabar las 20 frases (16 kHz mono WAV) y `phrases.json` |
| `bench/latency/fixtures/phrases.json` | crear | Lista de 20 frases con id, texto esperado, grupo (`pausa` / `comando` / `charla`) |
| `evals/__init__.py`, `evals/intents/__init__.py` | crear | Paquete |
| `evals/intents/__main__.py` | crear | CLI |
| `evals/intents/runner.py` | crear | Llamadas al proveedor + puntuación (≤ 400 l.) |
| `evals/intents/cases.jsonl` | crear | ≈ 120 casos (composición en `04-estrategia-llm.md` §8) |
| `evals/intents/tools_v2.json` | crear | Esquemas de E3 (copiados de `04-estrategia-llm.md` §3) |
| `src/ezagent/voice/app.py` | modificar | Solo: medir `_fast_command` con `TurnTimings` y escribir `logs/latency.jsonl` |
| `src/ezagent/voice/metrics.py` | modificar | `to_json()` y escritura JSONL |
| `pyproject.toml` | modificar | Incluir `bench` y `evals` en el paquete de pruebas (no en el wheel) |
| `tests/test_bench.py`, `tests/test_evals.py` | crear | Pruebas con LLM y STT falsos |
| `docs/roadmap/metricas.md` | modificar | Línea base |
| `.gitignore` | modificar | `bench/latency/fixtures/*.wav`, `bench/out/`, `evals/out/` |

## Archivos prohibidos
Todo `src/ezagent/agent/`, `src/ezagent/voice/notch/`, `voice/llm.py`
(se usa, no se modifica).

## Interfaces y contratos

### Banco de latencia
```
uv run python -m bench.latency --wav-dir bench/latency/fixtures \
    [--llm real|fake] [--stt real|fake] [--tts real|null] [--runs 3] \
    [--out bench/out/latency-<fecha>.json]
```
Funcionamiento:
1. Para cada WAV: lo trocea en chunks de 30 ms (`CHUNK_SECS`), añade 1,5 s de
   silencio (ruido de fondo grabado o ceros con ruido gaussiano de RMS 0,0004)
   y alimenta `Segmenter.feed` **a tiempo real** (`time.sleep` entre chunks)
   con los parámetros de `VoiceConfig` cargados de `voice.json`.
2. Marca `speech_end` con `Segmenter.last_speech_end` y `vad_end` al devolver
   el utterance.
3. STT real: `voice.stt.transcribe(cfg, pcm_to_wav(...))` → `stt_ready`.
4. LLM real: `Messenger(...).ask_stream(texto)` con un `AgentRuntime` real y
   callbacks nulos (sin Herdr: usa un `Herdr` falso que devuelve listas
   vacías). Marca `first_token` en el primer delta y `first_sentence` con la
   misma clase `_Sentences` de `voice/app.py` (importarla, no copiarla).
5. TTS real: `Speaker.synthesize(primera_frase)` → `first_sound` al terminar
   la síntesis + latencia de apertura de stream medida una vez (se suma).
   `--tts null` omite la síntesis.
6. Comparación de transcripción con `phrases.json` por WER simple (palabras).

Salida JSON:
```json
{"meta":{"commit":"…","date":"…","llm":"opencode/deepseek-v4.1-flash","stt_model":"…","runs":3},
 "turns":[{"id":"p01","run":1,"vad_wait_ms":0,"stt_ready_ms":0,"first_token_ms":0,"first_sentence_ms":0,"first_sound_ms":0,"ttfa_ms":0,"wer":0.0,"cut":false}],
 "summary":{"ttfa_ms":{"p50":0,"p95":0},"vad_wait_ms":{"p50":0,"p95":0},"stt_ready_ms":{"p50":0,"p95":0},"first_token_ms":{"p50":0,"p95":0},"cuts":0}}
```
`cut=true` si el Segmenter cierra el turno antes del final del WAV (pausa a
mitad de frase) — es la métrica de «turnos cortados».

### Set dorado
```
uv run python -m evals.intents --provider opencode --model deepseek-v4.1-flash \
    [--tools current|v2] [--lang es|en|all] [--limit N] [--out evals/out/<…>.md]
```
- `--tools current`: usa `AgentRuntime.tools_for(text)` y el system prompt
  actual; mapea las expectativas v2 a las actuales con la tabla de abajo.
- `--tools v2`: usa `tools_v2.json` y el prompt de `04-estrategia-llm.md` §4
  (texto literal guardado en `evals/intents/prompt_v2.txt`).
- Una sola llamada por caso, `stream=True` para medir TTFT, sin ejecutar
  herramientas.
- Puntuación por caso: `tool_ok`, `args_ok` (comparación normalizada:
  minúsculas, sin acentos; alias resueltos con un mapa fijo en el runner),
  `no_tool_ok` para charla, `ttft_ms`.
- Informe Markdown: tabla global, por grupo y por idioma; lista de fallos con
  texto, esperado y obtenido; coste estimado con tokens de `usage`.

**Doble expectativa (mapeo v2 → actual)**

| v2 | Actual |
|---|---|
| `run_agent` | `delegate_to_agent` con `action=run` |
| `agent_status` | `delegate_to_agent` con `action` ∈ {list, get, read} |
| `send_to_agent` | `delegate_to_agent` con `action=send` |
| `stop_agent` | `delegate_to_agent` con `action=kill` |
| `voice_control(hang_up/mute/…)` | `voice_control` con `close`/`mic_mute`/`repeat_last` |
| `remember` | `remember` |

## Pasos
1. `metrics.py`: `to_json()` + escritura JSONL; medir `_fast_command`. Prueba.
2. Banco con `--stt fake --llm fake --tts null` funcionando sobre un WAV
   sintético generado en la prueba (seno + silencio). Prueba.
3. Modos reales de STT, LLM y TTS. Ejecución manual.
4. `phrases.json` + README de fixtures (el usuario graba los WAV; el agente
   **no** inventa audio).
5. `cases.jsonl` (≈ 120 casos) + `tools_v2.json` + `prompt_v2.txt`.
6. Runner de evals con proveedor falso (prueba) y real (manual).
7. Ejecutar línea base: banco (20 frases × 3) y evals `current` y `v2` con
   DeepSeek V4.1 Flash y MiniMax M3.1 Flash; rellenar `metricas.md`.

## Criterios de aceptación
- [ ] `uv run python -m bench.latency --stt fake --llm fake --tts null` funciona
      sin red ni GPU y produce el JSON del contrato.
- [ ] Con los 20 WAV grabados, el modo real produce P50/P95 de cada fase.
- [ ] `uv run python -m evals.intents --provider fake` puntúa 100 % con el
      proveedor falso que devuelve las expectativas.
- [ ] `cases.jsonl` tiene ≥ 120 casos válidos (una prueba valida el esquema
      de cada línea) con la composición de `04-estrategia-llm.md` §8.
- [ ] `metricas.md` tiene la línea base de latencia y de evals para al menos
      un modelo real.
- [ ] Los comandos rápidos aparecen en `logs/latency.jsonl`.

## Pruebas
- `tests/test_bench.py`: WAV sintético → JSON con las claves del contrato;
  `cut` detectado con una pausa larga a mitad.
- `tests/test_evals.py`: validación del esquema de casos, puntuación con
  proveedor falso, mapeo v2 → actual.

## Comandos de verificación
```powershell
uv run pytest -q tests/test_bench.py tests/test_evals.py
uv run python -m bench.latency --stt fake --llm fake --tts null
uv run python -m evals.intents --provider fake --limit 10
```

## Riesgos y rollback
- `sleep` a tiempo real hace el banco lento (20 frases × 3 ≈ 4 min) → es
  aceptable; `--fast` puede desactivarlo para depurar, sin validez de medida.
- Coste de evals reales: ≈ 120 llamadas cortas por modelo; documentarlo en el
  informe.

## Definición de hecho
Criterios cumplidos, línea base en `metricas.md`, informe de `AGENTS.md` §6.

## Prompt para el agente
```
Lee AGENTS.md y docs/roadmap/etapas/E1-medicion.md completos, y la sección 8
de docs/roadmap/04-estrategia-llm.md.
Ejecuta E1 en el orden de «Pasos», un commit por paso. No modifiques
src/ezagent/agent/ ni la UI. No inventes archivos de audio: el usuario los
grabará siguiendo bench/latency/fixtures/README.md.
Al terminar, ejecuta los comandos de verificación y entrega el informe de
AGENTS.md §6.
```
