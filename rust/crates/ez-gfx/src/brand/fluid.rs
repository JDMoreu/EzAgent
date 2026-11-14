//! The fluid frame of `brand._fluid`: `FlowField("circle", seed=7)` at
//! `scale = 0.32`, stepped 45 times at 1/30 s in the `idle` pose, rendered
//! with the light theme palette.
//!
//! `ez-hud::paint::flow` holds the full port, but it depends on `ez-gfx`; the
//! brand mark lives in `ez-gfx`, so the slice the mark needs is repeated here
//! with the same NumPy `PCG64` stream. DECISION: keep it private and local
//! until the RNG moves to `ez-core`.

use std::sync::LazyLock;

/// Spatial frequency of the frame (`_FRAME_SCALE`).
const FRAME_SCALE: f64 = 0.32;
/// Rest speed of the fluid (`FLOW_SPEED`).
const FLOW_SPEED: f64 = 0.55;
/// `_FRAME_TIME * 30`.
const FRAME_STEPS: usize = 45;
/// Palette of `colors_for("light", "idle")`: deep, mid, bright, accent, glow.
const COLORS: [[f32; 3]; 5] = [
    [14.0, 111.0, 124.0],
    [31.0, 174.0, 159.0],
    [143.0, 227.0, 192.0],
    [255.0, 138.0, 101.0],
    [241.0, 255.0, 248.0],
];

const MASK32: u32 = 0xFFFF_FFFF;
const MULT_HIGH: u64 = 2_549_297_995_355_413_924;
const MULT_LOW: u64 = 4_865_540_595_714_422_341;
const DEFAULT_MULTIPLIER: u128 = ((MULT_HIGH as u128) << 64) | MULT_LOW as u128;
const INIT_A: u32 = 0x43B0_D7E5;
const MULT_A: u32 = 0x931E_8875;
const INIT_B: u32 = 0x8B51_F9DD;
const MULT_B: u32 = 0x58F3_8DED;
const MIX_MULT_L: u32 = 0xCA01_F9DD;
const MIX_MULT_R: u32 = 0x4973_F715;
const XSHIFT: u32 = 16;

/// `SeedSequence(seed).generate_state(4, uint64)`.
fn seed_state(seed: u64) -> [u64; 4] {
    let mut words = Vec::with_capacity(2);
    let mut rest = seed;
    if rest == 0 {
        words.push(0);
    }
    while rest > 0 {
        words.push((rest & MASK32 as u64) as u32);
        rest >>= 32;
    }

    let mut hash_const = INIT_A;
    let mut pool = [0u32; 4];
    for (index, slot) in pool.iter_mut().enumerate() {
        *slot = hash_mix(words.get(index).copied().unwrap_or(0), &mut hash_const);
    }
    for source in 0..4 {
        for destination in 0..4 {
            if source != destination {
                let mixed = hash_mix(pool[source], &mut hash_const);
                pool[destination] = mix(pool[destination], mixed);
            }
        }
    }
    for &word in words.iter().skip(pool.len()) {
        for slot in &mut pool {
            let mixed = hash_mix(word, &mut hash_const);
            *slot = mix(*slot, mixed);
        }
    }

    let mut state = [0u64; 4];
    let mut generated = [0u32; 8];
    let mut hash_const = INIT_B;
    for (index, slot) in generated.iter_mut().enumerate() {
        let mut value = pool[index % 4] ^ hash_const;
        hash_const = hash_const.wrapping_mul(MULT_B);
        value = value.wrapping_mul(hash_const);
        *slot = value ^ (value >> XSHIFT);
    }
    for index in 0..4 {
        state[index] = (generated[index * 2] as u64) | ((generated[index * 2 + 1] as u64) << 32);
    }
    state
}

fn hash_mix(value: u32, hash_const: &mut u32) -> u32 {
    let mut value = value ^ *hash_const;
    *hash_const = hash_const.wrapping_mul(MULT_A);
    value = value.wrapping_mul(*hash_const);
    value ^ (value >> XSHIFT)
}

fn mix(x: u32, y: u32) -> u32 {
    let mut result = MIX_MULT_L
        .wrapping_mul(x)
        .wrapping_sub(MIX_MULT_R.wrapping_mul(y));
    result ^= result >> XSHIFT;
    result
}

/// `numpy.random.PCG64`, the generator behind `default_rng`.
struct Pcg64 {
    state: u128,
    inc: u128,
    has_uint32: bool,
    uinteger: u32,
}

impl Pcg64 {
    fn new(seed: u64) -> Self {
        let words = seed_state(seed);
        let initstate = ((words[0] as u128) << 64) | words[1] as u128;
        let initseq = ((words[2] as u128) << 64) | words[3] as u128;
        let mut rng = Self {
            state: 0,
            inc: (initseq << 1) | 1,
            has_uint32: false,
            uinteger: 0,
        };
        rng.step();
        rng.state = rng.state.wrapping_add(initstate);
        rng.step();
        rng
    }

    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(DEFAULT_MULTIPLIER)
            .wrapping_add(self.inc);
    }

    fn next64(&mut self) -> u64 {
        self.step();
        let high = (self.state >> 64) as u64;
        let low = self.state as u64;
        (high ^ low).rotate_right((high >> 58) as u32)
    }

    /// `pcg64_next32`: the low half first, the high half on the next call.
    fn next32(&mut self) -> u32 {
        if self.has_uint32 {
            self.has_uint32 = false;
            return self.uinteger;
        }
        let next = self.next64();
        self.has_uint32 = true;
        self.uinteger = (next >> 32) as u32;
        next as u32
    }

    fn next_double(&mut self) -> f64 {
        (self.next64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }
}

/// `_LATTICE = default_rng(20261109).random((256, 256), dtype=float32)`.
fn lattice() -> &'static [f32] {
    static LATTICE: LazyLock<Vec<f32>> = LazyLock::new(|| {
        let mut rng = Pcg64::new(20_261_109);
        (0..256 * 256)
            .map(|_| (rng.next32() >> 8) as f32 * (1.0 / 16_777_216.0))
            .collect()
    });
    &LATTICE
}

/// `FlowField("circle", seed=7).origin`.
fn origin() -> [f32; 2] {
    let mut rng = Pcg64::new(7);
    [
        (rng.next_double() * 60.0) as f32,
        (rng.next_double() * 60.0) as f32,
    ]
}

/// `field.t` after `int(1.5 * 30)` idle steps of `1 / 30` s.
fn frame_time() -> f64 {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    for _ in 0..FRAME_STEPS {
        t += dt * FLOW_SPEED;
    }
    t
}

/// `_vn(x, y)` for values that NumPy already promoted to `float64`.
fn value_noise(x: f64, y: f64) -> f64 {
    let xi = x.floor();
    let yi = y.floor();
    let xf = x - xi;
    let yf = y - yi;
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let xi32 = xi as i32;
    let yi32 = yi as i32;
    let x0 = (xi32 & 255) as usize;
    let x1 = ((xi32 + 1) & 255) as usize;
    let y0 = (yi32 & 255) as usize;
    let y1 = ((yi32 + 1) & 255) as usize;
    let lattice = lattice();
    let a = lattice[y0 * 256 + x0];
    let b = lattice[y0 * 256 + x1];
    let c = lattice[y1 * 256 + x0];
    let d = lattice[y1 * 256 + x1];
    let ba = f64::from(b - a);
    let ca = f64::from(c - a);
    let abcd = f64::from((a - b) - c + d);
    f64::from(a) + ba * u + ca * v + abcd * u * v
}

/// `_fbm` for arguments that stay `float32` in NumPy.
fn fbm32(x: f32, y: f32) -> f64 {
    0.6 * value_noise(f64::from(x), f64::from(y))
        + 0.3
            * value_noise(
                f64::from(x * 2.03f32 + 3.1f32),
                f64::from(y * 2.03f32 + 1.7f32),
            )
        + 0.1
            * value_noise(
                f64::from(x * 4.1f32 + 7.3f32),
                f64::from(y * 4.1f32 + 2.9f32),
            )
}

/// `_fbm` for arguments already promoted to `float64` by `k * q`.
fn fbm64(x: f64, y: f64) -> f64 {
    0.6 * value_noise(x, y)
        + 0.3 * value_noise(x * 2.03 + 3.1, y * 2.03 + 1.7)
        + 0.1 * value_noise(x * 4.1 + 7.3, y * 4.1 + 2.9)
}

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let k = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// One RGB frame of the fluid (`size`², row-major), clipped and truncated to
/// bytes exactly like `np.clip(_fluid(size), 0, 255).astype(np.uint8)`.
pub(crate) fn fluid_frame(size: u32) -> Vec<u8> {
    let axis: Vec<f32> = (0..size)
        .map(|i| {
            if size > 1 {
                (-1.0 + 2.0 * f64::from(i) / f64::from(size - 1)) as f32
            } else {
                -1.0
            }
        })
        .collect();
    let origin = origin();
    let t = frame_time();
    let mut out = vec![0u8; size as usize * size as usize * 3];
    for (y, &gy) in axis.iter().enumerate() {
        let py = gy * FRAME_SCALE as f32 + origin[1];
        for (x, &gx) in axis.iter().enumerate() {
            let px = gx * FRAME_SCALE as f32 + origin[0];
            let q1 = fbm32(px + (t * 0.12) as f32, py - (t * 0.07) as f32);
            let q2 = fbm32(
                (px + 5.2f32) - (t * 0.09) as f32,
                (py + 1.3f32) + (t * 0.05) as f32,
            );
            let k = 1.7;
            let r1 = fbm64(
                f64::from(px) + k * q1 + 1.7 + t * 0.15,
                f64::from(py) + k * q2 + 9.2,
            );
            let r2 = fbm64(
                f64::from(px) + k * q1 + 8.3 - t * 0.11,
                f64::from(py) + k * q2 + 2.8,
            );
            let f = fbm64(f64::from(px) + 2.2 * r1, f64::from(py) + 2.2 * r2);
            let f = ((f - 0.22) / 0.56).clamp(0.0, 1.0);
            let a = smooth(0.0, 0.55, f);
            let b = smooth(0.45, 1.0, f);
            let spark = smooth(0.64, 0.8, r1) * 0.45;
            let sheen = ((-gy).clamp(0.0, 1.0)).powi(2) * 0.14f32;
            let index = (y * size as usize + x) * 3;
            for channel in 0..3 {
                let deep = f64::from(COLORS[0][channel]);
                let mid = f64::from(COLORS[1][channel]);
                let bright = f64::from(COLORS[2][channel]);
                let accent = f64::from(COLORS[3][channel]);
                let glow = f64::from(COLORS[4][channel]);
                let mut img = deep * (1.0 - a) + mid * a;
                img = img * (1.0 - b) + bright * b;
                let screen = 255.0 - (255.0 - img) * (255.0 - accent) / 255.0;
                img += (screen - img) * spark;
                img += (glow - img) * f64::from(sheen);
                out[index + channel] = img.clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv1a_f32(values: &[f32]) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for value in values {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        hash
    }

    #[test]
    fn the_seed_sequence_and_stream_match_numpy() {
        assert_eq!(
            seed_state(7),
            [
                0xEAD0_F701_7C32_6E58,
                0x0879_C4F0_F97E_037A,
                0x623A_8C4B_6745_675F,
                0xB344_3FAD_6038_6CAC,
            ]
        );
        let mut rng = Pcg64::new(7);
        assert_eq!(rng.next64(), 0xA006_41A9_F1E5_4A8B);
        assert_eq!(rng.next64(), 0xE5AF_CDBC_AF26_6A95);
    }

    #[test]
    fn origin_and_lattice_match_numpy() {
        assert_eq!(origin(), [37.50573, 53.83283]);
        let lattice = lattice();
        assert_eq!(lattice[0], 0.722_615_84);
        assert_eq!(lattice[1], 0.312_087_36);
        assert_eq!(lattice[2], 0.484_37);
        assert_eq!(lattice[3], 0.761_121_45);
        assert_eq!(lattice[32_768], 0.921_092_15);
        assert_eq!(fnv1a_f32(lattice), 0xECEA_D308_54BD_0BBF);
    }

    #[test]
    fn frame_time_matches_the_python_steps() {
        assert_eq!(frame_time(), 0.824_999_999_999_999_4);
    }
}
