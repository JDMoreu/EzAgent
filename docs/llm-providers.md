# Proveedores de LLM

EzAgent habla con el LLM en uno de dos formatos (`api_style`):

- **`openai`**: Chat Completions (`POST …/chat/completions`). Lo usan OpenCode,
  OpenAI, Gemini, Groq, Cerebras, DeepSeek, OpenRouter y la mayoría de
  servicios.
- **`anthropic`**: Messages (`POST …/v1/messages`). Lo usan Claude y los
  servicios «compatibles con Anthropic» (MiniMax, Kimi, GLM y otros que
  publican un endpoint `/anthropic`).

El streaming y las herramientas funcionan igual en los dos: el cliente de
Anthropic (`voice/anthropic_client.py`) traduce la petición y devuelve los
trozos con la misma forma que el SDK de OpenAI.

## Presets

| Proveedor (`LLM_PROVIDER`) | Clave | Endpoint | Formato | Modelo por defecto |
|---|---|---|---|---|
| OpenCode Go (`opencode`) | `OPENCODE_API_KEY` | `https://opencode.ai/inference/go/openai/v1` | openai | `deepseek-v4.1-flash` |
| OpenAI, GPT y Codex (`openai`) | `OPENAI_API_KEY` | `https://api.openai.com/v1` | openai | `gpt-4.1-mini` |
| Claude (`anthropic`) | `ANTHROPIC_API_KEY` | `https://api.anthropic.com/v1` | anthropic | `claude-sonnet-5-5` |
| Gemini (`gemini`) | `GEMINI_API_KEY` | `https://generativelanguage.googleapis.com/v1beta/openai/` | openai | `gemini-2.5-flash` |
| Groq (`groq`) | `GROQ_API_KEY` | `https://api.groq.com/openai/v1` | openai | `llama-3.3-70b-versatile` |
| Cerebras (`cerebras`) | `CEREBRAS_API_KEY` | `https://api.cerebras.ai/v1` | openai | `gpt-oss-120b` |
| DeepSeek (`deepseek`) | `DEEPSEEK_API_KEY` | `https://api.deepseek.com/v1` | openai | `deepseek-chat` |
| OpenRouter (`openrouter`) | `OPENROUTER_API_KEY` | `https://openrouter.ai/api/v1` | openai | `openai/gpt-4.1-mini` |
| Personalizado (`custom` u otro nombre) | `LLM_API_KEY` | la que pongas | el que elijas | el que escribas |

Los modelos por defecto son solo un punto de partida y pueden quedar viejos.
En el menú, **Modelos de la API** pide la lista real al proveedor.

Las suscripciones de **Claude Code** o **Codex** (inicio de sesión con tu
cuenta) no son claves de API. Para usar Claude o GPT como cerebro de EzAgent
hace falta una clave de API de Anthropic u OpenAI.

## Desde el menú (LLM · API)

1. **Proveedor:** elige un preset o **Personalizado**.
2. **Nombre del perfil:** para un personalizado, ponle un nombre (`minimax`,
   `kimi`…). Al guardar, aparece como un proveedor más en la lista.
3. **Compatibilidad:** OpenAI o Anthropic. Los presets ya traen la suya.
4. **Dirección de la API** y **Clave API**.
5. **Modelo:** texto libre; escribe el id exacto.
6. **Modelos de la API:** la lista real del proveedor (`GET /models`), pedida
   en segundo plano al arrancar, al abrir el menú, al cambiar de proveedor y al
   guardar. Elegir uno rellena **Modelo**. Se ocultan los modelos de audio,
   imagen y embeddings.
7. **Guardar.** El perfil se escribe en `llm.json` y el proveedor queda fijado
   en `voice.json` (`llm_pinned`), así un `LLM_PROVIDER` viejo del `.env` no lo
   revierte en el siguiente arranque.

**Ids sin prefijo.** Groq y OpenRouter usan ids como `qwen/qwen3-32b`. Si
escribes `qwen3-32b` y en la lista real hay una única coincidencia, EzAgent lo
completa al guardar y al arrancar (`Modelo «…» corregido a «…»`).

Para Anthropic vale la raíz que da el servicio (`https://api.minimax.io/anthropic`)
o una que termine en `/v1`: EzAgent añade `/v1/messages` o `/messages` según
corresponda.

## Variables de entorno

```dotenv
LLM_PROVIDER=cerebras
CEREBRAS_API_KEY=tu_clave
# Opcionales: si no los pones se usan los del proveedor.
# LLM_MODEL=gpt-oss-120b
# LLM_BASE_URL=https://api.cerebras.ai/v1
# LLM_API_STYLE=openai     # openai | anthropic
```

`LLM_MODEL`, `LLM_BASE_URL`, `LLM_API_KEY` y `LLM_API_STYLE` son overrides
genéricos.

## Archivo `llm.json`

`%APPDATA%\EzAgent\llm.json` (o el que indique `EZ_LLM_CONFIG`) guarda los
perfiles. El menú lo escribe al guardar; también se puede editar a mano. El
repositorio incluye `llm.example.json` como plantilla. `llm.json` está ignorado
por git porque guarda claves.

```json
{
  "provider": "minimax",
  "providers": {
    "groq": {
      "base_url": "https://api.groq.com/openai/v1",
      "api_key": "gsk_...",
      "model": "openai/gpt-oss-120b",
      "api_style": "openai"
    },
    "minimax": {
      "label": "MiniMax",
      "base_url": "https://api.minimax.io/anthropic",
      "api_key": "...",
      "model": "MiniMax-M2",
      "api_style": "anthropic"
    }
  }
}
```

Precedencia: valores fijados en el menú > variables de entorno > perfil de
`llm.json` > valores del preset. Para que vuelva a decidir el `.env`, pon
`llm_pinned: false` en `voice.json`.

## Errores habituales

EzAgent dice el problema por voz en una frase; el detalle técnico queda en el
log (`LLM (proveedor): …`).

| Lo que oyes | Causa | Qué hacer |
|---|---|---|
| «… no reconoce el modelo …» (404) | El id no existe en ese proveedor | Elige uno de **Modelos de la API** |
| «La clave de … no es válida …» (401/403) | Clave errónea o sin acceso | Revisa la clave |
| «… me limitó por demasiadas peticiones …» (429) | Límite de peticiones o tokens por minuto | Espera, o usa otro modelo o proveedor |
| «La petición es más grande de lo que permite tu plan …» (413) | Una sola petición supera el límite por minuto | Plan con más tokens u otro proveedor |
| «No pude conectar con …» | Red o dirección de la API | Revisa la conexión y la URL |

**Límites de tokens por minuto.** Cada petición de EzAgent lleva unos 7.000
tokens (instrucciones, herramientas, catálogo de skills e historial), y un turno
con herramientas hace de 2 a 3 peticiones. Los planes gratuitos con 7.000–8.000
tokens por minuto (Groq gratis, por ejemplo) no alcanzan. EzAgent no reintenta
en silencio: avisa al momento.

## Detalles por proveedor

- `reasoning_effort` solo se envía a OpenCode y OpenAI, y a los demás
  proveedores compatibles con OpenAI si no es `none`. Nunca a Gemini ni a
  Claude.
- Claude con API nativa acepta la temperatura configurada.
- Con clave personal o de servicio sin workspace, define
  `ANTHROPIC_WORKSPACE_ID`.
