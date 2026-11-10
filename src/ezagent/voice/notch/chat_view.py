"""Vista del chat: cabecera, mensajes con scroll, entrada de texto, confirmaciones e historial.

Es pura: dibuja en un buffer float32 RGBA premultiplicado (como el resto del notch) y devuelve
*acciones* («enviar», «cerrar», «copiar»…) en vez de tocar Windows, el portapapeles o la app.
Así se prueba entera; ``chat_ui.py`` la lleva a una ventana y ejecuta las acciones.

Todo se maqueta en píxeles lógicos y se pinta a ``× scale``.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

from ..chat import markdown as md
from ..chat.render import render_message
from ..chat.render_base import Ctx, Hit, paint_lines, round_rect, wrap
from ..chat.session import ChatMessage, ChatModel
from ..chat.text_input import TextInput
from .chat_footer import (
    BTN,
    HINT_H,
    INPUT_LINE,
    INPUT_MAX_LINES,
    INPUT_MIN,
    PAD,
    FooterMixin,
    _break,
)
from .chat_header import HEADER_H, HeaderMixin
from .menu import _fill_circle, _fill_round_rect
from .settings_window import composite_over
from .text import draw_text
from .tokens import FS_MICRO

GAP = 14.0
SCROLL_STEP = 56.0


@dataclass(frozen=True, slots=True)
class Action:
    """Lo que la vista le pide al dueño de la ventana."""

    kind: str  # send | close | voice | copy | open | reveal | confirm | stop | new
    data: object = None


@dataclass(slots=True)
class _Entry:
    """Un mensaje ya dibujado y colocado."""

    height: float
    arr: np.ndarray  # RGBA premultiplicado, a tamaño físico
    x: float
    hits: tuple[Hit, ...] = ()
    caption_hit: tuple[float, float, float, float] | None = None
    plain: str = ""
    caption_x: float = PAD  # dónde empieza la zona «Copiar» (los del usuario, a la derecha)
    label_y: float = 0.0  # altura (relativa al mensaje) del rótulo «Copiar»
    copy_align: str = "left"


@dataclass(slots=True)
class _Cached:
    key: tuple
    entry: _Entry
    at: float = 0.0


@dataclass(slots=True)
class _Geometry:
    width: float = 440.0
    height: float = 600.0
    scale: float = 1.0
    theme: str = "dark"
    pal: dict = field(default_factory=dict)


def _premult(img: Image.Image) -> np.ndarray:
    arr = np.asarray(img.convert("RGBA"), np.float32) / 255.0
    arr[..., :3] *= arr[..., 3:4]
    return arr


class ChatView(HeaderMixin, FooterMixin):
    """Estado y dibujo de la ventana del chat (sin Win32)."""

    def __init__(
        self,
        model: ChatModel,
        roots: tuple[Path, ...] | None = None,
        base_dirs: tuple[Path, ...] = (),
        hotkey_label: str = "Ctrl+Alt+T",
    ) -> None:
        self.model = model
        self.roots = roots
        self.base_dirs = base_dirs
        self.hotkey_label = hotkey_label
        self.input = TextInput()
        self.scroll = 0.0  # px lógicos desde el final (0 = pegado al último mensaje)
        self.mode = "chat"  # «chat» | «history»
        self.hover: str | None = None
        self.g = _Geometry()
        self._cache: dict[int, _Cached] = {}
        self._hits: list[tuple[tuple[float, float, float, float], str, object]] = []
        self._input_lines: list[tuple[int, int]] = []
        self._input_box = (0.0, 0.0, 0.0, 0.0)
        self._max_scroll = 0.0
        self._focus_t0 = time.monotonic()
        self._flash: tuple[str, float] | None = None  # (texto copiado, hasta cuándo se avisa)
        # Lo que el panel anima por encima de la imagen cacheada: el orbe vivo de la cabecera.
        self.status = ""  # «Escuchando», «Pensando»…: lo escribe el panel según la pose de la barra
        self.orb: tuple[float, float, float] | None = None  # (cx, cy, diámetro) lógicos

    # ---------------------------------------------------------- configuración
    def configure(self, width: float, height: float, scale: float, theme: str, pal: dict) -> None:
        """Tamaño, escala y colores; si cambian se rehacen los mensajes."""
        # Los mensajes dependen del ancho, no del alto: estirar hacia arriba o abajo no los rehace.
        key = (round(width, 1), round(scale, 3), theme)
        old = self.g
        if (round(old.width, 1), round(old.scale, 3), old.theme) != key:
            self._cache.clear()
        self.g = _Geometry(width, height, scale, theme, pal)

    @property
    def content_w(self) -> float:
        return self.g.width - 2 * PAD

    def _ctx(self, width: float) -> Ctx:
        return Ctx(self.g.scale, self.g.pal, width, self.roots, self.base_dirs)

    def _color(self, name: str, alpha: float | None = None) -> tuple[int, int, int, int]:
        return self._ctx(1.0).color(name, alpha)

    # --------------------------------------------------------------- mensajes
    def _entry(self, msg: ChatMessage, now: float) -> _Entry:
        """El dibujo de un mensaje; el que llega en streaming se rehace como mucho cada 90 ms."""
        key = (len(msg.text), msg.streaming, msg.error, self.g.theme, round(self.g.width))
        cached = self._cache.get(msg.id)
        if cached is not None and (cached.key == key or (msg.streaming and now - cached.at < 0.09)):
            return cached.entry
        entry = self._build_entry(msg)
        if len(self._cache) > 400:
            self._cache.pop(next(iter(self._cache)))
        self._cache[msg.id] = _Cached(key, entry, now)
        return entry

    def _build_entry(self, msg: ChatMessage) -> _Entry:
        if msg.role == "user":
            return self._user_entry(msg)
        if msg.role == "system":
            return self._note_entry(msg)
        return self._assistant_entry(msg)

    def _note_entry(self, msg: ChatMessage) -> _Entry:
        """Una nota de la app («conversación compactada»): una línea tenue, centrada."""
        ctx = self._ctx(self.content_w * 0.92)
        lines = wrap(ctx, md.parse_inline(msg.text), "small", ctx.width)
        for runs in lines:  # centradas
            used = max((r.x + r.width for r in runs), default=0.0)
            for run in runs:
                run.x += (ctx.width - used) / 2
        height = len(lines) * ctx.size("small") * 1.5 + 8
        sc = self.g.scale
        img = Image.new("RGBA", (round(ctx.width * sc), round(height * sc)), (0, 0, 0, 0))
        paint_lines(img, ctx, lines, "small", 0.0, 4.0, ctx.color("TEXT_3"))
        return _Entry(height, _premult(img), PAD + self.content_w * 0.04, plain=msg.text)

    def _user_entry(self, msg: ChatMessage) -> _Entry:
        blocks = md.parse(msg.text)
        rich = any(isinstance(b, (md.Document, md.Image, md.Table, md.Code)) for b in blocks)
        limit = self.content_w * (0.94 if rich else 0.82)
        measure = self._ctx(limit - 24)
        natural = 0.0
        for block in blocks:
            if isinstance(block, (md.Paragraph, md.Heading, md.ListItem, md.Quote)):
                for runs in wrap(measure, block.spans, "body", limit - 24):
                    natural = max(natural, max((r.x + r.width for r in runs), default=0.0))
        inner = limit - 24 if rich else min(limit - 24, max(natural + 2, 30.0))
        rendered = render_message(
            msg.text, self.g.pal, inner, self.g.scale, self.roots, self.base_dirs
        )
        bw, bh = inner + 24, rendered.height + 18
        sc = self.g.scale
        img = Image.new("RGBA", (round(bw * sc), round(bh * sc)), (0, 0, 0, 0))
        ctx = self._ctx(bw)
        round_rect(img, ctx, (0, 0, bw, bh), 16.0, ctx.color("MINT", 0.15), ctx.color("MINT", 0.34))
        img.alpha_composite(rendered.image, (round(12 * sc), round(9 * sc)))
        hits = tuple(_shift(h, 12.0, 9.0) for h in rendered.hits)
        extra = 20.0  # fila de «Copiar» (y «por voz») bajo la burbuja
        return _Entry(
            bh + extra,
            _premult(img),
            self.g.width - PAD - bw,
            hits,
            caption_hit=(0.0, bh, 64.0, bh + extra),
            plain=msg.text,  # lo escrito, tal cual (sin interpretar markdown)
            caption_x=self.g.width - PAD - 64.0,
            label_y=bh + 4.0,
            copy_align="right",
        )

    def _assistant_entry(self, msg: ChatMessage) -> _Entry:
        pal = dict(self.g.pal)
        if msg.error:
            pal["TEXT"] = pal["ERR"]
        rendered = render_message(
            msg.text, pal, self.content_w, self.g.scale, self.roots, self.base_dirs
        )
        sc = self.g.scale
        caption = 20.0
        copy_h = 22.0 if (msg.text and not msg.streaming and not msg.error) else 4.0
        height = caption + (rendered.height if msg.text else 16.0) + copy_h
        img = Image.new("RGBA", (round(self.content_w * sc), round(height * sc)), (0, 0, 0, 0))
        if msg.text:
            img.alpha_composite(rendered.image, (0, round(caption * sc)))
        hits = tuple(_shift(h, 0.0, caption) for h in rendered.hits)
        arr = _premult(img)
        copy_rect = (0.0, height - copy_h, 64.0, height) if copy_h > 5 else None
        return _Entry(height, arr, PAD, hits, copy_rect, rendered.plain, PAD, height - copy_h + 5.0)

    # -------------------------------------------------------------- dibujo
    def render(self, now: float) -> np.ndarray:
        """El panel completo (sin fondo): cabecera, cuerpo y pie."""
        g = self.g
        sc = g.scale
        buf = np.zeros((round(g.height * sc), round(g.width * sc), 4), np.float32)
        self._hits = []
        footer = self._footer_height()
        if self.mode == "history":
            self._draw_history(buf, HEADER_H, g.height - footer)
        else:
            self._draw_messages(buf, now, HEADER_H, g.height - footer)
        self._draw_header(buf)
        self._draw_footer(buf, now, footer)
        return buf

    def animating(self, now: float) -> bool:
        flashing = self._flash is not None and now < self._flash[1]
        return self.model.busy or self.model.confirm is not None or flashing

    def flash_copied(self, text: str, now: float | None = None) -> None:
        """Avisa un momento de que se copió ``text`` («COPIADO» donde estaba «COPIAR»)."""
        self._flash = (text, (time.monotonic() if now is None else now) + 1.6)

    @property
    def flash_token(self) -> tuple[str, bool]:
        """Huella del aviso de copiado para la caché de imagen."""
        flash = self._flash
        return (flash[0][:20], time.monotonic() < flash[1]) if flash else ("", False)

    def _text(self, buf, text, style, color, x, y, align="left") -> None:
        draw_text(buf, text, style, color, x, y, self.g.scale, align=align)

    def _hit(self, rect, kind: str, data: object = None) -> None:
        self._hits.append((rect, kind, data))

    def _blit(self, buf, arr, x: float, y: float, top: float, bottom: float) -> None:
        """Pega ``arr`` en ``(x, y)`` recortado entre ``top`` y ``bottom`` (px lógicos)."""
        sc = self.g.scale
        y0, y1 = round(y * sc), round(y * sc) + arr.shape[0]
        lo, hi = round(top * sc), round(bottom * sc)
        if y1 <= lo or y0 >= hi:
            return
        cut0, cut1 = max(0, lo - y0), arr.shape[0] - max(0, y1 - hi)
        composite_over(buf, arr[cut0:cut1], round(x * sc), max(y0, lo))

    def _draw_messages(self, buf, now: float, top: float, bottom: float) -> None:
        _, messages = self.model.snapshot()
        viewport = bottom - top
        if not messages:
            self._draw_empty(buf, top, bottom)
            self._max_scroll = 0.0
            return
        entries = [self._entry(m, now) for m in messages]
        total = PAD + sum(e.height for e in entries) + GAP * (len(entries) - 1) + GAP
        self._max_scroll = max(0.0, total - viewport)
        self.scroll = max(0.0, min(self.scroll, self._max_scroll))
        y = top - (self._max_scroll - self.scroll) + PAD
        for msg, entry in zip(messages, entries, strict=True):
            self._draw_message(buf, msg, entry, y, top, bottom, now)
            y += entry.height + GAP
        if self._max_scroll > 0:
            self._draw_scrollbar(buf, top, viewport, total)

    def _draw_message(self, buf, msg, entry: _Entry, y: float, top, bottom, now: float) -> None:
        if y + entry.height < top or y > bottom:
            return
        self._blit(buf, entry.arr, entry.x, y, top, bottom)
        # Los rótulos se pintan enteros o no se pintan: nunca asoman bajo la cabecera.
        if msg.role == "assistant" and y >= top - 1 and y + 16 <= bottom:
            label = "EZAGENT" + (" · VOZ" if msg.source == "voice" else "")
            self._text(buf, label, FS_MICRO, self._color("TEXT_3"), PAD, y + 2)
            if msg.streaming and not msg.text and y + 34 <= bottom:
                self._draw_dots(buf, PAD, y + 28, now)
        self._draw_copy_label(buf, msg, entry, y, top, bottom, now)
        self._register_message_hits(msg, entry, y, top, bottom)

    def _draw_copy_label(self, buf, msg, entry: _Entry, y: float, top, bottom, now: float) -> None:
        """«COPIAR» bajo cada mensaje (y «POR VOZ» en los hablados); «COPIADO» un momento después."""
        if entry.caption_hit is None or y + entry.label_y < top or y + entry.label_y + 12 > bottom:
            return
        copied = self._flash is not None and self._flash[0] == entry.plain and now < self._flash[1]
        hovered = self.hover == "copy_msg"
        tone = "TEXT_2" if copied else ("MINT" if not hovered else "TEXT")
        label = "COPIADO" if copied else "COPIAR"
        ly = y + entry.label_y
        if entry.copy_align == "right":
            self._text(
                buf, label, FS_MICRO, self._color(tone), self.g.width - PAD, ly, align="right"
            )
            if msg.source == "voice":
                self._text(
                    buf,
                    "POR VOZ ·",
                    FS_MICRO,
                    self._color("TEXT_3"),
                    self.g.width - PAD - 58,
                    ly,
                    align="right",
                )
        else:
            self._text(buf, label, FS_MICRO, self._color(tone), PAD, ly)

    def _register_message_hits(self, msg, entry: _Entry, y: float, top, bottom) -> None:
        for h in entry.hits:
            x0, y0, x1, y1 = h.rect
            rect = (entry.x + x0, y + y0, entry.x + x1, y + y1)
            if rect[3] > top and rect[1] < bottom:
                self._hit(rect, h.kind, h.data)
        if entry.caption_hit is not None:
            x0, y0, x1, y1 = entry.caption_hit
            if y + y1 > top and y + y0 < bottom:
                self._hit(
                    (entry.caption_x + x0, y + y0, entry.caption_x + x1, y + y1),
                    "copy_msg",
                    entry.plain,
                )

    def _draw_dots(self, buf, x: float, y: float, now: float) -> None:
        for i in range(3):
            phase = (now * 2.4 - i * 0.22) % 1.0
            k = 0.35 + 0.65 * max(0.0, 1.0 - abs(phase - 0.35) * 3.0)
            _fill_circle(buf, x + 5 + i * 11, y, 6.0, self._color("TEXT_2")[:3], self.g.scale, k)

    def _draw_scrollbar(self, buf, top: float, viewport: float, total: float) -> None:
        thumb = max(28.0, viewport * viewport / total)
        travel = viewport - thumb - 8
        pos = top + 4 + travel * (1.0 - self.scroll / max(1.0, self._max_scroll))
        x = self.g.width - 6
        _fill_round_rect(
            buf, x, pos, x + 3, pos + thumb, 1.5, self._color("TEXT_3")[:3], self.g.scale, 0.55
        )

    # --------------------------------------------------------- interacción
    def _at(self, x: float, y: float) -> tuple[str, object] | None:
        for rect, kind, data in reversed(self._hits):
            if rect[0] <= x <= rect[2] and rect[1] <= y <= rect[3]:
                return kind + (f":{data}" if kind == "session" else ""), data
        return None

    def pointer_move(self, x: float, y: float) -> bool:
        hit = self._at(x, y)
        new = hit[0] if hit else None
        if new != self.hover:
            self.hover = new
            return True
        return False

    def cursor_is_hand(self, x: float, y: float) -> bool:
        hit = self._at(x, y)
        return hit is not None and hit[0] != "input"

    def pointer_down(self, x: float, y: float) -> Action | None:
        hit = self._at(x, y)
        if hit is None:
            return None
        kind, data = hit
        if kind == "input":
            self._place_caret(x, y)
            return None
        return self._activate(kind.split(":")[0], data)

    def _activate(self, kind: str, data: object) -> Action | None:
        if kind == "send":
            return self._send()
        simple = {"close": Action("close"), "stop": Action("stop"), "new": Action("new")}
        if kind in simple:
            return simple[kind]
        if kind == "voice":
            return Action("voice", not self.model.voice_replies)
        if kind == "history":
            self.mode = "history" if self.mode == "chat" else "chat"
            self.scroll = 0.0
        elif kind == "back":
            self.model.view_session(None)
            self.mode, self.scroll = "chat", 0.0
        elif kind == "session":
            self.model.view_session(int(data))  # type: ignore[arg-type]
            self.mode, self.scroll = "chat", 0.0
        elif kind in ("copy_msg", "copy_code"):
            return Action("copy", data)
        elif kind in ("open", "reveal"):
            return Action(kind, data)
        elif kind == "image":
            return Action("open", data)
        elif kind in ("confirm_yes", "confirm_no"):
            return Action("confirm", kind == "confirm_yes")
        return None

    def wheel(self, delta: int) -> None:
        """Rueda del ratón: ``delta > 0`` sube (hacia mensajes anteriores)."""
        steps = delta / 120.0
        self.scroll = max(0.0, min(self._max_scroll, self.scroll + steps * SCROLL_STEP))

    def _send(self) -> Action | None:
        if self.model.busy and self.input.empty:
            return Action("stop")
        text = self.input.take()
        if not text:
            return None
        self.mode, self.scroll = "chat", 0.0
        return Action("send", text)

    def key(self, vk: int, ctrl: bool = False, shift: bool = False) -> Action | None:
        """Una tecla (códigos VK de Windows). Devuelve una acción para el dueño, si la hay."""
        self._focus_t0 = time.monotonic()
        if vk == 13:
            if shift:
                self.input.newline()
                return None
            return self._send()
        if vk == 27:
            return self._escape()
        if ctrl and vk in (65, 67, 88):
            return self._clipboard_key(vk)
        moves = {37: "left", 39: "right", 38: "up", 40: "down", 36: "home", 35: "end"}
        if vk in moves:
            self.input.move(moves[vk], select=shift, word=ctrl)  # type: ignore[arg-type]
        elif vk == 8:
            self.input.backspace(word=ctrl)
        elif vk == 46:
            self.input.delete(word=ctrl)
        elif vk in (33, 34):
            self.wheel(120 * 6 if vk == 33 else -120 * 6)
        return None

    def _clipboard_key(self, vk: int) -> Action | None:
        if vk == 65:
            self.input.select_all()
            return None
        selected = self.input.selected_text()
        if not selected:
            return None
        if vk == 88:
            self.input.backspace()
        return Action("copy", selected)

    def _escape(self) -> Action | None:
        if not self.input.empty:
            self.input.clear()
            return None
        if self.mode == "history":
            self.mode = "chat"
            return None
        if self.model.viewing is not None:
            self.model.view_session(None)
            return None
        return Action("close")

    def char(self, ch: str) -> None:
        if ch and ord(ch[0]) >= 32 and ord(ch[0]) != 127:
            self.input.insert(ch)
            self._focus_t0 = time.monotonic()

    def paste(self, text: str) -> None:
        self.input.insert(text)
        self._focus_t0 = time.monotonic()

    def add_files(self, paths: list[str]) -> None:
        """Archivos soltados en la ventana: entran como enlaces que el chat muestra como tarjetas."""
        lines = [f"[{Path(p).name}](<{p}>)" for p in paths if p]
        if lines:
            prefix = "\n" if self.input.text and not self.input.text.endswith("\n") else ""
            self.input.insert(prefix + "\n".join(lines) + "\n")


def _shift(hit: Hit, dx: float, dy: float) -> Hit:
    x0, y0, x1, y1 = hit.rect
    return Hit(hit.kind, (x0 + dx, y0 + dy, x1 + dx, y1 + dy), hit.data)


# Las constantes y ``_break`` viven en ``chat_footer``; se reexportan por compatibilidad.
__all__ = [
    "BTN",
    "GAP",
    "HEADER_H",
    "HINT_H",
    "INPUT_LINE",
    "INPUT_MAX_LINES",
    "INPUT_MIN",
    "PAD",
    "SCROLL_STEP",
    "Action",
    "ChatView",
    "_break",
]
