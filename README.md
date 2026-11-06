# EzAgent

Mensajero de voz para Herdr con **notch** y **menú desplegable**. No hay app de
escritorio, dashboard ni panel de llamada: la isla flotante y su menú son toda
la interfaz.

```
Micrófono 16 kHz
   │
   ▼
VAD por energía ──► whisper.cpp (es) ──► LLM compatible con OpenAI
   │                                      (OpenCode, OpenAI, Groq, Claude, Gemini…)
   ▼
Kokoro (ONNX local, español)
   │
   ▼
Altavoz
```

| Capa | Motor | Dónde corre |
|---|---|---|
| VAD / segmentación | `Segmenter` local con pre-roll e histéresis | Local |
| STT | whisper.cpp + `large-v3-turbo` | Local, GPU AMD vía Vulkan |
| LLM | Proveedor compatible con OpenAI, configurable | Nube |
| TTS | Kokoro v1.0, voces españolas | Local, CPU (ONNX) |

## Arranque

Requisitos: Windows 11, `uv`, `whisper-server` con Vulkan y un modelo
`large-v3-turbo`, y una clave del proveedor LLM elegido.

```powershell
cd C:\Users\JDMoreu\Documents\EzAgent
uv sync
copy .env.example .env
# edita .env: elige LLM_PROVIDER y pon su clave

# Terminal 1: STT
powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1

# Terminal 2: mensajero + notch
powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1
```

O doble clic en `EzAgent.bat` y elige **Iniciar mensajero de voz**. La app
arranca escuchando; `Ctrl+Alt+Space` cuelga o retoma.

## El notch

La isla cerrada mide **169×36** en horizontal o **36×186** acoplada a un
lateral, y se puede arrastrar por el cuerpo:

- Lejos de los bordes es una **píldora flotante**.
- A ≤12 px del borde superior se acopla con el cuello hacia arriba; a ≤12 px del
  borde inferior útil se acopla con el cuello hacia abajo.
- A ≤12 px del borde izquierdo o derecho se acopla como **píldora vertical de
  36×186**, con el mismo cuello girado hacia el lateral y los controles apilados
  (micrófono arriba, ojos en el centro, flecha abajo).
- Al desplegar el menú, la isla se ensancha con él hasta **320×596** (o
  **356×560** si está acoplada de lado) y forma una sola pieza continua. En el
  acople lateral, el menú crece hacia el interior de la pantalla.
- El micrófono silenciado muestra el icono tachado en rojo.
- La flecha derecha abre y cierra el menú; `Ctrl+Alt+O` hace lo mismo.

## El menú es Ajustes

El menú desplegable contiene todos los ajustes del mensajero y vive en la misma
ventana que el notch. Si hay más filas de las que caben, la rueda desplaza el
contenido entre la cabecera y el pie; el pulgar de scroll queda visible a la
derecha. Los campos de texto se editan en línea: al pulsarlos, el notch pide el
teclado y lo devuelve al confirmar con `Enter`, cancelar con `Esc` o pulsar
fuera. `Ctrl+V` pega desde el portapapeles.

- **AUDIO:** micrófono y **medidor de entrada con nivel en vivo y umbral
  arrastrable**; umbral de ruido y silencio de cierre.
- **MOTOR:** motor de Whisper (`Automático`, `GPU · Vulkan` o `CPU`) y variante
  de Kokoro (`int8`/`fp32`). El puerto local de Whisper se elige libre y se
  configura solo al arrancar. VAD y TTS corren en local.
- **TTS:** voz de Kokoro, velocidad y pausas de frase/cláusula. Las rutas de
  modelo/voces quedan como configuración avanzada.
- **VAD:** antelación de escucha, duración mínima, frase máxima y umbral de
  interrupción.
- **WHISPER · STT:** modelo en una **lista de los modelos instalados**, indicación,
  tiempo máximo y arranque automático. La dirección del servidor ya no se pide:
  EzAgent usa un puerto local libre.
- **LLM · API:** perfil guardado, proveedor, **modelo en lista**, dirección,
  clave, temperatura, máximo de tokens y **nivel de razonamiento**.
- **CONVERSACIÓN:** interrupción activa, confirmación opcional y prueba de voz.
- Guardar, cambios sin guardar, ocultar la isla y cerrar el menú.

Los ajustes de audio, voz, TTS, VAD, Whisper y conversación se guardan en
`%APPDATA%\EzAgent\voice.json`; los perfiles de proveedor, en
`%APPDATA%\EzAgent\llm.json`. No hay una segunda ventana de configuración.

## Proveedor y modelo LLM

Se pueden cambiar desde el grupo **LLM · API** del menú o con variables de
entorno y `llm.json`. Los proveedores integrados son OpenCode, OpenAI, Groq,
Claude (Anthropic), Gemini, OpenRouter y cualquier endpoint compatible con
OpenAI. Ejemplo por entorno:

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=...
# Opcionales; si faltan, se usan los del proveedor:
# LLM_MODEL=llama-3.3-70b-versatile
# LLM_BASE_URL=https://api.groq.com/openai/v1
```

Proveedores integrados: OpenCode, OpenAI, Groq, Claude (Anthropic), Gemini,
OpenRouter y cualquier endpoint compatible con OpenAI. También puedes usar
`%APPDATA%\EzAgent\llm.json` o `EZ_LLM_CONFIG` con perfiles por proveedor.
Guía completa: [`docs/llm-providers.md`](docs/llm-providers.md).

## Atajos

| Atajo | Acción |
|---|---|
| `Ctrl+Alt+Space` | Iniciar / colgar la llamada |
| `Ctrl+Alt+E` | Colgar (`Ctrl+Alt+C` sigue como alias) |
| `Ctrl+Alt+M` | Silenciar / reactivar el micrófono |
| `Ctrl+Alt+O` | Abrir / cerrar el menú del notch |
| `Ctrl+Alt+R` | Repetir el último resumen |

El micrófono no se cierra entre turnos durante una llamada. Las acciones
explícitas se ejecutan sin confirmación y el asistente responde en una frase
corta con qué abre y dónde.

## Pruebas

```powershell
.venv\Scripts\python.exe -m pytest -q
```

Detalle del mensajero, límites y arquitectura de audio:
[`src/ezagent/voice/README.md`](src/ezagent/voice/README.md).
