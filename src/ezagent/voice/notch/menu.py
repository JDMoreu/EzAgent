"""Menú desplegable del notch de EzAgent.

El menú vive en la **misma ventana layered** que la barra y dibuja solo su
contenido sobre el buffer float32 RGBA premultiplicado que ya trae la silueta.
No crea ventanas, no toca Tk/Win32, no escribe archivos y no reproduce audio:
las decisiones se notifican a la capa de aplicación mediante callbacks.

Coordenadas y layout
--------------------
``y = 0`` es el borde superior de la barra; el cuerpo del menú empieza en
``y = 36``. El layout vertical sale de :mod:`menu_layout`:

- cabecera 44-72, AUDIO 100-268, VOZ 300-380, CONVERSACIÓN 412-528, TTS
  560-772, VAD 804-980, WHISPER · STT 1012-1224 y LLM · API 1256-1556.
- el pie visible queda en 540-584 (con ``H_MENU = 560``) y el contenido que no
  cabe se desplaza con rueda; el pulgar queda visible a la derecha.
- grupos x 19-301, filas x 31-289, etiquetas x 23, contenido termina en x 301.
- si la pantalla es baja, el pie se sube y la zona 72-pie se vuelve desplazable.

API pública
-----------
``Menu(settings, *, scale, theme, screen_h, on_live_change, on_save, on_test,
on_hide, on_close, on_text_edit)``
    ``settings`` es un :class:`settings.MenuSettings`. ``screen_h`` se expresa
    en píxeles lógicos y decide el alto máximo visible:
    ``min(560, 0.7 * screen_h - 36)``.

``open(now)`` / ``close(now)`` / ``toggle(now)``
    Abren o cierran con tween de altura (320 ms ``EASE_OUT`` al abrir, 260 ms
    ``EASE_IN`` al cerrar). El contenido entra desplazado +6 px y escalonado
    por bloques; al cerrar se desvanece en 120 ms. ``now`` va en segundos y
    debe ser monótono (``time.monotonic()``).

``update(now)``
    Avanza todos los tweens. La ventana debe llamarlo una vez por fotograma;
    ``draw()`` también lo hace por comodidad.

``draw(buf, now)``
    ``buf`` es ``(H, W, 4)`` float32 premultiplicado 0..1, con el tamaño
    físico ``(36 + height) * scale``. Dibuja cabecera, grupos, filas,
    selectores, sliders, interruptores, botones, pie, scroll y degradados.

``height(now)``
    Alto actual del **cuerpo del menú** (0..560), sin la barra.

``is_open()`` / ``is_animating(now)``
    ``is_open`` es la intención lógica (False en cuanto se pide cerrar).
    ``is_animating`` incluye altura, contenido, selector, hover, pulsaciones,
    thumbs, interruptores y scroll.

``hit(x, y, now)``
    Devuelve un :class:`MenuHit` con ``kind``, ``key``, ``rect``, ``value`` e
    ``index``. Coordenadas lógicas de ventana. Por defecto no devuelve
    controles deshabilitados; ``include_disabled=True`` permite inspeccionarlos.
    ``kind`` es uno de ``hide``, ``select``, ``option``, ``slider``, ``toggle``,
    ``text``, ``action`` o ``save``.

``begin_text_edit(key)`` / ``text_input(char)`` / ``text_key(vk)`` /
``finish_text_edit(commit)``
    Edición de las filas de texto. La barra pide el teclado temporalmente al
    empezar y lo devuelve al confirmar con Enter, cancelar con Esc o pulsar
    fuera. ``on_text_edit(active)`` informa del cambio de foco.

``pointer_down/move/up(x, y, now)`` y ``wheel(x, y, delta, now)``
    Implementan el ratón de la especificación. El clic se confirma al soltar
    sobre el mismo control; el slider salta al pulsar, arrastra con emisión
    limitada a 50 ms y emite el valor final al soltar.

``set_testing(bool, now=None)``
    La aplicación lo llama al empezar y al terminar la prueba de voz.

``on_save(values) -> bool | None``
    Recibe :meth:`MenuSettings.snapshot`. ``False`` significa fallo inmediato;
    ``True``/``None`` muestran la confirmación optimista. Si el guardado real
    es asíncrono, la aplicación puede corregir el resultado con
    ``on_applied(ok)``; un fallo restaura la copia guardada anterior y deja el
    menú en estado *dirty*.

El menú nunca modifica ``voice.json`` ni el motor de audio: solo llama a los
callbacks. Los textos y comentarios están en español; la interfaz pública en
inglés, como el resto del proyecto.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from functools import lru_cache
from typing import Any, Callable, Final, Literal

import numpy as np

from . import hotkey_field as hk
from .anim import EASE_IN, EASE_OUT, EASE_SPRING, Tween, lerp_rgb
from .icons import draw_icon
from .menu_layout import (
    ADVANCED_ROWS,
    CUSTOM_THEME_ROWS,
    GAP_HEADER,
    GROUP_LABELS,
    GROUPS,
    ROW,
    layout_menu,
    visible_menu_height,
)
from .menu_layout import (
    Rect as LayoutRect,
)
from .settings import (
    ALL_KEYS,
    COLOR_KEYS,
    LABELS,
    METER_DB_CEIL,
    METER_DB_FLOOR,
    SECRET_KEYS,
    SELECT_KEYS,
    SLIDER_KEYS,
    SLIDERS,
    TEXT_KEYS,
    TOGGLE_KEYS,
    MenuSettings,
    SelectOption,
    db_to_rms,
)
from .text import draw_text, text_size
from .tokens import (
    AA_RAMP,
    BODY_X0,
    BODY_X1,
    FS_BODY,
    FS_MICRO,
    FS_ROW,
    FS_SECONDARY,
    FS_TITLE,
    H_ISLAND,
    R_GROUP,
    T_CLOSE_RETRACT,
    T_MICRO,
    T_SLOW,
    Color,
    FontStyle,
    Rect,
    W,
    palette,
)

# --------------------------------------------------------------------------
# Coordenadas fijas de la sección 7.2. menu_layout expone las alturas, pero no
# estos márgenes horizontales; se nombran aquí para no dejar números sueltos.
# --------------------------------------------------------------------------

GROUP_X0: Final[float] = 19.0  # contenedor de grupo (7.2)
GROUP_X1: Final[float] = 301.0
ROW_X0: Final[float] = 31.0  # contenido de fila (7.2)
ROW_X1: Final[float] = 289.0
LABEL_X: Final[float] = 23.0  # etiquetas de sección y cabecera

# C-04 · selector en línea y lista desplegada.
SELECT_VALUE_X1: Final[float] = 267.0
SELECT_VALUE_MAX_W: Final[float] = 130.0
CHEVRON_CX: Final[float] = 281.0
CHEVRON_SIZE: Final[float] = 16.0
LIST_ITEM_H: Final[float] = 32.0
LIST_MAX_VISIBLE: Final[int] = 5
LIST_VIEW_H: Final[float] = LIST_ITEM_H * LIST_MAX_VISIBLE
LIST_TEXT_X: Final[float] = 43.0
LIST_TEXT_MAX_W: Final[float] = 222.0
LIST_CHECK_CX: Final[float] = 281.0
LIST_CHECK_SIZE: Final[float] = 16.0
SELECTOR_OPEN_MS: Final[float] = 200.0
SELECTOR_CLOSE_MS: Final[float] = 140.0
SELECTOR_ITEM_FADE_MS: Final[float] = 120.0
SELECTOR_ITEM_STAGGER_MS: Final[float] = 20.0
SELECTOR_START_DELAY_MS: Final[float] = 20.0
CHEVRON_MS: Final[float] = 180.0

# C-05 · slider. Medidas 4 / 14 / 16 / 18 de la especificación.
SLIDER_TRACK_X0: Final[float] = ROW_X0
SLIDER_TRACK_X1: Final[float] = ROW_X1
SLIDER_TRACK_H: Final[float] = 4.0
SLIDER_THUMB_REST: Final[float] = 14.0
SLIDER_THUMB_HOVER: Final[float] = 16.0
SLIDER_THUMB_DRAG: Final[float] = 18.0
SLIDER_TRACK_PAD: Final[float] = 10.0
SLIDER_HOVER_MS: Final[float] = 120.0
SLIDER_DRAG_MS: Final[float] = 80.0
SLIDER_RELEASE_MS: Final[float] = 180.0
SLIDER_EMIT_MS: Final[float] = 50.0
SLIDER_LINE1_TOP: Final[float] = 9.0
SLIDER_LINE1_H: Final[float] = 16.0
SLIDER_LINE2_CY: Final[float] = 38.0

# Fila de texto editable: etiqueta arriba y valor en la segunda línea.
TEXT_LINE1_CY: Final[float] = 17.0
TEXT_LINE2_CY: Final[float] = 37.0
TEXT_CARET_W: Final[float] = 1.5
TEXT_CARET_H: Final[float] = 16.0

# Clave API: nunca se pinta; un botón «copiar» a la derecha de la línea de valor.
COPY_BTN: Final[float] = 26.0  # diámetro del botón (zona de clic y fondo de hover)
COPY_ICON: Final[float] = 18.0
COPIED_SECS: Final[float] = 1.6  # el icono muestra un check este tiempo tras copiar
DOT_R: Final[float] = 2.4
DOT_PITCH: Final[float] = 8.0
DOT_MAX: Final[int] = 22

# Atajos: teclas dibujadas (keycaps) y campo de captura.
KEYCAP_H: Final[float] = 22.0
KEYCAP_PAD: Final[float] = 8.0
KEYCAP_GAP: Final[float] = 4.0
CAPTURE_MSG_SECS: Final[float] = 2.6

# Medidor de entrada: barras horizontales, nivel en vivo y marca de umbral
# arrastrable. La escala dBFS (``settings.METER_DB_*``) es también el rango del
# umbral: toda la pista se puede arrastrar.
METER_TRACK_Y: Final[float] = 36.0
METER_TRACK_H: Final[float] = 4.0
METER_TRACK_PAD: Final[float] = 10.0
METER_THUMB: Final[float] = 14.0

# C-06 · interruptor.
TOGGLE_X0: Final[float] = 253.0
TOGGLE_W: Final[float] = 36.0
TOGGLE_H: Final[float] = 20.0
TOGGLE_THUMB: Final[float] = 16.0
TOGGLE_PAD: Final[float] = 2.0
TOGGLE_MS: Final[float] = 180.0

# C-07 · botón capsular.
BUTTON_H: Final[float] = 28.0
BUTTON_MIN_W: Final[float] = 72.0
BUTTON_PAD_X: Final[float] = 14.0
BUTTON_RADIUS: Final[float] = BUTTON_H / 2.0
BUTTON_HOVER_MIX: Final[float] = 0.08
BUTTON_PRESS_SCALE: Final[float] = 0.96
BUTTON_PRESS_MS: Final[float] = 60.0
BUTTON_RELEASE_MS: Final[float] = 120.0

# C-07 · variantes de color. La primaria no se usa en este menú, pero forma
# parte del catálogo y queda disponible para futuros botones.
BUTTON_VARIANTS: Final[dict[str, tuple[str, str]]] = {
    "primary": ("MINT_BTN", "MINT_INK"),
    "secondary": ("TRACK", "TEXT"),
    "amber": ("WARM_BTN", "WARM_INK"),
    "disabled": ("TRACK", "TEXT_3"),
}

# C-08 · prueba de voz. Mini onda de 5 barras de 2 px con 2 px de hueco.
TEST_BUTTON_X1: Final[float] = ROW_X1
TEST_WAVE_BARS: Final[int] = 5
TEST_WAVE_BAR_W: Final[float] = 2.0
TEST_WAVE_GAP: Final[float] = 2.0
TEST_WAVE_MIN_H: Final[float] = 3.0
TEST_WAVE_MAX_H: Final[float] = 10.0
TEST_WAVE_GAP_TEXT: Final[float] = 6.0
TEST_FADE_MS: Final[float] = 120.0
TEST_WAVE_LEVEL_BASE: Final[float] = 0.35
TEST_WAVE_LEVEL_SPAN: Final[float] = 0.65
TEST_WAVE_PHASE_SPEED: Final[float] = 9.0
TEST_WAVE_PHASE_STEP: Final[float] = 1.3

# C-09 · cabecera.
HIDE_SIZE: Final[float] = 24.0
HIDE_CX: Final[float] = 289.0
HIDE_ICON_SIZE: Final[float] = 16.0
HIDE_ICON_PRESS_SCALE: Final[float] = 0.92
HIDE_HOVER_DARK: Final[float] = 0.08
HIDE_PRESS_DARK: Final[float] = 0.14
HIDE_HOVER_LIGHT: Final[float] = 0.06
HIDE_PRESS_LIGHT: Final[float] = 0.10

# C-03 · opacidad de nombres y valores en filas deshabilitadas.
DISABLED_OPACITY: Final[float] = 0.40

# Grosor de las hairlines de fila y del anillo claro.
HAIRLINE_PX: Final[float] = 1.0

# C-10 · pie.
FOOTER_TEXT_X: Final[float] = 23.0
FOOTER_DOT: Final[float] = 6.0
FOOTER_DOT_GAP: Final[float] = 6.0
SAVE_BUTTON_X1: Final[float] = GROUP_X1
SAVE_BUTTON_X0: Final[float] = GROUP_X1 - 72.0
SAVE_CHECK_MS: Final[float] = 600.0
SAVE_FLASH_MS: Final[float] = 800.0
SAVE_ERROR_MS: Final[float] = 2000.0
SAVE_FADE_MS: Final[float] = 120.0
SAVE_COLOR_MS: Final[float] = 180.0  # A-10: color del botón Guardar
FOOTER_FADE_MS: Final[float] = 180.0  # A-11: fundido cruzado del pie

# C-11 · scroll.
SCROLL_STEP: Final[float] = 40.0
SCROLL_TWEEN_MS: Final[float] = 120.0
SCROLL_INDICATOR_W: Final[float] = 3.0
SCROLL_INDICATOR_X: Final[float] = 304.0
SCROLL_INDICATOR_ALPHA: Final[float] = 0.40
SCROLL_INDICATOR_IDLE_ALPHA: Final[float] = 0.12
SCROLL_INDICATOR_MIN_H: Final[float] = 24.0
SCROLL_INDICATOR_HOLD_MS: Final[float] = 400.0
SCROLL_INDICATOR_FADE_MS: Final[float] = 400.0
SCROLL_GRADIENT_H: Final[float] = 12.0

# A-04 / A-05 · entrada y salida del contenido.
BLOCK_FADE_MS: Final[float] = 180.0
BLOCK_OFFSET: Final[float] = 6.0
# Entrada escalonada: cabecera, cada grupo en el orden de GROUPS (40 ms entre
# uno y otro) y el pie al final. Se deriva de GROUPS para que un grupo nuevo
# no rompa la coreografía.
BLOCK_DELAYS_MS: Final[dict[str, float]] = {
    "header": 60.0,
    **{name: 100.0 + 40.0 * i for i, (name, _rows) in enumerate(GROUPS)},
    "footer": 100.0 + 40.0 * len(GROUPS),
}
CONTENT_CLOSE_MS: Final[float] = T_MICRO
CLOSE_HEIGHT_DELAY_MS: Final[float] = 60.0  # A-05: la retracción empieza tras el fundido

# Punto de actividad del scroll: 400 ms visible y 400 ms de fundido.
_INDICATOR_SHOW_MS: Final[float] = 120.0

# Los valores numéricos usan la métrica exacta de FS_MICRO pero sin la
# transformación a mayúsculas del token: así «1,6 s» conserva la unidad en
# minúscula que fija el formato de la sección 7.4.
FS_VALUE: Final[FontStyle] = FontStyle(
    "FS_VALUE",
    FS_MICRO.family,
    FS_MICRO.size,
    FS_MICRO.weight,
    FS_MICRO.tracking_em,
    uppercase=False,
)

# Acciones que no son claves persistidas: prueba de voz, abrir archivos y las
# de la conversación (repetir lo último, empezar de cero).
ACTION_KEYS: Final[tuple[str, ...]] = (
    "test_voice",
    "open_config_dir",
    "open_system_md",
    "open_permissions",
    "open_voice_json",
    "repeat_last",
    "new_conversation",
)
ACTION_BUTTON_TEXT: Final[dict[str, str]] = {
    "repeat_last": "Repetir",
    "new_conversation": "Borrar",
}
# Acciones destructivas: el primer clic arma el botón («¿Seguro?») y solo un
# segundo clic dentro de ARM_SECS la ejecuta. El texto lo pone el código.
CONFIRM_ACTIONS: Final[tuple[str, ...]] = ("new_conversation",)
ARM_SECS: Final[float] = 3.0
ARMED_TEXT: Final[str] = "¿Seguro?"


@dataclass(frozen=True, slots=True)
class MenuHit:
    """Resultado de ``Menu.hit`` en coordenadas lógicas de ventana."""

    kind: Literal[
        "hide",
        "select",
        "option",
        "slider",
        "toggle",
        "text",
        "meter",
        "action",
        "copy",
        "save",
    ]
    key: str
    rect: Rect
    value: Any = None
    index: int | None = None
    disabled: bool = False


# --------------------------------------------------------------------------
# Rasterizado de formas con SDF y anti-alias de 1 px lógico
# --------------------------------------------------------------------------


def _color_rgb(color: Color) -> np.ndarray:
    return np.asarray(color[:3], dtype=np.float32) / 255.0


def _color_alpha(color: Color) -> float:
    return (color[3] / 255.0) if len(color) == 4 else 1.0


def _paint_mask(
    layer: np.ndarray,
    py0: int,
    py1: int,
    px0: int,
    px1: int,
    coverage: np.ndarray,
    color: Color,
    alpha: float,
    *,
    replace: bool = False,
) -> None:
    """Compone una máscara premultiplicada sobre ``layer``.

    ``replace`` se usa cuando la región de destino todavía está vacía (fondo
    de grupo): escribe en vez de mezclar y evita la mitad de las operaciones.
    """
    if coverage.size == 0 or alpha <= 0.0:
        return
    factor = alpha * _color_alpha(color)
    a = coverage if factor >= 1.0 else coverage * factor
    rgb = _color_rgb(color)
    region = layer[py0:py1, px0:px1]
    if replace:
        region[..., :3] = rgb[None, None, :] * a[..., None]
        region[..., 3] = a
        return
    one = 1.0 - a
    region[..., :3] *= one[..., None]
    region[..., :3] += rgb[None, None, :] * a[..., None]
    region[..., 3] *= one
    region[..., 3] += a


_MASK_CACHE: dict[tuple, tuple[int, int, int, int, np.ndarray]] = {}
_MASK_LIMIT: Final[int] = 384


def _round_mask(
    shape: tuple[int, int],
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    radius_top: float,
    radius_bottom: float,
    scale: float,
    *,
    stroke: bool,
) -> tuple[int, int, int, int, np.ndarray] | None:
    """Caché de coberturas SDF por geometría (tema y color aparte)."""
    if x1 <= x0 or y1 <= y0:
        return None
    key = (
        shape[0],
        shape[1],
        round(x0, 3),
        round(y0, 3),
        round(x1, 3),
        round(y1, 3),
        round(radius_top, 3),
        round(radius_bottom, 3),
        round(scale, 4),
        stroke,
    )
    cached = _MASK_CACHE.get(key)
    if cached is not None:
        return cached
    dst_h, dst_w = shape
    pad = 1.5
    px0 = max(0, int(math.floor((x0 - pad) * scale)))
    px1 = min(dst_w, int(math.ceil((x1 + pad) * scale)))
    py0 = max(0, int(math.floor((y0 - pad) * scale)))
    py1 = min(dst_h, int(math.ceil((y1 + pad) * scale)))
    if px1 <= px0 or py1 <= py0:
        return None
    xs = (np.arange(px0, px1, dtype=np.float32) + 0.5) / scale
    ys = (np.arange(py0, py1, dtype=np.float32) + 0.5) / scale
    xx = xs[None, :]
    yy = ys[:, None]
    cx = (x0 + x1) / 2.0
    cy = (y0 + y1) / 2.0
    hw = (x1 - x0) / 2.0
    hh = (y1 - y0) / 2.0
    rr_top = max(0.0, min(float(radius_top), hw, hh))
    rr_bottom = max(0.0, min(float(radius_bottom), hw, hh))
    radius = np.where(yy < cy, rr_top, rr_bottom)
    qx = np.abs(xx - cx) - (hw - radius)
    qy = np.abs(yy - cy) - (hh - radius)
    distance = (
        np.sqrt(np.maximum(qx, 0.0) ** 2 + np.maximum(qy, 0.0) ** 2)
        + np.minimum(np.maximum(qx, qy), 0.0)
        - radius
    )
    if stroke:
        coverage = np.clip(0.5 - (np.abs(distance) - 0.5) / AA_RAMP, 0.0, 1.0)
    else:
        coverage = np.clip(0.5 - distance / AA_RAMP, 0.0, 1.0)
    result = (py0, py1, px0, px1, coverage)
    if len(_MASK_CACHE) >= _MASK_LIMIT:
        _MASK_CACHE.pop(next(iter(_MASK_CACHE)))
    _MASK_CACHE[key] = result
    return result


def _fill_round_rect(
    layer: np.ndarray,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    radius: float,
    color: Color,
    scale: float,
    alpha: float = 1.0,
    *,
    radius_top: float | None = None,
    radius_bottom: float | None = None,
    replace: bool = False,
) -> None:
    """Relleno anti-aliasado; permite radios distintos arriba y abajo."""
    top = radius if radius_top is None else radius_top
    bottom = radius if radius_bottom is None else radius_bottom
    data = _round_mask(
        layer.shape[:2],
        x0,
        y0,
        x1,
        y1,
        top,
        bottom,
        scale,
        stroke=False,
    )
    if data is None:
        return
    py0, py1, px0, px1, coverage = data
    _paint_mask(layer, py0, py1, px0, px1, coverage, color, alpha, replace=replace)


def _fill_round_gradient(
    layer: np.ndarray,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    radius: float,
    stops: np.ndarray,
    scale: float,
    alpha: float = 1.0,
) -> None:
    """Rectángulo redondeado con un degradado horizontal por ``stops`` (N, 3) 0..255."""
    data = _round_mask(layer.shape[:2], x0, y0, x1, y1, radius, radius, scale, stroke=False)
    if data is None:
        return
    py0, py1, px0, px1, coverage = data
    xs = (np.arange(px0, px1, dtype=np.float32) + 0.5) / scale
    t = np.clip((xs - x0) / max(1e-6, x1 - x0), 0.0, 1.0) * (len(stops) - 1)
    i0 = np.minimum(np.floor(t).astype(np.int32), len(stops) - 2)
    f = (t - i0)[:, None]
    rgb = (stops[i0] * (1.0 - f) + stops[i0 + 1] * f) / 255.0
    a = (coverage * alpha)[..., None]
    region = layer[py0:py1, px0:px1]
    region[..., :3] = rgb[None, :, :] * a + region[..., :3] * (1.0 - a)
    region[..., 3:4] = a + region[..., 3:4] * (1.0 - a)


def _stroke_round_rect(
    layer: np.ndarray,
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    radius: float,
    color: Color,
    scale: float,
    alpha: float = 1.0,
) -> None:
    """Anillo hairline de 1 px lógico sobre el borde del rectángulo."""
    data = _round_mask(
        layer.shape[:2],
        x0,
        y0,
        x1,
        y1,
        radius,
        radius,
        scale,
        stroke=True,
    )
    if data is None:
        return
    py0, py1, px0, px1, coverage = data
    _paint_mask(layer, py0, py1, px0, px1, coverage, color, alpha)


def _fill_circle(
    layer: np.ndarray,
    cx: float,
    cy: float,
    diameter: float,
    color: Color,
    scale: float,
    alpha: float = 1.0,
) -> None:
    radius = max(0.0, diameter / 2.0)
    _fill_round_rect(
        layer,
        cx - radius,
        cy - radius,
        cx + radius,
        cy + radius,
        radius,
        color,
        scale,
        alpha,
    )


def _fill_vertical_gradient(
    buf: np.ndarray,
    x0: float,
    x1: float,
    y0: float,
    y1: float,
    color: Color,
    scale: float,
    alpha_start: float,
    alpha_end: float,
) -> None:
    """Degradado vertical opaco->transparente usado en los bordes del scroll."""
    if y1 <= y0 or x1 <= x0:
        return
    dst_h, dst_w = buf.shape[:2]
    px0 = max(0, int(math.floor(x0 * scale)))
    px1 = min(dst_w, int(math.ceil(x1 * scale)))
    py0 = max(0, int(math.floor(y0 * scale)))
    py1 = min(dst_h, int(math.ceil(y1 * scale)))
    if px1 <= px0 or py1 <= py0:
        return
    rows = py1 - py0
    k = np.linspace(0.0, 1.0, rows, dtype=np.float32)
    a = (alpha_start + (alpha_end - alpha_start) * k)[:, None, None]
    region = buf[py0:py1, px0:px1]
    rgb = _color_rgb(color)
    region[..., :3] = rgb[None, None, :] * a + region[..., :3] * (1.0 - a)
    region[..., 3:4] = a + region[..., 3:4] * (1.0 - a)


def _composite_band(
    dst: np.ndarray,
    layer: np.ndarray,
    y0: float,
    y1: float,
    scale: float,
    x0: float = 0.0,
    x1: float | None = None,
) -> None:
    """Compone una banda horizontal premultiplicada de ``layer`` sobre ``dst``."""
    if y1 <= y0:
        return
    p0 = max(0, int(round(y0 * scale)))
    p1 = min(dst.shape[0], int(round(y1 * scale)))
    if p1 <= p0:
        return
    right = float(W) if x1 is None else x1
    q0 = max(0, int(math.floor(x0 * scale)))
    q1 = min(dst.shape[1], int(math.ceil(right * scale)))
    if q1 <= q0:
        return
    src = layer[p0:p1, q0:q1]
    region = dst[p0:p1, q0:q1]
    a = src[..., 3:4]
    one = 1.0 - a
    region[..., :3] *= one
    region[..., :3] += src[..., :3]
    region[..., 3] *= one[..., 0]
    region[..., 3] += src[..., 3]


@lru_cache(maxsize=512)
def _fit_text(text: str, style: FontStyle, max_width: float, scale: float) -> str:
    """Trunca con «…» usando el ancho real de la tinta."""
    if not text or max_width <= 0:
        return ""
    if text_size(text, style, scale)[0] <= max_width:
        return text
    ellipsis = "…"
    low, high = 0, len(text)
    while low < high:
        middle = (low + high + 1) // 2
        candidate = text[:middle].rstrip() + ellipsis
        if text_size(candidate, style, scale)[0] <= max_width:
            low = middle
        else:
            high = middle - 1
    return (text[:low].rstrip() + ellipsis) if low > 0 else ellipsis


LiveCallback = Callable[[str, Any], None]
SaveCallback = Callable[[dict[str, Any]], bool | None]
TestCallback = Callable[[dict[str, Any]], None]
VoidCallback = Callable[[], None]


class Menu:
    """Contenido interactivo del menú desplegable del notch."""

    def __init__(
        self,
        settings: MenuSettings,
        *,
        scale: float = 1.0,
        theme: str = "dark",
        screen_h: float = 1080.0,
        on_live_change: LiveCallback | None = None,
        on_save: SaveCallback | None = None,
        on_test: TestCallback | None = None,
        on_hide: VoidCallback | None = None,
        on_close: VoidCallback | None = None,
        on_text_edit: Callable[[bool], None] | None = None,
        on_action: Callable[[str], None] | None = None,
        on_copy: Callable[[str], bool] | None = None,
        on_hotkey_capture: Callable[[bool], None] | None = None,
    ) -> None:
        if not isinstance(settings, MenuSettings):
            raise TypeError("settings debe ser MenuSettings")
        if scale <= 0:
            raise ValueError("scale debe ser mayor que cero")
        self.settings = settings
        self._scale = float(scale)
        self._theme = theme if theme in ("auto", "dark", "light") else "dark"
        self._screen_h = max(1.0, float(screen_h))
        self._available_h: float | None = None
        # La ventana de ajustes muestra una sección cada vez, con su título y
        # más alto que la tarjeta; sin ventana se ven todos los grupos.
        # Ancho extra (px lógicos) a la derecha: la ventana de ajustes es más ancha
        # que la tarjeta. Lo anclado a la izquierda no se mueve; los valores,
        # interruptores, botones y bordes derechos se desplazan ``_dx``.
        self._dx = 0.0
        self._groups: tuple[tuple[str, tuple[tuple[str, str], ...]], ...] = GROUPS
        self._title = "AJUSTES"
        self._max_h: float | None = None

        self.on_live_change = on_live_change
        self.on_save = on_save
        self.on_test = on_test
        self.on_hide = on_hide
        self.on_close = on_close
        self.on_text_edit = on_text_edit
        self.on_action = on_action
        self.on_copy = on_copy
        # Captura de atajos: mientras dura, la app suelta los atajos globales para
        # que la ventana reciba la combinación en vez de que Windows la consuma.
        self.on_hotkey_capture = on_hotkey_capture
        self.modifier_source: Callable[[], frozenset[str]] = hk.os_modifiers
        self._capturing = False
        self._capture_mods: frozenset[str] = frozenset()
        self._capture_msg: tuple[str, float] | None = None
        # Botón «copiar» de la clave API: fila bajo el puntero y copias recientes.
        self._copy_hover: str | None = None
        self._copied: dict[str, float] = {}
        # Colores que sustituyen a la paleta de tokens (la barra usa los suyos).
        self._palette_overrides: Callable[[str], dict[str, Color]] | None = None
        # Acciones destructivas armadas por el primer clic: clave -> caduca en.
        self._armed: dict[str, float] = {}

        # Estado de apertura y contenido. La altura es la del cuerpo del menú,
        # sin la barra (0 .. min(560, 0.7 * pantalla - 36)).
        self._open = False
        self._closed_notified = True
        self._now = 0.0
        self._height = Tween(0.0)
        self._content_alpha = Tween(0.0)
        self._blocks: dict[str, Tween] = {name: Tween(0.0) for name in BLOCK_DELAYS_MS}

        # Selector abierto. Solo puede haber uno.
        self._selector_key: str | None = None
        self._selector_closing = False
        self._selector_extra = Tween(0.0)
        self._selector_item_start = 0.0
        self._list_scroll = 0.0
        self._list_scroll_target = 0.0
        self._chevron: dict[str, Tween] = {key: Tween(0.0) for key in SELECT_KEYS}

        # Hover, pulsación y arrastre.
        self._row_hover: dict[str, Tween] = {
            key: Tween(0.0) for key in (*ALL_KEYS, "input_meter", "theme_preview", *ACTION_KEYS)
        }
        self._button_hover: dict[str, Tween] = {
            "hide": Tween(0.0),
            "save": Tween(0.0),
            "test": Tween(0.0),
        }
        self._press: dict[str, Tween] = {
            "hide": Tween(0.0),
            "save": Tween(0.0),
            "test": Tween(0.0),
        }
        self._hover_id: str | None = None
        self._hover_option: int | None = None
        self._pointer: tuple[float, float] | None = None
        self._pressed_hit: MenuHit | None = None
        self._slider_drag: MenuHit | None = None
        self._slider_last_emit = -1e9
        self._slider_pending: tuple[str, Any] | None = None
        self._slider_changed = False

        # Apariencia dinámica de sliders e interruptores.
        self._thumb: dict[str, Tween] = {key: Tween(SLIDER_THUMB_REST) for key in SLIDER_KEYS}
        self._thumb_target: dict[str, float] = {key: SLIDER_THUMB_REST for key in SLIDER_KEYS}
        self._slider_color: dict[str, Tween] = {key: Tween(0.0) for key in SLIDER_KEYS}
        self._toggle_pos: dict[str, Tween] = {
            key: Tween(1.0 if settings.get(key) else 0.0) for key in TOGGLE_KEYS
        }
        self._toggle_color: dict[str, Tween] = {
            key: Tween(1.0 if settings.get(key) else 0.0) for key in TOGGLE_KEYS
        }

        # Edición de texto en línea. Solo una fila a la vez; la ventana
        # pide el teclado mientras dure y lo devuelve al confirmar.
        self._editing_key: str | None = None
        self._edit_buffer = ""
        self._edit_cursor = 0
        self._edit_select_all = False
        self._edit_t0 = 0.0

        # Medidor de entrada. El notch entrega el RMS del micrófono en cada
        # fotograma; el menú lo suaviza y pinta el umbral arrastrable.
        self._meter_target = 0.0
        self._meter_level = 0.0
        self._meter_muted = False
        self._meter_active = False
        self._meter_t = 0.0
        self._meter_drag = False
        self._meter_pending: tuple[str, Any] | None = None
        self._meter_last_emit = -1e9
        self._meter_changed = False

        # Scroll y suavizado.
        self._scroll = Tween(0.0)
        self._scroll_target = 0.0
        self._indicator_last = -1e9
        self._indicator_until = -1e9

        # Guardado, error y prueba de voz.
        self._check_until = 0.0
        self._saved_flash_until = 0.0
        self._error_until = 0.0
        self._save_backup: dict[str, Any] | None = None
        self._save_snapshot: dict[str, Any] | None = None
        self._saving_pending = False
        self._testing_t = Tween(0.0)

        # A-10: fundido cruzado del texto del pie y color del botón Guardar.
        self._footer_state: tuple[str, str, bool] | None = None
        self._footer_prev: tuple[str, str, bool] | None = None
        self._footer_state_at = -1e9
        self._save_variant: str | None = None
        self._save_prev_fill: Color | None = None
        self._save_prev_text: Color | None = None
        self._save_variant_at = -1e9

        self._reduced_motion = False

        # Capas intermedias reutilizadas entre fotogramas. Se dimensionan al
        # alto máximo del menú para que la animación de apertura no reasigne
        # memoria en cada frame.
        self._layer_shape: tuple[int, int, int] | None = None
        self._layers: tuple[np.ndarray, np.ndarray, np.ndarray] | None = None
        self._layer_signature: tuple | None = None

    # ------------------------------------------------------------------
    # Geometría y estado
    # ------------------------------------------------------------------
    @property
    def scale(self) -> float:
        return self._scale

    @property
    def theme(self) -> str:
        return self._theme

    @property
    def viewport_height(self) -> float:
        """Alto visible objetivo del cuerpo del menú, en px lógicos."""
        if self._max_h is not None:
            height = self._max_h
        else:
            height = visible_menu_height(self._screen_h)
        if self._available_h is not None:
            height = min(height, max(1.0, self._available_h))
        return max(1.0, height)

    def _shown_groups(self) -> tuple[tuple[str, tuple[tuple[str, str], ...]], ...]:
        """Grupos visibles: las filas del tema personalizado solo con «Personalizado»."""
        hidden: set[str] = set()
        if self.settings.get("theme") != "custom":
            hidden |= CUSTOM_THEME_ROWS
        if not self.settings.get("show_advanced"):
            hidden |= ADVANCED_ROWS
        shown = tuple(
            (name, tuple(row for row in rows if row[0] not in hidden))
            for name, rows in self._groups
        )
        return tuple(group for group in shown if group[1]) or shown

    def set_width(self, width: float | None) -> None:
        """Ancho lógico del menú (``None`` o menos de 320 = el de la tarjeta)."""
        self._dx = 0.0 if width is None else max(0.0, float(width) - float(W))

    @property
    def width(self) -> float:
        return float(W) + self._dx

    def resize(self, width: float, height: float, now: float) -> None:
        """Cambia el ancho y el alto visibles al instante (ventana redimensionada).

        La altura animada salta al nuevo valor: durante un arrastre de borde no
        debe haber retraso entre el marco y el contenido.
        """
        self.set_width(width)
        self.set_max_height(height)
        if self._open:
            self._height.to(self.viewport_height, float(now), 0.0, EASE_OUT)

    def set_max_height(self, height: float | None) -> None:
        """Alto máximo propio (ventana de ajustes); None vuelve al de la tarjeta."""
        self._max_h = None if height is None else max(160.0, float(height))

    def set_groups(
        self,
        groups: tuple[tuple[str, tuple[tuple[str, str], ...]], ...],
        title: str | None = None,
        now: float | None = None,
    ) -> None:
        """Muestra solo ``groups`` (una sección de la ventana de ajustes).

        ``title`` vacío deja la cabecera solo con «–» (la etiqueta del grupo
        ya nombra la sección); ``None`` vuelve a «AJUSTES».
        """
        moment = self._now if now is None else float(now)
        self._stop_text_edit()
        self._groups = tuple(groups) or GROUPS
        self._title = "AJUSTES" if title is None else str(title)
        self._selector_key = None
        self._selector_closing = False
        self._selector_extra.to(0.0, moment, 0.0, EASE_OUT)
        self._list_scroll = 0.0
        self._list_scroll_target = 0.0
        self._scroll.to(0.0, moment, 0.0, EASE_OUT)
        self._scroll_target = 0.0

    @property
    def groups(self) -> tuple[str, ...]:
        return tuple(name for name, _rows in self._groups)

    def set_available_height(self, height: float | None) -> None:
        """Limita el menú al espacio real arriba/abajo de la barra flotante."""
        self._available_h = None if height is None else max(1.0, float(height))

    def is_open(self) -> bool:
        """Intención lógica: False en cuanto se pide cerrar."""
        return self._open

    def _extra(self) -> dict[str, float]:
        if self._selector_key is None:
            return {}
        return {self._selector_key: max(0.0, self._selector_extra.value)}

    def _layout(self, extra: dict[str, float] | None = None) -> dict[str, LayoutRect]:
        """Layout con el viewport objetivo; el pie queda fijo y el resto scrollea."""
        return layout_menu(
            self._extra() if extra is None else extra,
            H_ISLAND + self.viewport_height,
            groups=self._shown_groups(),
        )

    def _nominal_layout(self, extra: dict[str, float] | None = None) -> dict[str, LayoutRect]:
        """Layout completo sin recortar; sirve para calcular el scroll máximo."""
        return layout_menu(
            self._extra() if extra is None else extra, None, groups=self._shown_groups()
        )

    @property
    def scroll_max(self) -> float:
        """Píxeles de desplazamiento disponibles con el contenido actual."""
        extra = self._extra()
        actual = self._layout(extra)
        nominal = self._nominal_layout(extra)
        return max(0.0, nominal["footer"].y0 - actual["footer"].y0)

    @property
    def scroll_offset(self) -> float:
        max_scroll = self.scroll_max
        return max(0.0, min(float(self._scroll.value), max_scroll))

    def _content_top(self, layout: dict[str, LayoutRect]) -> float:
        return layout["header"].y1 + GAP_HEADER

    def _footer_top(self, layout: dict[str, LayoutRect]) -> float:
        return layout["footer"].y0

    def _block_values(self, name: str, now: float) -> tuple[float, float]:
        """``(alfa, desplazamiento_y)`` de un bloque durante A-04."""
        raw = max(0.0, min(1.0, float(self._blocks[name].value)))
        return (raw * float(self._content_alpha.value), (1.0 - raw) * BLOCK_OFFSET)

    # ------------------------------------------------------------------
    # Apertura, cierre y reloj
    # ------------------------------------------------------------------
    def open(self, now: float) -> None:
        """Abre el menú (o invierte un cierre en curso) desde el valor actual."""
        now = float(now)
        self._advance(now)
        fresh = not self._open and float(self._height.value) <= 0.5
        if self._open and float(self._height.value) >= self.viewport_height - 0.5:
            return
        self._open = True
        self._closed_notified = False
        if fresh:
            self._stop_text_edit()
            self._selector_key = None
            self._selector_closing = False
            self._selector_extra.to(0.0, now, 0.0, EASE_OUT)
            self._list_scroll = 0.0
            self._list_scroll_target = 0.0
            self._scroll.to(0.0, now, 0.0, EASE_OUT)
            self._scroll_target = 0.0
            self._reset_interaction(now)
            self._content_alpha.to(1.0, now, 0.0 if not self._reduced_motion else 120.0, EASE_OUT)
            for name in self._blocks:
                self._blocks[name] = Tween(0.0)
            if self._reduced_motion:
                for name in self._blocks:
                    self._blocks[name].to(1.0, now, 120.0, EASE_OUT)
            else:
                for name, delay in BLOCK_DELAYS_MS.items():
                    self._blocks[name].to(
                        1.0,
                        now + delay / 1000.0,
                        BLOCK_FADE_MS,
                        EASE_OUT,
                    )
        else:
            # Inversión a mitad de cierre: el contenido vuelve sin escalonado.
            self._content_alpha.to(1.0, now, CONTENT_CLOSE_MS, EASE_OUT)
        duration = 0.0 if self._reduced_motion else T_SLOW
        self._height.to(self.viewport_height, now, duration, EASE_OUT)

    def close(self, now: float) -> None:
        """Cierra el menú con el tween de retracción y el fundido de contenido."""
        now = float(now)
        self._advance(now)
        if not self._open:
            # Ya se está cerrando: repetir close (p. ej. hide_notch después de
            # on_hide) no debe reiniciar el tween ni alargar A-05.
            return
        self._open = False
        self._closed_notified = False
        self._finish_text_edit(commit=True)
        self._finish_meter_drag(now, emit=True)
        self._finish_slider_drag(now, emit=True)
        self._pressed_hit = None
        self._clear_press(now)
        self._content_alpha.to(0.0, now, CONTENT_CLOSE_MS, EASE_OUT)
        duration = 0.0 if self._reduced_motion else T_CLOSE_RETRACT
        delay = 0.0 if self._reduced_motion else CLOSE_HEIGHT_DELAY_MS / 1000.0
        self._height.to(0.0, now + delay, duration, EASE_IN)

    def toggle(self, now: float) -> None:
        if self.is_open():
            self.close(now)
        else:
            self.open(now)

    def height(self, now: float) -> float:
        """Alto actual del cuerpo del menú, sin la barra."""
        self._advance(float(now))
        return max(0.0, min(self.viewport_height, float(self._height.value)))

    def update(self, now: float) -> None:
        """Avanza tweens y notifica el cierre completo una sola vez."""
        now = float(now)
        self._now = now
        self._advance(now)
        if self._capturing:  # las teclas que mantienes se ven en vivo, también al soltarlas
            self._capture_mods = frozenset(self.modifier_source())
        if not self._open and not self._closed_notified and float(self._height.value) <= 0.01:
            self._height.value = 0.0
            self._content_alpha.value = 0.0
            self._selector_key = None
            self._selector_closing = False
            self._selector_extra.value = 0.0
            self._closed_notified = True
            self._notify(self.on_close)

    def _advance(self, now: float) -> None:
        self._height.update(now)
        self._content_alpha.update(now)
        for tween in self._blocks.values():
            tween.update(now)
        self._selector_extra.update(now)
        for tween in self._chevron.values():
            tween.update(now)
        for tween in self._row_hover.values():
            tween.update(now)
        for tween in self._button_hover.values():
            tween.update(now)
        for tween in self._press.values():
            tween.update(now)
        for tween in self._thumb.values():
            tween.update(now)
        for tween in self._slider_color.values():
            tween.update(now)
        for key in TOGGLE_KEYS:
            desired = 1.0 if self.settings.get(key) else 0.0
            pos = self._toggle_pos[key]
            color = self._toggle_color[key]
            if not pos.active and abs(pos.value - desired) > 0.01:
                pos.to(desired, now, TOGGLE_MS, EASE_SPRING)
            if not color.active and abs(color.value - desired) > 0.01:
                color.to(desired, now, TOGGLE_MS, EASE_OUT)
            pos.update(now)
            color.update(now)
        self._scroll.update(now)
        self._testing_t.update(now)

        # El selector terminó de plegarse: se puede olvidar.
        if (
            self._selector_closing
            and not self._selector_extra.active
            and self._selector_extra.value <= 0.05
        ):
            self._selector_key = None
            self._selector_closing = False
            self._selector_extra.value = 0.0
            self._list_scroll = 0.0
            self._list_scroll_target = 0.0
            self._hover_option = None

        max_scroll = self.scroll_max
        if self._scroll_target > max_scroll:
            self._scroll_target = max_scroll
            if not self._scroll.active:
                self._scroll.to(max_scroll, now, SCROLL_TWEEN_MS, EASE_OUT)
        if self._scroll.value > max_scroll and not self._scroll.active:
            self._scroll.value = max_scroll
        # Balística del medidor: ataque rápido, caída suave.
        dt = max(0.0, min(0.05, now - self._meter_t)) if self._meter_t else 0.0
        self._meter_t = now
        target = 0.0 if self._meter_muted else self._meter_target
        if dt > 0.0:
            k = 1.0 - math.exp(-dt / (0.035 if target > self._meter_level else 0.12))
            self._meter_level += (target - self._meter_level) * k
        else:
            self._meter_level = target
        if self._meter_level < 1e-7:
            self._meter_level = 0.0
        self._clamp_list_scroll()

    def is_animating(self, now: float) -> bool:
        """True si queda alguna animación que exija repintar."""
        self._advance(float(now))
        tweens: list[Tween] = [
            self._height,
            self._content_alpha,
            self._selector_extra,
            self._scroll,
            self._testing_t,
        ]
        tweens.extend(self._blocks.values())
        tweens.extend(self._chevron.values())
        tweens.extend(self._row_hover.values())
        tweens.extend(self._button_hover.values())
        tweens.extend(self._press.values())
        tweens.extend(self._thumb.values())
        tweens.extend(self._slider_color.values())
        tweens.extend(self._toggle_pos.values())
        tweens.extend(self._toggle_color.values())
        if any(tween.active for tween in tweens):
            return True
        # La onda de «Hablando…» es función del tiempo, no de un tween.
        if float(self._testing_t.value) > 0.01:
            return True
        # El cursor de texto parpadea; mientras se edita hay que repintar.
        if self._editing_key is not None:
            return True
        if self._open and (
            self._meter_active or abs(self._meter_target - self._meter_level) > 1e-6
        ):
            return True
        # Timers sin tween: confirmación de guardado, error y desvanecido del
        # indicador de scroll. Mantienen vivo el repintado mientras se ven.
        if now < max(self._check_until, self._saved_flash_until, self._error_until):
            return True
        if any(now < until for until in self._copied.values()):
            return True
        if self._footer_prev is not None and now - self._footer_state_at < FOOTER_FADE_MS / 1000.0:
            return True
        if (
            self._save_prev_fill is not None
            and now - self._save_variant_at < SAVE_COLOR_MS / 1000.0
        ):
            return True
        return self._indicator_alpha(now) > 0.01

    # ------------------------------------------------------------------
    # Configuración externa
    # ------------------------------------------------------------------
    def set_settings(self, settings: MenuSettings, now: float | None = None) -> None:
        """Sustituye el modelo de vista (por ejemplo, al reabrir el menú).

        La integración puede conservar un único ``MenuSettings`` o entregar
        una instantánea nueva antes de abrir; esta última opción evita que el
        menú muestre valores viejos si la configuración cambió fuera de él.
        """
        if not isinstance(settings, MenuSettings):
            raise TypeError("settings debe ser MenuSettings")
        self._stop_text_edit()
        moment = self._now if now is None else float(now)
        self.settings = settings
        for key in TOGGLE_KEYS:
            desired = 1.0 if settings.get(key) else 0.0
            self._toggle_pos[key].to(desired, moment, TOGGLE_MS, EASE_SPRING)
            self._toggle_color[key].to(desired, moment, TOGGLE_MS, EASE_OUT)
        self._clamp_list_scroll()

    def set_theme(self, theme: str) -> None:
        self._theme = theme if theme in ("auto", "dark", "light") else "dark"

    def is_armed(self, key: str, now: float) -> bool:
        """¿El botón de una acción destructiva espera la confirmación?"""
        return self._armed.get(key, -1.0) > now

    def set_palette_overrides(self, provider: Callable[[str], dict[str, Color]] | None) -> None:
        """``provider(theme)`` devuelve colores que sustituyen a los tokens."""
        self._palette_overrides = provider

    def _palette(self) -> dict[str, Color]:
        pal = palette(self._theme)
        provider = self._palette_overrides
        if provider is None:
            return pal
        try:
            return {**pal, **provider(self._theme)}
        except Exception:
            return pal

    def set_scale(self, scale: float) -> None:
        if scale <= 0:
            raise ValueError("scale debe ser mayor que cero")
        self._scale = float(scale)

    def set_reduced_motion(self, reduced: bool) -> None:
        self._reduced_motion = bool(reduced)

    def set_input_level(
        self,
        level: float,
        *,
        muted: bool = False,
        active: bool = False,
    ) -> None:
        """Nivel RMS del micrófono para el medidor de entrada/umbral."""
        try:
            value = float(level or 0.0)
        except (TypeError, ValueError):
            value = 0.0
        self._meter_target = max(0.0, value)
        self._meter_muted = bool(muted)
        self._meter_active = bool(active)

    def set_testing(self, testing: bool, now: float | None = None) -> None:
        """Estado del botón «Probar»; la aplicación lo controla al terminar."""
        moment = self._now if now is None else float(now)
        target = 1.0 if testing else 0.0
        self._testing_t.to(target, moment, TEST_FADE_MS, EASE_OUT)

    # ------------------------------------------------------------------
    # Utilidades de estado
    # ------------------------------------------------------------------
    def _notify(self, callback: Callable[..., Any] | None, *args: Any) -> Any:
        if callback is None:
            return None
        try:
            return callback(*args)
        except Exception:
            return None

    def _reset_interaction(self, now: float) -> None:
        """Limpia hover/pulsación/thumbs al abrir desde cerrado."""
        self._hover_id = None
        self._hover_option = None
        self._pressed_hit = None
        self._slider_drag = None
        self._slider_pending = None
        self._slider_changed = False
        self._meter_drag = False
        self._meter_pending = None
        self._meter_changed = False
        self._stop_text_edit()
        for tween in self._row_hover.values():
            tween.to(0.0, now, 0.0, EASE_OUT)
        for tween in self._button_hover.values():
            tween.to(0.0, now, 0.0, EASE_OUT)
        for tween in self._press.values():
            tween.to(0.0, now, 0.0, EASE_OUT)
        for key in SELECT_KEYS:
            self._chevron[key].to(0.0, now, 0.0, EASE_OUT)
        for key in SLIDER_KEYS:
            self._thumb[key].to(SLIDER_THUMB_REST, now, 0.0, EASE_OUT)
            self._thumb_target[key] = SLIDER_THUMB_REST
            self._slider_color[key].to(0.0, now, 0.0, EASE_OUT)

    def _clamp_list_scroll(self) -> None:
        if self._selector_key is None:
            self._list_scroll = 0.0
            self._list_scroll_target = 0.0
            return
        try:
            options = self.settings.select_options(self._selector_key)
        except Exception:
            options = ()
        maximum = max(0.0, len(options) * LIST_ITEM_H - LIST_VIEW_H)
        self._list_scroll_target = max(0.0, min(self._list_scroll_target, maximum))
        self._list_scroll = max(0.0, min(self._list_scroll, maximum))

    def _indicator_activity(self, now: float) -> None:
        self._indicator_last = now
        self._indicator_until = now + SCROLL_INDICATOR_HOLD_MS / 1000.0

    def _indicator_alpha(self, now: float) -> float:
        if self.scroll_max <= 0.5:
            return 0.0
        if now < self._indicator_last:
            return 0.0
        if now <= self._indicator_until:
            return min(
                1.0,
                (now - self._indicator_last) / (_INDICATOR_SHOW_MS / 1000.0),
            )
        return max(
            0.0,
            1.0 - (now - self._indicator_until) / (SCROLL_INDICATOR_FADE_MS / 1000.0),
        )

    # ------------------------------------------------------------------
    # Selectores
    # ------------------------------------------------------------------
    def _selector_options(self, key: str) -> tuple[SelectOption, ...]:
        try:
            return self.settings.select_options(key)
        except Exception:
            return ()

    def _toggle_selector(self, key: str, now: float) -> None:
        if not self.settings.selector_enabled(key):
            return
        if self._selector_key == key and not self._selector_closing:
            self._close_selector(now)
            return
        old = self._selector_key
        if old is not None and old != key:
            self._chevron[old].to(0.0, now, CHEVRON_MS, EASE_OUT)
        options = self._selector_options(key)
        if not options:
            return
        self._selector_key = key
        self._selector_closing = False
        self._selector_item_start = now + SELECTOR_START_DELAY_MS / 1000.0
        target = min(len(options), LIST_MAX_VISIBLE) * LIST_ITEM_H
        self._selector_extra.to(float(target), now, SELECTOR_OPEN_MS, EASE_OUT)
        self._chevron[key].to(1.0, now, CHEVRON_MS, EASE_OUT)
        self._list_scroll = 0.0
        self._list_scroll_target = 0.0
        if self._pointer is not None:
            self._sync_hover(self._pointer[0], self._pointer[1], now)

    def _close_selector(self, now: float) -> None:
        if self._selector_key is None:
            return
        self._selector_closing = True
        self._chevron[self._selector_key].to(0.0, now, CHEVRON_MS, EASE_OUT)
        self._selector_extra.to(0.0, now, SELECTOR_CLOSE_MS, EASE_IN)
        self._list_scroll = 0.0
        self._list_scroll_target = 0.0
        self._hover_option = None

    def _selector_geometry(
        self,
        layout: dict[str, LayoutRect],
        scroll: float,
        key: str | None = None,
    ) -> tuple[float, float, tuple[SelectOption, ...], float] | None:
        """``(y_lista, alto_visible, opciones, scroll_interno)``."""
        target_key = self._selector_key if key is None else key
        if target_key is None:
            return None
        options = self._selector_options(target_key)
        if not options:
            return None
        row = layout[target_key]
        top = row.y0 - scroll + ROW["select"]
        extra = max(0.0, float(self._selector_extra.value))
        visible = max(0.0, min(extra, len(options) * LIST_ITEM_H))
        list_scroll = max(
            0.0,
            min(
                self._list_scroll,
                max(0.0, len(options) * LIST_ITEM_H - LIST_VIEW_H),
            ),
        )
        return (top, visible, options, list_scroll)

    def _selector_item_alpha(
        self, index: int, options: tuple[SelectOption, ...], now: float
    ) -> float:
        if self._selector_closing:
            return 1.0
        elapsed = (now - self._selector_item_start) * 1000.0 - index * SELECTOR_ITEM_STAGGER_MS
        k = max(0.0, min(1.0, elapsed / SELECTOR_ITEM_FADE_MS))
        return EASE_OUT(k)

    # ------------------------------------------------------------------
    # Rectángulos de control
    # ------------------------------------------------------------------
    def _hide_rect(self, layout: dict[str, LayoutRect]) -> Rect:
        header = layout["header"]
        cy = (header.y0 + header.y1) / 2.0
        radius = HIDE_SIZE / 2.0
        return Rect(
            (HIDE_CX + self._dx) - radius, cy - radius, (HIDE_CX + self._dx) + radius, cy + radius
        )

    def _save_rect(self, layout: dict[str, LayoutRect]) -> Rect:
        footer = layout["footer"]
        cy = (footer.y0 + footer.y1) / 2.0
        return Rect(
            (SAVE_BUTTON_X0 + self._dx),
            cy - BUTTON_H / 2.0,
            (SAVE_BUTTON_X1 + self._dx),
            cy + BUTTON_H / 2.0,
        )

    def _test_content_width(self) -> float:
        wave = TEST_WAVE_BARS * TEST_WAVE_BAR_W + (TEST_WAVE_BARS - 1) * TEST_WAVE_GAP
        talk = text_size("Hablando…", FS_SECONDARY, self._scale)[0]
        return wave + TEST_WAVE_GAP_TEXT + talk

    def _test_width(self) -> float:
        fraction = max(0.0, min(1.0, float(self._testing_t.value)))
        dynamic = max(
            BUTTON_MIN_W,
            BUTTON_PAD_X * 2.0 + self._test_content_width(),
        )
        return BUTTON_MIN_W + (dynamic - BUTTON_MIN_W) * fraction

    def _test_rect(self, layout: dict[str, LayoutRect], scroll: float) -> Rect:
        row = layout.get("test_voice")
        if row is None:
            # La ventana de ajustes muestra una sección cada vez: si «Prueba de
            # voz» no está a la vista, su botón no existe (rect fuera de todo).
            return Rect(-10.0, -10.0, -9.0, -9.0)
        cy = ((row.y0 + row.y1) / 2.0) - scroll
        width = self._test_width()
        return Rect(
            (TEST_BUTTON_X1 + self._dx) - width,
            cy - BUTTON_H / 2.0,
            (TEST_BUTTON_X1 + self._dx),
            cy + BUTTON_H / 2.0,
        )

    def _action_rect(self, layout: dict[str, LayoutRect], key: str, scroll: float) -> Rect:
        row = layout[key]
        cy = ((row.y0 + row.y1) / 2.0) - scroll
        return Rect(
            (TEST_BUTTON_X1 + self._dx) - BUTTON_MIN_W,
            cy - BUTTON_H / 2.0,
            (TEST_BUTTON_X1 + self._dx),
            cy + BUTTON_H / 2.0,
        )

    # ------------------------------------------------------------------
    # Hit testing
    # ------------------------------------------------------------------
    def hit(
        self,
        x: float,
        y: float,
        now: float,
        *,
        include_disabled: bool = False,
    ) -> MenuHit | None:
        """Control bajo ``(x, y)`` en coordenadas lógicas de ventana."""
        now = float(now)
        self._advance(now)
        self._now = now
        if not self._open:
            return None
        window_h = H_ISLAND + float(self._height.value)
        if y < H_ISLAND - 1.0 or y > window_h + 0.5:
            return None

        layout = self._layout()
        scroll = self.scroll_offset

        # Cabecera: el botón «–» es un círculo de 24 px.
        header = layout["header"]
        hide = self._hide_rect(layout)
        if header.y0 <= y <= header.y1 and hide.x0 <= x <= hide.x1 and hide.y0 <= y <= hide.y1:
            cx, cy = hide.center
            if math.hypot(x - cx, y - cy) <= HIDE_SIZE / 2.0:
                return MenuHit("hide", "hide", hide)

        content_top = self._content_top(layout)
        footer_top = self._footer_top(layout)
        if content_top <= y <= min(footer_top, window_h):
            # Lista del selector abierto (si está plegándose, no es pulsable).
            if self._selector_key is not None and not self._selector_closing:
                geometry = self._selector_geometry(layout, scroll)
                if geometry is not None:
                    top, visible, options, list_scroll = geometry
                    if GROUP_X0 <= x <= (GROUP_X1 + self._dx) and top <= y <= top + visible:
                        for index, option in enumerate(options):
                            item_y0 = top + index * LIST_ITEM_H - list_scroll
                            item_y1 = item_y0 + LIST_ITEM_H
                            if item_y0 < top - 0.5 or item_y1 > top + visible + 0.5:
                                continue
                            if item_y0 <= y <= item_y1:
                                return MenuHit(
                                    "option",
                                    self._selector_key,
                                    Rect(
                                        GROUP_X0,
                                        item_y0,
                                        (GROUP_X1 + self._dx),
                                        item_y1,
                                    ),
                                    value=option.value,
                                    index=index,
                                )

            for _group_name, rows in self._shown_groups():
                for key, kind in rows:
                    row = layout[key]
                    base_h = float(ROW[kind])
                    y0 = row.y0 - scroll
                    y1 = y0 + base_h
                    disabled = kind == "select" and not self.settings.selector_enabled(key)
                    if disabled and not include_disabled:
                        continue
                    rect = Rect(GROUP_X0, y0, (GROUP_X1 + self._dx), y1)
                    if kind == "slider":
                        cy = row.y0 - scroll + SLIDER_LINE2_CY
                        half = SLIDER_TRACK_H / 2.0 + SLIDER_TRACK_PAD
                        if (
                            SLIDER_TRACK_X0 <= x <= (SLIDER_TRACK_X1 + self._dx)
                            and cy - half <= y <= cy + half
                        ):
                            return MenuHit("slider", key, rect, disabled=disabled)
                    elif kind == "select":
                        if rect.x0 <= x <= rect.x1 and y0 <= y <= y1:
                            return MenuHit("select", key, rect, disabled=disabled)
                    elif kind == "toggle":
                        if rect.x0 <= x <= rect.x1 and y0 <= y <= y1:
                            return MenuHit(
                                "toggle",
                                key,
                                rect,
                                value=bool(self.settings.get(key)),
                                disabled=disabled,
                            )
                    elif kind == "text":
                        if key in SECRET_KEYS and self._secret_text(key):
                            button = self._copy_rect(y0)
                            if button.x0 <= x <= button.x1 and button.y0 <= y <= button.y1:
                                return MenuHit("copy", key, button)
                        if rect.x0 <= x <= rect.x1 and y0 <= y <= y1:
                            return MenuHit("text", key, rect)
                    elif kind == "palette":
                        for value, _label, pill in self._palette_pills(y0):
                            if pill.x0 <= x <= pill.x1 and y0 <= y <= y1:
                                return MenuHit("palette", "theme", pill, value=value)
                    elif kind == "meter":
                        track_cy = y0 + METER_TRACK_Y + METER_TRACK_H / 2.0
                        half = METER_TRACK_H / 2.0 + METER_TRACK_PAD
                        if (
                            SLIDER_TRACK_X0 <= x <= (SLIDER_TRACK_X1 + self._dx)
                            and track_cy - half <= y <= track_cy + half
                        ):
                            return MenuHit("meter", "input_meter", rect)
                    elif kind == "action":
                        if key == "test_voice":
                            button = self._test_rect(layout, scroll)
                            testing = float(self._testing_t.value) > 0.01
                            disabled = testing
                        else:
                            button = self._action_rect(layout, key, scroll)
                            disabled = False
                        if disabled and not include_disabled:
                            continue
                        if button.x0 <= x <= button.x1 and button.y0 <= y <= button.y1:
                            return MenuHit(
                                "action",
                                key,
                                button,
                                disabled=disabled,
                            )

        footer = layout["footer"]
        if footer.y0 <= y <= min(footer.y1, window_h):
            save = self._save_rect(layout)
            disabled = not self.settings.dirty()
            if disabled and not include_disabled:
                return None
            if save.x0 <= x <= save.x1 and save.y0 <= y <= save.y1:
                return MenuHit("save", "save", save, disabled=disabled)
        return None

    # ------------------------------------------------------------------
    # Puntero
    # ------------------------------------------------------------------
    @staticmethod
    def _same_hit(first: MenuHit | None, second: MenuHit | None) -> bool:
        if first is None or second is None:
            return False
        return first.kind == second.kind and first.key == second.key and first.index == second.index

    def _row_at(
        self, x: float, y: float, layout: dict[str, LayoutRect], scroll: float
    ) -> tuple[str, str, bool] | None:
        content_top = self._content_top(layout)
        footer_top = self._footer_top(layout)
        window_h = H_ISLAND + float(self._height.value)
        if not (content_top <= y <= min(footer_top, window_h)):
            return None
        for _group_name, rows in self._shown_groups():
            for key, kind in rows:
                row = layout[key]
                base_h = float(ROW[kind])
                y0 = row.y0 - scroll
                y1 = y0 + base_h
                if GROUP_X0 <= x <= (GROUP_X1 + self._dx) and y0 <= y <= y1:
                    disabled = kind == "select" and not self.settings.selector_enabled(key)
                    return (key, kind, disabled)
        return None

    def _sync_hover(self, x: float, y: float, now: float) -> None:
        new_id: str | None = None
        new_option: int | None = None
        layout = self._layout()
        scroll = self.scroll_offset
        window_h = H_ISLAND + float(self._height.value)

        hide = self._hide_rect(layout)
        save = self._save_rect(layout)
        test = self._test_rect(layout, scroll)
        header = layout["header"]
        footer = layout["footer"]

        if (
            header.y0 <= y <= header.y1
            and hide.x0 <= x <= hide.x1
            and hide.y0 <= y <= hide.y1
            and math.hypot(x - hide.center[0], y - hide.center[1]) <= HIDE_SIZE / 2.0
        ):
            new_id = "btn:hide"
        elif (
            footer.y0 <= y <= min(footer.y1, window_h)
            and save.x0 <= x <= save.x1
            and save.y0 <= y <= save.y1
            and self.settings.dirty()
        ):
            new_id = "btn:save"
        elif (
            test.x0 <= x <= test.x1
            and test.y0 <= y <= test.y1
            and float(self._testing_t.value) <= 0.01
        ):
            new_id = "btn:test"
        elif self._selector_key is not None and not self._selector_closing:
            geometry = self._selector_geometry(layout, scroll)
            if geometry is not None:
                top, visible, options, list_scroll = geometry
                if GROUP_X0 <= x <= (GROUP_X1 + self._dx) and top <= y <= top + visible:
                    for index in range(len(options)):
                        item_y0 = top + index * LIST_ITEM_H - list_scroll
                        item_y1 = item_y0 + LIST_ITEM_H
                        if item_y0 < top - 0.5 or item_y1 > top + visible + 0.5:
                            continue
                        if item_y0 <= y <= item_y1:
                            new_option = index
                            break
        if new_id is None and new_option is None:
            row = self._row_at(x, y, layout, scroll)
            if row is not None:
                key, _kind, disabled = row
                if not disabled:
                    new_id = "row:" + key

        if new_id != self._hover_id:
            if self._hover_id is not None:
                target = self._hover_state(self._hover_id)
                if target is not None:
                    target.to(0.0, now, T_MICRO, EASE_OUT)
            if new_id is not None:
                target = self._hover_state(new_id)
                if target is not None:
                    target.to(1.0, now, T_MICRO, EASE_OUT)
            self._hover_id = new_id
        self._hover_option = new_option
        hit = self.hit(x, y, now)
        self._copy_hover = hit.key if hit is not None and hit.kind == "copy" else None

        # Thumb del slider: 16 px al pasar por su fila, 14 al salir (120 ms).
        for key in SLIDER_KEYS:
            if self._slider_drag is not None and self._slider_drag.key == key:
                continue
            hovering = new_id == "row:" + key
            target = SLIDER_THUMB_HOVER if hovering else SLIDER_THUMB_REST
            if self._thumb_target[key] != target:
                self._thumb_target[key] = target
                self._thumb[key].to(target, now, SLIDER_HOVER_MS, EASE_OUT)

    def _secret_text(self, key: str) -> str:
        """Valor de una fila secreta: lo que se está escribiendo o lo guardado."""
        if self._editing_key == key:
            return self._edit_buffer
        return str(self.settings.get(key, "") or "")

    def _copy_rect(self, row_y0: float) -> Rect:
        """Botón «copiar» de una fila secreta, alineado a la derecha de la línea de valor."""
        right = ROW_X1 + self._dx + 2.0
        cy = row_y0 + TEXT_LINE2_CY
        half = COPY_BTN / 2.0
        return Rect(right - COPY_BTN, cy - half, right, cy + half)

    def _copy_secret(self, key: str, now: float) -> None:
        """Copia la clave al portapapeles; el icono lo confirma con un check."""
        text = self._secret_text(key)
        if text and self._notify(self.on_copy, text):
            self._copied[key] = now + COPIED_SECS

    def _hover_state(self, hover_id: str) -> Tween | None:
        if hover_id.startswith("row:"):
            return self._row_hover.get(hover_id[4:])
        if hover_id == "btn:hide":
            return self._button_hover["hide"]
        if hover_id == "btn:save":
            return self._button_hover["save"]
        if hover_id == "btn:test":
            return self._button_hover["test"]
        return None

    def _clear_press(self, now: float) -> None:
        for tween in self._press.values():
            tween.to(0.0, now, BUTTON_RELEASE_MS, EASE_OUT)

    def pointer_down(self, x: float, y: float, now: float) -> bool:
        """Pulsa un control. Devuelve True dentro del menú (evita arrastrar)."""
        now = float(now)
        self._advance(now)
        self._now = now
        if not self._open:
            return False
        self._pointer = (x, y)
        self._indicator_activity(now)
        self._sync_hover(x, y, now)
        hit = self.hit(x, y, now)
        if self._editing_key is not None and not (
            hit is not None and hit.kind == "text" and hit.key == self._editing_key
        ):
            self._finish_text_edit(commit=True)
        if hit is not None and hit.kind == "meter":
            self._meter_drag = True
            self._meter_pending = None
            self._meter_changed = False
            self._set_meter_from_x(x, now)
            return True
        if hit is not None and hit.kind == "slider":
            self._slider_drag = hit
            self._pressed_hit = None
            self._slider_changed = False
            self._slider_pending = None
            self._thumb_target[hit.key] = SLIDER_THUMB_DRAG
            self._thumb[hit.key].to(SLIDER_THUMB_DRAG, now, SLIDER_DRAG_MS, EASE_OUT)
            self._slider_color[hit.key].to(1.0, now, T_MICRO, EASE_OUT)
            self._set_slider_from_x(hit.key, x, now)
            return True
        self._pressed_hit = hit
        if hit is not None:
            press_key = {
                "hide": "hide",
                "save": "save",
                "action": "test",
            }.get(hit.kind)
            tween = self._press.get(press_key) if press_key else None
            if tween is not None:
                tween.to(1.0, now, BUTTON_PRESS_MS, EASE_OUT)
        return True

    def pointer_move(self, x: float, y: float, now: float) -> bool:
        """Actualiza hover y arrastre. Devuelve True mientras el menú esté abierto."""
        now = float(now)
        self._advance(now)
        self._now = now
        if not self._open:
            return False
        self._pointer = (x, y)
        self._indicator_activity(now)
        self._sync_hover(x, y, now)
        if self._meter_drag:
            self._set_meter_from_x(x, now)
        if self._slider_drag is not None:
            self._set_slider_from_x(self._slider_drag.key, x, now)
        return True

    def pointer_up(self, x: float, y: float, now: float) -> bool:
        """Confirma el clic al soltar sobre el mismo control."""
        now = float(now)
        self._advance(now)
        self._now = now
        if not self._open:
            return False
        self._pointer = (x, y)
        self._indicator_activity(now)
        if self._meter_drag:
            self._set_meter_from_x(x, now)
            self._finish_meter_drag(now, emit=True)
            self._sync_hover(x, y, now)
            return True
        if self._slider_drag is not None:
            key = self._slider_drag.key
            self._set_slider_from_x(key, x, now)
            self._finish_slider_drag(now, emit=True)
            hit = self.hit(x, y, now)
            hovering = hit is not None and hit.kind == "slider" and hit.key == key
            target = SLIDER_THUMB_HOVER if hovering else SLIDER_THUMB_REST
            self._thumb_target[key] = target
            self._thumb[key].to(target, now, SLIDER_RELEASE_MS, EASE_SPRING)
            self._sync_hover(x, y, now)
            return True
        pressed = self._pressed_hit
        self._pressed_hit = None
        self._clear_press(now)
        hit = self.hit(x, y, now)
        if self._same_hit(pressed, hit) and hit is not None:
            self._activate_hit(hit, now)
        self._sync_hover(x, y, now)
        return True

    def _activate_hit(self, hit: MenuHit, now: float) -> None:
        if hit.kind == "hide":
            self.close(now)
            self._notify(self.on_hide)
        elif hit.kind == "save":
            self._save(now)
        elif hit.kind == "action":
            if hit.key == "test_voice":
                self._start_test(now)
            elif hit.key in CONFIRM_ACTIONS and not self.is_armed(hit.key, now):
                self._armed[hit.key] = now + ARM_SECS
            else:
                self._armed.pop(hit.key, None)
                self._notify(self.on_action, hit.key)
        elif hit.kind == "copy":
            self._copy_secret(hit.key, now)
        elif hit.kind == "select":
            self._toggle_selector(hit.key, now)
        elif hit.kind == "option":
            self.settings.set(hit.key, hit.value)
            self._close_selector(now)
            self._notify(self.on_live_change, hit.key, hit.value)
        elif hit.kind == "palette":
            self.settings.set("theme", hit.value)
            self._notify(self.on_live_change, "theme", hit.value)
        elif hit.kind == "toggle":
            value = not bool(self.settings.get(hit.key))
            self.settings.set(hit.key, value)
            self._animate_toggle(hit.key, now)
            self._notify(self.on_live_change, hit.key, value)
        elif hit.kind == "text":
            self.begin_text_edit(hit.key, now)

    def _animate_toggle(self, key: str, now: float) -> None:
        target = 1.0 if self.settings.get(key) else 0.0
        self._toggle_pos[key].to(target, now, TOGGLE_MS, EASE_SPRING)
        self._toggle_color[key].to(target, now, TOGGLE_MS, EASE_OUT)

    # ------------------------------------------------------------------
    # Sliders
    # ------------------------------------------------------------------
    def _set_slider_from_x(self, key: str, x: float, now: float) -> None:
        spec = SLIDERS[key]
        width = (SLIDER_TRACK_X1 + self._dx) - SLIDER_TRACK_X0
        fraction = 0.0 if width <= 0 else (float(x) - SLIDER_TRACK_X0) / width
        fraction = max(0.0, min(1.0, fraction))
        value = spec.minimum + fraction * (spec.maximum - spec.minimum)
        old = self.settings.get(key)
        new = self.settings.set(key, value)
        if new == old:
            return
        self._slider_changed = True
        self._slider_pending = (key, new)
        self._emit_slider(now, force=False)

    def _set_meter_from_x(self, x: float, now: float) -> None:
        """Convierte la posición del puntero en el umbral de voz."""
        width = (SLIDER_TRACK_X1 + self._dx) - SLIDER_TRACK_X0
        fraction = 0.0 if width <= 0 else (float(x) - SLIDER_TRACK_X0) / width
        fraction = max(0.0, min(1.0, fraction))
        # La posición se convierte en dB, no en amplitud lineal: es la misma escala con
        # la que se dibuja el medidor, así el pulgar acaba justo bajo el puntero.
        value = db_to_rms(METER_DB_FLOOR + fraction * (METER_DB_CEIL - METER_DB_FLOOR))
        old = self.settings.get("min_rms")
        new = self.settings.set("min_rms", value)
        if new == old:
            return
        self._meter_changed = True
        self._meter_pending = ("min_rms", new)
        if now - self._meter_last_emit >= SLIDER_EMIT_MS / 1000.0:
            self._emit_meter(now)

    def _emit_meter(self, now: float) -> None:
        if self._meter_pending is None:
            return
        key, value = self._meter_pending
        self._meter_pending = None
        self._meter_last_emit = now
        self._notify(self.on_live_change, key, value)

    def _finish_meter_drag(self, now: float, *, emit: bool) -> None:
        if not self._meter_drag:
            return
        if emit and (self._meter_pending is not None or self._meter_changed):
            self._meter_last_emit = now
            self._notify(self.on_live_change, "min_rms", self.settings.get("min_rms"))
        self._meter_pending = None
        self._meter_changed = False
        self._meter_drag = False

    def _emit_slider(self, now: float, *, force: bool) -> None:
        if self._slider_pending is None:
            return
        if not force and now - self._slider_last_emit < SLIDER_EMIT_MS / 1000.0:
            return
        key, value = self._slider_pending
        self._slider_pending = None
        self._slider_last_emit = now
        self._notify(self.on_live_change, key, value)

    def _finish_slider_drag(self, now: float, *, emit: bool) -> None:
        if self._slider_drag is None:
            return
        key = self._slider_drag.key
        if emit and (self._slider_pending is not None or self._slider_changed):
            value = self.settings.get(key)
            self._slider_last_emit = now
            self._notify(self.on_live_change, key, value)
        self._slider_pending = None
        self._slider_changed = False
        self._slider_drag = None
        self._slider_color[key].to(0.0, now, T_MICRO, EASE_OUT)

    # ------------------------------------------------------------------
    # Rueda
    # ------------------------------------------------------------------
    def wheel(self, x: float, y: float, delta: int, now: float) -> bool:
        """Rueda: lista interna, scroll del menú o paso del slider."""
        now = float(now)
        self._advance(now)
        self._now = now
        if not self._open or not delta:
            return False
        self._pointer = (x, y)
        self._indicator_activity(now)
        notches = int(delta / 120)
        if notches == 0:
            notches = 1 if delta > 0 else -1
        layout = self._layout()
        scroll = self.scroll_offset

        # Prioridad 1: lista abierta con más de cinco elementos.
        if self._selector_key is not None and not self._selector_closing:
            geometry = self._selector_geometry(layout, scroll)
            if geometry is not None:
                top, visible, options, _list_scroll = geometry
                if (
                    len(options) > LIST_MAX_VISIBLE
                    and GROUP_X0 <= x <= (GROUP_X1 + self._dx)
                    and top <= y <= top + visible
                ):
                    maximum = len(options) * LIST_ITEM_H - LIST_VIEW_H
                    self._list_scroll_target = max(
                        0.0,
                        min(
                            maximum,
                            self._list_scroll_target - notches * LIST_ITEM_H,
                        ),
                    )
                    self._list_scroll = self._list_scroll_target
                    self._sync_hover(x, y, now)
                    return True

        # Prioridad 2: si el menú no cabe, la rueda desplaza el menú.
        max_scroll = self.scroll_max
        if max_scroll > 0.5:
            self._scroll_target = max(
                0.0,
                min(max_scroll, self._scroll_target - notches * SCROLL_STEP),
            )
            self._scroll.to(
                self._scroll_target,
                now,
                SCROLL_TWEEN_MS,
                EASE_OUT,
            )
            self._sync_hover(x, y, now)
            return True

        # Prioridad 3: slider bajo el puntero (solo si el menú no scrollea).
        hit = self.hit(x, y, now)
        if hit is not None and hit.kind == "slider":
            spec = SLIDERS[hit.key]
            old = self.settings.get(hit.key)
            new = self.settings.set(
                hit.key,
                float(old) + notches * spec.step,
            )
            if new != old:
                self._notify(self.on_live_change, hit.key, new)
            self._thumb_target[hit.key] = SLIDER_THUMB_HOVER
            self._thumb[hit.key].to(SLIDER_THUMB_HOVER, now, SLIDER_HOVER_MS, EASE_OUT)
            return True
        return False

    # ------------------------------------------------------------------
    # Edición de texto
    # ------------------------------------------------------------------
    @property
    def editing_key(self) -> str | None:
        """Clave de la fila que tiene el teclado, o None."""
        return self._editing_key

    @property
    def text_buffer(self) -> str:
        return self._edit_buffer

    def begin_text_edit(self, key: str, now: float | None = None) -> None:
        """Activa la edición de una fila de texto y pide el teclado."""
        if key not in TEXT_KEYS:
            return
        if self._editing_key == key:
            return
        if self._editing_key is not None:
            self._finish_text_edit(commit=True)
        if key in hk.HOTKEY_KEYS:
            self._begin_capture(key, now)
            return
        self._editing_key = key
        self._edit_buffer = str(self.settings.get(key, "") or "")
        self._edit_cursor = len(self._edit_buffer)
        self._edit_select_all = False
        self._edit_t0 = self._now if now is None else float(now)
        self._notify(self.on_text_edit, True)

    def finish_text_edit(self, commit: bool = True) -> None:
        """Confirma o descarta el texto y devuelve el teclado."""
        self._finish_text_edit(commit=commit)

    def _finish_text_edit(self, *, commit: bool) -> None:
        key = self._editing_key
        if key is None:
            return
        if self._capturing:  # sin combinación completa no hay nada que confirmar
            self._end_capture()
            return
        text = self._edit_buffer
        self._editing_key = None
        self._edit_buffer = ""
        self._edit_cursor = 0
        self._edit_select_all = False
        if commit:
            old = self.settings.get(key)
            try:
                new = self.settings.set(key, text)
            except Exception:
                new = old
            if new != old:
                self._notify(self.on_live_change, key, new)
        self._notify(self.on_text_edit, False)

    def _stop_text_edit(self) -> None:
        """Descarta la edición sin avisar al llamador (cierre/reemplazo)."""
        if self._editing_key is None:
            return
        if self._capturing:
            self._end_capture()
            return
        self._editing_key = None
        self._edit_buffer = ""
        self._edit_cursor = 0
        self._edit_select_all = False
        self._notify(self.on_text_edit, False)

    def _begin_capture(self, key: str, now: float | None) -> None:
        """Pasa la fila de atajo a modo captura: la siguiente combinación la sustituye."""
        self._editing_key = key
        self._capturing = True
        self._capture_mods = frozenset()
        self._capture_msg = None
        self._edit_buffer = ""
        self._edit_cursor = 0
        self._edit_select_all = False
        self._edit_t0 = self._now if now is None else float(now)
        self._notify(self.on_text_edit, True)
        self._notify(self.on_hotkey_capture, True)

    def _end_capture(self) -> None:
        if not self._capturing:
            return
        self._capturing = False
        self._editing_key = None
        self._capture_mods = frozenset()
        self._capture_msg = None
        self._notify(self.on_text_edit, False)
        self._notify(self.on_hotkey_capture, False)

    def _capture_press(self, vk: int) -> bool:
        """Una tecla durante la captura: espera, cancela, rechaza o cierra el atajo."""
        key = self._editing_key or ""
        mods = frozenset(self.modifier_source())
        self._capture_mods = mods
        event = hk.press(vk, mods)
        if event.kind == "cancel":
            self._end_capture()
        elif event.kind == "reject":
            self._capture_msg = (event.reason, self._now + CAPTURE_MSG_SECS)
        elif event.kind == "done":
            others = {name: str(self.settings.get(name, "") or "") for name in hk.HOTKEY_KEYS}
            taken = hk.conflict(event.spec, others, key, LABELS)
            if taken is not None:
                self._capture_msg = (f"Ya la usa «{taken}»", self._now + CAPTURE_MSG_SECS)
                return True
            old = self.settings.get(key)
            new = self.settings.set(key, event.spec)
            if new != old:  # primero el valor nuevo: al soltar la captura se registran con él
                self._notify(self.on_live_change, key, new)
            self._end_capture()
        return True

    def paste_text(self, text: str) -> bool:
        """Pega texto de una sola línea en la fila activa."""
        if self._editing_key is None:
            return False
        if self._capturing:
            return True
        clean = str(text or "").replace("\r", " ").replace("\n", " ")
        self._insert_text(clean)
        return True

    def text_input(self, char: str) -> bool:
        """Introduce un carácter imprimible en la fila activa."""
        if self._editing_key is None:
            return False
        if self._capturing or not char:
            return True
        for ch in str(char):
            if ch == "\r" or ch == "\n":
                self._finish_text_edit(commit=True)
                return True
            # Backspace y Supr ya se gestionan en WM_KEYDOWN. WM_CHAR también
            # los emite; ignorarlos aquí evita borrar dos caracteres.
            if ord(ch) >= 32:
                self._insert_text(ch)
        return True

    def text_key(self, vk: int, ctrl: bool = False) -> bool:
        """Teclas de edición (Windows VK). Devuelve True si se consumieron."""
        if self._editing_key is None:
            return False
        try:
            vk = int(vk)
        except (TypeError, ValueError):
            return False
        if self._capturing:
            return self._capture_press(vk)
        if vk == 27:  # Esc
            self._finish_text_edit(commit=False)
        elif vk == 13:  # Enter
            self._finish_text_edit(commit=True)
        elif vk == 8:  # Backspace
            self._backspace()
        elif vk == 46:  # Delete
            self._delete()
        elif vk == 37:  # Left
            self._edit_select_all = False
            self._edit_cursor = max(0, self._edit_cursor - 1)
        elif vk == 39:  # Right
            self._edit_select_all = False
            self._edit_cursor = min(len(self._edit_buffer), self._edit_cursor + 1)
        elif vk == 36:  # Home
            self._edit_select_all = False
            self._edit_cursor = 0
        elif vk == 35:  # End
            self._edit_select_all = False
            self._edit_cursor = len(self._edit_buffer)
        elif vk == 65 and ctrl:  # Ctrl+A
            self._edit_select_all = True
            self._edit_cursor = len(self._edit_buffer)
        else:
            return False
        return True

    def _insert_text(self, text: str) -> None:
        if not text:
            return
        if self._edit_select_all:
            self._edit_buffer = ""
            self._edit_select_all = False
        cursor = max(0, min(len(self._edit_buffer), self._edit_cursor))
        self._edit_buffer = self._edit_buffer[:cursor] + text + self._edit_buffer[cursor:]
        self._edit_cursor = cursor + len(text)

    def _backspace(self) -> None:
        if self._edit_select_all:
            self._edit_buffer = ""
            self._edit_cursor = 0
            self._edit_select_all = False
            return
        cursor = max(0, min(len(self._edit_buffer), self._edit_cursor))
        if cursor <= 0:
            return
        self._edit_buffer = self._edit_buffer[: cursor - 1] + self._edit_buffer[cursor:]
        self._edit_cursor = cursor - 1

    def _delete(self) -> None:
        if self._edit_select_all:
            self._edit_buffer = ""
            self._edit_cursor = 0
            self._edit_select_all = False
            return
        cursor = max(0, min(len(self._edit_buffer), self._edit_cursor))
        if cursor >= len(self._edit_buffer):
            return
        self._edit_buffer = self._edit_buffer[:cursor] + self._edit_buffer[cursor + 1 :]

    # ------------------------------------------------------------------
    # Prueba de voz y guardado
    # ------------------------------------------------------------------
    def _start_test(self, now: float) -> None:
        self._finish_text_edit(commit=True)
        self.set_testing(True, now)
        self._notify(self.on_test, self.settings.snapshot())

    def _save(self, now: float) -> None:
        self._finish_text_edit(commit=True)
        if self._saving_pending or not self.settings.dirty():
            return
        self._save_backup = self.settings.saved_snapshot()
        self._save_snapshot = self.settings.snapshot()
        try:
            result = self.on_save(self._save_snapshot) if self.on_save is not None else True
        except Exception:
            result = False
        if result is False:
            self._save_failed(now)
        elif result is True:
            self._save_succeeded(now)
        else:
            # None = aceptado y pendiente: no se muestra éxito hasta que el
            # hilo que persiste confirme con on_applied(True).
            self._saving_pending = True
            self._check_until = 0.0
            self._saved_flash_until = 0.0
            self._error_until = 0.0

    def _save_succeeded(self, now: float) -> None:
        self._saving_pending = False
        self.settings.mark_saved()
        self._error_until = 0.0
        self._check_until = now + SAVE_CHECK_MS / 1000.0
        self._saved_flash_until = now + SAVE_FLASH_MS / 1000.0

    def _save_failed(self, now: float) -> None:
        self._saving_pending = False
        if self._save_backup is not None:
            self.settings.restore_saved(self._save_backup)
        self._save_backup = None
        self._save_snapshot = None
        self._check_until = 0.0
        self._saved_flash_until = 0.0
        self._error_until = now + SAVE_ERROR_MS / 1000.0

    def on_applied(self, result: bool) -> None:
        """Resultado real de un guardado asíncrono (True = persistido)."""
        now = self._now
        self._saving_pending = False
        if result:
            saved = self._save_snapshot
            if saved is not None:
                self.settings.mark_saved(saved)
            else:
                self.settings.mark_saved()
            self._save_backup = None
            self._save_snapshot = None
            self._error_until = 0.0
            self._check_until = now + SAVE_CHECK_MS / 1000.0
            self._saved_flash_until = now + SAVE_FLASH_MS / 1000.0
        else:
            # El fallo es de persistencia: los valores editados siguen
            # aplicados en vivo y el menú sigue dirty (spec 10.2). Se
            # conserva la copia guardada anterior como baseline para que
            # dirty() siga siendo verdadero, sin revertir la interfaz.
            if self._save_backup is not None and self._save_snapshot is not None:
                backup = dict(self._save_backup)
                edited = dict(self._save_snapshot)
                self.settings.restore_saved(backup)
                for key, value in edited.items():
                    try:
                        self.settings.set(key, value)
                    except Exception:
                        pass
            self._save_backup = None
            self._save_snapshot = None
            self._check_until = 0.0
            self._saved_flash_until = 0.0
            self._error_until = now + SAVE_ERROR_MS / 1000.0

    # ------------------------------------------------------------------
    # Dibujo
    # ------------------------------------------------------------------
    def _content_tweens(self) -> list[Tween]:
        """Tweens que afectan a las capas de contenido (sin altura)."""
        tweens: list[Tween] = [
            self._content_alpha,
            self._selector_extra,
            self._scroll,
            self._testing_t,
        ]
        tweens.extend(self._blocks.values())
        tweens.extend(self._chevron.values())
        tweens.extend(self._row_hover.values())
        tweens.extend(self._button_hover.values())
        tweens.extend(self._press.values())
        tweens.extend(self._thumb.values())
        tweens.extend(self._slider_color.values())
        tweens.extend(self._toggle_pos.values())
        tweens.extend(self._toggle_color.values())
        return tweens

    def _layers_static(self, now: float) -> bool:
        """True si el contenido visual ya no depende de ``now``.

        Las capas se pueden reutilizar y solo queda recomponerlas sobre la
        silueta. El indicador de scroll y los degradados viven fuera de las
        capas, así que no impiden la reutilización.
        """
        if self._testing_t.value > 0.01:
            return False
        if self._editing_key is not None:
            return False
        if self._selector_key is not None and not self._selector_closing:
            options = self._selector_options(self._selector_key)
            if options:
                end = (
                    (len(options) - 1) * SELECTOR_ITEM_STAGGER_MS
                    + SELECTOR_ITEM_FADE_MS
                    + SELECTOR_START_DELAY_MS
                ) / 1000.0
                if now - self._selector_item_start < end:
                    return False
        if now < max(self._check_until, self._saved_flash_until, self._error_until):
            return False
        if self._footer_prev is not None or self._save_prev_fill is not None:
            return False
        return not any(tween.active for tween in self._content_tweens())

    def frame_key(self, now: float) -> tuple | None:
        """Huella de todo lo que se ve del menú, o ``None`` si algo se está animando.

        Dos llamadas con la misma huella pintarían el mismo cuadro: la ventana de
        ajustes reutiliza el último en vez de volver a componerlo.
        """
        if not self._layers_static(now):
            return None
        return (
            self._layer_signature_value(now),
            self._open,
            round(float(self._height.value), 3),
            round(float(self._content_alpha.value), 3),
            round(float(self.scroll_offset), 2),
            round(float(self._indicator_alpha(now)), 3),
        )

    def _layer_signature_value(self, now: float) -> tuple:
        """Huella de todo lo que pinta las capas (no de la composición)."""
        settings_values = tuple(
            (key, round(float(value), 6) if isinstance(value, float) else value)
            for key, value in self.settings.snapshot().items()
        )
        devices = tuple(
            (int(device["index"]), str(device["name"])) for device in self.settings.devices
        )
        voices = tuple((str(option.value), str(option.label)) for option in self.settings.voices)
        return (
            self._theme,
            round(self._scale, 4),
            self._layer_shape,
            round(self.viewport_height, 3),
            self._selector_key,
            self._selector_closing,
            round(float(self._selector_extra.value), 3),
            round(float(self._scroll.value), 3),
            round(float(self._list_scroll), 3),
            self._hover_id,
            self._hover_option,
            self._editing_key,
            self._edit_buffer,
            self._edit_cursor,
            self._capturing,
            tuple(sorted(self._capture_mods)),
            self._capture_msg is not None and now < self._capture_msg[1],
            int(now * 6.0) if self._capturing else 0,
            self.settings.get("llm_profile"),
            self.settings.get("theme"),
            self.settings.get("show_advanced"),
            round(self._dx, 2),
            round(self._meter_level, 3),
            round(self._meter_target, 3),
            self._meter_muted,
            self._meter_active,
            self.settings.dirty(),
            self._footer_state,
            self._save_variant,
            now < self._check_until,
            now < self._saved_flash_until,
            now < self._error_until,
            self._copy_hover,
            tuple(sorted(key for key, until in self._copied.items() if now < until)),
            self._footer_prev is not None,
            self._save_prev_fill is not None,
            tuple(round(float(tween.value), 4) for tween in self._content_tweens()),
            settings_values,
            devices,
            voices,
        )

    def draw(self, buf: np.ndarray, now: float) -> None:
        """Dibuja el menú sobre ``buf`` (float32 RGBA premultiplicado)."""
        if buf.ndim != 3 or buf.shape[2] != 4 or buf.dtype != np.float32:
            raise TypeError("buf debe ser float32 con forma (H, W, 4)")
        now = float(now)
        self.update(now)
        if not self._open and float(self._height.value) <= 0.5:
            return

        pal = self._palette()
        # Tres capas disjuntas: así el contenido que se desborda por el scroll
        # nunca se cuela sobre la cabecera ni sobre el pie. Se reutilizan para
        # no pagar una reserva de varios MB en cada fotograma.
        target_h = max(
            buf.shape[0],
            int(round((H_ISLAND + self.viewport_height) * self._scale)),
        )
        shape = (target_h, buf.shape[1], 4)
        if self._layer_shape != shape or self._layers is None:
            self._layer_shape = shape
            self._layers = (
                np.zeros(shape, np.float32),
                np.zeros(shape, np.float32),
                np.zeros(shape, np.float32),
            )
            self._layer_signature = None
        header_layer, content_layer, footer_layer = self._layers
        layout = self._layout()
        scroll = self.scroll_offset
        reuse = False
        if self._layers_static(now):
            signature = self._layer_signature_value(now)
            if signature == self._layer_signature:
                reuse = True
        if not reuse:
            for layer in self._layers:
                layer.fill(0.0)
            self._draw_header(header_layer, layout, pal, now)
            for group_name, rows in self._shown_groups():
                self._draw_group(content_layer, group_name, rows, layout, pal, now, scroll)
            self._draw_footer(footer_layer, layout, pal, now)
            self._layer_signature = self._layer_signature_value(now)

        window_h = H_ISLAND + float(self._height.value)
        content_top = self._content_top(layout)
        footer_top = self._footer_top(layout)
        _composite_band(
            buf,
            header_layer,
            H_ISLAND,
            min(content_top, window_h),
            self._scale,
            GROUP_X0,
            (GROUP_X1 + self._dx),
        )
        if content_top < footer_top:
            _composite_band(
                buf,
                content_layer,
                content_top,
                min(footer_top, window_h),
                self._scale,
                GROUP_X0,
                (GROUP_X1 + self._dx),
            )
        if footer_top < window_h:
            _composite_band(
                buf,
                footer_layer,
                footer_top,
                window_h,
                self._scale,
                GROUP_X0,
                (GROUP_X1 + self._dx),
            )

        self._draw_scroll(buf, layout, pal, now, scroll)
        np.minimum(buf[..., :3], buf[..., 3:4], out=buf[..., :3])

    def _text_at(
        self,
        layer: np.ndarray,
        text: str,
        style: FontStyle,
        color: Color,
        x: float,
        center_y: float,
        alpha: float,
        align: Literal["left", "center", "right"] = "left",
    ) -> None:
        if not text or alpha <= 0.001:
            return
        _width, height = text_size(text, style, self._scale)
        draw_text(
            layer,
            text,
            style,
            color,
            x,
            center_y - height / 2.0,
            self._scale,
            align=align,
            alpha=min(1.0, max(0.0, alpha)),
        )

    def _draw_header(
        self,
        layer: np.ndarray,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
    ) -> None:
        alpha, dy = self._block_values("header", now)
        if alpha <= 0.001:
            return
        header = layout["header"]
        cy = (header.y0 + header.y1) / 2.0 + dy
        self._text_at(layer, self._title, FS_TITLE, pal["TEXT"], LABEL_X, cy, alpha)
        hover = max(0.0, min(1.0, float(self._button_hover["hide"].value)))
        press = max(0.0, min(1.0, float(self._press["hide"].value)))
        if hover > 0.001 or press > 0.001:
            if self._theme == "light":
                overlay: Color = (0, 0, 0)
                overlay_alpha = HIDE_HOVER_LIGHT * hover + HIDE_PRESS_LIGHT * press
            else:
                overlay = (255, 255, 255)
                overlay_alpha = HIDE_HOVER_DARK * hover + HIDE_PRESS_DARK * press
            _fill_circle(
                layer,
                (HIDE_CX + self._dx),
                cy,
                HIDE_SIZE,
                overlay,
                self._scale,
                alpha=alpha * overlay_alpha,
            )
        icon_size = HIDE_ICON_SIZE * (1.0 - (1.0 - HIDE_ICON_PRESS_SCALE) * press)
        draw_icon(
            layer,
            "close",
            (HIDE_CX + self._dx),
            cy,
            icon_size,
            pal["TEXT_2"],
            self._scale,
            alpha=alpha,
        )

    def _draw_group(
        self,
        layer: np.ndarray,
        group_name: str,
        rows: tuple[tuple[str, str], ...],
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
        scroll: float,
    ) -> None:
        alpha, dy = self._block_values(group_name, now)
        if alpha <= 0.001:
            return
        y_offset = dy - scroll
        label_rect = layout["label_" + group_name]
        self._text_at(
            layer,
            GROUP_LABELS[group_name],
            FS_MICRO,
            pal["TEXT_3"],
            LABEL_X,
            (label_rect.y0 + label_rect.y1) / 2.0 + y_offset,
            alpha,
        )
        group = layout["group_" + group_name]
        gx0 = GROUP_X0
        gx1 = GROUP_X1 + self._dx
        gy0 = group.y0 + y_offset
        gy1 = group.y1 + y_offset
        _fill_round_rect(
            layer,
            gx0,
            gy0,
            gx1,
            gy1,
            R_GROUP,
            pal["GROUP"],
            self._scale,
            alpha,
            replace=True,
        )

        last = len(rows) - 1
        for index, (key, kind) in enumerate(rows):
            row = layout[key]
            base_h = float(ROW[kind])
            row_y0 = row.y0 + y_offset
            row_y1 = row_y0 + base_h
            disabled = kind == "select" and not self.settings.selector_enabled(key)
            hover = 0.0
            if not disabled:
                hover = max(
                    0.0,
                    min(1.0, float(self._row_hover[key].value)),
                )
            if hover > 0.001:
                fill = lerp_rgb(
                    pal["GROUP"],
                    pal["GROUP_HOVER"],
                    hover,  # type: ignore[arg-type]
                )
                _fill_round_rect(
                    layer,
                    GROUP_X0,
                    row_y0,
                    (GROUP_X1 + self._dx),
                    row_y1,
                    0.0,
                    fill,
                    self._scale,
                    alpha,
                    radius_top=R_GROUP if index == 0 else 0.0,
                    radius_bottom=R_GROUP if index == last else 0.0,
                )
            self._draw_row(
                layer,
                key,
                kind,
                row,
                row_y0,
                layout,
                pal,
                now,
                scroll,
                alpha,
            )
            if key == self._selector_key:
                self._draw_selector_list(layer, key, row_y0, layout, pal, now, alpha)
            if index < last:
                boundary = row.y1 + y_offset
                _fill_round_rect(
                    layer,
                    ROW_X0,
                    boundary - HAIRLINE_PX / 2.0,
                    (ROW_X1 + self._dx),
                    boundary + HAIRLINE_PX / 2.0,
                    0.0,
                    pal["LINE_2"],
                    self._scale,
                    alpha,
                )

        if self._theme == "light":
            _stroke_round_rect(
                layer,
                gx0,
                gy0,
                gx1,
                gy1,
                R_GROUP,
                pal["LINE"],
                self._scale,
                alpha,
            )

    def _draw_row(
        self,
        layer: np.ndarray,
        key: str,
        kind: str,
        row: LayoutRect,
        row_y0: float,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
        scroll: float,
        alpha: float,
    ) -> None:
        disabled = kind == "select" and not self.settings.selector_enabled(key)
        center_y = row_y0 + float(ROW[kind]) / 2.0
        if kind == "slider":
            self._draw_slider_row(layer, key, row_y0, pal, alpha)
        elif kind == "select":
            self._draw_select_row(layer, key, row_y0, center_y, pal, alpha, disabled)
        elif kind == "toggle":
            self._draw_toggle_row(layer, key, center_y, pal, alpha)
        elif kind == "text":
            self._draw_text_row(layer, key, row_y0, pal, alpha, now)
        elif kind == "meter":
            self._draw_meter_row(layer, key, row_y0, pal, alpha, now)
        elif kind == "palette":
            self._draw_palette_row(layer, row_y0, pal, alpha)
        elif kind == "action":
            self._draw_action_row(layer, key, row_y0, center_y, layout, pal, now, scroll, alpha)

    _PALETTES = (("light", "Claro"), ("dark", "Oscuro"), ("custom", "Personalizado"))

    def _palette_pills(self, row_y0: float) -> list[tuple[str, str, Rect]]:
        """Las tres muestras de la fila de paletas, en coordenadas del menú."""
        x0, x1 = ROW_X0, ROW_X1 + self._dx
        gap = 10.0
        width = (x1 - x0 - gap * 2.0) / 3.0
        out = []
        for index, (value, label) in enumerate(self._PALETTES):
            left = x0 + index * (width + gap)
            out.append((value, label, Rect(left, row_y0 + 10.0, left + width, row_y0 + 36.0)))
        return out

    def _palette_stops(self, value: str) -> np.ndarray:
        """Colores de cada tema: fondo, principal, secundario y acento."""
        from .flow import COLORS, custom_palette

        if value == "custom":
            primary, secondary, accent, background = self.settings.custom_colors()
            palette = custom_palette(primary, secondary, accent, background)
            if palette is not None:
                return palette[:4]
            value = "light"
        return COLORS["light" if value == "light" else "dark"]["idle"][:4]

    def _draw_palette_row(
        self, layer: np.ndarray, row_y0: float, pal: dict[str, Color], alpha: float
    ) -> None:
        active = str(self.settings.get("theme") or "")
        for value, label, pill in self._palette_pills(row_y0):
            _fill_round_gradient(
                layer,
                pill.x0,
                pill.y0,
                pill.x1,
                pill.y1,
                12.0,
                self._palette_stops(value),
                self._scale,
                alpha,
            )
            if value == active:
                for inset in (-3.0, -2.0):
                    _stroke_round_rect(
                        layer,
                        pill.x0 + inset,
                        pill.y0 + inset,
                        pill.x1 - inset,
                        pill.y1 - inset,
                        12.0 - inset,
                        pal["MINT"],
                        self._scale,
                        alpha,
                    )
            color = pal["MINT"] if value == active else pal["TEXT_2"]
            self._text_at(
                layer,
                label,
                FS_SECONDARY,
                color,
                (pill.x0 + pill.x1) / 2.0,
                row_y0 + 52.0,
                alpha,
                align="center",
            )

    def _draw_color_swatch(
        self, layer: np.ndarray, key: str, row_y0: float, pal: dict[str, Color], alpha: float
    ) -> None:
        """Muestra redonda del color de la fila (lo que se escribe, si ya vale)."""
        from .flow import parse_hex

        raw = self._edit_buffer if self._editing_key == key else self.settings.get(key)
        rgb = parse_hex(str(raw or "")) or parse_hex(str(self.settings.get(key) or ""))
        if rgb is None:
            return
        cx, cy = ROW_X1 + self._dx - 11.0, row_y0 + 26.0
        _fill_circle(layer, cx, cy, 24.0, pal["LINE"], self._scale, alpha)
        _fill_circle(layer, cx, cy, 22.0, tuple(int(c) for c in rgb), self._scale, alpha)

    def _draw_select_row(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        center_y: float,
        pal: dict[str, Color],
        alpha: float,
        disabled: bool,
    ) -> None:
        label_alpha = alpha * (DISABLED_OPACITY if disabled else 1.0)
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            center_y,
            label_alpha,
        )
        value = self.settings.value_label(key)
        if key == "input_device_index" and not self.settings.devices:
            value_color = pal["ERR"]
            value_alpha = alpha
        else:
            value_color = pal["TEXT_2"]
            value_alpha = alpha * (DISABLED_OPACITY if disabled else 1.0)
        fitted = _fit_text(value, FS_BODY, (SELECT_VALUE_MAX_W + self._dx), self._scale)
        self._text_at(
            layer,
            fitted,
            FS_BODY,
            value_color,
            (SELECT_VALUE_X1 + self._dx),
            center_y,
            value_alpha,
            align="right",
        )
        fraction = 0.0
        if key == self._selector_key:
            fraction = max(0.0, min(1.0, float(self._chevron[key].value)))
        chevron_alpha = alpha * (DISABLED_OPACITY if disabled else 1.0)
        if fraction < 0.999:
            draw_icon(
                layer,
                "chevron-down",
                (CHEVRON_CX + self._dx),
                center_y,
                CHEVRON_SIZE,
                pal["TEXT_3"],
                self._scale,
                alpha=chevron_alpha * (1.0 - fraction),
            )
        if fraction > 0.001:
            draw_icon(
                layer,
                "chevron-up",
                (CHEVRON_CX + self._dx),
                center_y,
                CHEVRON_SIZE,
                pal["TEXT_3"],
                self._scale,
                alpha=chevron_alpha * fraction,
            )

    def _meter_fraction(self, value: float) -> float:
        """Convierte un RMS lineal a la posición dBFS del medidor."""
        try:
            rms = max(0.0, float(value))
        except (TypeError, ValueError):
            rms = 0.0
        if rms <= 0.0:
            db = METER_DB_FLOOR
        else:
            db = 20.0 * math.log10(rms)
        return max(
            0.0,
            min(
                1.0,
                (db - METER_DB_FLOOR) / (METER_DB_CEIL - METER_DB_FLOOR),
            ),
        )

    def _draw_meter_row(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        pal: dict[str, Color],
        alpha: float,
        now: float,
    ) -> None:
        """Medidor con la misma pista y el mismo lenguaje que los sliders.

        El relleno mint es el nivel de entrada en vivo; el thumb blanco es el
        umbral y se arrastra. La fila conserva label/valor arriba y estado
        abajo, como el resto del menú.
        """
        threshold = float(self.settings.get("min_rms") or 0.0)
        threshold_db = 20.0 * math.log10(threshold) if threshold > 0.0 else METER_DB_FLOOR
        self._text_at(
            layer,
            LABELS["input_meter"],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            row_y0 + 17.0,
            alpha,
        )
        self._text_at(
            layer,
            f"{threshold_db:.0f} dB",
            FS_VALUE,
            pal["TEXT_2"],
            (ROW_X1 + self._dx),
            row_y0 + 17.0,
            alpha,
            align="right",
        )

        x0 = SLIDER_TRACK_X0
        x1 = SLIDER_TRACK_X1 + self._dx
        cy = row_y0 + METER_TRACK_Y + METER_TRACK_H / 2.0
        _fill_round_rect(
            layer,
            x0,
            cy - METER_TRACK_H / 2.0,
            x1,
            cy + METER_TRACK_H / 2.0,
            METER_TRACK_H / 2.0,
            pal["TRACK"],
            self._scale,
            alpha,
        )
        level = self._meter_fraction(self._meter_level)
        if level > 0.0:
            _fill_round_rect(
                layer,
                x0,
                cy - METER_TRACK_H / 2.0,
                x0 + (x1 - x0) * level,
                cy + METER_TRACK_H / 2.0,
                METER_TRACK_H / 2.0,
                pal["MINT_BTN"],
                self._scale,
                alpha,
            )
        threshold_fraction = self._meter_fraction(threshold)
        thumb_cx = x0 + (x1 - x0) * threshold_fraction
        thumb_color: Color = (255, 255, 255) if self._theme == "light" else pal["TEXT"]
        _fill_circle(
            layer,
            thumb_cx,
            cy,
            METER_THUMB,
            thumb_color,
            self._scale,
            alpha,
        )
        if self._theme == "light":
            _stroke_round_rect(
                layer,
                thumb_cx - METER_THUMB / 2.0,
                cy - METER_THUMB / 2.0,
                thumb_cx + METER_THUMB / 2.0,
                cy + METER_THUMB / 2.0,
                METER_THUMB / 2.0,
                pal["LINE"],
                self._scale,
                alpha,
            )

        # La fila no dibuja estado ni nivel textual: solo el medidor, la
        # etiqueta y el umbral en dB. El valor lineal de min_rms tampoco se
        # muestra; se ajusta arrastrando el thumb.

    def _draw_text_row(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        pal: dict[str, Color],
        alpha: float,
        now: float,
    ) -> None:
        editing = self._editing_key == key
        label_cy = row_y0 + TEXT_LINE1_CY
        value_cy = row_y0 + TEXT_LINE2_CY
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            label_cy,
            alpha,
        )
        if key in hk.HOTKEY_KEYS:
            self._draw_hotkey_value(layer, key, row_y0, pal, alpha, now)
            return
        if editing:
            raw = self._edit_buffer
            display = raw
            color = pal["TEXT"]
        else:
            raw = str(self.settings.get(key, "") or "")
            display = self.settings.format(key, raw)
            if key in SECRET_KEYS:
                color = pal["TEXT_2"] if raw else pal["TEXT_3"]
            else:
                color = pal["TEXT_2"] if raw else pal["TEXT_3"]

        max_w = (ROW_X1 + self._dx) - ROW_X0
        if editing:
            # La fila se lee como un campo: fondo tenue y cursor visible.
            _fill_round_rect(
                layer,
                ROW_X0 - 6.0,
                value_cy - 11.0,
                (ROW_X1 + self._dx) + 6.0,
                value_cy + 11.0,
                6.0,
                pal["TRACK"],
                self._scale,
                alpha,
            )
        secret = key in SECRET_KEYS
        if secret:
            self._draw_secret_value(layer, raw, editing, value_cy, pal, alpha)
            self._draw_copy_button(layer, key, row_y0, pal, alpha, now)
        else:
            fitted = _fit_text(display or " ", FS_BODY, max_w, self._scale)
            self._text_at(layer, fitted, FS_BODY, color, ROW_X0, value_cy, alpha)
        if key in COLOR_KEYS:
            self._draw_color_swatch(layer, key, row_y0, pal, alpha)
        if editing and int((now - self._edit_t0) * 2.0) % 2 == 0:
            cursor = max(0, min(len(raw), self._edit_cursor))
            prefix = raw[:cursor]
            if secret:
                width = min(len(prefix), DOT_MAX) * DOT_PITCH
            else:
                width = min(max_w, text_size(prefix, FS_BODY, self._scale)[0])
            _fill_round_rect(
                layer,
                ROW_X0 + width,
                value_cy - TEXT_CARET_H / 2.0,
                ROW_X0 + width + TEXT_CARET_W,
                value_cy + TEXT_CARET_H / 2.0,
                TEXT_CARET_W / 2.0,
                pal["MINT"],
                self._scale,
                alpha,
            )

    def _draw_keycap(
        self,
        layer: np.ndarray,
        x: float,
        cy: float,
        label: str,
        pal: dict[str, Color],
        alpha: float,
        *,
        accent: bool = False,
        ghost: bool = False,
    ) -> float:
        """Una tecla dibujada; devuelve la ``x`` donde empieza la siguiente."""
        width = text_size(label, FS_SECONDARY, self._scale)[0] + 2.0 * KEYCAP_PAD
        width = max(width, KEYCAP_H)
        y0, y1 = cy - KEYCAP_H / 2.0, cy + KEYCAP_H / 2.0
        box = (x, y0, x + width, y1)
        if ghost:
            _stroke_round_rect(layer, *box, 6.0, pal["TEXT_3"], self._scale, alpha * 0.7)
            color = pal["TEXT_3"]
        elif accent:
            _fill_round_rect(layer, *box, 6.0, pal["MINT"], self._scale, alpha * 0.18)
            _stroke_round_rect(layer, *box, 6.0, pal["MINT"], self._scale, alpha * 0.8)
            color = pal["MINT"]
        else:
            _fill_round_rect(layer, *box, 6.0, pal["TRACK"], self._scale, alpha * 0.8)
            color = pal["TEXT"]
        self._text_at(layer, label, FS_SECONDARY, color, x + KEYCAP_PAD, cy, alpha)
        return x + width + KEYCAP_GAP

    def _draw_hotkey_value(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        pal: dict[str, Color],
        alpha: float,
        now: float,
    ) -> None:
        """Atajo como teclas dibujadas; al hacer clic captura la combinación en vivo."""
        cy = row_y0 + TEXT_LINE2_CY
        right = ROW_X1 + self._dx
        if not (self._capturing and self._editing_key == key):
            x = ROW_X0
            for label in hk.parts_for(str(self.settings.get(key, "") or "")):
                x = self._draw_keycap(layer, x, cy, label, pal, alpha)
            hover = max(0.0, min(1.0, float(self._row_hover[key].value)))
            if hover > 0.01:
                self._text_at(
                    layer,
                    "Cambiar",
                    FS_BODY,
                    pal["TEXT_3"],
                    right,
                    cy,
                    alpha * hover,
                    align="right",
                )
            return
        top, bottom = cy - KEYCAP_H / 2.0 - 1.5, cy + KEYCAP_H / 2.0 + 1.5
        field = (ROW_X0 - 8.0, top, right + 6.0, bottom)
        _fill_round_rect(layer, *field, 9.0, pal["TRACK"], self._scale, alpha * 0.5)
        _stroke_round_rect(layer, *field, 9.0, pal["MINT"], self._scale, alpha)
        if self._capture_msg is not None and now < self._capture_msg[1]:
            self._text_at(layer, self._capture_msg[0], FS_BODY, pal["ERR"], ROW_X0, cy, alpha)
            return
        x = ROW_X0
        for label in hk.mod_labels(self._capture_mods):
            x = self._draw_keycap(layer, x, cy, label, pal, alpha, accent=True)
        if self._capture_mods:
            self._draw_keycap(layer, x, cy, "…", pal, alpha, ghost=True)
            return
        pulse = 0.65 + 0.35 * math.sin((now - self._edit_t0) * 5.0)
        self._text_at(
            layer, "Pulsa la combinación", FS_BODY, pal["TEXT_2"], ROW_X0, cy, alpha * pulse
        )
        self._text_at(layer, "Esc cancela", FS_BODY, pal["TEXT_3"], right, cy, alpha, align="right")

    def _draw_secret_value(
        self,
        layer: np.ndarray,
        raw: str,
        editing: bool,
        value_cy: float,
        pal: dict[str, Color],
        alpha: float,
    ) -> None:
        """Puntos en lugar de la clave: se ve cuánto mide, nunca qué dice."""
        if not raw and not editing:
            self._text_at(layer, "Sin clave", FS_BODY, pal["TEXT_3"], ROW_X0, value_cy, alpha)
            return
        color = pal["TEXT"] if editing else pal["TEXT_2"]
        for index in range(min(len(raw), DOT_MAX)):
            cx = ROW_X0 + DOT_R + index * DOT_PITCH
            _fill_circle(layer, cx, value_cy, DOT_R * 2.0, color, self._scale, alpha)

    def _draw_copy_button(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        pal: dict[str, Color],
        alpha: float,
        now: float,
    ) -> None:
        """Icono «copiar» (un check un momento después de copiar) con fondo al pasar."""
        if not self._secret_text(key):
            return
        rect = self._copy_rect(row_y0)
        cx, cy = rect.center
        copied = now < self._copied.get(key, 0.0)
        hovered = self._copy_hover == key
        if hovered and not copied:
            _fill_circle(layer, cx, cy, COPY_BTN, pal["GROUP_HOVER"], self._scale, alpha)
        if copied:
            icon, color = "check", pal["MINT"]
        else:
            icon, color = "copy", pal["TEXT"] if hovered else pal["TEXT_2"]
        draw_icon(layer, icon, cx, cy, COPY_ICON, color, self._scale, alpha=alpha)

    def _draw_slider_row(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        pal: dict[str, Color],
        alpha: float,
    ) -> None:
        line_cy = row_y0 + SLIDER_LINE1_TOP + SLIDER_LINE1_H / 2.0
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            line_cy,
            alpha,
        )
        active = max(0.0, min(1.0, float(self._slider_color[key].value)))
        value_color = lerp_rgb(
            pal["TEXT_2"],
            pal["MINT"],
            active,  # type: ignore[arg-type]
        )
        self._text_at(
            layer,
            self.settings.format(key),
            FS_VALUE,
            value_color,
            (ROW_X1 + self._dx),
            line_cy,
            alpha,
            align="right",
        )

        track_cy = row_y0 + SLIDER_LINE2_CY
        _fill_round_rect(
            layer,
            SLIDER_TRACK_X0,
            track_cy - SLIDER_TRACK_H / 2.0,
            (SLIDER_TRACK_X1 + self._dx),
            track_cy + SLIDER_TRACK_H / 2.0,
            SLIDER_TRACK_H / 2.0,
            pal["TRACK"],
            self._scale,
            alpha,
        )
        spec = SLIDERS[key]
        value = float(self.settings.get(key))
        span = spec.maximum - spec.minimum
        fraction = 0.0 if span <= 0 else (value - spec.minimum) / span
        fraction = max(0.0, min(1.0, fraction))
        width = (SLIDER_TRACK_X1 + self._dx) - SLIDER_TRACK_X0
        thumb_cx = SLIDER_TRACK_X0 + fraction * width
        if thumb_cx > SLIDER_TRACK_X0 + 0.01:
            _fill_round_rect(
                layer,
                SLIDER_TRACK_X0,
                track_cy - SLIDER_TRACK_H / 2.0,
                thumb_cx,
                track_cy + SLIDER_TRACK_H / 2.0,
                SLIDER_TRACK_H / 2.0,
                pal["MINT_BTN"],
                self._scale,
                alpha,
            )
        diameter = max(
            SLIDER_THUMB_REST,
            min(SLIDER_THUMB_DRAG, float(self._thumb[key].value)),
        )
        radius = diameter / 2.0
        thumb_color: Color = (255, 255, 255) if self._theme == "light" else pal["TEXT"]
        _fill_circle(layer, thumb_cx, track_cy, diameter, thumb_color, self._scale, alpha)
        if self._theme == "light":
            _stroke_round_rect(
                layer,
                thumb_cx - radius,
                track_cy - radius,
                thumb_cx + radius,
                track_cy + radius,
                radius,
                pal["LINE"],
                self._scale,
                alpha,
            )

    def _draw_toggle_row(
        self,
        layer: np.ndarray,
        key: str,
        center_y: float,
        pal: dict[str, Color],
        alpha: float,
    ) -> None:
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            center_y,
            alpha,
        )
        if key == "show_advanced":
            hidden = len(ADVANCED_ROWS)
            note = f"{hidden} visibles" if self.settings.get(key) else f"+{hidden} ajustes"
            self._text_at(
                layer,
                note,
                FS_VALUE,
                pal["TEXT_3"],
                (TOGGLE_X0 + self._dx) - 12.0,
                center_y,
                alpha,
                align="right",
            )
        color_k = max(0.0, min(1.0, float(self._toggle_color[key].value)))
        pos = max(0.0, min(1.0, float(self._toggle_pos[key].value)))
        track_color = lerp_rgb(
            pal["TRACK"],
            pal["MINT_BTN"],
            color_k,  # type: ignore[arg-type]
        )
        track_y0 = center_y - TOGGLE_H / 2.0
        _fill_round_rect(
            layer,
            (TOGGLE_X0 + self._dx),
            track_y0,
            (TOGGLE_X0 + self._dx) + TOGGLE_W,
            track_y0 + TOGGLE_H,
            TOGGLE_H / 2.0,
            track_color,
            self._scale,
            alpha,
        )
        thumb_color = lerp_rgb(
            pal["TEXT_2"],
            pal["MINT_INK"],
            color_k,  # type: ignore[arg-type]
        )
        travel = TOGGLE_W - TOGGLE_PAD * 2.0 - TOGGLE_THUMB
        thumb_cx = (TOGGLE_X0 + self._dx) + TOGGLE_PAD + TOGGLE_THUMB / 2.0 + pos * travel
        _fill_circle(
            layer,
            thumb_cx,
            center_y,
            TOGGLE_THUMB,
            thumb_color,
            self._scale,
            alpha,
        )

    def _draw_action_row(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        center_y: float,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
        scroll: float,
        alpha: float,
    ) -> None:
        if key != "test_voice":
            self._draw_generic_action_row(
                layer, key, center_y, pal, alpha, armed=self.is_armed(key, now)
            )
            return
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            center_y,
            alpha,
        )
        testing = max(0.0, min(1.0, float(self._testing_t.value)))
        width = self._test_width()
        rect = Rect(
            (TEST_BUTTON_X1 + self._dx) - width,
            center_y - BUTTON_H / 2.0,
            (TEST_BUTTON_X1 + self._dx),
            center_y + BUTTON_H / 2.0,
        )
        hover = max(0.0, min(1.0, float(self._button_hover["test"].value)))
        press = max(0.0, min(1.0, float(self._press["test"].value)))
        hover *= 1.0 - testing
        variant = "disabled" if testing > 0.01 else "secondary"
        fill, text_color = self._button_colors(variant, pal)
        if hover > 0.001:
            target: Color = (255, 255, 255) if self._theme != "light" else (0, 0, 0)
            fill = lerp_rgb(fill, target, BUTTON_HOVER_MIX * hover)  # type: ignore[arg-type]
        scale_factor = 1.0 - (1.0 - BUTTON_PRESS_SCALE) * press
        draw_rect = self._scaled_rect(rect, scale_factor)
        _fill_round_rect(
            layer,
            draw_rect.x0,
            draw_rect.y0,
            draw_rect.x1,
            draw_rect.y1,
            BUTTON_RADIUS * scale_factor,
            fill,
            self._scale,
            alpha,
        )
        if self._theme == "light" and variant == "secondary":
            _stroke_round_rect(
                layer,
                draw_rect.x0,
                draw_rect.y0,
                draw_rect.x1,
                draw_rect.y1,
                BUTTON_RADIUS * scale_factor,
                pal["LINE"],
                self._scale,
                alpha,
            )

        if testing < 0.999:
            self._text_at(
                layer,
                "Probar",
                FS_SECONDARY,
                text_color,
                draw_rect.center[0],
                draw_rect.center[1],
                alpha * (1.0 - testing),
                align="center",
            )
        if testing > 0.001:
            content_w = self._test_content_width()
            wave_w = TEST_WAVE_BARS * TEST_WAVE_BAR_W + (TEST_WAVE_BARS - 1) * TEST_WAVE_GAP
            start_x = draw_rect.center[0] - content_w / 2.0
            bar_cy = draw_rect.center[1]
            for index in range(TEST_WAVE_BARS):
                phase = now * TEST_WAVE_PHASE_SPEED + index * TEST_WAVE_PHASE_STEP
                level = TEST_WAVE_LEVEL_BASE + TEST_WAVE_LEVEL_SPAN * abs(math.sin(phase))
                height = TEST_WAVE_MIN_H + (TEST_WAVE_MAX_H - TEST_WAVE_MIN_H) * level
                bar_x = start_x + index * (TEST_WAVE_BAR_W + TEST_WAVE_GAP)
                _fill_round_rect(
                    layer,
                    bar_x,
                    bar_cy - height / 2.0,
                    bar_x + TEST_WAVE_BAR_W,
                    bar_cy + height / 2.0,
                    TEST_WAVE_BAR_W / 2.0,
                    pal["MINT"],
                    self._scale,
                    alpha * testing,
                )
            self._text_at(
                layer,
                "Hablando…",
                FS_SECONDARY,
                text_color,
                start_x + wave_w + TEST_WAVE_GAP_TEXT,
                bar_cy,
                alpha * testing,
            )

    def _draw_generic_action_row(
        self,
        layer: np.ndarray,
        key: str,
        center_y: float,
        pal: dict[str, Color],
        alpha: float,
        armed: bool = False,
    ) -> None:
        """Fila con etiqueta y un botón («Abrir», «Repetir», «Borrar»…)."""
        self._text_at(
            layer,
            LABELS[key],
            FS_ROW,
            pal["TEXT"],
            ROW_X0,
            center_y,
            alpha,
        )
        rect = Rect(
            (TEST_BUTTON_X1 + self._dx) - BUTTON_MIN_W,
            center_y - BUTTON_H / 2.0,
            (TEST_BUTTON_X1 + self._dx),
            center_y + BUTTON_H / 2.0,
        )
        hover = max(0.0, min(1.0, float(self._row_hover[key].value)))
        fill, text_color = self._button_colors("amber" if armed else "secondary", pal)
        if hover > 0.001:
            target: Color = (255, 255, 255) if self._theme != "light" else (0, 0, 0)
            fill = lerp_rgb(fill, target, BUTTON_HOVER_MIX * hover)  # type: ignore[arg-type]
        _fill_round_rect(
            layer,
            rect.x0,
            rect.y0,
            rect.x1,
            rect.y1,
            BUTTON_RADIUS,
            fill,
            self._scale,
            alpha,
        )
        if self._theme == "light":
            _stroke_round_rect(
                layer,
                rect.x0,
                rect.y0,
                rect.x1,
                rect.y1,
                BUTTON_RADIUS,
                pal["LINE"],
                self._scale,
                alpha,
            )
        self._text_at(
            layer,
            ARMED_TEXT if armed else ACTION_BUTTON_TEXT.get(key, "Abrir"),
            FS_SECONDARY,
            text_color,
            rect.center[0],
            rect.center[1],
            alpha,
            align="center",
        )

    def _draw_selector_list(
        self,
        layer: np.ndarray,
        key: str,
        row_y0: float,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
        alpha: float,
    ) -> None:
        options = self._selector_options(key)
        if not options:
            return
        top = row_y0 + ROW["select"]
        visible = max(
            0.0,
            min(
                float(self._selector_extra.value),
                len(options) * LIST_ITEM_H,
            ),
        )
        if visible <= 0.01:
            return
        list_scroll = max(
            0.0,
            min(
                self._list_scroll,
                max(0.0, len(options) * LIST_ITEM_H - LIST_VIEW_H),
            ),
        )
        selected = self.settings.get(key)
        for index, option in enumerate(options):
            item_y0 = top + index * LIST_ITEM_H - list_scroll
            item_y1 = item_y0 + LIST_ITEM_H
            if item_y0 < top - 0.5 or item_y1 > top + visible + 0.5:
                continue
            item_alpha = alpha * self._selector_item_alpha(index, options, now)
            if item_alpha <= 0.001:
                continue
            center_y = (item_y0 + item_y1) / 2.0
            if self._hover_option == index:
                _fill_round_rect(
                    layer,
                    GROUP_X0,
                    item_y0,
                    (GROUP_X1 + self._dx),
                    item_y1,
                    0.0,
                    pal["GROUP_HOVER"],
                    self._scale,
                    item_alpha,
                )
            chosen = option.value == selected
            if chosen:
                _fill_round_rect(
                    layer,
                    GROUP_X0,
                    item_y0,
                    (GROUP_X1 + self._dx),
                    item_y1,
                    0.0,
                    pal["MINT"],
                    self._scale,
                    item_alpha * 0.10,
                )
            label = _fit_text(option.label, FS_BODY, (LIST_TEXT_MAX_W + self._dx), self._scale)
            self._text_at(
                layer,
                label,
                FS_BODY,
                pal["MINT"] if chosen else pal["TEXT"],
                LIST_TEXT_X,
                center_y,
                item_alpha,
            )
            if option.value == selected:
                draw_icon(
                    layer,
                    "check",
                    (LIST_CHECK_CX + self._dx),
                    center_y,
                    LIST_CHECK_SIZE,
                    pal["MINT_BTN"],
                    self._scale,
                    alpha=item_alpha,
                )

    def _button_colors(self, variant: str, pal: dict[str, Color]) -> tuple[Color, Color]:
        """Colores de fondo y texto de las variantes de C-07."""
        fill_key, text_key = BUTTON_VARIANTS[variant]
        return pal[fill_key], pal[text_key]

    @staticmethod
    def _scaled_rect(rect: Rect, factor: float) -> Rect:
        cx, cy = rect.center
        half_w = rect.width * factor / 2.0
        half_h = rect.height * factor / 2.0
        return Rect(cx - half_w, cy - half_h, cx + half_w, cy + half_h)

    def _draw_footer(
        self,
        layer: np.ndarray,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
    ) -> None:
        alpha, dy = self._block_values("footer", now)
        if alpha <= 0.001:
            return
        footer = layout["footer"]
        center_y = (footer.y0 + footer.y1) / 2.0 + dy
        dirty = self.settings.dirty()
        if now < self._error_until:
            state = ("NO SE PUDO GUARDAR", "ERR", False)
        elif now < self._saved_flash_until:
            state = ("TODO GUARDADO", "MINT", False)
        elif dirty:
            state = ("CAMBIOS SIN GUARDAR", "WARM", True)
        else:
            state = ("TODO GUARDADO", "TEXT_3", False)

        if state != self._footer_state:
            self._footer_prev = self._footer_state
            self._footer_state = state
            self._footer_state_at = now
        fade = 1.0
        if self._footer_prev is not None:
            progress = max(0.0, min(1.0, (now - self._footer_state_at) / (FOOTER_FADE_MS / 1000.0)))
            fade = EASE_OUT(progress)
            if progress >= 1.0:
                self._footer_prev = None
            else:
                self._draw_footer_state(
                    layer,
                    self._footer_prev,
                    pal,
                    center_y,
                    alpha * (1.0 - fade),
                )
        self._draw_footer_state(layer, state, pal, center_y, alpha * fade)

        rect = self._save_rect(layout)
        rect = Rect(rect.x0, rect.y0 + dy, rect.x1, rect.y1 + dy)
        enabled = dirty
        variant = "amber" if enabled else "disabled"
        if variant != self._save_variant:
            if self._save_variant is not None:
                (
                    self._save_prev_fill,
                    self._save_prev_text,
                ) = self._button_colors(self._save_variant, pal)
            else:
                self._save_prev_fill = None
                self._save_prev_text = None
            self._save_variant = variant
            self._save_variant_at = now
        base, text_color = self._button_colors(variant, pal)
        if self._save_prev_fill is not None and self._save_prev_text is not None:
            progress = max(
                0.0,
                min(
                    1.0,
                    (now - self._save_variant_at) / (SAVE_COLOR_MS / 1000.0),
                ),
            )
            if progress >= 1.0:
                self._save_prev_fill = None
                self._save_prev_text = None
            else:
                k = EASE_OUT(progress)
                base = lerp_rgb(
                    self._save_prev_fill,
                    base,
                    k,  # type: ignore[arg-type]
                )
                text_color = lerp_rgb(
                    self._save_prev_text,
                    text_color,
                    k,  # type: ignore[arg-type]
                )
        hover = max(0.0, min(1.0, float(self._button_hover["save"].value))) if enabled else 0.0
        press = max(0.0, min(1.0, float(self._press["save"].value)))
        fill = base
        if hover > 0.001:
            target: Color = (255, 255, 255) if self._theme != "light" else (0, 0, 0)
            fill = lerp_rgb(fill, target, BUTTON_HOVER_MIX * hover)  # type: ignore[arg-type]
        scale_factor = 1.0 - (1.0 - BUTTON_PRESS_SCALE) * press
        draw_rect = self._scaled_rect(rect, scale_factor)
        _fill_round_rect(
            layer,
            draw_rect.x0,
            draw_rect.y0,
            draw_rect.x1,
            draw_rect.y1,
            BUTTON_RADIUS * scale_factor,
            fill,
            self._scale,
            alpha,
        )
        check_active = now < self._check_until and not dirty
        if check_active:
            remaining = self._check_until - now
            elapsed = SAVE_CHECK_MS / 1000.0 - remaining
            fade = min(
                1.0,
                elapsed / (SAVE_FADE_MS / 1000.0),
                remaining / (SAVE_FADE_MS / 1000.0),
            )
            draw_icon(
                layer,
                "check",
                draw_rect.center[0],
                draw_rect.center[1],
                LIST_CHECK_SIZE,
                pal["MINT_BTN"],
                self._scale,
                alpha=alpha * max(0.0, fade),
            )
        else:
            self._text_at(
                layer,
                "Guardar",
                FS_SECONDARY,
                text_color,
                draw_rect.center[0],
                draw_rect.center[1],
                alpha,
                align="center",
            )

    def _draw_footer_state(
        self,
        layer: np.ndarray,
        state: tuple[str, str, bool],
        pal: dict[str, Color],
        center_y: float,
        alpha: float,
    ) -> None:
        if alpha <= 0.001:
            return
        text, color_key, show_dot = state
        if show_dot:
            dot_cx = FOOTER_TEXT_X + FOOTER_DOT / 2.0
            _fill_circle(
                layer,
                dot_cx,
                center_y,
                FOOTER_DOT,
                pal["WARM"],
                self._scale,
                alpha,
            )
            text_x = FOOTER_TEXT_X + FOOTER_DOT + FOOTER_DOT_GAP
        else:
            text_x = FOOTER_TEXT_X
        self._text_at(
            layer,
            text,
            FS_MICRO,
            pal[color_key],
            text_x,
            center_y,
            alpha,
        )

    def _draw_scroll(
        self,
        buf: np.ndarray,
        layout: dict[str, LayoutRect],
        pal: dict[str, Color],
        now: float,
        scroll: float,
    ) -> None:
        max_scroll = self.scroll_max
        if max_scroll <= 0.5 or float(self._height.value) <= 4.0:
            return
        content_top = self._content_top(layout)
        footer_top = self._footer_top(layout)
        # 1 px hacia dentro: el degradado no debe tapar la línea del borde.
        x0, x1 = BODY_X0 + 1.0, (BODY_X1 + self._dx) - 1.0
        if scroll > 0.5:
            _fill_vertical_gradient(
                buf,
                x0,
                x1,
                content_top,
                content_top + SCROLL_GRADIENT_H,
                pal["BODY"],
                self._scale,
                1.0,
                0.0,
            )
        if scroll < max_scroll - 0.5:
            _fill_vertical_gradient(
                buf,
                x0,
                x1,
                footer_top - SCROLL_GRADIENT_H,
                footer_top,
                pal["BODY"],
                self._scale,
                0.0,
                1.0,
            )
        # El pulgar queda visible en reposo: con muchas secciones hay que
        # saber que el menú sigue sin tener que descubrir la rueda.
        indicator = max(SCROLL_INDICATOR_IDLE_ALPHA, self._indicator_alpha(now))
        if indicator <= 0.01:
            return
        clip_h = max(0.0, footer_top - content_top)
        if clip_h <= 0.0:
            return
        total = clip_h + max_scroll
        indicator_h = max(
            SCROLL_INDICATOR_MIN_H,
            clip_h * (clip_h / total),
        )
        indicator_h = min(indicator_h, clip_h)
        travel = max(0.0, clip_h - indicator_h)
        y0 = content_top + travel * (scroll / max_scroll)
        _fill_round_rect(
            buf,
            (SCROLL_INDICATOR_X + self._dx),
            y0,
            (SCROLL_INDICATOR_X + self._dx) + SCROLL_INDICATOR_W,
            y0 + indicator_h,
            SCROLL_INDICATOR_W / 2.0,
            pal["TEXT_3"],
            self._scale,
            SCROLL_INDICATOR_ALPHA * indicator,
        )


__all__ = [
    "Menu",
    "MenuHit",
]
