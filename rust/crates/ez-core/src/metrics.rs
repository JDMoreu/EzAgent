//! Port of `src/ezagent/voice/metrics.py` (F3-15).
//!
//! Marcas de latencia del turno de voz: un turno se mide desde que el VAD deja
//! de oír voz hasta que suena el primer audio de la respuesta. Las marcas se
//! toman en el hilo de audio, el de STT y el de TTS; el objeto es de un solo
//! turno y no necesita compartirse entre turnos.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::pyjson::{dumps_with, DumpOptions};
use crate::pytime::timestamp;

/// Lista blanca de metadatos para no guardar transcripciones ni claves.
const META_KEYS: &[&str] = &[
    "provider",
    "model",
    "rounds",
    "outcome",
    "cancelled",
    "first_ack_ms",
];

/// `dict[str, float]` del informe de un turno.
pub type Report = IndexMap<String, f64>;

/// `time.monotonic()`: base única del proceso para que las marcas de los
/// distintos hilos (audio, STT, TTS) sean comparables.
// DECISION: Python usa el reloj monotónico del sistema; aquí una base del
// proceso, que es lo que necesitan las diferencias dentro de un turno.
pub fn monotonic() -> f64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn xor_lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Timestamps monotónicos de un turno.
#[derive(Debug)]
pub struct TurnTimings {
    pub speech_end: f64,
    pub vad_end: f64,
    pub marks: Mutex<IndexMap<String, f64>>,
    /// Tiempo acumulado (s) de etapas que ocurren a trozos durante el turno:
    /// `llm` (espera de tokens), `tts_synth` (síntesis) y `tts_play` (voz).
    pub spans: Mutex<IndexMap<String, f64>>,
}

impl TurnTimings {
    pub fn new(speech_end: f64, vad_end: f64) -> Self {
        Self {
            speech_end,
            vad_end,
            marks: Mutex::new(IndexMap::new()),
            spans: Mutex::new(IndexMap::new()),
        }
    }

    /// `mark()`: sin `at`, usa `time.monotonic()`.
    pub fn mark(&self, name: &str, at: Option<f64>) {
        let at = at.unwrap_or_else(monotonic);
        xor_lock(&self.marks).insert(name.to_string(), at);
    }

    /// Suma segundos de una etapa; los negativos no restan.
    pub fn add_span(&self, name: &str, seconds: f64) {
        let mut spans = xor_lock(&self.spans);
        let total = spans.get(name).copied().unwrap_or(0.0);
        spans.insert(name.to_string(), total + seconds.max(0.0));
    }

    /// Duraciones en ms entre las marcas disponibles.
    pub fn report(&self) -> Report {
        let marks = xor_lock(&self.marks).clone();
        let spans = xor_lock(&self.spans).clone();
        let mut result = Report::new();
        result.insert(
            "vad_wait_ms".to_string(),
            (self.vad_end - self.speech_end) * 1000.0,
        );
        self.add_mark_durations(&marks, &mut result);
        add_span_durations(&spans, &mut result);
        if let Some(turn_end) = marks.get("turn_end") {
            result.insert(
                "turn_total_ms".to_string(),
                (turn_end - self.speech_end) * 1000.0,
            );
        }
        result
    }

    fn add_mark_durations(&self, marks: &IndexMap<String, f64>, result: &mut Report) {
        let mut previous = self.vad_end;
        for name in [
            "stt_ready",
            "first_token",
            "first_sentence",
            "first_sound",
            "turn_end",
        ] {
            let Some(at) = marks.get(name) else {
                continue;
            };
            result.insert(format!("{name}_ms"), (at - previous) * 1000.0);
            previous = *at;
        }
        if let Some(first_sound) = marks.get("first_sound") {
            let ttfa = (first_sound - self.speech_end) * 1000.0;
            result.insert("ttfa_ms".to_string(), ttfa);
            result.insert("total_ms".to_string(), ttfa);
        }
        if let Some(first_ack) = marks.get("first_ack") {
            // Acuse local: se mide aparte del primer contenido útil del modelo.
            result.insert(
                "first_ack_ms".to_string(),
                (first_ack - self.vad_end) * 1000.0,
            );
        }
        // Duración propia de cada etapa, para leerla sin sumar a mano.
        if let (Some(llm_start), Some(first_token)) =
            (marks.get("llm_start"), marks.get("first_token"))
        {
            result.insert(
                "llm_ttft_ms".to_string(),
                (first_token - llm_start) * 1000.0,
            );
        }
    }
}

fn add_span_durations(spans: &IndexMap<String, f64>, result: &mut Report) {
    for (span, key) in [
        ("llm", "llm_ms"),
        ("tts_synth", "tts_synth_ms"),
        ("tts_play", "tts_play_ms"),
    ] {
        if let Some(value) = spans.get(span) {
            result.insert(key.to_string(), value * 1000.0);
        }
    }
}

fn ms(value: f64) -> String {
    format!("{value:.0} ms")
}

// DECISION: `TurnTimings::report()` siempre trae `vad_wait_ms`; un mapa ajeno
// sin esa clave se lee como 0 ms en vez de lanzar, para no tumbar el turno.
fn value(report: &Report, key: &str) -> f64 {
    report.get(key).copied().unwrap_or(0.0)
}

/// Línea corta con lo que ya se sabe al terminar de transcribir.
pub fn format_stt(report: &Report) -> String {
    let mut parts = vec![format!("VAD {}", ms(value(report, "vad_wait_ms")))];
    if let Some(stt) = report.get("stt_ready_ms") {
        parts.push(format!("STT {}", ms(*stt)));
    }
    parts.join(" | ")
}

/// Resumen legible de un turno: una etapa por campo, en milisegundos.
pub fn format_turn(report: &Report) -> String {
    let mut parts = vec![format!("VAD {}", ms(value(report, "vad_wait_ms")))];
    if let Some(stt) = report.get("stt_ready_ms") {
        parts.push(format!("STT {}", ms(*stt)));
    }
    if let Some(llm) = report.get("llm_ms") {
        let detail = report
            .get("llm_ttft_ms")
            .map(|ttft| format!(" (1.er token {})", ms(*ttft)))
            .unwrap_or_default();
        parts.push(format!("LLM {}{detail}", ms(*llm)));
    }
    if let Some(tts) = report.get("tts_synth_ms") {
        parts.push(format!("TTS {} síntesis", ms(*tts)));
    }
    if let Some(voice) = report.get("tts_play_ms") {
        parts.push(format!("Voz {}", ms(*voice)));
    }
    if let Some(ttfa) = report.get("ttfa_ms") {
        parts.push(format!("1.er audio a los {}", ms(*ttfa)));
    }
    if let Some(total) = report.get("turn_total_ms") {
        parts.push(format!("Turno {}", ms(*total)));
    }
    parts.join(" | ")
}

fn clean_meta(meta: Option<&Map<String, Value>>) -> Map<String, Value> {
    let Some(meta) = meta else {
        return Map::new();
    };
    let mut out = Map::new();
    for key in META_KEYS {
        if let Some(value) = meta.get(*key) {
            if !value.is_null() {
                out.insert((*key).to_string(), value.clone());
            }
        }
    }
    out
}

/// `round(value, 1)` de Python.
fn round1(value: f64) -> f64 {
    (value * 10.0).round_ties_even() / 10.0
}

/// Acumula los turnos y publica media y P95 cada diez turnos.
pub struct LatencyStats {
    // Each turn keeps its timings and its metadata for the summary.
    turns: VecDeque<(Report, Map<String, Value>)>,
    total: usize,
    jsonl: Option<PathBuf>,
    maxlen: usize,
}

impl LatencyStats {
    pub fn new(maxlen: usize, jsonl_path: Option<PathBuf>) -> Self {
        Self {
            turns: VecDeque::new(),
            total: 0,
            jsonl: jsonl_path,
            maxlen,
        }
    }

    /// Escribe una línea JSON por turno; un fallo de disco nunca rompe la voz.
    fn append_jsonl(&self, record: &Map<String, Value>) {
        let Some(path) = &self.jsonl else {
            return;
        };
        // `open("a", encoding="utf-8")` traduce los finales de línea en Windows.
        let line = dumps_with(&Value::Object(record.clone()), &DumpOptions::utf8());
        let text = if cfg!(windows) {
            format!("{line}\r\n")
        } else {
            format!("{line}\n")
        };
        let write = || -> std::io::Result<()> {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)?;
            }
            let mut handle = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            handle.write_all(text.as_bytes())
        };
        let _ = write();
    }

    /// Registra un comando local (sin LLM): tiempo hasta ejecutarlo.
    pub fn record_command(&self, name: &str, handled_ms: f64) {
        let mut record = Map::new();
        record.insert("ts".to_string(), Value::from(timestamp()));
        record.insert("kind".to_string(), Value::from("command"));
        record.insert("name".to_string(), Value::from(name));
        record.insert("handled_ms".to_string(), Value::from(round1(handled_ms)));
        self.append_jsonl(&record);
    }

    /// Registra un turno con sus tiempos y los metadatos permitidos.
    pub fn record(&mut self, timings: &TurnTimings, meta: Option<&Map<String, Value>>) -> Report {
        let report = timings.report();
        let extra = clean_meta(meta);
        self.turns.push_back((report.clone(), extra.clone()));
        while self.turns.len() > self.maxlen {
            self.turns.pop_front();
        }
        self.total += 1;
        let mut record = Map::new();
        record.insert("ts".to_string(), Value::from(timestamp()));
        record.insert("kind".to_string(), Value::from("turn"));
        // DECISION: un turno sin meta declara su resultado como desconocido.
        record.insert("outcome".to_string(), Value::from("unknown"));
        for (key, value) in &report {
            record.insert(key.clone(), Value::from(round1(*value)));
        }
        for (key, value) in &extra {
            record.insert(key.clone(), value.clone());
        }
        self.append_jsonl(&record);
        tracing::info!("Tiempos: {}", format_turn(&report));
        if self.total >= 10 && self.total.is_multiple_of(10) {
            self.log_summary();
        }
        report
    }

    fn log_summary(&self) {
        let labels = [
            ("vad_wait_ms", "VAD"),
            ("stt_ready_ms", "STT"),
            ("llm_ms", "LLM"),
            ("llm_ttft_ms", "LLM 1.er token"),
            ("tts_synth_ms", "TTS"),
            ("ttfa_ms", "1.er audio"),
        ];
        let mut parts = Vec::new();
        for (key, label) in labels {
            let values: Vec<f64> = self
                .turns
                .iter()
                .filter_map(|(report, _)| report.get(key).copied())
                .collect();
            if values.is_empty() {
                continue;
            }
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            parts.push(format!(
                "{label} media {mean:.0} ms P95 {:.0} ms",
                Self::percentile(&values, 95.0)
            ));
        }
        if !parts.is_empty() {
            tracing::info!("Latencia acumulada: {}", parts.join(" | "));
        }
    }

    /// Percentil con interpolación lineal, sin dependencias externas.
    pub fn percentile(values: &[f64], pct: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let mut ordered = values.to_vec();
        ordered.sort_by(f64::total_cmp);
        if ordered.len() == 1 {
            return ordered[0];
        }
        let pos = (ordered.len() - 1) as f64 * (pct / 100.0);
        let lo = pos as usize;
        let hi = (lo + 1).min(ordered.len() - 1);
        let frac = pos - lo as f64;
        ordered[lo] + (ordered[hi] - ordered[lo]) * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ez-core-metrics-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directorio temporal");
        dir.join("latency.jsonl")
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    fn read_jsonl(path: &PathBuf) -> Vec<Value> {
        let text = std::fs::read_to_string(path).expect("jsonl legible");
        text.lines()
            .map(|line| serde_json::from_str(line).expect("línea JSON"))
            .collect()
    }

    #[test]
    fn report_mide_las_etapas_en_orden() {
        let timings = TurnTimings::new(0.0, 0.1);
        timings.mark("stt_ready", Some(0.2));
        timings.mark("first_token", Some(0.4));
        timings.mark("first_sentence", Some(0.5));
        timings.mark("first_sound", Some(0.9));
        timings.mark("turn_end", Some(2.0));
        timings.mark("first_ack", Some(0.15));
        timings.mark("llm_start", Some(0.25));
        timings.add_span("llm", 0.3);
        timings.add_span("tts_synth", 0.2);
        timings.add_span("tts_play", 0.1);
        let report = timings.report();
        assert!((report["vad_wait_ms"] - 100.0).abs() < 1e-9);
        assert!((report["stt_ready_ms"] - 100.0).abs() < 1e-9);
        assert!((report["first_token_ms"] - 200.0).abs() < 1e-9);
        assert!((report["first_sentence_ms"] - 100.0).abs() < 1e-9);
        assert!((report["first_sound_ms"] - 400.0).abs() < 1e-9);
        assert!((report["ttfa_ms"] - 900.0).abs() < 1e-9);
        assert!((report["total_ms"] - 900.0).abs() < 1e-9);
        assert!((report["turn_end_ms"] - 1100.0).abs() < 1e-9);
        assert!((report["turn_total_ms"] - 2000.0).abs() < 1e-9);
        assert!((report["first_ack_ms"] - 50.0).abs() < 1e-9);
        assert!((report["llm_ttft_ms"] - 150.0).abs() < 1e-9);
        assert!((report["llm_ms"] - 300.0).abs() < 1e-9);
        assert!((report["tts_synth_ms"] - 200.0).abs() < 1e-9);
        assert!((report["tts_play_ms"] - 100.0).abs() < 1e-9);
        assert_eq!(report.len(), 14);
    }

    #[test]
    fn spans_negativos_no_restan() {
        let timings = TurnTimings::new(0.0, 0.0);
        timings.add_span("llm", -3.0);
        timings.add_span("llm", 0.5);
        let report = timings.report();
        assert_eq!(report["llm_ms"], 500.0);
    }

    #[test]
    fn formatos_coinciden_con_el_python() {
        let mut report = Report::new();
        report.insert("vad_wait_ms".into(), 100.4);
        report.insert("stt_ready_ms".into(), 12.5);
        report.insert("llm_ms".into(), 250.0);
        report.insert("llm_ttft_ms".into(), 42.6);
        report.insert("tts_synth_ms".into(), 80.0);
        report.insert("tts_play_ms".into(), 120.2);
        report.insert("ttfa_ms".into(), 300.5);
        report.insert("turn_total_ms".into(), 800.0);
        assert_eq!(format_stt(&report), "VAD 100 ms | STT 12 ms");
        assert_eq!(
            format_turn(&report),
            "VAD 100 ms | STT 12 ms | LLM 250 ms (1.er token 43 ms) | TTS 80 ms síntesis | Voz 120 ms | 1.er audio a los 300 ms | Turno 800 ms"
        );
    }

    #[test]
    fn format_stt_sin_stt_solo_muestra_vad() {
        let mut report = Report::new();
        report.insert("vad_wait_ms".into(), 0.0);
        assert_eq!(format_stt(&report), "VAD 0 ms");
    }

    #[test]
    fn percentile_interpola_y_acepta_listas_vacias() {
        assert_eq!(LatencyStats::percentile(&[], 95.0), 0.0);
        assert_eq!(LatencyStats::percentile(&[7.0], 95.0), 7.0);
        assert_eq!(LatencyStats::percentile(&[0.0, 10.0], 50.0), 5.0);
        assert!((LatencyStats::percentile(&[0.0, 10.0], 95.0) - 9.5).abs() < 1e-9);
    }

    #[test]
    fn record_guarda_solo_metadatos_permitidos() {
        let path = temp_path("record-meta");
        let timings = TurnTimings::new(0.0, 0.1);
        timings.mark("first_token", Some(0.4));
        timings.mark("first_sound", Some(0.9));
        let mut stats = LatencyStats::new(5, Some(path.clone()));
        let mut meta = Map::new();
        meta.insert("provider".into(), Value::from("openai"));
        meta.insert("model".into(), Value::from("modelo-prueba"));
        meta.insert("rounds".into(), Value::from(2));
        meta.insert("outcome".into(), Value::from("ok"));
        meta.insert("cancelled".into(), Value::from(false));
        meta.insert("first_ack_ms".into(), Value::from(123.4));
        meta.insert("transcript".into(), Value::from("no debe guardarse"));
        meta.insert("api_key".into(), Value::from("no debe guardarse"));
        let report = stats.record(&timings, Some(&meta));
        assert!((report["vad_wait_ms"] - 100.0).abs() < 1e-9);
        let records = read_jsonl(&path);
        let record = records.last().expect("un registro");
        assert_eq!(record["provider"], "openai");
        assert_eq!(record["model"], "modelo-prueba");
        assert_eq!(record["rounds"], 2);
        assert_eq!(record["outcome"], "ok");
        assert_eq!(record["cancelled"], false);
        assert_eq!(record["first_ack_ms"], 123.4);
        assert!(record.get("transcript").is_none());
        assert!(record.get("api_key").is_none());
        assert!(record.get("ttfa_ms").is_some());
        cleanup(&path);
    }

    #[test]
    fn record_sin_meta_declara_outcome_desconocido() {
        let path = temp_path("record-unknown");
        let mut stats = LatencyStats::new(50, Some(path.clone()));
        stats.record(&TurnTimings::new(0.0, 0.1), None);
        let records = read_jsonl(&path);
        let record = records.last().expect("un registro");
        assert_eq!(record["outcome"], "unknown");
        assert_eq!(record["kind"], "turn");
        cleanup(&path);
    }

    #[test]
    fn record_command_escribe_la_linea_redondeada() {
        let path = temp_path("command");
        let stats = LatencyStats::new(50, Some(path.clone()));
        stats.record_command("saludo", 12.34);
        let records = read_jsonl(&path);
        let record = records.last().expect("un registro");
        assert_eq!(record["kind"], "command");
        assert_eq!(record["name"], "saludo");
        assert_eq!(record["handled_ms"], 12.3);
        cleanup(&path);
    }

    #[test]
    fn el_deque_respeta_maxlen() {
        let mut stats = LatencyStats::new(2, None);
        for _ in 0..4 {
            stats.record(&TurnTimings::new(0.0, 0.1), None);
        }
        assert_eq!(stats.turns.len(), 2);
        assert_eq!(stats.total, 4);
    }

    #[test]
    fn sin_ruta_jsonl_no_falla() {
        let mut stats = LatencyStats::new(5, None);
        let report = stats.record(&TurnTimings::new(0.0, 0.1), None);
        assert!((report["vad_wait_ms"] - 100.0).abs() < 1e-9);
    }

    #[test]
    fn mark_sin_at_usa_el_reloj_monotonico() {
        let timings = TurnTimings::new(0.0, 0.0);
        let before = monotonic();
        timings.mark("first_token", None);
        let after = monotonic();
        let at = timings.marks.lock().expect("marcas")["first_token"];
        assert!(at >= before && at <= after);
    }
}
