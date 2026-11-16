//! Umbral de barge-in adaptativo con referencia de eco (P-IMP-BARGEIN).
//!
//! El umbral fijo (`barge_in_rms`, 0,008) no separa la voz normal del usuario
//! del eco del propio TTS: con la webcam como micrófono la voz da RMS p50≈0,0014
//! y p90≈0,0085-0,011, y el ruido de sala cruza 0,008 en el 8-28 % de los
//! bloques. Mientras la app habla, el altavoz publica la envolvente de lo que
//! suena y cada bloque del micrófono se compara con ella:
//!
//! `umbral = máx(piso de ruido × margen, acople × referencia × margen, mínimo)`
//!
//! El acople se estima con la mediana de `mic/ref` de los bloques del principio
//! de cada frase (casi seguro solo eco), emparejando el retardo altavoz→micrófono
//! que mejor encaja: así la voz del usuario, que sube el cociente de algunos
//! bloques, no infla el acople. Sin referencia (el arnés de reproducción no
//! suena) se usa el detector clásico.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ez_core::metrics::monotonic;

use crate::CHUNK_SECS;

/// Ventana de referencia: cubre el retardo altavoz→micrófono (50-250 ms).
pub const REFERENCE_WINDOW_SECS: f64 = 0.35;
/// Bloques de 30 ms que conserva la referencia (≈1 s).
const SLOTS: usize = 32;
/// Retardo altavoz→micrófono que se busca al estimar el acople: 60-240 ms.
const MIN_LAG: usize = 2;
const MAX_LAG: usize = 8;
/// Bloques de historia para emparejar el eco con su referencia.
const HISTORY: usize = 16;
/// 20 bits para el nivel: resolución 1e-6, de sobra para umbrales de 0,001.
const LEVEL_BITS: u32 = 20;
const LEVEL_MASK: u64 = (1 << LEVEL_BITS) - 1;

/// Modo del detector mientras la app habla.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BargeInMode {
    /// `barge_in_rms` fijo, como el Python.
    #[default]
    Fixed,
    /// Umbral por bloque descontando el eco de la referencia.
    Adaptive,
}

fn now_ms() -> u64 {
    (monotonic() * 1000.0) as u64
}

fn pack(at_ms: u64, level: f64) -> u64 {
    let quantized = (level.clamp(0.0, 1.0) * LEVEL_MASK as f64) as u64;
    // La marca de tiempo va +1: el 0 queda reservado para «sin dato».
    ((at_ms.saturating_add(1)) << LEVEL_BITS) | quantized
}

fn unpack(value: u64) -> Option<(u64, f64)> {
    let stamp = value >> LEVEL_BITS;
    if stamp == 0 {
        return None;
    }
    Some(((stamp - 1), (value & LEVEL_MASK) as f64 / LEVEL_MASK as f64))
}

/// Envolvente de lo que el altavoz está reproduciendo, por bloques de 30 ms.
///
/// Sin cerrojos: cada bloque es un `AtomicU64` con la marca de tiempo (ms del
/// reloj monotónico del proceso) y el nivel cuantizado. Publicar y leer cuesta
/// nanosegundos y no puede bloquear al hilo de audio ni al de captura.
#[derive(Debug)]
pub struct PlaybackReference {
    slots: [AtomicU64; SLOTS],
    head: AtomicU64,
    phrase: AtomicU64,
    active: AtomicBool,
}

impl Default for PlaybackReference {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| AtomicU64::new(0)),
            head: AtomicU64::new(0),
            phrase: AtomicU64::new(0),
            active: AtomicBool::new(false),
        }
    }
}

impl PlaybackReference {
    pub fn new() -> Self {
        Self::default()
    }

    /// Empieza una frase: descarta la anterior y sube el número de frase.
    pub fn begin(&self) {
        self.phrase.fetch_add(1, Ordering::Relaxed);
        for slot in &self.slots {
            slot.store(0, Ordering::Relaxed);
        }
        self.head.store(0, Ordering::Relaxed);
        self.active.store(true, Ordering::Release);
    }

    /// La frase terminó (o no llegó a sonar).
    pub fn end(&self) {
        self.active.store(false, Ordering::Release);
    }

    /// Publica el nivel RMS del bloque que suena en este momento.
    pub fn publish(&self, level: f64) {
        self.publish_at(now_ms(), level);
    }

    /// Como [`Self::publish`], con la marca de tiempo explícita (pruebas).
    pub fn publish_at(&self, at_ms: u64, level: f64) {
        let slot = (self.head.fetch_add(1, Ordering::Relaxed) as usize) % SLOTS;
        self.slots[slot].store(pack(at_ms, level), Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn phrase(&self) -> u64 {
        self.phrase.load(Ordering::Relaxed)
    }

    /// Nivel máximo de la referencia en los últimos `window_secs`.
    pub fn peak_in_window(&self, window_secs: f64) -> Option<f64> {
        self.peak_at(now_ms(), window_secs)
    }

    /// Como [`Self::peak_in_window`], con el reloj explícito (pruebas).
    pub fn peak_at(&self, now_ms: u64, window_secs: f64) -> Option<f64> {
        if !self.is_active() {
            return None;
        }
        let window = (window_secs.max(0.0) * 1000.0) as u64;
        let since = now_ms.saturating_sub(window);
        let mut peak: Option<f64> = None;
        for slot in &self.slots {
            let Some((at, level)) = unpack(slot.load(Ordering::Relaxed)) else {
                continue;
            };
            if at < since || at > now_ms {
                continue;
            }
            peak = Some(peak.map_or(level, |best: f64| best.max(level)));
        }
        peak
    }

    /// Nivel del bloque publicado más recientemente (el que suena ahora).
    pub fn latest(&self) -> Option<f64> {
        if !self.is_active() {
            return None;
        }
        self.slots
            .iter()
            .filter_map(|slot| unpack(slot.load(Ordering::Relaxed)))
            .max_by_key(|(at, _)| *at)
            .map(|(_, level)| level)
    }
}

/// Ajustes del umbral adaptativo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveSettings {
    /// Multiplica el suelo de ruido del VAD.
    pub noise_margin: f64,
    /// Multiplica el eco esperado (`acople × referencia`).
    pub echo_margin: f64,
    /// Suelo absoluto: en una sala en silencio no hace falta más.
    pub min_threshold: f64,
    /// Acople inicial: alto, para no confundir el primer eco con voz.
    pub initial_coupling: f64,
    pub min_coupling: f64,
    pub max_coupling: f64,
    /// Por debajo de esta referencia no se puede medir el acople.
    pub min_reference: f64,
    /// Los primeros bloques de la frase aún no traen el eco del altavoz.
    pub learn_delay_secs: f64,
    /// Solo se aprende al principio de la frase: allí casi seguro hay solo eco.
    pub learn_secs: f64,
}

impl Default for AdaptiveSettings {
    fn default() -> Self {
        Self {
            noise_margin: 1.8,
            echo_margin: 1.25,
            min_threshold: 0.0012,
            initial_coupling: 0.9,
            min_coupling: 0.05,
            max_coupling: 1.5,
            min_reference: 0.003,
            learn_delay_secs: 0.2,
            learn_secs: 0.6,
        }
    }
}

/// Resumen de una frase del TTS para el diagnóstico de afinado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BargeSummary {
    pub mic_max: f64,
    /// Eco estimado: `acople × referencia`, el máximo de la frase.
    pub echo_max: f64,
    pub coupling: f64,
    pub threshold: f64,
    pub barged: bool,
}

/// Estima el eco y calcula el umbral de cada bloque mientras la app habla.
pub struct AdaptiveBargeIn {
    settings: AdaptiveSettings,
    coupling: f64,
    /// `(mic, referencia)` por bloque, para emparejar el eco con su retardo.
    history: VecDeque<(f64, f64)>,
    /// Cocientes `mic/ref` por retardo durante la ventana de aprendizaje.
    learn: Vec<Vec<f64>>,
    phrase: u64,
    age_secs: f64,
    blocks: usize,
    mic_max: f64,
    echo_max: f64,
    threshold_max: f64,
    barged: bool,
}

impl Default for AdaptiveBargeIn {
    fn default() -> Self {
        Self::with_settings(AdaptiveSettings::default())
    }
}

impl AdaptiveBargeIn {
    pub fn with_settings(settings: AdaptiveSettings) -> Self {
        Self {
            coupling: settings.initial_coupling,
            settings,
            history: VecDeque::with_capacity(HISTORY),
            learn: vec![Vec::new(); MAX_LAG - MIN_LAG + 1],
            phrase: 0,
            age_secs: 0.0,
            blocks: 0,
            mic_max: 0.0,
            echo_max: 0.0,
            threshold_max: 0.0,
            barged: false,
        }
    }

    pub fn coupling(&self) -> f64 {
        self.coupling
    }

    /// Cambia de frase del TTS; devuelve `true` si la frase es nueva.
    pub fn observe_phrase(&mut self, phrase: u64) -> bool {
        if phrase == self.phrase {
            return false;
        }
        self.phrase = phrase;
        self.history.clear();
        for ratios in &mut self.learn {
            ratios.clear();
        }
        self.age_secs = 0.0;
        true
    }

    /// Umbral de este bloque; aprende el acople con los bloques de solo eco.
    ///
    /// `reference` es el pico de la ventana (lo que el micrófono pudo oír) y
    /// `current` el bloque que suena ahora, para emparejar el eco con su
    /// retardo: un bloque flojo tras un pico no es un acople bajo.
    pub fn threshold(
        &mut self,
        level: f64,
        reference: Option<f64>,
        current: Option<f64>,
        noise: f64,
    ) -> f64 {
        if self.age_secs > self.settings.learn_secs {
            self.commit_learning();
        }
        let echo_level = reference.unwrap_or(0.0);
        let echo = echo_level * self.coupling;
        let floor = noise.max(0.0) * self.settings.noise_margin;
        let threshold = (echo * self.settings.echo_margin)
            .max(floor)
            .max(self.settings.min_threshold);
        let now_reference = current.unwrap_or(0.0);
        self.push_history(level, now_reference);
        if now_reference >= self.settings.min_reference
            && self.age_secs >= self.settings.learn_delay_secs
            && self.age_secs <= self.settings.learn_secs
            && level <= threshold
        {
            self.learn_lags(level);
        }
        self.age_secs += CHUNK_SECS;
        self.blocks += 1;
        self.mic_max = self.mic_max.max(level);
        self.echo_max = self.echo_max.max(echo);
        self.threshold_max = self.threshold_max.max(threshold);
        threshold
    }

    pub fn note_barge(&mut self) {
        self.barged = true;
    }

    /// Cierra la frase en curso y devuelve sus cifras (si hubo bloques).
    pub fn summary(&mut self) -> Option<BargeSummary> {
        if self.blocks == 0 {
            return None;
        }
        let summary = BargeSummary {
            mic_max: self.mic_max,
            echo_max: self.echo_max,
            coupling: self.coupling,
            threshold: self.threshold_max,
            barged: self.barged,
        };
        self.blocks = 0;
        self.mic_max = 0.0;
        self.echo_max = 0.0;
        self.threshold_max = 0.0;
        self.barged = false;
        Some(summary)
    }

    fn push_history(&mut self, level: f64, reference: f64) {
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back((level, reference));
    }

    /// Guarda `mic/ref` para cada retardo posible; la voz sube el cociente de
    /// algunos bloques, y la mediana de la frase lo descarta.
    fn learn_lags(&mut self, level: f64) {
        for (slot, lag) in self.learn.iter_mut().zip(MIN_LAG..=MAX_LAG) {
            let Some((_, reference)) = self.history.iter().rev().nth(lag) else {
                continue;
            };
            if *reference < self.settings.min_reference {
                continue;
            }
            let ratio = level / reference;
            if ratio.is_finite() && ratio > 0.0 {
                slot.push(ratio);
            }
        }
    }

    /// Cierra la ventana de aprendizaje: el acople mediano, con el retardo que
    /// mejor encaja. Sin muestras, conserva el acople anterior.
    fn commit_learning(&mut self) {
        let mut medians: Vec<f64> = self
            .learn
            .iter()
            .filter(|ratios| ratios.len() >= 3)
            .map(|ratios| median(ratios))
            .collect();
        for ratios in &mut self.learn {
            ratios.clear();
        }
        if medians.is_empty() {
            return;
        }
        medians.sort_by(f64::total_cmp);
        let coupling = medians[medians.len() / 2];
        self.coupling = coupling.clamp(self.settings.min_coupling, self.settings.max_coupling);
    }
}

/// Mediana con interpolación lineal (como `np.median`).
fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

#[cfg(test)]
mod tests;
