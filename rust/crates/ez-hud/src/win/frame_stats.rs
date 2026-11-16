//! Port of `src/ezagent/voice/hud/frame_stats.py` (F5-01).
//!
//! Tiempos de dibujo por ventana: la base para pulir la fluidez con datos. Cada
//! ventana *layered* anota lo que tardó `render()` más la copia a pantalla y,
//! cada `REPORT_SECS`, se escribe una línea con cuadros, p50, p95, máximo y
//! cuántos pasaron del presupuesto de 60 fps. Sin cuadros no hay registro.

use std::sync::Arc;

/// Cada cuánto se resume en el registro.
pub const REPORT_SECS: f64 = 60.0;
/// Presupuesto de un cuadro a 60 fps.
pub const BUDGET_MS: f64 = 1000.0 / 60.0;
/// Muestras como máximo entre informes.
const LIMIT: usize = 4096;

/// Reloj inyectable (`time.monotonic` por defecto).
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// `FrameStats`: medidor de tiempos de cuadro.
pub struct FrameStats {
    pub name: String,
    clock: Clock,
    samples: Vec<f64>,
    since: f64,
}

impl FrameStats {
    /// Medidor con el reloj monótono del proceso.
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_clock(name, Arc::new(super::monotonic_seconds))
    }

    /// Medidor con un reloj propio (pruebas).
    pub fn with_clock(name: impl Into<String>, clock: Clock) -> Self {
        let since = clock();
        Self {
            name: name.into(),
            clock,
            samples: Vec::new(),
            since,
        }
    }

    /// Anota un cuadro; devuelve el resumen si tocaba informar (ya registrado).
    pub fn add(&mut self, ms: f64) -> Option<String> {
        if self.samples.len() < LIMIT {
            self.samples.push(ms);
        }
        let now = (self.clock)();
        if now - self.since < REPORT_SECS {
            return None;
        }
        let line = self.summary();
        self.samples.clear();
        self.since = now;
        if !line.is_empty() {
            tracing::info!("{line}");
        }
        Some(line)
    }

    /// Resumen de una línea, o cadena vacía si no hay muestras.
    pub fn summary(&self) -> String {
        if self.samples.is_empty() {
            return String::new();
        }
        let mut data = self.samples.clone();
        data.sort_by(f64::total_cmp);
        let n = data.len();
        let p50 = data[n / 2];
        let p95 = data[(n - 1).min((n as f64 * 0.95) as usize)];
        let slow = data.iter().filter(|value| **value > BUDGET_MS).count();
        format!(
            "Cuadros {}: {n} · p50 {p50:.1} ms · p95 {p95:.1} ms · máx {:.1} ms · {slow} sobre {BUDGET_MS:.1} ms",
            self.name,
            data[n - 1]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    fn clock_at(start: f64) -> (Clock, Arc<Mutex<f64>>) {
        let now = Arc::new(Mutex::new(start));
        let handle = Arc::clone(&now);
        let clock: Clock = Arc::new(move || *handle.lock());
        (clock, now)
    }

    #[test]
    fn summary_matches_python_percentiles_and_text() {
        let (clock, _) = clock_at(0.0);
        let mut stats = FrameStats::with_clock("bar", clock);
        for sample in [1.0, 5.0, 3.0, 2.0, 4.0] {
            stats.add(sample);
        }
        assert_eq!(
            stats.summary(),
            "Cuadros bar: 5 · p50 3.0 ms · p95 5.0 ms · máx 5.0 ms · 0 sobre 16.7 ms"
        );
    }

    #[test]
    fn empty_summary_is_blank_and_slow_frames_are_counted() {
        let (clock, _) = clock_at(0.0);
        let mut stats = FrameStats::with_clock("card", clock);
        assert_eq!(stats.summary(), "");
        for sample in [10.0, 20.0, 30.0, 1.0] {
            stats.add(sample);
        }
        assert_eq!(
            stats.summary(),
            "Cuadros card: 4 · p50 20.0 ms · p95 30.0 ms · máx 30.0 ms · 2 sobre 16.7 ms"
        );
    }

    #[test]
    fn add_reports_once_per_period_and_clears_samples() {
        let (clock, now) = clock_at(100.0);
        let mut stats = FrameStats::with_clock("bar", clock);
        assert_eq!(stats.add(2.0), None);
        *now.lock() = 100.0 + REPORT_SECS;
        let first = stats.add(4.0);
        assert_eq!(
            first.as_deref(),
            Some("Cuadros bar: 2 · p50 4.0 ms · p95 4.0 ms · máx 4.0 ms · 0 sobre 16.7 ms")
        );
        assert!(stats.samples.is_empty());
        assert_eq!(stats.add(1.0), None);
    }

    #[test]
    fn samples_stop_at_the_limit() {
        let (clock, _) = clock_at(0.0);
        let mut stats = FrameStats::with_clock("bar", clock);
        for _ in 0..(LIMIT + 10) {
            stats.add(1.0);
        }
        assert_eq!(stats.samples.len(), LIMIT);
    }

    #[test]
    fn the_report_counts_four_frames_and_one_slow_one() {
        let (clock, now) = clock_at(0.0);
        let mut stats = FrameStats::with_clock("drop", clock);
        for ms in [4.0, 5.0, 30.0] {
            assert_eq!(stats.add(ms), None);
        }
        *now.lock() = REPORT_SECS + 1.0;
        let line = stats.add(6.0).expect("informe del periodo");
        assert!(line.contains("Cuadros drop: 4"), "{line}");
        assert!(line.contains("1 sobre"), "{line}");
        assert!(line.contains(&format!("{BUDGET_MS:.1}")), "{line}");
        assert_eq!(stats.summary(), "");
    }
}
