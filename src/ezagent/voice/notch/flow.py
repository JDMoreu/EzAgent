"""«Marea»: the living gradient inside the bar (EzAgent's own look).

The form is filled with a slow, organic fluid of colour, like inks mixing in
water: a domain-warped noise field mapped onto a brand palette (deep teal,
aqua, mint/lime) with an occasional warm coral filament. It is deliberately not
an orb of clouds or an edge glow: the identity is the moving liquid itself.

How it reacts:

* **Idle:** the fluid drifts and folds slowly.
* **You speak (listen):** pressure from your voice speeds the flow up and pushes
  it upwards from the bottom; a bright inflow rises where the air comes in.
* **EzAgent speaks:** pulses of light travel along the form with its voice.
* **Thinking:** the whole fluid turns slowly, like a swirl being stirred.
* **Mute / sleep:** slow and desaturated; reduced motion freezes it.

Pure module (numpy only). ``FlowField`` keeps the interface that ``drop.py``
expects from the bar interior: ``step(dt, level, mode, reduced)``, ``render``,
``t``, ``pressure`` and ``layout``. Coordinates: units of half the form height,
``y`` grows downwards, ``x`` in [-aspect, aspect] and ``y`` in [-1, 1].
"""

from __future__ import annotations

import math

import numpy as np

# --------------------------------------------------------------------- colours
# Per theme and pose: deep, mid, bright, accent (filaments), glow (light).
_HEX: dict[str, dict[str, tuple[str, str, str, str, str]]] = {
    "light": {
        "idle": ("#0E6F7C", "#1FAE9F", "#8FE3C0", "#FF8A65", "#F1FFF8"),
        "listen": ("#0A6286", "#16B6C2", "#9BEFD6", "#FF7A59", "#FFFFFF"),
        "think": ("#24479E", "#1F8FB0", "#86D2E6", "#9BF2FF", "#EEF8FF"),
        "speak": ("#08706F", "#14B898", "#C4EE6A", "#FF7A59", "#FFFFFF"),
        "ask": ("#C2661A", "#EE9A34", "#FFD48A", "#FF6B4A", "#FFF4E0"),
        "error": ("#B92635", "#E2525A", "#FFAFA6", "#FF8A65", "#FFE6E3"),
        "mute": ("#646D7D", "#8E97A7", "#C3C9D4", "#A3ABBA", "#EEF0F4"),
    },
    "dark": {
        "idle": ("#05262E", "#0C6670", "#29BFA9", "#FF7A59", "#8CF5DA"),
        "listen": ("#042437", "#0A738C", "#2CD5CB", "#FF8A65", "#B0FFF2"),
        "think": ("#0A1A3E", "#1D3F93", "#2FA3C4", "#8CF0FF", "#CAEFFF"),
        "speak": ("#032C2A", "#0B8576", "#78E59A", "#FF8A65", "#DAFFB4"),
        "ask": ("#371C05", "#A15710", "#F0A038", "#FF6B4A", "#FFD68E"),
        "error": ("#36090F", "#9E222C", "#E2525A", "#FF8A65", "#FFB0A8"),
        "mute": ("#13161D", "#2C323E", "#5B6373", "#485060", "#8A92A3"),
    },
}
THEMES = ("light", "dark")
POSES = ("idle", "listen", "think", "speak", "ask", "error", "mute")


def _rgb(h: str) -> tuple[float, float, float]:
    return (float(int(h[1:3], 16)), float(int(h[3:5], 16)), float(int(h[5:7], 16)))


COLORS: dict[str, dict[str, np.ndarray]] = {
    theme: {pose: np.asarray([_rgb(c) for c in cols], np.float32) for pose, cols in poses.items()}
    for theme, poses in _HEX.items()
}


# Tema personalizado: el usuario elige cuatro colores. Estas paletas base son el
# punto de partida: (fondo, principal, secundario, acento).
PRESETS: dict[str, tuple[str, str, str, str]] = {
    "marea": ("#0E6F7C", "#1FAE9F", "#8FE3C0", "#FF8A65"),
    "atardecer": ("#5B1A3A", "#E0564F", "#FFB36B", "#FFE08A"),
    "bosque": ("#13361F", "#2E7D4F", "#A8D672", "#F2C94C"),
    "lavanda": ("#2E2A6B", "#7A6FF0", "#C9B8FF", "#FF9EC4"),
    "medianoche": ("#0B1026", "#2D3A8C", "#5FA8FF", "#9BF2FF"),
}
PRESET_LABELS: dict[str, str] = {
    "marea": "Marea",
    "atardecer": "Atardecer",
    "bosque": "Bosque",
    "lavanda": "Lavanda",
    "medianoche": "Medianoche",
}
# Con el tema personalizado, los estados de aviso conservan su color propio
# (ámbar, rojo, gris): comunican algo, no son decoración.
CUSTOM_POSES = frozenset({"idle", "listen", "think", "speak", "done", "sleep"})


def parse_hex(value: str) -> tuple[float, float, float] | None:
    """``#RRGGBB`` (o ``RRGGBB``, ``#RGB``) -> RGB 0..255; None si no vale."""
    text = str(value or "").strip().lstrip("#")
    if len(text) == 3:
        text = "".join(ch * 2 for ch in text)
    if len(text) != 6:
        return None
    try:
        return (float(int(text[0:2], 16)), float(int(text[2:4], 16)), float(int(text[4:6], 16)))
    except ValueError:
        return None


def custom_palette(primary: str, secondary: str, accent: str, background: str) -> np.ndarray | None:
    """Los cuatro colores del usuario como paleta de Marea (5, 3), o None."""
    parsed = [parse_hex(c) for c in (background, primary, secondary, accent)]
    if any(c is None for c in parsed):
        return None
    deep, mid, bright, spark = (np.asarray(c, np.float32) for c in parsed)
    glow = bright + (255.0 - bright) * 0.75  # la luz: el secundario casi blanco
    return np.stack([deep, mid, bright, spark, glow]).astype(np.float32)


def colors_for(theme: str, pose: str) -> np.ndarray:
    """(5, 3) float array: deep, mid, bright, accent, glow."""
    table = COLORS["light" if theme == "light" else "dark"]
    if pose in table:
        return table[pose]
    # Colgado («sleep») mantiene la calma; el gris queda solo para la pausa.
    return table["speak" if pose == "done" else "idle"]


# ----------------------------------------------------------------------- noise
_RNG = np.random.default_rng(20261109)
_LATTICE = _RNG.random((256, 256), dtype=np.float32)


def _vn(x: np.ndarray, y: np.ndarray) -> np.ndarray:
    """Smooth value noise in [0, 1] (vectorised)."""
    xi = np.floor(x).astype(np.int32)
    yi = np.floor(y).astype(np.int32)
    xf, yf = x - xi, y - yi
    u = xf * xf * (3.0 - 2.0 * xf)
    v = yf * yf * (3.0 - 2.0 * yf)
    x0, x1 = xi & 255, (xi + 1) & 255
    y0, y1 = yi & 255, (yi + 1) & 255
    a, b = _LATTICE[y0, x0], _LATTICE[y0, x1]
    c, d = _LATTICE[y1, x0], _LATTICE[y1, x1]
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v


def _fbm(x: np.ndarray, y: np.ndarray) -> np.ndarray:
    return (
        0.6 * _vn(x, y)
        + 0.3 * _vn(x * 2.03 + 3.1, y * 2.03 + 1.7)
        + 0.1 * _vn(x * 4.1 + 7.3, y * 4.1 + 2.9)
    )


def _smooth(edge0: float, edge1: float, x: np.ndarray) -> np.ndarray:
    k = np.clip((x - edge0) / (edge1 - edge0), 0.0, 1.0)
    return k * k * (3.0 - 2.0 * k)


# ----------------------------------------------------------------- simulation
FLOW_SPEED = 0.55  # how fast the fluid folds at rest
SCALE = 0.55  # spatial frequency of the fluid (lower = larger, calmer shapes)
CIRCLE_SCALE = 0.9  # the circle is small: a little more detail inside
WARP = 1.7  # how strongly the fluid folds over itself
RISE = 0.9  # units/s the fluid rises while you speak
SPIN = 0.55  # rad/s of the swirl while thinking
WAVE_SPEED = 5.0  # rad/s of EzAgent's voice pulses
ATTACK = 0.07  # s: the voice pressure rises quickly...
RELEASE = 0.7  # s: ...and fades slowly, so the fluid keeps moving a moment


class FlowField:
    """A living gradient that fills the form and reacts to both voices."""

    def __init__(self, layout: str = "pill", aspect: float | None = None, seed: int = 7) -> None:
        self.layout = layout
        self.aspect = float(aspect if aspect is not None else (1.0 if layout == "circle" else 3.9))
        rng = np.random.default_rng(seed)
        self.origin = rng.uniform(0.0, 60.0, 2).astype(np.float32)
        self.scale = CIRCLE_SCALE if layout == "circle" else SCALE
        self.t = 0.0
        self.pressure = 0.0
        self.wave = 0.0
        self.lift = 0.0  # accumulated upward flow from your voice
        self.angle = 0.0  # swirl while thinking
        self.spin_rate = 0.0
        self.mode = "idle"

    def inlet(self, mode: str) -> tuple[float, float]:
        """Where your voice enters the fluid: the bottom centre of the form."""
        return (0.0, 1.25)

    def step(self, dt: float, level: float, mode: str, reduced: bool = False) -> None:
        """Advances the fluid ``dt`` seconds. ``level`` is the voice (0..1)."""
        if reduced or dt <= 0.0:
            return
        dt = min(dt, 0.05)
        self.mode = mode
        air = level if mode in ("listen", "speak") else 0.0
        tau = ATTACK if air > self.pressure else RELEASE
        self.pressure += (air - self.pressure) * (1.0 - math.exp(-dt / tau))
        calm = {"mute": 0.25, "sleep": 0.35, "think": 1.5}.get(mode, 1.0)
        self.t += dt * FLOW_SPEED * calm * (1.0 + 2.5 * self.pressure)
        self.wave += dt * WAVE_SPEED * (0.6 + self.pressure)
        if mode == "listen":
            self.lift += dt * RISE * self.pressure
        target = SPIN if mode == "think" else 0.0
        self.spin_rate += (target - self.spin_rate) * (1.0 - math.exp(-dt / 0.6))
        self.angle += self.spin_rate * dt

    def field(self, gx: np.ndarray, gy: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """Fluid value ``f`` and filament mask ``r`` (both 0..1) on the grid."""
        ca, sa = math.cos(self.angle), math.sin(self.angle)
        x = gx * ca - gy * sa
        y = gx * sa + gy * ca
        px = x * self.scale + self.origin[0]
        py = (y + self.lift) * self.scale + self.origin[1]
        t = self.t
        q1 = _fbm(px + t * 0.12, py - t * 0.07)
        q2 = _fbm(px + 5.2 - t * 0.09, py + 1.3 + t * 0.05)
        k = WARP + 1.0 * self.pressure
        r1 = _fbm(px + k * q1 + 1.7 + t * 0.15, py + k * q2 + 9.2)
        r2 = _fbm(px + k * q1 + 8.3 - t * 0.11, py + k * q2 + 2.8)
        f = _fbm(px + 2.2 * r1, py + 2.2 * r2)
        return np.clip((f - 0.22) / 0.56, 0.0, 1.0), r1

    def render(self, gx: np.ndarray, gy: np.ndarray, theme: str, colors: np.ndarray) -> np.ndarray:
        """RGB float image (0..255) of the form interior on the grid."""
        deep, mid, bright, accent, glow = colors
        f, filaments = self.field(gx, gy)
        # Palette ramp: deep -> mid -> bright, with smooth overlaps.
        a = _smooth(0.0, 0.55, f)[..., None]
        b = _smooth(0.45, 1.0, f)[..., None]
        img = deep * (1.0 - a) + mid * a
        img = img * (1.0 - b) + bright * b
        # Warm filaments where the fluid folds hardest: the coral signature. A
        # «screen» blend lightens like light does, so coral over teal turns
        # peach instead of mud.
        spark = (_smooth(0.64, 0.8, filaments) * (0.45 + 0.3 * self.pressure))[..., None]
        screen = 255.0 - (255.0 - img) * (255.0 - accent) / 255.0
        img += (screen - img) * spark
        # Soft light from above: a glassy sheen on the top of the form.
        sheen = (np.clip(-gy, 0.0, 1.0) ** 2 * 0.14)[..., None]
        img += (glow - img) * sheen

        if self.pressure > 1e-3:
            if self.mode == "speak":
                # EzAgent's voice: bands of light that travel along the form.
                band = 0.5 + 0.5 * np.sin(gx * 1.4 - self.wave + f * 2.5)
                g = band**3 * self.pressure * 0.45
            else:
                # Your voice: a bright inflow rising from the bottom centre.
                ix, iy = self.inlet(self.mode)
                d = ((gx - ix) / (self.aspect * 0.9)) ** 2 + ((gy - iy) / 0.85) ** 2
                g = np.exp(-d) * self.pressure * 0.6
            img += (glow - img) * g[..., None]
        return img
