"""Layout vertical del menú del notch.

Implementa la tabla 7.3 de la especificación. Todas las medidas están en
píxeles lógicos a 96 DPI. El layout devuelve rectángulos en coordenadas de
ventana: y = 0 es el borde superior de la isla.

Las claves de fila son las claves reales de `voice.json` (RG-3). El menú
cubre audio, VAD, voz/TTS, Whisper/STT, LLM/API, conversación y atajos; el
contenido que no cabe en el alto visible se desplaza con la rueda.
`test_voice`, `llm_profile` y `llm_model_pick` son controles del menú, no claves
persistidas.
"""

from __future__ import annotations

from dataclasses import dataclass

from .tokens import H_ISLAND, H_MENU

ISLAND_H = H_ISLAND
PAD_TOP = 10
HEADER = 32
GAP_HEADER = 10
LABEL = 22
GROUP_GAP = 14
FOOTER = 48
PAD_BOTTOM = 10

# Rejilla de filas: 44 (una línea) y 52 (dos líneas: etiqueta y control). Todos
# los controles comparten los mismos márgenes interiores.
ROW_SELECT = 44
ROW_SLIDER = 52
ROW_TOGGLE = 44
ROW_TEXT = 52
ROW_METER = 52
ROW_ACTION = 44
# Vista de las paletas (Claro, Oscuro, Personalizado): tres muestras con nombre.
ROW_PALETTE = 68
ROW = {
    "select": ROW_SELECT,
    "slider": ROW_SLIDER,
    "toggle": ROW_TOGGLE,
    "text": ROW_TEXT,
    "meter": ROW_METER,
    "action": ROW_ACTION,
    "palette": ROW_PALETTE,
}

# Grupos de ajustes. Cada grupo es una tarjeta con su etiqueta; varias tarjetas
# forman una sección de la barra lateral (ver ``SECTIONS``). Las filas de
# ``ADVANCED_ROWS`` solo se ven con «Opciones avanzadas» y las de
# ``CUSTOM_THEME_ROWS`` solo con el tema «Personalizado».
GROUPS: tuple[tuple[str, tuple[tuple[str, str], ...]], ...] = (
    (
        "look",
        (
            ("ui_layout", "select"),
            ("ui_scale", "slider"),
            ("ui_motion", "select"),
        ),
    ),
    (
        "theme",
        (
            ("theme", "select"),
            ("theme_preview", "palette"),
            # Solo con el tema «Personalizado» (el menú las oculta si no):
            ("theme_base", "select"),
            ("theme_preset", "select"),
            ("theme_primary", "text"),
            ("theme_secondary", "text"),
            ("theme_accent", "text"),
            ("theme_background", "text"),
        ),
    ),
    (
        "mic",
        (
            ("input_device_index", "select"),
            ("input_meter", "meter"),
            ("noise_multiplier", "slider"),
        ),
    ),
    (
        # Cuándo se da por terminada tu frase.
        "turn",
        (
            ("silence_secs", "slider"),
            ("smart_turn", "toggle"),
        ),
    ),
    (
        "wake",
        (
            ("voice_wake", "toggle"),
            ("wake_phrase", "text"),
            ("close_action", "select"),
        ),
    ),
    (
        "speech",
        (
            ("voice", "select"),
            ("speed", "slider"),
        ),
    ),
    (
        "dialog",
        (
            ("barge_in", "toggle"),
            ("confirm_actions", "toggle"),
            ("chat_voice", "toggle"),
        ),
    ),
    (
        "actions",
        (
            ("test_voice", "action"),
            ("repeat_last", "action"),
            ("new_conversation", "action"),
        ),
    ),
    (
        "stt",
        (
            ("stt_model", "select"),
            ("auto_start_whisper", "toggle"),
        ),
    ),
    (
        "llm",
        (
            # Proveedor -> qué modelo -> cómo se conecta -> cómo responde.
            ("llm_profile", "select"),
            ("llm_model", "text"),
            ("api_key", "text"),
        ),
    ),
    (
        "hotkeys",
        (
            ("hotkey", "text"),
            ("cancel_hotkey", "text"),
            ("repeat_hotkey", "text"),
            ("mute_hotkey", "text"),
            ("menu_hotkey", "text"),
            ("chat_hotkey", "text"),
        ),
    ),
    (
        "advanced",
        (("show_advanced", "toggle"),),
    ),
    (
        # Ajustes finos: viven aquí, justo debajo del interruptor, para que al
        # activarlo se vea lo que aparece. Con el interruptor apagado el menú
        # oculta todas sus filas y, con ellas, los cuatro grupos.
        "adv_listen",
        (
            ("turn_threshold", "slider"),
            ("pre_roll_secs", "slider"),
            ("min_speech_secs", "slider"),
            ("max_utterance_secs", "slider"),
        ),
    ),
    (
        "adv_voice",
        (
            ("tts_model_variant", "select"),
            ("tts_sentence_pause", "slider"),
            ("tts_clause_pause", "slider"),
            ("barge_in_rms", "slider"),
        ),
    ),
    (
        "adv_stt",
        (
            ("stt_engine", "select"),
            ("stt_prompt", "text"),
            ("stt_timeout_secs", "slider"),
        ),
    ),
    (
        # Conexión con el proveedor y muestreo del modelo.
        "adv_llm",
        (
            ("llm_provider", "text"),
            ("llm_api_style", "select"),
            ("llm_base_url", "text"),
            ("llm_model_pick", "select"),
            ("llm_temperature", "slider"),
            ("llm_max_tokens", "slider"),
            ("llm_context_tokens", "slider"),
            ("llm_reasoning_effort", "select"),
        ),
    ),
    (
        # Los archivos que la app lee de verdad, en %APPDATA%\EzAgent.
        "files",
        (
            ("open_config_dir", "action"),
            ("open_system_md", "action"),
            ("open_permissions", "action"),
            ("open_voice_json", "action"),
        ),
    ),
)

# Filas que solo se ven con el tema personalizado.
CUSTOM_THEME_ROWS = frozenset(
    {
        "theme_base",
        "theme_preset",
        "theme_primary",
        "theme_secondary",
        "theme_accent",
        "theme_background",
    }
)

# Filas poco habituales: ajustes finos de VAD, pausas del TTS, motor de Whisper,
# conexión a la API y muestreo del modelo. Se ocultan hasta que el usuario pide
# verlas (``show_advanced``); sus valores siguen aplicándose igual.
ADVANCED_ROWS = frozenset(
    {
        "turn_threshold",
        "pre_roll_secs",
        "min_speech_secs",
        "max_utterance_secs",
        "tts_model_variant",
        "tts_sentence_pause",
        "tts_clause_pause",
        "barge_in_rms",
        "stt_engine",
        "stt_prompt",
        "stt_timeout_secs",
        "llm_provider",
        "llm_api_style",
        "llm_base_url",
        "llm_model_pick",
        "llm_temperature",
        "llm_max_tokens",
        "llm_context_tokens",
        "llm_reasoning_effort",
    }
)

# Secciones de la barra lateral: ``(id, título, grupos)``. Cinco, por tarea:
# cómo se ve, cómo te escucha, cómo habla, con qué piensa y el sistema.
SECTIONS: tuple[tuple[str, str, tuple[str, ...]], ...] = (
    ("appearance", "Aspecto", ("look", "theme")),
    ("listening", "Escucha", ("mic", "turn", "wake")),
    ("voice", "Voz", ("speech", "dialog", "actions")),
    ("models", "Modelos", ("stt", "llm")),
    (
        "system",
        "Sistema",
        ("hotkeys", "advanced", "adv_listen", "adv_voice", "adv_stt", "adv_llm", "files"),
    ),
)

# Orden y títulos de la barra lateral de la ventana de ajustes.
SIDEBAR_ORDER = tuple(name for name, _title, _groups in SECTIONS)
SECTION_TITLES = {name: title for name, title, _groups in SECTIONS}
SECTION_GROUPS = {name: groups for name, _title, groups in SECTIONS}

GROUP_LABELS = {
    "look": "FORMA Y TAMAÑO",
    "theme": "TEMA",
    "mic": "MICRÓFONO",
    "turn": "FIN DE TURNO",
    "wake": "REACTIVACIÓN",
    "speech": "VOZ · KOKORO",
    "dialog": "CONVERSACIÓN",
    "actions": "ACCIONES",
    "stt": "TRANSCRIPCIÓN · WHISPER",
    "llm": "MODELO DE IA",
    "hotkeys": "ATAJOS",
    "advanced": "AVANZADO",
    "adv_listen": "ESCUCHA · AFINADO",
    "adv_voice": "VOZ · AFINADO",
    "adv_stt": "TRANSCRIPCIÓN · AFINADO",
    "adv_llm": "MODELO DE IA · CONEXIÓN",
    "files": "ARCHIVOS",
}

MENU_HEIGHT = H_MENU


@dataclass(frozen=True)
class Rect:
    x0: float
    y0: float
    x1: float
    y1: float

    @property
    def width(self) -> float:
        return self.x1 - self.x0

    @property
    def height(self) -> float:
        return self.y1 - self.y0

    @property
    def center_y(self) -> float:
        return (self.y0 + self.y1) / 2.0


def layout_menu(
    extra: dict[str, float] | None = None,
    viewport_h: float | None = None,
    groups: tuple[tuple[str, tuple[tuple[str, str], ...]], ...] | None = None,
) -> dict[str, Rect]:
    """Devuelve el layout de la tabla 7.3.

    ``extra`` añade altura a una fila (por ejemplo, un selector desplegado:
    ``n * 32``). ``viewport_h`` es el alto visible del menú; si es menor que
    ``MENU_HEIGHT``, el pie se sube y el contenido entre la cabecera y el pie
    se convierte en zona con scroll (C-11). Sin ``viewport_h``, el resultado
    coincide exactamente con la tabla 7.3.
    """
    extra = extra or {}
    y = float(ISLAND_H)
    out: dict[str, Rect] = {}

    def add(key: str, y0: float, y1: float) -> None:
        out[key] = Rect(0.0, y0, 0.0, y1)

    y += PAD_TOP
    add("header", y, y + HEADER)
    y += HEADER + GAP_HEADER

    for group_name, rows in groups or GROUPS:
        add("label_" + group_name, y, y + LABEL)
        y += LABEL
        group_top = y
        for key, kind in rows:
            height = ROW[kind] + float(extra.get(key, 0.0))
            out[key] = Rect(0.0, y, 0.0, y + height)
            y += height
        add("group_" + group_name, group_top, y)
        y += GROUP_GAP

    footer_top = y
    footer_bottom = y + FOOTER
    if viewport_h is not None:
        # El pie va siempre al fondo de lo visible: sube si el contenido no
        # cabe y baja si sobra sitio (la ventana de ajustes tiene alto fijo).
        footer_bottom = float(viewport_h) - PAD_BOTTOM
        footer_top = footer_bottom - FOOTER
    add("footer", footer_top, footer_bottom)
    y = footer_bottom + PAD_BOTTOM
    add("total", 0.0, y)
    return out


def visible_menu_height(screen_h: float) -> float:
    """Alto visible del menú: min(560, 0,7 · alto de pantalla − 36)."""
    return min(float(MENU_HEIGHT), max(0.0, 0.7 * float(screen_h) - ISLAND_H))
