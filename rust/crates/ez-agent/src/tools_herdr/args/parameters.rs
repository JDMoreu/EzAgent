//! Esquemas de parámetros de `delegate_to_agent` y `voice_control`.
//!
//! Copiados literalmente de `_DELEGATE_PARAMETERS` y `_VOICE_PARAMETERS`.

use serde_json::{json, Value};

use super::{DELEGATE_ACTIONS, VOICE_ACTIONS};

/// `_DELEGATE_PARAMETERS`.
pub fn delegate_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": DELEGATE_ACTIONS,
                "description": (
                    "list: agentes vivos; get/read/focus/watch: operan sobre una \
                     dirección; send: envía una tarea; run: lanza un agente \
                     (con o sin tarea); open_terminal: abre una terminal vacía; \
                     kill: termina un agente; agents: catálogo de agentes, \
                     modelos y esfuerzos; configure: cambia modelo/esfuerzo de un \
                     agente ya abierto; keys: pulsa teclas en un agente; guide: \
                     manual del agente."
                ),
            },
            "address": {
                "type": "string",
                "description": "Dirección del terminal/agente (por ejemplo, t1).",
            },
            "agent": {
                "type": "string",
                "description": (
                    "Agente a lanzar: claude, opencode, codex, mcode, pi... (valores del catálogo)."
                ),
            },
            "task": {
                "type": "string",
                "description": (
                    "Tarea o mensaje que se entrega al agente, como instrucción directa del \
                     usuario con los errores de transcripción corregidos y sin encabezados \
                     (nada de «Mensaje para…»). En run es opcional: sin tarea solo se abre el agente."
                ),
            },
            "cwd": {"type": "string", "description": "Carpeta de trabajo."},
            "name": {
                "type": "string",
                "description": "Nombre de la pestaña o etiqueta del terminal.",
            },
            "background": {
                "type": "boolean",
                "description": (
                    "Solo si el usuario pide explícitamente segundo plano; por \
                     defecto la pestaña es visible."
                ),
            },
            "gui": {
                "type": "boolean",
                "description": (
                    "true si la tarea requiere controlar la interfaz gráfica (cua-driver)."
                ),
            },
            "notify_on_done": {
                "type": "boolean",
                "description": (
                    "Avisar por voz cuando el agente termine (y en cada etapa de then). \
                     Por defecto sí; false solo si el usuario no quiere aviso."
                ),
            },
            "deliver_answer": {
                "type": "boolean",
                "description": (
                    "Decir por voz la respuesta del agente (o del último paso de \
                     then) en cuanto termine, sin preguntar antes."
                ),
            },
            "close_listening": {
                "type": "boolean",
                "description": "Cerrar la escucha al terminar de hablar.",
            },
            "space": {"type": "string", "description": "Espacio de Herdr."},
            "model": {
                "type": "string",
                "description": (
                    "Modelo del agente lanzado (por ejemplo, sonnet, opus, \
                     deepseek-v4.1-flash). Se aplica según su perfil de voice.json."
                ),
            },
            "effort": {
                "type": "string",
                "description": (
                    "Esfuerzo de razonamiento del agente lanzado: low, medium, \
                     high o max, según su perfil."
                ),
            },
            "input_from": {
                "type": "string",
                "description": (
                    "Dirección de otro agente cuya salida se pasa como contexto a \
                     este. Si aún está trabajando, se encadena y se lanza al terminar."
                ),
            },
            "keys": {
                "type": "array",
                "maxItems": 12,
                "items": {"type": "string"},
                "description": (
                    "Teclas para action=keys: enter, esc, tab, up, down, left, \
                     right, pageup, pagedown, ctrl+t, shift+tab, una letra..."
                ),
            },
            "then": {
                "type": "array",
                "maxItems": 5,
                "description": (
                    "Pasos siguientes, en orden, para flujos de varios agentes. \
                     Cada uno se abre mientras el anterior trabaja y, al terminar \
                     este, recibe solo su respuesta; en task puede ir {output} \
                     donde debe colocarse."
                ),
                "items": {
                    "type": "object",
                    "properties": {
                        "agent": {"type": "string"},
                        "task": {"type": "string"},
                        "model": {"type": "string"},
                        "effort": {"type": "string"},
                        "cwd": {"type": "string"},
                        "name": {"type": "string"},
                    },
                    "required": ["agent"],
                },
            },
            "shell": {
                "type": "string",
                "description": "Shell de la terminal nueva (powershell.exe por defecto).",
            },
            "lines": {
                "type": "integer",
                "minimum": 10,
                "maximum": 120,
                "description": "Líneas del buffer que se leen.",
            },
        },
        "required": ["action"],
    })
}

/// `_VOICE_PARAMETERS`.
pub fn voice_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": VOICE_ACTIONS,
                "description": (
                    "close: cierra la escucha; close_and_watch: cierra y avisa al \
                     terminar; mic_mute: silencia o reactiva el micrófono; \
                     repeat_last: repite el último resumen; mute: silencia los \
                     avisos N minutos."
                ),
            },
            "address": {
                "type": "string",
                "description": (
                    "Dirección del agente para close_and_watch; si se omite se usa \
                     el último agente conocido."
                ),
            },
            "muted": {
                "type": "boolean",
                "description": "true silencia el micrófono; false lo reactiva.",
            },
            "minutes": {
                "type": "integer",
                "minimum": 1,
                "maximum": 480,
                "description": "Minutos que se silencian los avisos.",
            },
        },
        "required": ["action"],
    })
}
