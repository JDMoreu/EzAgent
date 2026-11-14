//! Constantes y textos del `Messenger` de `src/ezagent/voice/llm.py` (F4-07).

/// Aviso de espera: si el turno anterior sigue ocupando el lock, se avisa una vez.
pub const BUSY_WAIT_SECS: f64 = 1.5;
pub const BUSY_POLL_SECS: f64 = 0.1;

/// Avisos internos: viajan como dato del usuario, nunca como `system` a mitad
/// del historial (hay modelos que solo aceptan un system al principio y otros
/// que rechazan secuencias de roles no alternas).
pub const CLOSE_NOTICE: &str = "Cierra ahora con los resultados ya obtenidos, sin herramientas. \
No repitas consultas, no prometas seguir en otro turno ni pidas reformular. \
Distingue lo verificado, lo pendiente y los archivos realmente creados. \
Si delegaste, di en qué pestaña está (su nombre visible) y que la tarea sigue pendiente. \
Por voz usa como máximo dos frases.";

pub const EMPTY_ANSWER: &str =
    "El modelo devolvió una respuesta vacía. Prueba otra vez o cambia de modelo en ajustes.";

pub const LENGTH_NOTICE: &str =
    " La respuesta se cortó por el límite de tokens; pide algo más corto.";

/// `json.dumps({"ok": False, "error": …}, ensure_ascii=False)`.
pub const CANCELLED_TOOL_RESULT: &str =
    "{\"ok\": false, \"error\": \"Cancelado o interrumpido por el usuario.\"}";

/// Texto del `LLMTurnTimeout` del Python.
pub const TURN_TIMEOUT_TEXT: &str =
    "El turno superó el tiempo máximo configurado sin terminar. Prueba con una petición más \
corta o sube el límite en ajustes.";

/// Modo chat: el usuario escribe y lee en pantalla (a veces también escucha la respuesta).
/// Sustituye la regla de «una o dos frases, sin markdown» del modo voz.
pub const CHAT_MODE_PROMPT: &str = "# Modo chat\n\
El usuario te escribe y lee tu respuesta en una ventana de chat. Aquí puedes usar markdown \
(títulos, listas, **negrita**, tablas, bloques de código) y respuestas más largas cuando \
ayuden: resúmenes, comparativas, documentos, pasos. Empieza por lo importante y sé claro, \
sin relleno. Si la respuesta es larga, abre con una o dos frases que se entiendan solas.\n\
- Para enseñar un archivo o documento que creaste o encontraste, escribe en una línea sola \
`[Nombre](ruta completa)`; para una imagen local, `![descripción](ruta completa)`. Usa solo \
rutas reales que hayas visto en una herramienta; nunca las inventes.\n\
- No uses HTML. No pegues enlaces de imágenes de la web.";

pub const CHAT_MAX_TOKENS: i64 = 2400;
/// Con herramientas disponibles, la respuesta puede ser una llamada con
/// argumentos largos (el prompt de un agente). Con el tope de voz (~320) se
/// cortaba a mitad y quedaba JSON roto.
pub const TOOL_MAX_TOKENS: i64 = 1500;
/// Tope duro de mensajes en el historial; el límite real lo pone la ventana de
/// contexto.
pub const MAX_HISTORY_MESSAGES: usize = 60;

/// Base estable de 5 herramientas de uso real (`BASE_TOOLS` del runtime).
pub const BASE_TOOLS: [&str; 5] = [
    "voice_control",
    "web_search",
    "fetch_page",
    "read_file",
    "delegate_to_agent",
];
