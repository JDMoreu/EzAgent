# 01 · Producto y marca

## 1. Posicionamiento

**Una frase:** EzAgent es la capa de voz local para tus agentes de código:
hablas, despacha la tarea a Claude Code, OpenCode o Codex, la vigila y te avisa
cuando termina.

**Para quién:** desarrolladores en Windows 11 que ya usan uno o varios agentes
de código por CLI y quieren delegar, consultar y recibir avisos sin cambiar de
ventana ni escribir.

**Por qué ganaría frente a la alternativa:**

| Alternativa | Qué le falta | Qué aporta EzAgent |
|---|---|---|
| Asistentes de voz generales | No conocen tus agentes ni tus proyectos | Orquesta los CLIs que ya usas, con tus modelos y suscripciones |
| Dictado + escribir en la terminal | Te ata a la ventana; no te avisa | Manos libres, aviso por voz al terminar o bloquearse |
| Asistentes en la nube | Audio fuera de tu máquina, coste por minuto | STT y TTS locales; el LLM es barato y lo eliges tú |
| Herdr u otros multiplexores | Sin voz | Voz encima de Herdr o de lanzamiento directo |

**Promesa de producto (medible):**
1. Respuesta audible en menos de un segundo (P50) desde que dejas de hablar.
2. Una orden, un agente lanzado: sin confirmar lo que no es destructivo.
3. Te avisa por voz cuando un agente termina, se bloquea o falla.
4. Coste de LLM despreciable: un modelo flash basta para orquestar.

## 2. Qué se queda, qué se va, qué cambia, qué se añade

### Se queda
- Pipeline local VAD → whisper.cpp → LLM → Kokoro.
- Notch con los dos ojos como interfaz y marca.
- Comandos locales sin LLM, tareas pendientes, encadenado `input_from`,
  perfiles de modelo/esfuerzo por agente.
- Presets de proveedor (OpenCode Go por defecto) y perfiles en `llm.json`.
- Bandeja del sistema y atajos globales.
- Política «sin shell libre» y confirmaciones generadas por código.

### Se va (E4)
- Herramientas de archivos (`tools_fs.py`, `journal.py`, `undo_last`).
- Web (`tools_web.py`, `search.py`; dependencias `ddgs`, `bs4`, `lxml`).
- Skills y scripts (`skills.py`, `tools_skills.py`, `default_skills/`, extra `office`).
- Herramientas declarativas `tools.d`.
- Historial FTS5 y las herramientas `recall`, `forget`, `compact_memory`.
- Taint multi-fuente (al no haber web ni archivos, se simplifica).
- Ajustes técnicos visibles por defecto (pasan a «Avanzado»).

> Todo lo retirado queda en el historial de git (`eb3dd1d`). Si en el futuro
> se quiere un asistente general, se rehace como plugin sobre el protocolo IPC,
> no dentro del núcleo.

### Cambia
- `delegate_to_agent` → 4 herramientas planas (`run_agent`, `agent_status`,
  `send_to_agent`, `stop_agent`) + `voice_control` + `remember` (E3).
- Herdr pasa de requisito a backend opcional; se añade `DirectBackend` (E3).
- Narración de resultados por plantillas, sin segunda llamada al LLM (E3).
- Español de España → español neutro latinoamericano + inglés (E4).
- Paleta, coreografía y earcons (E7).

### Se añade
- Banco de latencia y set dorado de intenciones (E1).
- Earcons: escuché, despachado, terminó, bloqueado, error (E2/E7).
- `agents.toml` y `projects.toml`: agentes y proyectos con alias hablados (E3).
- Onboarding de 3 pasos y descarga gestionada de modelos (E8).

## 3. Personalidad: «el despachador de guardia»

EzAgent habla como una persona de operaciones con experiencia que coordina un
equipo: breve, cálida, segura, sin adornos. Dice qué hizo, dónde y qué sigue.

| Rasgo | Sí | No |
|---|---|---|
| Breve | Una frase; dos como máximo | Explicaciones, listas, markdown |
| Concreta | «Lancé OpenCode en *ezagent*; te aviso al terminar.» | «¡Claro! He procedido a iniciar el agente…» |
| Cálida | «Listo.» «Va.» «Hecho, ¿algo más?» | Emojis, exclamaciones, halagos |
| Con criterio | Pregunta una sola cosa si falta un dato | Preguntar por lo que puede deducir |
| Honesta | «No encuentro el proyecto *api*. ¿Cuál es?» | Inventar resultados o estados |

### Reglas de voz (van al prompt y a las plantillas)
1. Máximo 2 frases y 30 palabras por respuesta hablada, salvo resumen pedido.
2. Nunca leas código, rutas largas, URLs ni logs; resume en lenguaje llano.
3. Al ejecutar: verbo en pasado + agente + proyecto + siguiente paso.
4. Al fallar: qué pasó + qué puede hacer el usuario, en una frase.
5. Nombres de agentes y proyectos tal como el usuario los dice.
6. Sin muletillas de apertura («¡Claro!», «Por supuesto», «Entendido»).

### Ejemplos canónicos (es / en)

| Situación | Español | English |
|---|---|---|
| Lanzar | «Lancé Claude Code en *ezagent* para arreglar los tests. Te aviso.» | "Started Claude Code on *ezagent* to fix the tests. I'll let you know." |
| Terminó | «OpenCode terminó en *api*: los tests pasan. ¿Te lo resumo?» | "OpenCode finished on *api*: tests pass. Want a summary?" |
| Bloqueado | «Codex se detuvo en *web*: pide permiso para instalar paquetes.» | "Codex is blocked on *web*: it wants to install packages." |
| Estado | «Dos trabajando: Claude en *ezagent* y OpenCode en *api*.» | "Two running: Claude on *ezagent* and OpenCode on *api*." |
| Falta dato | «¿En qué proyecto?» | "Which project?" |
| Error | «No pude lanzar OpenCode: no está instalado.» | "Couldn't start OpenCode: it isn't installed." |

## 4. Identidad visual (resumen; detalle en `03-diseno-y-movimiento.md`)

- **Símbolo:** los dos ojos. Se conserva: es lo único memorable de la marca actual.
- **Idea rectora nueva:** «la mirada que delega». Los ojos expresan la relación
  con los agentes: miran hacia donde despachan, esperan y celebran.
- **Paleta:** se abandona el mint genérico por una señal propia (ver doc 03).
- **Sonido:** cinco earcons cortos y sintetizados forman parte de la marca.

## 5. Naming (decisión abierta, se cierra en E9)

«EzAgent» funciona como nombre de trabajo, pero contiene «Agent» y se pierde
entre cientos de proyectos. Antes del lanzamiento:

| Opción | Pro | Contra |
|---|---|---|
| Mantener **EzAgent** | Coste cero, repo y paquete ya existen | Genérico, difícil de buscar |
| **Relevo** | Español, describe el gesto (pasar el testigo al agente) | Menos claro en inglés |
| **Hark** | Corto, inglés, «escucha» | Posibles colisiones de marca |

Criterios: dominio `.dev` disponible, nombre libre en PyPI y GitHub, sin
marca registrada en software (búsqueda en EUIPO/USPTO) y pronunciable en
español e inglés. **Recomendación:** mantener EzAgent hasta E8 y decidir en E9
con esa comprobación.

## 6. Principios de producto (sustituyen a los de `PRODUCT.md` desde E4)

1. **La voz es la interfaz; el notch es el testigo.** Todo se puede hacer
   hablando; el notch muestra el estado y los ajustes.
2. **EzAgent no hace el trabajo; lo despacha.** Si una tarea exige leer o
   escribir archivos, la hace un agente.
3. **Rápido antes que listo.** Una respuesta corta ya es mejor que una brillante tarde.
4. **El estado nunca es ambiguo** (forma + color + sonido).
5. **Local por defecto, honesto siempre.** Se dice qué va a la nube (solo el
   texto al LLM).
6. **Simple por defecto, potente en «Avanzado».**
