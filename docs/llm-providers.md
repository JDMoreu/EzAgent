# Proveedores de LLM

EzAgent usa la API de chat compatible con OpenAI. Puedes cambiar de proveedor
sin tocar el código: OpenCode, OpenAI, Groq, Claude, Gemini, OpenRouter o
cualquier endpoint propio que hable `POST /chat/completions`.

## Variables de entorno

La forma rápida: define `LLM_PROVIDER` y la clave del proveedor en `.env`.

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=tu_clave
# Opcionales: si no los pones se usan los valores del proveedor.
# LLM_MODEL=llama-3.3-70b-versatile
# LLM_BASE_URL=https://api.groq.com/openai/v1
```

| `LLM_PROVIDER` | Clave | Endpoint por defecto | Modelo por defecto |
|---|---|---|---|
| `opencode` | `OPENCODE_API_KEY` | `https://opencode.ai/inference/go/openai/v1` | `deepseek-v4.1-flash` |
| `openai` | `OPENAI_API_KEY` | `https://api.openai.com/v1` | `gpt-4.1-mini` |
| `groq` | `GROQ_API_KEY` | `https://api.groq.com/openai/v1` | `llama-3.3-70b-versatile` |
| `anthropic` | `ANTHROPIC_API_KEY` | `https://api.anthropic.com/v1/` | `claude-sonnet-5-5` |
| `gemini` | `GEMINI_API_KEY` | `https://generativelanguage.googleapis.com/v1beta/openai/` | `gemini-2.5-flash` |
| `openrouter` | `OPENROUTER_API_KEY` | `https://openrouter.ai/api/v1` | `openai/gpt-4.1-mini` |
| otro | `LLM_API_KEY` | `LLM_BASE_URL` obligatorio | `LLM_MODEL` obligatorio |

`LLM_MODEL`, `LLM_BASE_URL` y `LLM_API_KEY` son overrides genéricos. Si usas
Claude con una clave de cuenta personal o de servicio sin workspace, añade
`ANTHROPIC_WORKSPACE_ID`; la capa de compatibilidad de Anthropic lo exige.

## Archivo `llm.json`

Para tener varios perfiles con su propia `base_url` y `api_key`, crea
`%APPDATA%\EzAgent\llm.json` (Windows) o apunta `EZ_LLM_CONFIG` a otro archivo.
El repo incluye `llm.example.json` como plantilla.

```json
{
  "provider": "groq",
  "providers": {
    "groq": {
      "base_url": "https://api.groq.com/openai/v1",
      "api_key": "tu_clave",
      "model": "llama-3.3-70b-versatile"
    },
    "anthropic": {
      "base_url": "https://api.anthropic.com/v1/",
      "api_key": "tu_clave",
      "model": "claude-sonnet-5-5"
    },
    "custom": {
      "base_url": "https://api.tu-proveedor.com/v1",
      "api_key": "tu_clave",
      "model": "tu-modelo"
    }
  }
}
```

Precedencia: argumentos explícitos > variables de entorno > perfil de `llm.json`
> valores integrados. `LLM_PROVIDER` cambia el perfil; si dejas el modelo o el
endpoint de OpenCode en `.env`, se ignoran al cambiar a otro proveedor. No
guardes claves en `llm.json` si el archivo está dentro del repositorio; `llm.json`
está ignorado por git.

## Dónde se configura

Hay dos caminos equivalentes:

- **Menú del notch → LLM · API.** Muestra los perfiles guardados y permite
  editar proveedor, modelo, dirección, clave, temperatura y máximo de tokens.
  Al pulsar **Guardar**, el perfil se escribe en `llm.json` y el proveedor
  elegido queda fijado en `voice.json` (`llm_pinned`), de modo que un
  `LLM_PROVIDER` antiguo del `.env` no lo revierta en el siguiente arranque.
- **`.env` o `llm.json`.** Útil para instalar o automatizar. La precedencia
  normal es argumentos explícitos > entorno > perfil > valores integrados.
  Cuando el proveedor se ha elegido desde el menú, `voice.json` manda sobre
  `LLM_PROVIDER` del entorno; así el cambio sobrevive al reinicio. Para volver
  a dejar que el `.env` decida, edita `voice.json` y pon `llm_pinned: false`.

Los campos de texto del menú se editan en línea: el notch pide el teclado solo
mientras dura la edición y lo devuelve al confirmar con `Enter`, cancelar con
`Esc` o pulsar fuera. `Ctrl+V` pega desde el portapapeles. Los perfiles nuevos
aparecen en **Perfil guardado** sin reiniciar. Los cambios de modelo, endpoint
o clave se aplican al guardar; los de audio, VAD y conversación se aplican en
vivo.

## Ejemplos

**Groq**

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=gsk_...
LLM_MODEL=llama-3.3-70b-versatile
```

**OpenAI**

```dotenv
LLM_PROVIDER=openai
OPENAI_API_KEY=sk-...
LLM_MODEL=gpt-4.1-mini
```

**Claude**

```dotenv
LLM_PROVIDER=anthropic
ANTHROPIC_API_KEY=sk-ant-...
LLM_MODEL=claude-sonnet-5-5
```

**Gemini**

```dotenv
LLM_PROVIDER=gemini
GEMINI_API_KEY=...
LLM_MODEL=gemini-2.5-flash
```

**Endpoint propio**

```dotenv
LLM_PROVIDER=custom
LLM_BASE_URL=https://api.mi-empresa.com/v1
LLM_API_KEY=...
LLM_MODEL=mi-modelo
```

Claude y Gemini se usan a través de su capa compatible con OpenAI; las
herramientas de Herdr y el streaming funcionan con el mismo cliente. Si un
proveedor rechaza `reasoning_effort`, EzAgent solo lo envía para OpenCode y
OpenAI. Para Claude se usa temperatura 1,0 porque su capa OpenAI rechaza valores
menores en modelos recientes.
