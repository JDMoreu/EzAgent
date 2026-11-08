# E2 · Pipeline de voz fluido

| Campo | Valor |
|---|---|
| Depende de | E1 (banco y línea base) |
| Agente sugerido | MiniMax M3.1 Flash (más contexto); tareas T1, T2 y T6 aptas para DeepSeek |
| Esfuerzo | L (1–2 semanas) |
| Rama | `stage/E2-pipeline-voz` |
| Hallazgos | L1, L2, L3, L4, L9, L10, L11, D3 |
| Paralelo con | E3 (solo coinciden en `voice/app.py`, en zonas distintas) |

## Objetivo
Bajar el tiempo de fin de voz → primer sonido a P50 ≤ 900 ms / P95 ≤ 1 500 ms
sin cortar turnos, con reproducción continua (sin huecos entre frases) y
barge-in fiable.

## Por qué
El turno espera 1,3 s fijos de silencio, es estrictamente serie y la
reproducción bloquea la lectura del LLM y la síntesis de la siguiente frase.
Son los mayores costes de latencia medibles (ver `00-auditoria.md` §2).

## Dentro del alcance
- T1 Reproductor persistente con cola (`voice/tts_player.py`).
- T2 Motor de síntesis en hilo propio con cola de frases (`voice/tts_engine.py`).
- T3 Desacoplar el answer worker: el LLM se lee sin esperar la reproducción.
- T4 VAD Silero + endpoint con extensión semántica.
- T5 STT especulativo + `httpx.Client` persistente.
- T6 Earcons (`voice/earcons.py`).
- T7 Barge-in por VAD y cancelación del stream LLM.
- T8 Calentamiento TLS del LLM y mover la consulta Herdr fuera del hilo de Tk.

## Fuera del alcance
- No tocar herramientas, prompt ni `agent/` (es E3).
- No mover a la estructura `core/` (es E5): los módulos nuevos van en
  `src/ezagent/voice/` con los nombres indicados; E5 los reubica.
- No cancelación de eco acústica (AEC) en esta etapa; se asume auriculares y
  se documenta.
- No cambiar el modelo de whisper ni Kokoro.

## Archivos que se tocan
| Archivo | Acción | Tarea |
|---|---|---|
| `src/ezagent/voice/tts_player.py` | crear | T1 |
| `src/ezagent/voice/tts_engine.py` | crear | T2 |
| `src/ezagent/voice/tts.py` | modificar | T1, T2: `Speaker` delega en engine + player; `normalize_for_kokoro` sin `asyncio.run` por frase (un loop persistente o versión síncrona) |
| `src/ezagent/voice/app.py` | modificar | T3, T5, T7, T8: solo `_on_utterance`, `_stt_worker`, `_answer_worker`, `_speak*`, barge-in (≈ l. 1084-1380) y `_fast_command` (l. 1525) |
| `src/ezagent/voice/vad.py` | crear | T4 |
| `src/ezagent/voice/endpoint.py` | crear | T4 |
| `src/ezagent/voice/audio.py` | modificar | T4: `Segmenter` acepta un `VadModel` opcional |
| `src/ezagent/voice/stt.py` | modificar | T5 |
| `src/ezagent/voice/earcons.py` | crear | T6 |
| `src/ezagent/voice/llm.py` | modificar | T7, T8: `cancel()` del stream activo y `warm_up()` |
| `src/ezagent/voice/config.py` | modificar | Nuevos campos (ver contrato) con migración |
| `pyproject.toml` | — | sin dependencias nuevas (`onnxruntime` ya llega con `kokoro-onnx`) |
| `tests/test_tts_pipeline.py`, `tests/test_vad_endpoint.py`, `tests/test_earcons.py`, `tests/test_stt_speculative.py` | crear | |

## Archivos prohibidos
`src/ezagent/agent/**`, `src/ezagent/voice/notch/**` (salvo llamar a la API
pública existente), `src/ezagent/voice/herdr.py`.

## Interfaces y contratos

```python
# voice/tts_player.py
class Player:
    """Un único stream de salida PyAudio abierto mientras dura la llamada."""
    def __init__(self, rate: int, device: int | None, block_frames: int = 480,
                 on_level: Callable[[float], None] | None = None) -> None: ...
    def start(self) -> None: ...                      # abre stream + hilo
    def enqueue(self, pcm: np.ndarray, turn_id: int) -> None: ...
    def stop_turn(self, turn_id: int) -> None: ...     # vacía la cola de ese turno y corta en ≤ 20 ms
    def wait_idle(self, timeout: float | None = None) -> bool: ...
    def close(self) -> None: ...
    @property
    def playing(self) -> bool: ...
    # Evento on_first_block(turn_id) para la marca first_sound.
```

```python
# voice/tts_engine.py
class SynthEngine:
    """Hilo de síntesis: texto → PCM, en orden, por turno."""
    def __init__(self, speaker_model: "Kokoro", player: Player, cfg: VoiceConfig) -> None: ...
    def submit(self, text: str, turn_id: int) -> None: ...  # no bloquea
    def cancel_turn(self, turn_id: int) -> None: ...
    def flush_turn(self, turn_id: int) -> None: ...         # marca fin de texto del turno
    def done_event(self, turn_id: int) -> threading.Event: ...  # se activa al terminar de sonar el turno
```
Regla: mientras suena la frase N se sintetiza la N+1; la cola de entrada está
acotada a 4 frases (backpressure al lector del LLM, sin bloquear la UI).

```python
# voice/vad.py
class VadModel(Protocol):
    def prob(self, chunk_16k: bytes) -> float: ...  # 0..1, chunk de 30 ms (480 muestras)
    def reset(self) -> None: ...

class SileroVad:  # onnxruntime, modelo silero_vad.onnx v5
    def __init__(self, model_path: Path) -> None: ...
class EnergyVad:  # fallback: reproduce la lógica actual del Segmenter
    ...
def load_vad(cfg: VoiceConfig) -> VadModel: ...  # Silero si el modelo existe; si no, Energy + aviso en log
```
Modelo: `%LOCALAPPDATA%\EzAgent\models\silero_vad.onnx`. El agente **no**
lo descarga en código en esta etapa (eso es E8): documenta en el README cómo
obtenerlo del repositorio oficial `snakers4/silero-vad` (licencia MIT) y fija
su SHA256 en `config.py` como constante tras verificarlo.
Silero espera ventanas de 512 muestras a 16 kHz: el adaptador acumula los
chunks de 480 y procesa cuando hay 512 (documentar con `# DECISION`).

```python
# voice/endpoint.py
@dataclass
class EndpointConfig:
    base_silence_s: float = 0.65
    extend_s: float = 0.40
    max_silence_s: float = 1.40
    speculative_after_s: float = 0.35

OPEN_ENDINGS_ES = ("y", "o", "que", "de", "del", "con", "para", "en", "pero", "porque", "como", "a", "el", "la", "los", "las", "un", "una")
OPEN_ENDINGS_EN = ("and", "or", "that", "to", "of", "with", "for", "in", "but", "because", "the", "a", "an")

def needs_more_time(partial_text: str, lang: str) -> bool:
    """True si el texto termina en conector/artículo o en coma."""
```
Flujo: silencio ≥ `speculative_after_s` → STT especulativo; silencio ≥
`base_silence_s` → si el texto especulativo `needs_more_time`, esperar hasta
`base + extend` (sin pasar de `max_silence_s`); si la voz vuelve, se descarta
el especulativo y se sigue acumulando.

```python
# voice/stt.py
class SttClient:
    def __init__(self, cfg: VoiceConfig) -> None: ...   # httpx.Client persistente, keep-alive
    def transcribe(self, wav: bytes) -> str: ...
    def submit(self, wav: bytes) -> "Future[str]": ...  # ThreadPoolExecutor(max_workers=2)
    def close(self) -> None: ...
# transcribe(cfg, wav) se conserva como envoltorio para compatibilidad.
```

```python
# voice/earcons.py
EARCONS: dict[str, list[tuple[float, float]]]  # nombre → [(frecuencia_hz, duración_s)]
def render(name: str, rate: int = 24000, gain_dbfs: float = -20.0) -> np.ndarray: ...
# Valores exactos: docs/roadmap/03-diseno-y-movimiento.md §7. ADSR: 5 ms ataque, 30 % release.
```

```python
# voice/llm.py
class Messenger:
    def cancel(self) -> None: ...   # cierra el stream HTTP activo (response.close()); idempotente
    def warm_up(self) -> None: ...  # GET {base_url}/models con timeout 3 s, ignora errores
```

**Nuevos campos de `VoiceConfig`** (con valores por defecto y migración de
`voice.json` sin romper archivos antiguos):
`vad_engine: str = "silero"`, `endpoint_base_s: float = 0.65`,
`endpoint_extend_s: float = 0.40`, `endpoint_max_s: float = 1.40`,
`speculative_stt: bool = True`, `earcons: dict[str, bool]` (por defecto los de
doc 03 §7), `barge_in_vad_prob: float = 0.6`, `barge_in_min_s: float = 0.15`.
`silence_secs` se conserva solo para el `EnergyVad`.

### Barge-in (T7)
Durante `SPEAKING`: si `vad.prob ≥ barge_in_vad_prob` durante
`barge_in_min_s` **y** el RMS supera `barge_in_rms` → `player.stop_turn`,
`engine.cancel_turn`, `messenger.cancel()`, nuevo turno. La cola de eco de
0,25 s tras el TTS se mantiene.

## Pasos (cada uno, un commit; ejecutar el banco de E1 tras T3, T4 y T5)
1. T1 `Player` + pruebas con un dispositivo falso (inyectar la clase de stream).
2. T2 `SynthEngine` + `Speaker` adaptado; `asyncio.run` eliminado. Pruebas con
   Kokoro falso (función que devuelve ceros con duración proporcional).
3. T3 `_answer_worker`: lee el stream del LLM → `_Sentences` →
   `engine.submit`; espera `done_event` solo al final. Marcas
   `first_sentence`/`first_sound` desde el engine/player.
4. T6 Earcons + reproducción por `Player` (prioridad delante de la siguiente frase).
5. T4 `vad.py` + `endpoint.py` + `Segmenter` con `VadModel`; pruebas con WAV
   sintético y con prob falsas.
6. T5 `SttClient` + especulativo; pruebas: se descarta si vuelve la voz, se
   reutiliza si no.
7. T7 Barge-in por VAD + `Messenger.cancel()`.
8. T8 `warm_up()` en el arranque (hilo) y `_fast_command` «qué agentes hay» en
   worker.
9. Banco completo (20 frases × 3) y registro en `metricas.md`.

## Criterios de aceptación
- [ ] Banco E1 con DeepSeek V4.1 Flash: fin de voz → primer sonido P50 ≤ 900 ms y
      P95 ≤ 1 500 ms (charla y comandos; los turnos con herramienta se miden
      pero su puerta es E3).
- [ ] 0 turnos cortados (`cut=false`) en las 20 frases del protocolo.
- [ ] Hueco entre frases consecutivas ≤ 60 ms más la pausa configurada
      (prueba con engine falso + medición en el banco).
- [ ] Barge-in: la voz se corta en ≤ 150 ms desde el inicio de habla del
      usuario (prueba con prob falsas + verificación manual).
- [ ] Sin `asyncio.run` en `voice/tts.py`; un solo stream de salida por llamada.
- [ ] Sin el modelo Silero, la app funciona con `EnergyVad` y lo registra.
- [ ] Todas las pruebas y ruff en verde.

## Pruebas
- `test_tts_pipeline.py`: orden de frases, solapamiento síntesis/reproducción,
  `stop_turn` corta en ≤ 1 bloque, turno nuevo no reproduce restos del viejo.
- `test_vad_endpoint.py`: `needs_more_time` (es/en), extensión, máximo,
  especulativo descartado.
- `test_stt_speculative.py`: reutilización y descarte con `SttClient` falso.
- `test_earcons.py`: duración, pico ≤ −20 dBFS, sin clipping.

## Comandos de verificación
```powershell
uv run pytest -q
uv run ruff check src tests
uv run python -m bench.latency --wav-dir bench/latency/fixtures --runs 3
uv run ezagent-voice   # prueba manual: conversación de 5 turnos, barge-in, mute
```

## Riesgos y rollback
- Silero en CPU: ≈ 1 ms por ventana; si el banco muestra subida de CPU > 5 %
  sostenida, `vad_engine="energy"` como valor por defecto y anotarlo.
- Endpoint más agresivo → cortes: la extensión semántica y el máximo de 1,4 s
  lo mitigan; si hay cortes, subir `endpoint_base_s` en pasos de 0,05 y
  registrar en `metricas.md`.
- Rollback por tarea: cada T es un commit revertible; el flag
  `speculative_stt` y `vad_engine` permiten desactivar sin revertir.

## Definición de hecho
Criterios cumplidos, banco registrado, revisión `code-review` + prueba con
la app real (skill `run` o a mano), informe de `AGENTS.md` §6.

## Prompt para el agente
```
Lee AGENTS.md, docs/roadmap/etapas/E2-pipeline-voz.md y la sección 7 de
docs/roadmap/03-diseno-y-movimiento.md.
Ejecuta solo la tarea <T#> de E2 (o todas en orden si se indica). Respeta
exactamente las interfaces del brief. En voice/app.py toca solo las zonas
indicadas. Tras T3, T4 y T5 ejecuta el banco de latencia y añade el resultado
al informe. Entrega el informe de AGENTS.md §6.
```
