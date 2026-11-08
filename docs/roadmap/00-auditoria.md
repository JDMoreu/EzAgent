# 00 · Auditoría completa

Fecha: 8 nov 2026 · Commit auditado: `eb3dd1d` · Alcance: todo el repositorio
(≈ 39 000 líneas entre código, pruebas y docs; 442 funciones de prueba).

Severidad: **C** crítica (bloquea el objetivo del producto) · **A** alta
(degrada la experiencia de forma visible) · **M** media (deuda que frena) ·
**B** baja (higiene).

Cada hallazgo indica la etapa que lo resuelve.

---

## 1. Producto y posicionamiento

| ID | Sev | Hallazgo | Evidencia | Etapa |
|---|---|---|---|---|
| P1 | C | Hay dos productos en uno: orquestador de agentes y asistente general (archivos, web, Office, memoria, skills). El asistente general compite en un mercado saturado y es lo que más exige al LLM barato. | `src/ezagent/agent/tools_fs.py` (917 l.), `tools_web.py` (612), `memory.py` (1062), `skills.py` (564), `default_skills/` | E3, E4 |
| P2 | C | Sin Herdr no hay función principal: la delegación solo funciona con `hc`/`herdr-app`. Un usuario open source no puede usarlo. | `src/ezagent/voice/herdr.py`, `agent/tools_herdr.py:21` | E3 |
| P3 | C | Whisper depende de una instalación hermana de Dicta (`..\dicta\whisper_vulkan\`). 50 referencias a «Dicta» en 13 archivos; rutas personales en el README. | `scripts/start-whisper.ps1:21-27`, `voice/stt.py:39` (`dicta_home`), `README.md:37`, `packaging/EzAgent.spec:7-8` | E0, E8 |
| P4 | A | Sin LICENSE, CI, CONTRIBUTING ni plantillas de issues. | raíz del repo | E0 |
| P5 | A | El menú tiene 9 grupos y 36 filas; unas 20 son técnicas (RMS, pre-roll, pausas de TTS, temperatura, URL base, rutas de modelos). Contradice «fácil y sencillo». | `voice/notch/menu_layout.py:46-123`, `voice/notch/settings.py:26-68, 224-238` | E4 |
| P6 | M | Idioma incoherente: el `system.md` del usuario pide «español de Latam»; código, `DESIGN.md` y `PRODUCT.md` dicen «España». Sin i18n para inglés. | `agent/system_prompt.py:58`, `DESIGN.md:143` | E4, E7 |
| P7 | M | Documentación desalineada con el código (marcas de latencia, cola de eco 0,5 s vs 0,25 s, tamaño 142×32 vs 169×36, «OpenCode streaming»). | `docs/voice-latency.md:14` vs `voice/metrics.py:32`; `docs/voice-architecture.md:302` vs `voice/app.py:890`; `docs/design/notch-animation.html` vs `voice/notch/tokens.py:217-221` | E0, E7 |

## 2. Latencia y fluidez de la conversación

Nada está medido todavía: `docs/voice-latency.md:53-55` solo recoge una
estimación (0,8–1,2 s) y el protocolo de 20 frases no se ha ejecutado.

| ID | Sev | Hallazgo | Evidencia | Etapa |
|---|---|---|---|---|
| L1 | C | Espera fija de 1,3 s de silencio en cada turno con VAD por energía RMS; es el mayor coste individual. | `voice/config.py:121`, `voice/audio.py:235-266` | E2 |
| L2 | C | Turno estrictamente en serie: VAD → STT por lotes → LLM → TTS. Sin STT especulativo ni parcial. | `voice/app.py:1155-1188`, `voice/stt.py:137-181` | E2 |
| L3 | C | TTS sin solapamiento: `speak_sentence` corre inline en el hilo de respuesta; mientras suena una frase no se lee el LLM ni se sintetiza la siguiente. | `voice/app.py:1314-1344` | E2 |
| L4 | A | Se abre y cierra un stream de PyAudio por frase, y se crea un event loop (`asyncio.run`) por frase para normalizar texto. | `voice/tts.py:85-112`, `voice/tts.py:65` | E2 |
| L5 | C | Turnos con herramientas: hasta 4 rondas de LLM en serie y sin acuse previo. El texto previo a la herramienta solo se dice si llega a un signo de puntuación. | `voice/llm.py:435, 513-544` | E3 |
| L6 | A | Herramientas ejecutadas en serie con `subprocess.run` y timeouts de 15–45 s; confirmación bloqueante de hasta 15 s. | `voice/herdr.py:48-54, 151, 161, 281, 311`, `agent/config.py:75` | E3 |
| L7 | A | Trabajo síncrono antes del primer token en cada turno: re-escaneo de skills, reconstrucción de todas las herramientas, `tools.d`, SQLite y memoria. | `agent/runtime.py:192-227, 278-303`, `agent/skills.py:100-116`, `agent/memory.py:860-884` | E3 |
| L8 | A | La caché de prompt del proveedor no funciona: `{{hora}}` cambia el prefijo cada minuto y `messages[0]` se reconstruye en cada turno; además la selección de herramientas por regex cambia el bloque de tools. | `agent/system_prompt.py:89, 305-306`, `voice/llm.py:422`, `agent/runtime.py:82-109` | E3 |
| L9 | M | Sin reutilización de conexiones: STT usa `httpx.post` suelto; no hay precalentamiento TLS del LLM. | `voice/stt.py:137-181` | E2 |
| L10 | M | Consulta a Herdr (`subprocess`) en el hilo de Tk para «¿qué agentes hay?»; polling de 30 ms entre STT y LLM. | `voice/app.py:1525, 768-798` | E2, E5 |
| L11 | M | Barge-in por umbral RMS fijo (0,008) sin cancelación de eco; no cancela el stream HTTP en curso. | `voice/audio.py:389-399`, `voice/config.py:128`, `voice/app.py:1084-1105` | E2 |
| L12 | B | Los comandos rápidos no se miden; solo se registran turnos que llegan a `answer_done`. | `voice/metrics.py`, `voice/app.py:_speak` | E1 |

## 3. LLM barato, herramientas y memoria

Contexto fijo medido con los archivos reales del usuario: **≈ 17 700
caracteres (≈ 5 000 tokens) por petición**, repetidos en hasta 4 rondas.

| ID | Sev | Hallazgo | Evidencia | Etapa |
|---|---|---|---|---|
| T1 | C | 14 herramientas en cada turno con `tool_choice=auto`; no existe modo conversación sin herramientas. | `agent/runtime.py:58-109, 339-352` | E3 |
| T2 | C | `delegate_to_agent` es una megaherramienta: 9 acciones, 16 parámetros, 2 604 caracteres de esquema. Es la fuente principal de acciones equivocadas y argumentos inventados. | `agent/tools_herdr.py:815-842` | E3 |
| T3 | A | Catálogo de skills de 4 806 caracteres: 19 skills, 12 de ellas de programación global (`~/.agents/skills`: rea-*, reverse-engineering…) sin relación con voz; cada entrada lleva su ruta absoluta. | `agent/skills.py:145-157` | E4 |
| T4 | A | Sin parser de respaldo: si el modelo escribe la llamada como texto (JSON/XML), se envía al TTS y se lee en voz alta. | `voice/llm.py:478-480` | E3 |
| T5 | A | Validación estricta sin coerción: `"5"` o `"true"` se rechazan; no hay reintento guiado. `max_tokens=320` puede cortar argumentos JSON. | `agent/registry.py:134-149`, `voice/config.py:139` | E3 |
| T6 | A | Una segunda llamada al LLM tras cada herramienta solo para narrar el resultado; podría ser una plantilla. | `voice/llm.py:513-555` | E3 |
| T7 | A | Memoria dependiente de que el modelo llame a `remember`/`recall`; los modelos débiles casi nunca lo hacen. `recall` devuelve el propio mensaje actual porque se graba antes de ejecutar el modelo. | `agent/runtime.py:314`, `agent/memory.py:926-984` | E3, E4 |
| T8 | A | Estado de sesión perdido entre turnos: `_SessionState.last_address` se recrea en cada `begin_turn`, así que «avísame cuando termine» falla en el turno siguiente. | `agent/runtime.py:303`, `agent/tools_herdr.py:813` | E3 |
| T9 | M | Lista de palabras de aviso triplicada; incluye «dime» y «resumen», que activan vigilancia sin que se pida. | `voice/llm.py:355-373`, `agent/runtime.py:552-595`, `agent/tools_herdr.py:53` | E3 |
| T10 | M | Dobles confirmaciones (política + handler en memoria); el *taint* de `load_skill` hace que toda acción guiada por skill pida confirmación. | `agent/tools_memory.py:108, 176, 207`, `agent/runtime.py:447-454` | E3, E4 |
| T11 | M | Dos bucles de herramientas: `voice/llm.py:415-555` (producción) y `agent/loop.py` (solo pruebas), con reglas de *taint* distintas. Más un camino legado con 14 herramientas Herdr. | `agent/loop.py`, `voice/llm.py:16-265, 557-648` | E0, E3 |
| T12 | M | Dependencia invertida: `agent/` importa `voice/`. | `agent/tools_herdr.py:21` | E3 |
| T13 | M | Resultados de herramienta recortados a 2 000 caracteres, pero `load_skill` puede devolver 20 000 y `fetch_page` 8 000. | `agent/runtime.py:528`, `agent/skills.py:34` | E4 |
| T14 | B | ~13 de ~20 reglas de `system.md` repiten el esquema de `delegate_to_agent`. | `agent/system_prompt.py:57-90` | E3 |

## 4. UI, rendimiento gráfico y animación

| ID | Sev | Hallazgo | Evidencia | Etapa |
|---|---|---|---|---|
| U1 | A | Render del frame completo en numpy float32 a 60 fps, sin dirty rects, en el mismo proceso que Kokoro y PyAudio (contención de GIL). | `voice/notch/island.py:1534-1816`, `voice/notch/layered.py:852-869, 947-1014` | E5, E6 |
| U2 | A | Cada frame de mostrar/ocultar hace float → uint8 → `Image` → `resize` → float. | `voice/notch/island.py:1905-1919` | E6 |
| U3 | A | La caché de silueta falla en cada frame de apertura del menú (la clave incluye `menu_h` y `tab_width`). | `voice/notch/silhouette.py:185-205, 427-447` | E6 |
| U4 | A | Estado de la isla escrito sin lock desde el hilo de audio y leído por el hilo de render. | `voice/notch/island.py:990-1023`, `voice/app.py:1322, 1375, 1874` | E0 |
| U5 | M | La firma de caché del menú llama a `settings.snapshot()` y recorre dispositivos y voces en cada frame estático. | `voice/notch/menu.py:2232-2278` | E6 |
| U6 | M | Archivos dios: `menu.py` 3 513 l., `app.py` 2 566, `island.py` 1 919. | — | E5, E7 |
| U7 | M | Tokens duplicados: `tokens.py` dice ser la única fuente, pero `menu.py:147-300`, `menu_layout.py:19-35`, `eyes.py:50-60`, `island.py:1821-1840`, `DESIGN.md` y `notch-animation.html` repiten valores. Constantes de FPS sin uso (`layered.py:489`, `tokens.py:310-311`). | — | E7 |
| U8 | M | «Spring» implementado como bezier con overshoot y resuelto por bisección en Python en cada llamada. | `voice/notch/anim.py:30-61`, `tokens.py:315-318` | E7 |
| U9 | B | `print` dentro del bucle de render ante excepciones del menú. | `voice/notch/island.py:1736` | E0 |

## 5. Marca, diseño y personalidad

| ID | Sev | Hallazgo | Etapa |
|---|---|---|---|
| D1 | A | La paleta grafito + mint se lee como «herramienta de IA genérica». El activo distintivo, los dos ojos, está infrautilizado: sus estados no hablan de agentes (despachar, trabajando, terminó, bloqueado). | E7 |
| D2 | A | La personalidad de voz no está definida más allá de «sereno y factual»; no hay guía de frases, muletillas prohibidas ni ejemplos. | E3, E7 |
| D3 | M | No hay sonido de interfaz (earcons): la única señal de «te escuché» es visual. En un producto de voz con el notch fuera de la vista, falta feedback auditivo. | E2, E7 |
| D4 | M | `DESIGN.md` mezcla reglas vigentes con decisiones históricas (Tauri, dashboard, display de 38 px) y tablas que el notch no usa. | E7 |
| D5 | B | «EzAgent» incluye «Agent» y lo coloca en la categoría saturada; el naming debe revisarse antes del lanzamiento. | E9 |

## 6. Robustez y concurrencia

| ID | Sev | Hallazgo | Evidencia | Etapa |
|---|---|---|---|---|
| R1 | A | Carrera en `_tts_stop`: se reasigna sin lock desde el hilo de respuesta y desde el de Tk; un barge-in puede dejar sin señalar el evento del worker. | `voice/app.py:1136, 1227-1229, 1248-1250, 1305` | E0 |
| R2 | M | Hilos nuevos por turno (STT, respuesta, TTS) y por clic; sin ciclo de vida centralizado. | `voice/app.py:1188, 860`, `voice/notch/layered.py:1121, 1230, 1242` | E5 |
| R3 | B | Código muerto: `MicCapture.record`, `runtime.note_done`, default `Segmenter.silence_secs=2.5` nunca usado. | `voice/audio.py:429-476`, `agent/runtime.py:597-606`, `voice/audio.py:167` | E0 |

## 7. Pruebas

- 442 funciones de prueba; buena cobertura de política, rutas, memoria, web y
  menú.
- **Huecos:** fragmentos de tool-call en streaming malformados, llamadas
  escritas como texto, llamadas paralelas, agotamiento de rondas,
  comportamiento con modelos reales, latencia y regresiones de VAD. Se cubren
  en E1 (set dorado y banco) y E3.

## 8. Lo que está bien y se conserva

- Pipeline local de STT/TTS con calentamiento al arrancar (`voice/app.py:567-580`).
- Streaming de la primera cláusula corta (`voice/app.py:84-138`).
- Comandos locales sin LLM (`_fast_command`, `voice/app.py:1490-1600`).
- Presets de proveedor y perfiles (`llm_providers.py`).
- Política de seguridad sin shell libre y confirmaciones generadas por código.
- Tareas pendientes persistentes y encadenado `input_from` (`voice/tasks.py`).
- Los ojos como marca única, la máquina de estados pura (`voice/notch/state.py`)
  y el soporte de movimiento reducido y DPI.
- La disciplina de `DESIGN.md` (sin glow decorativo, sin tarjetas anidadas, sin
  métricas inventadas).
