"""Clouds kept inside the bar, moved by air.

The form is a closed container: clear sky in the top half and a bank of soft
clouds in the bottom half. The clouds are bodies with position, velocity and
inertia: each one is held to a home position by a soft spring, drifts slowly
with a calm wind and wobbles with a little turbulence. When you speak, air
enters from the bottom centre: the gust lifts the clouds, parts them and makes
them puff up; when you stop, they settle back with a slow, damped sway. While
EzAgent speaks a swell runs along the bank to the rhythm of its voice; while it
thinks the clouds swirl.

Pure module (numpy only): ``CloudField.step`` advances the simulation and
``CloudField.render`` returns an RGB image on a coarse grid that ``drop.py``
scales up and masks with the exact outline of the form.

Coordinates: units of half the form height. ``y`` grows downwards, the form
spans ``x`` in [-aspect, aspect] and ``y`` in [-1, 1].
"""

from __future__ import annotations

import math

import numpy as np

# --------------------------------------------------------------------- colours
# Per theme and pose: sky top, sky bottom, cloud light, cloud shadow, air glow.
_HEX: dict[str, dict[str, tuple[str, str, str, str, str]]] = {
    "light": {
        "idle": ("#6E8FF7", "#AFC5FF", "#FFFFFF", "#C9D6FA", "#EEF3FF"),
        "listen": ("#4B78F2", "#93B3FF", "#FFFFFF", "#BFD0FA", "#FFFFFF"),
        "think": ("#7A7CF0", "#BCAEFF", "#FFFFFF", "#D6CCFA", "#F3ECFF"),
        "speak": ("#3F6EEB", "#86ABFF", "#FFFFFF", "#BCCDF8", "#FFFFFF"),
        "ask": ("#F29A3A", "#FFC88A", "#FFF7EC", "#F6D2A8", "#FFF1DE"),
        "error": ("#E5484D", "#FF8E86", "#FFF1F0", "#F7B9B4", "#FFE3E1"),
        "mute": ("#8E98AE", "#BCC4D5", "#F3F5F9", "#CDD3DF", "#E9ECF2"),
    },
    "dark": {
        "idle": ("#0E1A3F", "#2A4FB8", "#CFE0FF", "#5B78C9", "#6F98FF"),
        "listen": ("#10215A", "#3466E0", "#EAF2FF", "#6B8EE0", "#9DBBFF"),
        "think": ("#1B1748", "#5146C8", "#E6E0FF", "#7D70D8", "#A99BFF"),
        "speak": ("#0D1E52", "#2F6BF0", "#F0F6FF", "#6E95EA", "#A8C6FF"),
        "ask": ("#3A2208", "#C77B1E", "#FFE9C9", "#B98446", "#FFC879"),
        "error": ("#3A0D10", "#C23A3F", "#FFD9D6", "#B85A5C", "#FF8F88"),
        "mute": ("#161A24", "#3A4253", "#B9C0CE", "#5F6879", "#7D8697"),
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


def colors_for(theme: str, pose: str) -> np.ndarray:
    """(5, 3) float array: sky top, sky bottom, cloud, cloud shadow, glow."""
    table = COLORS["light" if theme == "light" else "dark"]
    if pose in table:
        return table[pose]
    # Colgado («sleep») mantiene el cielo en calma; el gris queda solo para la pausa.
    return table["speak" if pose == "done" else "idle"]


# ----------------------------------------------------------------------- noise
_RNG = np.random.default_rng(20261108)
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


# ----------------------------------------------------------------- simulation
CALM_WIND = 0.08  # units/s of slow drift across the container
HOME_K = 3.2  # spring back to the home position (soft: ~1.5 s to settle)
DRAG = 2.2  # damping: slightly underdamped, so clouds sway when they return
GUST = 8.0  # force of your voice: air from the bottom lifts and parts the clouds
WAVE = 5.0  # force of EzAgent's voice: a wave that runs along the cloud bank
CEILING = 0.15  # clouds may rise a little past the middle, no further...
CEILING_K = 14.0  # ...a stiff, soft spring holds them there: the top stays sky
WAVE_SPEED = 5.0  # rad/s of that wave (faster with a louder voice)
TURBULENCE = 1.1
SWIRL = 0.9  # tangential force while thinking
ATTACK = 0.07  # s: air pressure rises quickly when the voice starts...
RELEASE = 0.7  # s: ...and fades slowly, so the clouds keep moving for a moment
CLOUD_STRETCH = 1.5  # clouds are wider than tall
BANK_Y = (0.5, 0.8)  # homes in the lower half: the top half is clear sky


class CloudField:
    """A bank of clouds with inertia in the lower half of the form."""

    def __init__(self, layout: str = "pill", aspect: float | None = None, seed: int = 7) -> None:
        self.layout = layout
        self.aspect = float(aspect if aspect is not None else (1.0 if layout == "circle" else 3.9))
        rng = np.random.default_rng(seed)
        n = 4 if layout == "circle" else 8
        a = self.aspect
        # Spread past the edges so the bank has no gaps at the rounded ends.
        xs = np.linspace(-a - 0.2, a + 0.2, n) + rng.uniform(-0.12, 0.12, n)
        ys = rng.uniform(*BANK_Y, n)
        self.home = np.stack([xs, ys], axis=1).astype(np.float32)
        self.pos = self.home.copy()
        self.vel = np.zeros_like(self.pos)
        base = 0.58 if layout == "circle" else 0.72
        self.radius = (base * rng.uniform(0.85, 1.15, n)).astype(np.float32)
        self.opacity = np.ones(n, np.float32)
        self.puff = np.ones(n, np.float32)
        self.phase = rng.uniform(0.0, 100.0, n).astype(np.float32)
        self.pressure = 0.0
        self.t = 0.0
        self.wave = 0.0
        self.mode = "idle"

    @property
    def count(self) -> int:
        return int(self.pos.shape[0])

    def inlet(self, mode: str) -> tuple[float, float]:
        """Where the air enters: always from the bottom of the container."""
        return (0.0, 1.35)

    def step(self, dt: float, level: float, mode: str, reduced: bool = False) -> None:
        """Advances the clouds ``dt`` seconds. ``level`` is the voice (0..1)."""
        if reduced or dt <= 0.0:
            return
        dt = min(dt, 0.05)
        self.mode = mode
        air = level if mode in ("listen", "speak") else 0.0
        tau = ATTACK if air > self.pressure else RELEASE
        self.pressure += (air - self.pressure) * (1.0 - math.exp(-dt / tau))
        calm = {"mute": 0.25, "sleep": 0.4, "think": 1.3}.get(mode, 1.0)
        self.t += dt * calm * (1.0 + 3.0 * self.pressure)
        self.wave += dt * WAVE_SPEED * (0.6 + self.pressure)
        self._drift(dt, calm)

        force = -HOME_K * (self.pos - self.home) - DRAG * self.vel
        force += self._turbulence(calm) * (0.4 + 1.5 * self.pressure)
        if self.pressure > 1e-3:
            force += self._wave() if mode == "speak" else self._gust(mode)
        high = np.minimum(self.pos[:, 1] - CEILING, 0.0)  # negative when above the ceiling
        force[:, 1] -= CEILING_K * high
        if mode == "think":
            rel = self.pos - np.asarray([0.0, 0.5], np.float32)
            swirl = np.stack([-rel[:, 1], rel[:, 0]], axis=1)
            force += SWIRL * swirl / (1.0 + np.abs(rel).sum(axis=1, keepdims=True))
        self.vel += force * dt
        self.pos += self.vel * dt
        near = self._near_inlet(mode) if mode != "speak" else np.ones(self.count, np.float32)
        target = 1.0 + 0.35 * self.pressure * near
        self.puff += (target - self.puff) * (1.0 - math.exp(-dt / 0.2))

    def _drift(self, dt: float, calm: float) -> None:
        """Homes drift with the calm wind; a cloud that leaves one side, hidden
        behind the outline, comes back in from the other."""
        a = self.aspect
        self.home[:, 0] += CALM_WIND * calm * dt
        span = a + self.radius * CLOUD_STRETCH * 1.1
        gone = self.home[:, 0] > span
        if gone.any():
            self.home[gone, 0] -= 2 * span[gone]
            self.pos[gone, 0] -= 2 * span[gone]

    def _turbulence(self, calm: float) -> np.ndarray:
        t = self.t * 0.35
        ph = self.phase
        fx = _vn(ph + t, ph * 0.5) - 0.5
        fy = _vn(ph * 0.7 + 11.0, ph + t * 0.8) - 0.5
        return TURBULENCE * calm * np.stack([fx, fy * 0.6], axis=1).astype(np.float32)

    def _near_inlet(self, mode: str) -> np.ndarray:
        ix, iy = self.inlet(mode)
        d2 = ((self.pos[:, 0] - ix) / max(1.0, self.aspect * 0.75)) ** 2 + (
            self.pos[:, 1] - iy
        ) ** 2
        return np.exp(-d2 * 0.6).astype(np.float32)

    def _gust(self, mode: str) -> np.ndarray:
        """Your voice: air from the bottom centre lifts the clouds and parts them."""
        ix, iy = self.inlet(mode)
        rel = self.pos - np.asarray([ix, iy], np.float32)
        dist = np.sqrt((rel**2).sum(axis=1, keepdims=True)) + 0.15
        push = rel / dist
        push[:, 1] -= 0.7  # air from below lifts the clouds
        return (GUST * self.pressure * self._near_inlet(mode)[:, None] * push).astype(np.float32)

    def _wave(self) -> np.ndarray:
        """EzAgent's voice: a swell that runs along the bank (up and a little sideways)."""
        x = self.pos[:, 0]
        crest = np.sin(x * 1.3 - self.wave)
        fy = -WAVE * self.pressure * (0.15 + crest)  # rises and falls: an undulating swell
        fx = 0.35 * WAVE * self.pressure * np.cos(x * 1.3 - self.wave)
        return np.stack([fx, fy], axis=1).astype(np.float32)

    # ---------------------------------------------------------------- render
    def density(self, gx: np.ndarray, gy: np.ndarray, dy: float = 0.0) -> np.ndarray:
        """Cloud density on the grid (sum of soft, stretched blobs)."""
        out = np.zeros(gx.shape, np.float32)
        for (px, py), r, puff, op in zip(
            self.pos, self.radius, self.puff, self.opacity, strict=True
        ):
            rr = r * puff
            d2 = ((gx - px) / (rr * CLOUD_STRETCH)) ** 2 + ((gy - dy - py) / rr) ** 2
            out += op * np.exp(-d2 * 1.7)
        return out

    def render(self, gx: np.ndarray, gy: np.ndarray, theme: str, colors: np.ndarray) -> np.ndarray:
        """RGB float image (0..255) of the container interior on the grid."""
        t = self.t
        # Domain warp: fluffy, ever-changing cloud edges instead of perfect ovals.
        wx = _fbm(gx * 0.9 + t * 0.25, gy * 0.9) - 0.5
        wy = _fbm(gx * 0.9 + 5.2, gy * 0.9 + t * 0.2) - 0.5
        k = 0.55 + 0.6 * self.pressure  # la voz agita también el borde de las nubes
        x, y = gx + wx * k, gy + wy * k
        dens = self.density(x, y)
        detail = _fbm(x * 2.2 - t * 0.3, y * 2.2 + t * 0.15)
        dens = dens * (0.7 + 0.6 * detail)
        alpha = np.clip((dens - 0.3) / 0.75, 0.0, 1.0)  # rampa ancha: bordes de nube muy suaves
        alpha = alpha * alpha * (3.0 - 2.0 * alpha)
        # Light from above: the underside of each cloud is in soft shadow.
        shade = np.clip((self.density(x, y, dy=0.35) - dens) * 0.9, 0.0, 1.0)

        top, bottom, light, shadow, glow = colors
        v = np.clip((gy + 1.0) / 2.0, 0.0, 1.0)[..., None]
        sky = top * (1.0 - v) + bottom * v
        cloud = light + (shadow - light) * shade[..., None]
        img = sky * (1.0 - alpha[..., None]) + cloud * alpha[..., None]

        # The air shows as a soft glow where it comes in.
        if self.pressure > 1e-3:
            ix, iy = self.inlet(self.mode)
            wide = 1.3 if self.mode == "speak" else 0.7  # el modelo ilumina todo el banco
            g = np.exp(-(((gx - ix) / (self.aspect * wide)) ** 2 + ((gy - iy) / 0.9) ** 2))
            img += (glow - img) * (g * self.pressure * 0.55)[..., None]
        return img
