"""Cuadros dorados y números del chat (F1-10c).

Tres capas, todas desde el código real del Python (``py-baseline``) y en memoria:

* ``render_message``: markdown completo (títulos, listas, citas, código, tablas,
  imágenes y documentos) en varios anchos, escalas y temas;
* el panel entero de ``ChatView`` (vacío, conversación, historial, confirmación,
  vista anterior, streaming, error y tema personalizado) a varios DPI;
* el morph forma → panel de ``chat_paint`` con la composición real del panel.

Los PNG van a ``parity/golden/hud/chat/``. La misma fixture trae los números de
``chat_footer`` (``input_box_height``, ``_break``), ``chat_labels``,
``chat_paint`` (``shape_progress``, ``row_reveal``), ``chat_ui`` (``fluid_array``,
``_hotkey_label``) y los golpes del panel para las pruebas de Rust.
"""

from __future__ import annotations

import os
import time
from datetime import datetime
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import numpy as np
from _common import GOLDEN, FixtureFile, main_guard
from PIL import Image

from ezagent.voice.chat import markdown as md
from ezagent.voice.chat import render as chat_render
from ezagent.voice.chat import render_base, render_rich
from ezagent.voice.chat.render import render_message
from ezagent.voice.chat.render_base import Ctx
from ezagent.voice.chat.session import ChatModel
from ezagent.voice.chat.store import ChatStore, SessionInfo
from ezagent.voice.hud import chat_footer, chat_header, chat_labels, chat_paint, chat_ui, chat_view
from ezagent.voice.hud.bar_theme import card_palette
from ezagent.voice.hud.chat_footer import _break, input_box_height
from ezagent.voice.hud.chat_geometry import MARGIN, panel_rect
from ezagent.voice.hud.chat_ui import ChatPanel, _Anim, _hotkey_label, fluid_array
from ezagent.voice.hud.chat_view import ChatView
from ezagent.voice.hud.drop import PILL_H, PILL_W, DropModel, render_fluid
from ezagent.voice.hud.theme_accent import derive
from ezagent.voice.hud.tokens import set_themed

DOMAIN = "hud_chat"
OUT = GOLDEN / "hud" / "chat"
NOW = 0.0
WALL = (2026, 11, 13, 16, 46, 12)
FILE_WALL = (2026, 11, 13, 9, 5, 7)
CAPTION = "Captura de chat"


def epoch(wall: tuple[int, ...]) -> float:
    """Instante que ``time.localtime`` devuelve con esa hora de pared (cualquier zona)."""
    target = datetime(*wall)
    try:
        moment = time.mktime((*wall, 0, 0, -1))
    except (OverflowError, ValueError):
        moment = 0.0
    for _ in range(5):
        now = time.localtime(moment)[:6]
        if tuple(now) == wall:
            return moment
        moment += (target - datetime(*now)).total_seconds()
    return moment


def wall_of(t: float) -> list[int]:
    return list(time.localtime(int(t))[:6])


FIXED_TS = epoch(WALL)
FILE_MTIME = epoch(FILE_WALL)



def plain(value: Any) -> Any:
    if isinstance(value, np.ndarray):
        return [plain(item) for item in value.tolist()]
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, dict):
        return {str(key): plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(item) for item in value]
    return value


def add(out: FixtureFile, name: str, fn: str, data: Any, value: Any) -> None:
    out.add(f"hud_chat/{name}", fn, plain(data), plain(value))


def mark_home() -> Path:
    return Path(os.environ["HOME"])


def collapse(value: Any, root: Path) -> Any:
    if isinstance(value, str):
        return value.replace(str(root), "<tmp>")
    if isinstance(value, dict):
        return {key: collapse(item, root) for key, item in value.items()}
    if isinstance(value, list):
        return [collapse(item, root) for item in value]
    return value


def straight_float(arr: np.ndarray) -> Image.Image:
    """RGBA premultiplicado float32 0..1 -> imagen RGBA con alfa directo."""
    a = arr[..., 3]
    safe = np.maximum(a, 1e-6)[..., None]
    rgb = np.clip(np.where(a[..., None] > 0.0, arr[..., :3] / safe, 0.0), 0.0, 1.0)
    data = np.empty((*a.shape, 4), np.uint8)
    data[..., :3] = np.round(rgb * 255.0).astype(np.uint8)
    data[..., 3] = np.round(np.clip(a, 0.0, 1.0) * 255.0).astype(np.uint8)
    return Image.fromarray(data, "RGBA")


def straight_bgra(pixels: np.ndarray) -> Image.Image:
    """BGRA premultiplicado uint8 (contrato del HUD) -> RGBA con alfa directo."""
    floats = pixels.astype(np.float32) / 255.0
    return straight_float(floats[..., [2, 1, 0, 3]])


def save_frame(name: str, image: Image.Image) -> str:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.png"
    image.save(path, format="PNG")
    return f"hud/chat/{path.name}"


# --------------------------------------------------------------------- archivos
DOC_TEXT = (
    "# Plan de lanzamiento\n\n"
    "La migración se entrega en **tres pasos**:\n\n"
    "1. Código primero\n2. Pruebas después\n\n"
    "> Nada se abre solo.\n\n"
    "| Paso | Estado |\n|------|--------|\n| Uno | Hecho |\n| Dos | En curso |\n"
)


def prepare_files(root: Path) -> dict[str, dict[str, Any]]:
    base = root / "chat"
    (base / "ok").mkdir(parents=True, exist_ok=True)
    (base / "private").mkdir(parents=True, exist_ok=True)
    photo = base / "foto.png"
    Image.new("RGBA", (64, 40), (94, 226, 196, 255)).save(photo, format="PNG")
    notes = base / "notas.md"
    notes.write_text(DOC_TEXT, encoding="utf-8", newline="\n")
    secret = base / "private" / "secreto.png"
    Image.new("RGBA", (24, 16), (200, 36, 26, 255)).save(secret, format="PNG")
    pdf = base / "informe.pdf"
    pdf.write_bytes(b"%PDF-1.4\n% fixture\n")
    binary = base / "datos.bin"
    binary.write_bytes(b"\x00\x01\x02\x03")
    for path in (photo, notes, secret, pdf, binary):
        os.utime(path, (FILE_MTIME, FILE_MTIME))
    return {
        "foto.png": {"kind": "image", "size": [64, 40], "rgba": [94, 226, 196, 255],
                     "mtime_wall": list(FILE_WALL)},
        "notas.md": {"kind": "text", "text": DOC_TEXT, "mtime_wall": list(FILE_WALL)},
        "informe.pdf": {"kind": "bytes", "hex": pdf.read_bytes().hex(),
                        "mtime_wall": list(FILE_WALL)},
        "datos.bin": {"kind": "bytes", "hex": binary.read_bytes().hex(),
                      "mtime_wall": list(FILE_WALL)},
        "private/secreto.png": {"kind": "image", "size": [24, 16],
                                "rgba": [200, 36, 26, 255],
                                "mtime_wall": list(FILE_WALL)},
    }


def hit_data(data: Any, root: Path) -> Any:
    if data is None or isinstance(data, (str, int, float, bool)):
        return collapse(data, root)
    keys = ("name", "ext", "size", "kind", "allowed", "openable")
    if all(hasattr(data, key) for key in keys):
        summary = {key: getattr(data, key) for key in keys}
        summary["path"] = collapse(str(data.path), root)
        summary["mtime_wall"] = wall_of(float(data.mtime))
        return summary
    return str(data)


def hits_of(hits: Any, root: Path) -> list[dict[str, Any]]:
    return [
        {"kind": hit.kind, "rect": list(hit.rect), "data": hit_data(hit.data, root)}
        for hit in hits
    ]


# --------------------------------------------------------------------- mensajes
MESSAGES: list[tuple[str, str, float, float, str, str | None, list[str]]] = [
    ("headings", "# Título\n\n## Subtítulo\n\n### Menor\n\nUn párrafo normal.", 380.0, 1.0,
     "light", None, []),
    ("inline", "Con **negrita**, *cursiva*, `código` y un [enlace](https://example.com).\n\n"
               "También ~~tachado~~, _énfasis_ y escapadas: \\*literal\\*.", 380.0, 1.0,
     "dark", None, []),
    ("lists", "- uno\n- dos\n  - anidado\n    - más dentro\n- tres\n\n"
              "1. primero\n2. segundo\n\n- [x] hecho\n- [ ] pendiente", 360.0, 1.0,
     "light", None, []),
    ("quote-rule", "> Una cita con **negrita** y `código`.\n>\n> Otra línea.\n\n---\n\n"
                   "Después de la regla.", 380.0, 1.0, "dark", None, []),
    ("code", "```python\nprint('hola')\n\nfor i in range(3):\n    print(i)\n```\n\n"
             "```\n" + "x" * 120 + "\n```", 380.0, 1.0, "dark", None, []),
    ("table", "| Plan | Precio | Detalle |\n|:-----|-------:|:--------:|\n"
              "| Básico | 9 € | Para probar |\n| Pro | 19 € | **Recomendado** |\n"
              "| Equipo | 49 € | Hasta 5 |", 420.0, 1.0, "light", None, []),
    ("table-narrow", "| A | B | C | D | E | F |\n|---|---|---|---|---|---|\n"
                     "| 1 | 2 | 3 | 4 | 5 | 6 |\n| 7 | 8 | 9 | 10 | 11 | 12 |",
     260.0, 1.0, "dark", None, []),
    ("image-web", "![Playa](https://example.com/playa.png)\n\nTexto debajo.", 380.0, 1.0,
     "light", None, []),
    ("image-missing", "![No está](./no-existe.png)", 380.0, 1.0, "light", None, []),
    ("image-ok", "![Foto del plan](./foto.png)", 380.0, 1.0, "light", "<tmp>/chat",
     ["foto.png"]),
    ("image-denied", "![Fuera](./private/secreto.png)", 380.0, 1.0, "dark", "<tmp>/chat/ok",
     ["private/secreto.png"]),
    ("doc-missing", "[Notas](./no-existe.md)", 380.0, 1.0, "dark", None, []),
    ("doc-preview", "[Plan de lanzamiento](./notas.md)", 380.0, 1.0, "light", "<tmp>/chat",
     ["notas.md"]),
    ("doc-pdf", "[Informe](./informe.pdf)", 380.0, 1.0, "dark", "<tmp>/chat", ["informe.pdf"]),
    ("doc-denied", "[Secreto](./private/secreto.png)", 420.0, 1.0, "light", "<tmp>/chat/ok",
     ["private/secreto.png"]),
    ("doc-other", "[Datos](./datos.bin)", 380.0, 1.0, "light", "<tmp>/chat", ["datos.bin"]),
    ("mixed", "# Resumen\n\nUn párrafo con **datos** y una lista:\n\n"
              "- primer punto\n- segundo punto\n\n> cita breve\n\n"
              "| A | B |\n|---|---:|\n| 1 | 2 |\n\n```js\nconsole.log(1)\n```", 340.0,
     1.0, "dark", None, []),
    ("unicode", "Añadí ñandúes, ángeles y 🚀 cohetes. 日本語 también.\n\n"
                "UnaPalabraMuyLargaSinEspaciosQueDebePartirseEnVariosTrozosParaCaber.",
     300.0, 1.0, "light", None, []),
    ("empty", "", 380.0, 1.0, "light", None, []),
    ("scale-150", "**Escalado** a 1,5× con `código` y una tabla:\n\n| A | B |\n|---|---|\n"
                  "| 1 | 2 |", 300.0, 1.5, "light", None, []),
]


def message_cases(out: FixtureFile, files: dict[str, dict[str, Any]]) -> None:
    root = mark_home()
    for name, text, width, scale, theme, roots, used_files in MESSAGES:
        set_themed(None)
        pal = card_palette(theme)
        base = root / "chat"
        try:
            rendered = render_message(
                text,
                pal,
                width,
                scale,
                (root / "chat" / "ok",) if roots == "<tmp>/chat/ok" else (
                    base,) if roots == "<tmp>/chat" else None,
                (base,),
            )
        except Exception as error:  # noqa: BLE001 - se registra el texto real
            add(out, f"render/error-{name}", "render_message",
                {"text": text, "width": width, "scale": scale, "theme": theme}, {})
            out.cases[-1]["error"] = str(error)
            continue
        png = save_frame(f"msg-{name}", rendered.image)
        add(
            out,
            f"render/{name}",
            "render_message",
            {
                "text": text,
                "width": width,
                "scale": scale,
                "theme": theme,
                "roots": [roots] if roots else None,
                "base_dirs": ["<tmp>/chat"],
                "files": {key: files[key] for key in used_files},
            },
            {
                "png": png,
                "height": rendered.height,
                "image_px": list(rendered.image.size),
                "plain": rendered.plain,
                "hits": hits_of(rendered.hits, root),
            },
        )
    # Tema personalizado: la paleta derivada tiñe el mensaje.
    overrides = derive("#7A6FF0", "#2E2A6B")
    set_themed(overrides)
    rendered = render_message("**Lavanda** con `código` y [enlace](https://example.com).",
                              card_palette("dark"), 380.0, 1.0)
    png = save_frame("msg-custom-lavanda", rendered.image)
    add(
        out,
        "render/custom-lavanda",
        "render_message",
        {"text": "**Lavanda** con `código` y [enlace](https://example.com).",
         "width": 380.0, "scale": 1.0, "theme": "dark",
         "custom": {"primary": "#7A6FF0", "background": "#2E2A6B", "base": "dark"},
         "roots": None, "base_dirs": ["<tmp>/chat"], "files": {}},
        {"png": png, "height": rendered.height, "image_px": list(rendered.image.size),
         "plain": rendered.plain, "hits": hits_of(rendered.hits, root)},
    )
    set_themed(None)


# ----------------------------------------------------------------------- panel
def message_dict(message: Any) -> dict[str, Any]:
    return {
        "id": message.id,
        "role": message.role,
        "text": message.text,
        "source": message.source,
        "t_wall": wall_of(message.t),
        "streaming": message.streaming,
        "error": message.error,
    }


def populate(model: ChatModel, specs: list[dict[str, Any]]) -> None:
    for spec in specs:
        role = spec["role"]
        if role == "user":
            msg = model.add_user(spec["text"], spec.get("source", "chat"))
        elif role == "system":
            msg = model.add_note(spec["text"])
        else:
            msg = model.begin_assistant(spec.get("source", "chat"))
            if spec["text"]:
                model.append(msg.id, spec["text"])
            if spec.get("error"):
                model.fail(msg.id, spec["text"])
            elif spec.get("streaming"):
                pass
            else:
                model.finish(msg.id)
            found = model._find(msg.id)
            if found is not None:
                msg = found
        msg.t = spec.get("t", FIXED_TS)


def make_store(root: Path) -> ChatStore:
    path = root / "chat" / "history.jsonl"
    path.unlink(missing_ok=True)  # archivo temporal propio: se rehace en cada corrida
    store = ChatStore(path)
    store.append("user", "Plan de lanzamiento", "chat", FIXED_TS)
    store.append("assistant", "Listo, lo preparo.", "chat", FIXED_TS + 1)
    store.new_session()
    store._starts[store.session] = FIXED_TS + 10
    store.append("user", "Otra conversación con un título bastante más largo de lo normal",
                 "voice", FIXED_TS + 10)
    store.append("assistant", "Vale.", "voice", FIXED_TS + 11)
    store.append("system", "Conversación compactada", "agent", FIXED_TS + 12)
    store.new_session()
    store._starts[store.session] = FIXED_TS + 20
    store.append("user", "Y una tercera", "chat", FIXED_TS + 20)
    return store


def history_of(model: ChatModel) -> list[dict[str, Any]]:
    store = model.store
    if store is None:
        return []
    rows = []
    for info in model.sessions():
        rows.append({
            "session": info.session,
            "started_wall": wall_of(info.started),
            "messages": info.messages,
            "title": info.title,
            "entries": [message_dict_from_entry(entry) for entry in store.entries(info.session)],
        })
    return rows


def message_dict_from_entry(entry: Any) -> dict[str, Any]:
    return {
        "t_wall": wall_of(entry.t), "role": entry.role, "source": entry.source,
        "text": entry.text, "seq": entry.seq,
    }


def panel_frame(
    out: FixtureFile,
    name: str,
    model: ChatModel,
    *,
    width: float = 440.0,
    height: float = 640.0,
    scale: float = 1.0,
    theme: str = "light",
    mode: str = "chat",
    scroll: float = 0.0,
    text_input: str = "",
    cursor: int | None = None,
    anchor: int | None = None,
    hover: str | None = None,
    status: str = "",
    in_call: bool = False,
    pending_delete: int | None = None,
    deleting: tuple[int, Any, float] | None = None,
    now: float = NOW,
    custom: dict[str, str] | None = None,
) -> None:
    if custom:
        set_themed(derive(custom["primary"], custom["background"]))
    view = ChatView(model, hotkey_label="Ctrl+Alt+T")
    view.configure(width, height, scale, theme, card_palette(theme))
    view._focus_t0 = 0.0
    view.mode, view.scroll, view.hover = mode, scroll, hover
    view.status, view.in_call = status, in_call
    if text_input:
        view.input.set_text(text_input)
    if cursor is not None:
        view.input.cursor = cursor
    if anchor is not None:
        view.input.anchor = anchor
    if pending_delete is not None:
        view._pending_delete = pending_delete
    if deleting is not None:
        view._deleting = deleting
    buf = view.render(now)
    image = straight_float(buf)
    png = save_frame(f"panel-{name}", image)
    set_themed(None)
    add(
        out,
        f"panel/{name}",
        "ChatView.render",
        {
            "width": width,
            "height": height,
            "scale": scale,
            "theme": theme,
            "custom": custom,
            "mode": mode,
            "scroll": scroll,
            "hover": hover,
            "status": status,
            "in_call": in_call,
            "now": now,
            "focus_t0": 0.0,
            "input": {"text": view.input.text, "cursor": view.input.cursor,
                      "anchor": view.input.anchor},
            "messages": [message_dict(message) for message in model.snapshot()[1]],
            "confirm": (
                {"summary": model.confirm.summary, "strong": model.confirm.strong}
                if model.confirm is not None else None
            ),
            "busy": model.busy,
            "voice_replies": model.voice_replies,
            "mic_muted": model.mic_muted,
            "current_session": model.current_session,
            "viewing": model.viewing,
            "pending_delete": pending_delete,
            "deleting": (
                {"index": deleting[0], "session": deleting[1].session,
                 "started_wall": wall_of(deleting[1].started),
                 "messages": deleting[1].messages,
                 "title": deleting[1].title, "at": deleting[2]}
                if deleting is not None else None
            ),
            "history": history_of(model),
        },
        {"png": png, "image_px": list(image.size),
         "hits": [{"kind": hit[1], "rect": list(hit[0]), "data": hit_data(hit[2], mark_home())}
                  for hit in view._hits]},
    )


def conversation_model() -> ChatModel:
    model = ChatModel()
    populate(model, [
        {"role": "user", "text": "¿Puedes comparar estos dos planes?", "t": FIXED_TS},
        {"role": "assistant",
         "text": "Claro. Aquí va la comparación:\n\n| Plan | Precio |\n|---|---:|\n"
                 "| Básico | 9 € |\n| Pro | 19 € |\n\n**Recomiendo** el Pro.",
         "t": FIXED_TS + 2},
        {"role": "user", "text": "Gracias, muéstrame el informe.", "source": "voice",
         "t": FIXED_TS + 3},
        {"role": "assistant", "text": "Aquí lo tienes.", "t": FIXED_TS + 4},
    ])
    return model


def panel_cases(out: FixtureFile, root: Path) -> None:
    empty = ChatModel()
    for name, theme, scale in (
        ("empty-light-100", "light", 1.0),
        ("empty-dark-100", "dark", 1.0),
        ("empty-light-200", "light", 2.0),
        ("empty-dark-125", "dark", 1.25),
    ):
        panel_frame(out, name, empty, theme=theme, scale=scale)

    for name, theme, scale, width, height in (
        ("conversation-light-100", "light", 1.0, 440.0, 640.0),
        ("conversation-dark-100", "dark", 1.0, 440.0, 640.0),
        ("conversation-light-125", "light", 1.25, 440.0, 640.0),
        ("conversation-dark-150", "dark", 1.5, 440.0, 640.0),
        ("conversation-light-200", "light", 2.0, 440.0, 640.0),
        ("conversation-dark-200", "dark", 2.0, 440.0, 640.0),
        ("conversation-small-380x520", "light", 1.0, 380.0, 520.0),
    ):
        panel_frame(out, name, conversation_model(), theme=theme, scale=scale,
                    width=width, height=height)

    scrolled = conversation_model()
    for index in range(6):
        user = scrolled.add_user(f"Pregunta {index + 1} con algo de texto para llenar el panel.")
        user.t = FIXED_TS + 10 + index * 2
        note = scrolled.add_note(f"Nota {index + 1}")
        note.t = FIXED_TS + 11 + index * 2
    panel_frame(out, "long-scrolled-light-100", scrolled, scroll=260.0, theme="light")

    model = conversation_model()
    panel_frame(out, "input-multiline-light-100", model,
                text_input="Primera línea\nSegunda línea con selección\nTercera",
                cursor=18, anchor=8, theme="light", width=440.0, height=640.0)
    panel_frame(out, "input-empty-caret-dark-150", ChatModel(),
                text_input="", cursor=0, theme="dark", scale=1.5)

    confirm = conversation_model()
    confirm.ask_confirm("Borrar la conversación y empezar de cero", True)
    panel_frame(out, "confirm-sending-dark-100", confirm, theme="dark",
                text_input="Sí, adelante")

    streaming = ChatModel()
    populate(streaming, [
        {"role": "user", "text": "Cuéntame el plan", "t": FIXED_TS},
        {"role": "assistant", "text": "", "streaming": True, "t": FIXED_TS},
    ])
    panel_frame(out, "streaming-empty-light-100", streaming, theme="light")

    failed = ChatModel()
    populate(failed, [
        {"role": "user", "text": "Hazlo", "t": FIXED_TS},
        {"role": "assistant", "text": "No pude conectar con el modelo.", "error": True,
         "t": FIXED_TS + 1},
    ])
    panel_frame(out, "error-dark-100", failed, theme="dark")

    status = conversation_model()
    panel_frame(out, "status-listening-light-100", status, theme="light",
                status="Escuchando…")
    sending = ChatModel()
    populate(sending, [
        {"role": "user", "text": "Sigue contando", "t": FIXED_TS},
        {"role": "assistant", "text": "", "streaming": True, "t": FIXED_TS},
    ])
    panel_frame(out, "sending-stop-dark-100", sending, theme="dark",
                text_input="Otra pregunta mientras responde")

    note = ChatModel()
    populate(note, [
        {"role": "system", "text": "Conversación compactada para seguir trabajando",
         "source": "agent", "t": FIXED_TS},
    ])
    panel_frame(out, "note-system-light-100", note, theme="light")

    panel_frame(out, "custom-lavanda-dark-100", conversation_model(), theme="dark",
                custom={"primary": "#7A6FF0", "background": "#2E2A6B"})

    store = make_store(root)
    history = ChatModel(store)
    panel_frame(out, "history-light-100", history, mode="history", theme="light")
    panel_frame(out, "history-dark-100", history, mode="history", theme="dark")
    sessions = history.sessions()
    panel_frame(out, "history-hover-dark-100", history, mode="history", theme="dark",
                hover=f"session:{sessions[0].session}")
    panel_frame(out, "history-deleting-dark-100", history, mode="history", theme="dark",
                pending_delete=sessions[0].session)
    panel_frame(out, "history-ghost-light-100", history, mode="history", theme="light",
                deleting=(0, SessionInfo(999, FIXED_TS, 3, "Borrada"), NOW - 0.05))
    empty_history = ChatModel()
    panel_frame(out, "history-empty-dark-100", empty_history, mode="history", theme="dark")
    busy_history = ChatModel(store)
    streaming = busy_history.begin_assistant()
    streaming.t = FIXED_TS + 30
    panel_frame(out, "history-busy-light-100", busy_history, mode="history", theme="light")
    panel_frame(out, "history-scrolled-light-100", long_history_model(root), mode="history",
                theme="light", scroll=180.0)


def long_history_model(root: Path) -> ChatModel:
    path = root / "chat" / "history-long.jsonl"
    path.unlink(missing_ok=True)
    store = ChatStore(path)
    for index in range(12):
        store.append("user", f"Conversación número {index + 1} con un título largo",
                     "chat", FIXED_TS + index * 60)
        store.append("assistant", f"Respuesta {index + 1}", "chat", FIXED_TS + index * 60 + 1)
        if index < 11:
            store.new_session()
            store._starts[store.session] = FIXED_TS + (index + 1) * 60
    return ChatModel(store)


def viewing_case(out: FixtureFile, root: Path) -> None:
    store = make_store(root)
    model = ChatModel(store)
    previous = store.sessions()[1].session
    model.view_session(previous)
    panel_frame(out, "viewing-previous-real-light-100", model, theme="light")


# ---------------------------------------------------------------------- morph
class FakeUI:
    """Lo mínimo que ``chat_paint`` pide a la interfaz, con valores reales."""

    def __init__(self, layout: str, theme: str, scale: float, reduced: bool = False) -> None:
        self.s = scale
        self.bs = scale
        self.layout = layout
        self._reduced = reduced
        self.theme = theme
        self._fluid_cache = None
        self.model = DropModel(layout, None)
        for index in range(48):
            self.model.update(index / 30.0, 1 / 30.0, "idle", 0.35, theme,
                              False, False, False, False)

    def _theme_name(self) -> str:
        return self.theme

    def chat_body_rgb(self) -> tuple[int, int, int]:
        return tuple(card_palette(self.theme)["BODY"][:3])  # type: ignore[return-value]

    def chat_fluid(self, w: int, h: int, now: float) -> np.ndarray | None:
        cached = self._fluid_cache
        if cached is None or cached[0] != now:
            cached = (now, render_fluid(self.model, now))
            self._fluid_cache = cached
        return fluid_array(cached[1], w, h)

    def set_chat_open(self, active: bool) -> None:
        pass

    def set_form_hidden(self, hidden: bool) -> None:
        pass

    def form_size(self) -> tuple[float, float]:
        return (PILL_W, PILL_H)


def morph_frame(
    out: FixtureFile,
    name: str,
    model: ChatModel,
    layout: str,
    theme: str,
    p: float,
    *,
    scale: float = 1.0,
    closing: bool = False,
    reduced: bool = False,
) -> None:
    ui = FakeUI(layout, theme, scale, reduced)
    panel = ChatPanel(ui)  # type: ignore[arg-type]
    view = ChatView(model)
    anchor = (960.0, 1000.0)
    form_w, form_h = ui.form_size()
    form = (anchor[0] - form_w / 2, anchor[1] - form_h / 2,
            anchor[0] + form_w / 2, anchor[1] + form_h / 2)
    work = (0, 0, 1920, 1080)
    rect = panel_rect(form, work, scale, None)
    margin = round(MARGIN * scale)
    window = SimpleNamespace(
        x=round(rect[0]) - margin,
        y=round(rect[1]) - margin,
        w=round(rect[2] - rect[0]) + 2 * margin,
        h=round(rect[3] - rect[1]) + 2 * margin,
    )
    panel.window = window  # type: ignore[assignment]
    panel.view = view  # type: ignore[assignment]
    panel._form = form  # type: ignore[assignment]
    panel.anim = _Anim("closing" if closing else "opening", 0.0, p)
    view.configure((rect[2] - rect[0]) / scale, (rect[3] - rect[1]) / scale, scale,
                   theme, card_palette(theme))
    view._focus_t0 = 0.0
    pixels = panel._compose(window, view, p, 0.0, live=False)  # type: ignore[arg-type]
    image = straight_bgra(pixels)
    png = save_frame(f"morph-{name}", image)
    add(
        out,
        f"morph/{name}",
        "ChatPanel._compose",
        {
            "layout": layout,
            "theme": theme,
            "scale": scale,
            "p": p,
            "closing": closing,
            "reduced": reduced,
            "anchor": list(anchor),
            "work": list(work),
            "form": list(form),
            "window": [window.x, window.y, window.w, window.h],
            "panel": list(rect),
            "messages": [message_dict(message) for message in model.snapshot()[1]],
            "input": {"text": "", "cursor": 0, "anchor": None},
        },
        {"png": png, "image_px": list(image.size)},
    )


def morph_cases(out: FixtureFile) -> None:
    for p in (0.0, 0.25, 0.5, 0.75, 1.0):
        morph_frame(out, f"open-light-{int(p * 100):02d}", conversation_model(),
                    "pill", "light", p)
    for p in (0.5, 1.0):
        morph_frame(out, f"open-dark-{int(p * 100):02d}", conversation_model(),
                    "pill", "dark", p)
    for p in (0.5, 1.0):
        morph_frame(out, f"open-circle-{int(p * 100):02d}", conversation_model(),
                    "circle", "light", p)
    for p in (0.75, 0.25):
        morph_frame(out, f"close-dark-{int(p * 100):02d}", conversation_model(),
                    "pill", "dark", p, closing=True)
    morph_frame(out, "open-reduced-light-50", ChatModel(), "pill", "light", 0.5,
                reduced=True)


# --------------------------------------------------------------------- números
def number_cases(out: FixtureFile) -> None:
    for index, rows in enumerate((0, 1, 2, 3, 6, 7, -1), start=1):
        add(out, f"numbers/input-box-height/{index:02d}", "chat_footer.input_box_height",
            {"rows": rows}, {"height": input_box_height(rows)})
    ctx = Ctx(1.0, card_palette("light"), 1000.0)
    breaks = [
        ("hola mundo", 120.0), ("hola mundo", 60.0), ("", 120.0),
        ("una palabra muy larga sin espacios", 80.0),
        ("ñandú ángel 🚀", 40.0), ("  espacios  ", 100.0),
        ("a b c d e f g", 30.0), ("sin espacio", 0.0),
    ]
    for index, (text, width) in enumerate(breaks, start=1):
        add(out, f"numbers/break/{index:02d}", "chat_footer._break",
            {"text": text, "width": width, "scale": 1.0},
            {"ranges": _break(ctx, text, width)})
    for index, role in enumerate(("user", "assistant", "system", "tool", "", "USER"),
                                  start=1):
        msg = SimpleNamespace(role=role)
        add(out, f"numbers/stamp-height/{index:02d}", "chat_labels.stamp_height",
            {"role": role}, {"height": chat_labels.stamp_height(msg)})
    add(
        out,
        "numbers/labels-constants",
        "chat_labels.constants",
        {},
        {
            "FS_STAMP": {
                "token": chat_labels.FS_STAMP.token,
                "family": chat_labels.FS_STAMP.family,
                "size": chat_labels.FS_STAMP.size,
                "weight": chat_labels.FS_STAMP.weight,
            },
            "STAMP_H": chat_labels.STAMP_H,
            "STAMP_INSET": chat_labels.STAMP_INSET,
        },
    )
    for index, (p, opening) in enumerate(
        [(0.0, True), (0.25, True), (0.45, True), (0.5, True), (0.75, True),
         (0.9, True), (1.0, True), (0.5, False), (0.75, False), (1.0, False)],
        start=1,
    ):
        add(out, f"numbers/shape-progress/{index:02d}", "chat_paint.shape_progress",
            {"p": p, "opening": opening}, {"value": chat_paint.shape_progress(p, opening)})
    for index, (rows, p, opening) in enumerate(
        [(1, 0.0, True), (1, 0.5, True), (1, 1.0, True), (4, 0.25, True),
         (4, 0.5, True), (4, 1.0, True), (10, 0.6, True), (4, 0.5, False),
         (4, 1.0, False), (0, 0.5, True)],
        start=1,
    ):
        values = chat_paint.row_reveal(rows, p, opening)
        add(out, f"numbers/row-reveal/{index:02d}", "chat_paint.row_reveal",
            {"rows": rows, "p": p, "opening": opening},
            {"values": values})
    rgb = np.zeros((3, 4, 3), np.uint8)
    for y in range(3):
        for x in range(4):
            rgb[y, x] = (12 + x * 20, 112 + y * 30, 117 + x * 10)
    for index, (w, h) in enumerate(((5, 4), (2, 2), (1, 1)), start=1):
        result = fluid_array(rgb, w, h)
        add(out, f"numbers/fluid-array/{index:02d}", "chat_ui.fluid_array",
            {"rgb": rgb, "w": w, "h": h}, {"values": result})
    add(out, "numbers/fluid-array-none", "chat_ui.fluid_array",
        {"rgb": None, "w": 4, "h": 4}, {"values": fluid_array(None, 4, 4)})
    for index, spec in enumerate(("", "Ctrl+Alt+T", "ctrl+shift+k", "F9",
                                  "Ctrl+Alt+Shift+Q"), start=1):
        add(out, f"numbers/hotkey-label/{index:02d}", "chat_ui._hotkey_label",
            {"spec": spec}, {"label": _hotkey_label(spec)})
    add(
        out,
        "numbers/constants",
        "chat.constants",
        {},
        {
            "GAP": 14.0,
            "COPY_W": 30.0,
            "COPY_SLOT": 26.0,
            "COPY_ROW": 24.0,
            "SCROLL_STEP": 56.0,
            "STREAM_REFRESH": 0.05,
            "HEADER_H": 56.0,
            "DELETE_SECS": 0.18,
            "INPUT_MIN": 46.0,
            "INPUT_LINE": 21.0,
            "INPUT_MAX_LINES": 6,
            "PAD": 16.0,
            "BTN": 32.0,
            "HINT_H": 0.0,
            "AUDIO_W": 80.0,
            "PANEL_W": 440.0,
            "PANEL_H": 640.0,
            "MIN_W": 340.0,
            "MIN_H": 380.0,
            "MARGIN": MARGIN,
            "PANEL_RADIUS": 22.0,
            "MORPH_SECS": 0.28,
            "FADE_SECS": 0.12,
        },
    )


def action_dict(action: Any, root: Path) -> Any:
    if action is None:
        return None
    return {"kind": action.kind, "data": hit_data(action.data, root)}


def api_cases(out: FixtureFile, root: Path) -> None:
    # Estilos, medidas y texto de ``render_base``.
    for key, style in render_base.STYLES.items():
        add(out, f"api/render-base-style/{key}", "render_base.styles",
            {"style": key},
            {"token": style.token, "family": style.family, "size": style.size,
             "weight": style.weight, "tracking_em": style.tracking_em,
             "uppercase": style.uppercase})
    ctx = Ctx(1.0, card_palette("light"), 1000.0)
    add(out, "api/render-base-constants", "render_base.constants", {},
        {"LINE_K": render_base.LINE_K, "PARA_GAP": render_base.PARA_GAP})
    for index, (value, scale) in enumerate(
        [(0.0, 1.0), (0.5, 1.0), (1.5, 1.0), (2.5, 1.0), (-0.5, 1.0),
         (0.125, 1.25), (0.375, 1.5), (1.25, 2.0), (10.49, 1.0)],
        start=1,
    ):
        add(out, f"api/render-base-px/{index:02d}", "render_base.Ctx.px",
            {"value": value, "scale": scale},
            {"px": Ctx(scale, card_palette("light"), 1000.0).px(value)})
    for index, (name, alpha) in enumerate(
        [("MINT", None), ("MINT", 0.0), ("MINT", 0.5), ("MINT", 1.0),
         ("MINT", -0.5), ("MINT", 2.0), ("LINE", None), ("LINE", 0.5),
         ("TEXT_3", 0.25)],
        start=1,
    ):
        add(out, f"api/render-base-color/{index:02d}", "render_base.Ctx.color",
            {"name": name, "alpha": alpha},
            {"rgba": ctx.color(name, alpha)})
    for index, (text, key) in enumerate(
        [("Hola", "body"), ("Hola", "bold"), ("Hola", "code"), ("Ángel ñandú", "body"),
         ("WWWW", "body"), ("", "body"), ("123", "h1"), ("x", "cell")],
        start=1,
    ):
        add(out, f"api/render-base-length/{index:02d}", "render_base.Ctx.length",
            {"text": text, "style": key},
            {"length": ctx.length(text, key)})
    for index, (text, base, width) in enumerate(
        [("Hola mundo", "body", 300.0), ("Hola mundo", "body", 40.0),
         ("**negrita** y `código`", "body", 200.0), ("uno dos tres cuatro", "body", 30.0),
         ("UnaPalabraLarga", "body", 20.0), ("  espacios  ", "body", 80.0),
         ("", "body", 100.0), ("cursiva _x_", "body", 50.0)],
        start=1,
    ):
        lines = render_base.wrap(ctx, md.parse_inline(text), base, width)
        add(
            out,
            f"api/render-base-wrap/{index:02d}",
            "render_base.wrap",
            {"text": text, "base": base, "width": width},
            {"lines": [[{
                "x": run.x, "text": run.text, "width": run.width, "key": run.piece.key,
                "italic": run.piece.italic, "code": run.piece.code,
                "link": run.piece.link, "space": run.piece.space,
            } for run in line] for line in lines]},
        )
    # Bloques ricos: recorte con «…», fecha y constantes.
    for index, (text, width, key) in enumerate(
        [("Plan de lanzamiento.pdf", 180.0, "small"),
         ("Plan de lanzamiento.pdf", 60.0, "small"),
         ("ñandú", 20.0, "small"), ("", 20.0, "small"),
         ("a b c", 24.0, "micro")],
        start=1,
    ):
        add(out, f"api/render-rich-clip/{index:02d}", "render_rich._clip",
            {"text": text, "width": width, "style": key},
            {"text": render_rich._clip(ctx, text, width, key)})
    for index, wall in enumerate(
        [(2026, 11, 13, 9, 5, 7), (2024, 2, 29, 23, 59, 59), (2025, 1, 1, 0, 0, 0),
         (1999, 12, 31, 23, 59, 59)], start=1,
    ):
        add(out, f"api/render-rich-date/{index:02d}", "render_rich._date",
            {"mtime_wall": list(wall)}, {"date": render_rich._date(epoch(wall))})
    add(
        out,
        "api/render-rich-constants",
        "render_rich.constants",
        {},
        {
            "CELL_PAD_X": render_rich.CELL_PAD_X,
            "CELL_PAD_Y": render_rich.CELL_PAD_Y,
            "CELL_MIN": render_rich.CELL_MIN,
            "CARD_H": render_rich.CARD_H,
            "PREVIEW_MAX": render_rich.PREVIEW_MAX,
            "IMAGE_MAX_H": render_rich.IMAGE_MAX_H,
            "MONTHS": list(render_rich._MONTHS),
            "FAMILY": render_rich._FAMILY,
        },
    )
    add(out, "api/render-constants", "chat_render.constants", {},
        {"CODE_LINE": chat_render.CODE_LINE, "CODE_HEAD": chat_render.CODE_HEAD,
         "CODE_PAD": chat_render.CODE_PAD, "LIST_STEP": chat_render.LIST_STEP,
         "LIST_MARKER": chat_render.LIST_MARKER})
    add(out, "api/chat-header-constants", "chat_header.constants", {},
        {"HEADER_H": chat_header.HEADER_H, "DELETE_SECS": chat_header.DELETE_SECS,
         "ORB_D": chat_header.ORB_D, "NEW_ROW_H": chat_header.NEW_ROW_H})
    add(out, "api/chat-footer-constants", "chat_footer.constants", {},
        {"PAD": chat_footer.PAD, "BTN": chat_footer.BTN,
         "INPUT_MIN": chat_footer.INPUT_MIN, "INPUT_LINE": chat_footer.INPUT_LINE,
         "INPUT_MAX_LINES": chat_footer.INPUT_MAX_LINES, "HINT_H": chat_footer.HINT_H,
         "AUDIO_W": chat_footer.AUDIO_W})
    add(out, "api/chat-view-constants", "chat_view.constants", {},
        {"GAP": chat_view.GAP, "KEYED": list(chat_view.KEYED),
         "COPY_W": chat_view.COPY_W, "COPY_SLOT": chat_view.COPY_SLOT,
         "COPY_ROW": chat_view.COPY_ROW, "SCROLL_STEP": chat_view.SCROLL_STEP,
         "STREAM_REFRESH": chat_view.STREAM_REFRESH})
    add(out, "api/chat-ui-constants", "chat_ui.constants", {},
        {"CLOSE_SECS": chat_ui.CLOSE_SECS, "STATUS_TEXT": chat_ui.STATUS_TEXT,
         "ACTIVE_POSES": sorted(chat_ui.ACTIVE_POSES)})

    # Ciclo de vida del panel: duración, inversa del easing y pasos.
    ui = FakeUI("pill", "light", 1.0)
    panel = ChatPanel(ui)  # type: ignore[arg-type]
    for index, reduced in enumerate((False, True), start=1):
        ui._reduced = reduced
        add(out, f"api/chat-panel-duration/{index:02d}", "chat_ui.ChatPanel._duration",
            {"reduced": reduced, "closing": False},
            {"seconds": panel._duration(False), "closing": panel._duration(True)})
    ui._reduced = False
    for index, (p, duration) in enumerate(
        [(0.0, 0.28), (0.25, 0.28), (0.5, 0.28), (0.75, 0.28), (1.0, 0.28),
         (0.5, 0.20)], start=1,
    ):
        add(out, f"api/chat-panel-elapsed/{index:02d}", "chat_ui.ChatPanel._elapsed_for",
            {"p": p, "duration": duration}, {"seconds": panel._elapsed_for(p, duration)})
    for state in ("opening", "closing"):
        panel.anim = _Anim(state, 0.0, 0.0)
        values = []
        for now in (0.0, 0.05, 0.1, 0.16, 0.2, 0.28, 0.3):
            values.append({"now": now, "p": panel._step(now),
                           "state": panel.anim.state, "finished": panel._finished})
        add(out, f"api/chat-panel-step/{state}", "chat_ui.ChatPanel._step",
            {"state": state}, {"values": values})

    # Acciones de la vista: puntero, teclado, rueda y edición.
    base_model = conversation_model()
    view = ChatView(base_model)
    view.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
    view._focus_t0 = 0.0
    view.render(0.0)
    for index, kind in enumerate(
        ("new", "close", "settings", "history", "mic", "voice", "send", "copy_msg",
         "input"), start=1,
    ):
        target = next((hit for hit in view._hits if hit[1] == kind), None)
        if target is None:
            continue
        rect = target[0]
        x, y = (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2
        if kind == "send":
            view.input.set_text("Hola")
            view.render(0.0)
            target = next((hit for hit in view._hits if hit[1] == "send"), None)
            rect = target[0] if target else rect
            x, y = (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2
        action = view.pointer_down(x, y)
        add(out, f"api/view-pointer/{index:02d}-{kind}", "chat_view.ChatView.pointer_down",
            {"kind": kind, "x": x, "y": y},
            {"action": action_dict(action, root), "hover": view.hover,
             "input": {"text": view.input.text, "cursor": view.input.cursor,
                       "anchor": view.input.anchor}, "mode": view.mode})
        view.input.clear()
        view.mode, view.hover = "chat", None
        view.render(0.0)
    hover_view = ChatView(conversation_model())
    hover_view.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
    hover_view.render(0.0)
    target = next((hit for hit in hover_view._hits if hit[1] == "settings"), None)
    if target is not None:
        rect = target[0]
        changed = hover_view.pointer_move((rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2)
        add(out, "api/view-pointer-move", "chat_view.ChatView.pointer_move",
            {"x": (rect[0] + rect[2]) / 2, "y": (rect[1] + rect[3]) / 2},
            {"changed": changed, "hover": hover_view.hover})
    for index, (vk, ctrl, shift) in enumerate(
        [(13, False, False), (13, False, True), (27, False, False), (27, False, False),
         (65, True, False), (78, True, False), (8, False, False), (46, False, False),
         (37, False, False), (39, False, True), (36, False, False), (35, False, False)],
        start=1,
    ):
        key_view = ChatView(conversation_model())
        key_view.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
        key_view._focus_t0 = 0.0
        key_view.input.set_text("Hola mundo")
        key_view.input.cursor = 5
        if index == 3:
            key_view.input.clear()
        action = key_view.key(vk, ctrl, shift)
        add(out, f"api/view-key/{index:02d}", "chat_view.ChatView.key",
            {"vk": vk, "ctrl": ctrl, "shift": shift,
             "text": "Hola mundo" if index != 3 else ""},
            {"action": action_dict(action, root),
             "input": {"text": key_view.input.text, "cursor": key_view.input.cursor,
                       "anchor": key_view.input.anchor}, "mode": key_view.mode})
    for index, (kind, value) in enumerate(
        (("char", "ñ"), ("char", "\x00"), ("paste", "línea 1\nlínea 2")), start=1
    ):
        edit = ChatView(ChatModel())
        edit.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
        if kind == "char":
            edit.char(value)
        else:
            edit.paste(value)
        add(out, f"api/view-edit/{index:02d}", "chat_view.ChatView.edit",
            {"kind": kind, "value": value},
            {"input": {"text": edit.input.text, "cursor": edit.input.cursor,
                       "anchor": edit.input.anchor}})
    files = ChatView(ChatModel())
    files.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
    files.add_files([str(root / "chat" / "notas.md"), str(root / "chat" / "foto.png"), ""])
    add(out, "api/view-add-files", "chat_view.ChatView.add_files",
        {"paths": ["<tmp>/chat/notas.md", "<tmp>/chat/foto.png", ""]},
        {"input": {"text": collapse(files.input.text, root), "cursor": files.input.cursor}})
    for index, scroll in enumerate((0.0, 56.0, 5000.0), start=1):
        wheel_view = ChatView(conversation_model())
        wheel_view.configure(440.0, 640.0, 1.0, "light", card_palette("light"))
        wheel_view.render(0.0)
        wheel_view.scroll = scroll
        wheel_view.wheel(120)
        add(out, f"api/view-wheel/{index:02d}", "chat_view.ChatView.wheel",
            {"scroll": scroll, "delta": 120},
            {"scroll": wheel_view.scroll, "max_scroll": wheel_view._max_scroll})
    empty_view = ChatView(ChatModel())
    add(out, "api/view-can-start-empty", "chat_view.ChatView.can_start_new", {},
        {"value": empty_view.can_start_new()})
    add(out, "api/view-can-start-messages", "chat_view.ChatView.can_start_new", {},
        {"value": ChatView(conversation_model()).can_start_new()})
    busy_model = ChatModel()
    busy_model.begin_assistant()
    add(out, "api/view-animating-busy", "chat_view.ChatView.animating", {"now": 0.0},
        {"value": ChatView(busy_model).animating(0.0)})
    confirm_model = ChatModel()
    confirm_model.ask_confirm("¿Seguro?", True)
    add(out, "api/view-animating-confirm", "chat_view.ChatView.animating", {"now": 0.0},
        {"value": ChatView(confirm_model).animating(0.0)})
    add(out, "api/view-animating-idle", "chat_view.ChatView.animating", {"now": 0.0},
        {"value": ChatView(ChatModel()).animating(0.0)})
    flash_view = ChatView(ChatModel())
    flash_view._flash = ("texto", 1e18)
    add(out, "api/view-animating-flash", "chat_view.ChatView.animating", {"now": 0.0},
        {"value": flash_view.animating(0.0)})


def generate(out: FixtureFile) -> None:
    root = mark_home()
    files = prepare_files(root)
    set_themed(None)
    number_cases(out)
    message_cases(out, files)
    panel_cases(out, root)
    viewing_case(out, root)
    morph_cases(out)
    api_cases(out, root)
    set_themed(None)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)
