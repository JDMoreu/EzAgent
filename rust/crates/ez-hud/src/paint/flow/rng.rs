//! `numpy.random.default_rng`: `PCG64` con `SeedSequence` (F5-02).
//!
//! `flow.py` siembra la rejilla de ruido con `default_rng(20261109)` y el
//! origen del fluido con `default_rng(seed)`. Para que los píxeles coincidan
//! con los cuadros dorados hay que reproducir el generador bit a bit, no solo
//! el algoritmo de ruido: se portan `SeedSequence` (mezcla de entropía) y
//! `PCG64` (XSL RR 128/64) de NumPy 2.x.

use std::sync::LazyLock;

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

/// `SeedSequence(seed).generate_state(4, uint64)`: cuatro palabras de 64 bits,
/// dos de estado y dos de incremento.
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
    // Entropía que no cabe en la piscina (el Python admite enteros enormes).
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

/// `numpy.random.PCG64`: el generador que usa `default_rng`.
pub(super) struct Pcg64 {
    state: u128,
    inc: u128,
    has_uint32: bool,
    uinteger: u32,
}

impl Pcg64 {
    pub(super) fn new(seed: u64) -> Self {
        // `generate_state` pide 8 palabras de 32 bits con `hash_const = INIT_B`
        // y las empareja en little-endian para obtener los cuatro uint64.
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

    pub(super) fn next64(&mut self) -> u64 {
        self.step();
        let high = (self.state >> 64) as u64;
        let low = self.state as u64;
        (high ^ low).rotate_right((high >> 58) as u32)
    }

    /// `pcg64_next32`: la mitad baja de cada entero y la alta en la llamada
    /// siguiente (así rellena NumPy los arrays de `float32`).
    pub(super) fn next32(&mut self) -> u32 {
        if self.has_uint32 {
            self.has_uint32 = false;
            return self.uinteger;
        }
        let next = self.next64();
        self.has_uint32 = true;
        self.uinteger = (next >> 32) as u32;
        next as u32
    }

    /// `(next64() >> 11) * 2**-53`: el doble de `uniform`/`random`.
    pub(super) fn next_double(&mut self) -> f64 {
        (self.next64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }
}

/// `_LATTICE = default_rng(20261109).random((256, 256), dtype=float32)`.
pub(super) fn lattice() -> &'static [f32] {
    static LATTICE: LazyLock<Vec<f32>> = LazyLock::new(|| {
        let mut rng = Pcg64::new(20_261_109);
        (0..256 * 256)
            .map(|_| (rng.next32() >> 8) as f32 * (1.0 / 16_777_216.0))
            .collect()
    });
    &LATTICE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_sequence_matches_numpy() {
        // numpy.random.SeedSequence(7).generate_state(4, np.uint64)
        assert_eq!(
            seed_state(7),
            [
                0xEAD0_F701_7C32_6E58,
                0x0879_C4F0_F97E_037A,
                0x623A_8C4B_6745_675F,
                0xB344_3FAD_6038_6CAC,
            ]
        );
        assert_eq!(
            seed_state(20_261_109),
            [
                0xB7B6_F25E_3038_6058,
                0xBE3D_7392_69F9_072F,
                0xA86A_24B2_2F08_DF98,
                0xD134_155B_E2AB_294D,
            ]
        );
    }

    #[test]
    fn the_raw_stream_matches_numpy() {
        let mut rng = Pcg64::new(7);
        assert_eq!(rng.next64(), 0xA006_41A9_F1E5_4A8B);
        assert_eq!(rng.next64(), 0xE5AF_CDBC_AF26_6A95);
        let mut rng = Pcg64::new(20_261_109);
        assert_eq!(rng.next64(), 0x4FE4_F5AF_B8FD_5A8E);
    }

    #[test]
    fn the_origin_and_the_lattice_match_numpy() {
        let mut rng = Pcg64::new(7);
        let origin = [rng.next_double() * 60.0, rng.next_double() * 60.0];
        assert_eq!(origin, [37.505_727_996_280_015, 53.832_828_058_174_53]);

        let lattice = lattice();
        assert_eq!(lattice[0], 0.722_615_84);
        assert_eq!(lattice[1], 0.312_087_36);
        assert_eq!(lattice[2], 0.484_37);
        assert_eq!(lattice[3], 0.761_121_45);
        assert_eq!(lattice[32_768], 0.921_092_15);
    }

    #[test]
    fn the_whole_lattice_matches_numpy() {
        // FNV-1a de `flow._LATTICE.tobytes()` (65536 float32 little-endian).
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for value in lattice() {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        assert_eq!(lattice().len(), 65_536);
        assert_eq!(hash, 0xECEA_D308_54BD_0BBF);
    }

    #[test]
    fn float32_draws_take_the_low_and_high_halves_in_turn() {
        let mut rng = Pcg64::new(20_261_109);
        let first: Vec<f32> = (0..4)
            .map(|_| (rng.next32() >> 8) as f32 * (1.0 / 16_777_216.0))
            .collect();
        assert_eq!(
            first,
            vec![0.722_615_84, 0.312_087_36, 0.484_37, 0.761_121_45]
        );
    }
}
