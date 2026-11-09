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
PAD_TOP = 8
HEADER = 28
GAP_HEADER = 8
LABEL = 20
GROUP_GAP = 12
FOOTER = 44
PAD_BOTTOM = 12

ROW_SELECT = 36
ROW_SLIDER = 44
ROW_TOGGLE = 36
ROW_TEXT = 44
# El medidor ya no dibuja la línea de estado inferior: cabe en una fila
# compacta como el resto de controles.
ROW_METER = 44
ROW_ACTION = 44
ROW = {
    "select": ROW_SELECT,
    "slider": ROW_SLIDER,
    "toggle": ROW_TOGGLE,
    "text": ROW_TEXT,
    "meter": ROW_METER,
    "action": ROW_ACTION,
}

# Orden por tarea: cómo se ve, cómo te escucha, cuándo cierra tu turno, cómo
# conversa, cómo habla, cómo transcribe y con qué modelo piensa. La etiqueta de
# sección va fuera del grupo.
GROUPS: tuple[tuple[str, tuple[tuple[str, str], ...]], ...] = (
    (
        "appearance",
        (
            ("ui_layout", "select"),
            ("theme", "select"),
            ("ui_motion", "select"),
        ),
    ),
    (
        "listening",
        (
            ("close_action", "select"),
            ("voice_wake", "toggle"),
            ("wake_phrase", "text"),
        ),
    ),
    (
        "audio",
        (
            ("input_device_index", "select"),
            ("input_meter", "meter"),
            ("noise_multiplier", "slider"),
        ),
    ),
    (
        # Cuándo se da por terminada tu frase.
        "vad",
        (
            ("silence_secs", "slider"),
            ("smart_turn", "toggle"),
            ("turn_threshold", "slider"),
            ("pre_roll_secs", "slider"),
            ("min_speech_secs", "slider"),
            ("max_utterance_secs", "slider"),
        ),
    ),
    (
        "conversation",
        (
            ("barge_in", "toggle"),
            ("barge_in_rms", "slider"),
            ("confirm_actions", "toggle"),
            ("test_voice", "action"),
            ("repeat_last", "action"),
            ("new_conversation", "action"),
        ),
    ),
    (
        "tts",
        (
            ("voice", "select"),
            ("tts_model_variant", "select"),
            ("speed", "slider"),
            ("tts_sentence_pause", "slider"),
            ("tts_clause_pause", "slider"),
        ),
    ),
    (
        "stt",
        (
            ("stt_engine", "select"),
            ("stt_model", "select"),
            ("stt_prompt", "text"),
            ("stt_timeout_secs", "slider"),
            ("auto_start_whisper", "toggle"),
        ),
    ),
    (
        "llm",
        (
            # Proveedor -> cómo se conecta -> qué modelo -> cómo responde.
            ("llm_profile", "select"),
            ("llm_provider", "text"),
            ("llm_api_style", "select"),
            ("llm_base_url", "text"),
            ("api_key", "text"),
            ("llm_model", "text"),
            ("llm_model_pick", "select"),
            ("llm_temperature", "slider"),
            ("llm_max_tokens", "slider"),
            ("llm_reasoning_effort", "select"),
        ),
    ),
    (
        "agent",
        (
            ("open_system_md", "action"),
            ("open_voice_json", "action"),
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
        ),
    ),
)

GROUP_LABELS = {
    "appearance": "APARIENCIA",
    "listening": "ESCUCHA",
    "audio": "MICRÓFONO",
    "vad": "FIN DE TURNO",
    "conversation": "CONVERSACIÓN",
    "tts": "VOZ · KOKORO",
    "stt": "TRANSCRIPCIÓN · WHISPER",
    "llm": "LLM · API",
    "agent": "AGENTE",
    "hotkeys": "ATAJOS",
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

    for group_name, rows in GROUPS:
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
        visible_bottom = float(viewport_h) - PAD_BOTTOM
        footer_bottom = min(footer_bottom, visible_bottom)
        footer_top = footer_bottom - FOOTER
    add("footer", footer_top, footer_bottom)
    y = footer_bottom + PAD_BOTTOM
    add("total", 0.0, y)
    return out


def visible_menu_height(screen_h: float) -> float:
    """Alto visible del menú: min(560, 0,7 · alto de pantalla − 36)."""
    return min(float(MENU_HEIGHT), max(0.0, 0.7 * float(screen_h) - ISLAND_H))

