//! Port of `src/ezagent/voice/turn.py` (F4-02).
//!
//! Detección semántica del fin de turno con Smart Turn v3 (Pipecat, BSD-2): el
//! VAD por energía solo sabe si hay silencio; Smart Turn mira los últimos 8 s de
//! audio y estima la probabilidad de que la frase haya acabado.
//!
//! La inferencia ONNX corre en [`OrtTurnModel`], sobre el hilo `ezagent-turn`;
//! [`UnavailableTurnModel`] queda para pruebas y para cuando no hay modelo.

mod features;
mod session;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

use crate::segmenter::Probe;

pub use features::{
    hann_window, hz_to_mel, log_mel, mel_filters, mel_to_hz, prepare, TurnFeatures, HOP, N_FFT,
    N_FRAMES, N_MELS, N_SAMPLES, SAMPLE_RATE, WINDOW_SECS,
};
pub use session::{onnxruntime_path, shared_environment, OrtTurnModel};

pub const ASSET_ID: &str = "smart-turn-v3.2-cpu";

const NOT_LOADED: &str = "Smart Turn no está cargado";
const WORKER_NAME: &str = "ezagent-turn";

/// Ejecuta la inferencia del modelo sobre la ventana ya preparada.
pub trait TurnModel: Send + Sync {
    /// Probabilidad de que el turno haya terminado (0 a 1).
    fn probability(&self, features: &TurnFeatures) -> Result<f64, String>;
}

/// Implementación para cuando no hay modelo: siempre «no disponible».
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableTurnModel;

impl TurnModel for UnavailableTurnModel {
    fn probability(&self, _features: &TurnFeatures) -> Result<f64, String> {
        Err(NOT_LOADED.to_string())
    }
}

/// Ruta del modelo si ya está instalado, sin descargarlo (como `_model_path`
/// del generador de fixtures).
pub fn installed_model_path() -> Result<Option<PathBuf>, String> {
    use ez_core::downloads::{asset_path, is_installed, load_manifest};

    let assets = load_manifest().map_err(|error| error.to_string())?;
    let asset = assets
        .get(ASSET_ID)
        .ok_or_else(|| format!("KeyError: '{ASSET_ID}'"))?;
    Ok(is_installed(asset, None).then(|| asset_path(asset, None)))
}

/// Ruta del modelo; lo descarga (con hash verificado) si falta.
pub fn resolve_model_path() -> Result<PathBuf, String> {
    use ez_core::downloads::{download, load_manifest, DownloadOptions};

    if let Some(path) = installed_model_path()? {
        return Ok(path);
    }
    let assets = load_manifest().map_err(|error| error.to_string())?;
    let asset = assets
        .get(ASSET_ID)
        .ok_or_else(|| format!("KeyError: '{ASSET_ID}'"))?;
    download(asset, &DownloadOptions::default(), None).map_err(|error| error.to_string())
}

/// De dónde sale el modelo del hilo `ezagent-turn`.
enum Source {
    Model(Arc<dyn TurnModel>),
    Path(String),
}

struct Job {
    pcm: Vec<u8>,
    rate: u32,
    probe: Probe,
}

/// Evalúa el fin de turno en un hilo propio, sin bloquear el de audio.
pub struct SmartTurn {
    threshold: Arc<Mutex<f64>>,
    ready: Arc<AtomicBool>,
    model: Arc<Mutex<Option<Arc<dyn TurnModel>>>>,
    sender: Option<Sender<Job>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SmartTurn {
    /// Carga el modelo en segundo plano; si falla, manda el silencio.
    pub fn new(threshold: f64, model_path: &str) -> Self {
        Self::spawn(threshold, Source::Path(model_path.to_string()), true)
    }

    /// Inyecta un modelo ya cargado (pruebas y cableado futuro de `ort`).
    pub fn with_model(threshold: f64, model: Arc<dyn TurnModel>) -> Self {
        // DECISION: sin precalentado; es la costura de inyección, no la carga real.
        let turn = Self::spawn(threshold, Source::Model(Arc::clone(&model)), false);
        *turn.model.lock() = Some(model);
        turn.ready.store(true, Ordering::SeqCst);
        turn
    }

    fn spawn(threshold: f64, source: Source, warmup: bool) -> Self {
        let (sender, receiver) = unbounded::<Job>();
        let ready = Arc::new(AtomicBool::new(false));
        let threshold = Arc::new(Mutex::new(threshold));
        let model: Arc<Mutex<Option<Arc<dyn TurnModel>>>> = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_ready = Arc::clone(&ready);
        let worker_threshold = Arc::clone(&threshold);
        let worker_model = Arc::clone(&model);
        let worker_cancel = Arc::clone(&cancel);
        let worker = std::thread::Builder::new()
            .name(WORKER_NAME.to_string())
            .spawn(move || {
                run_worker(
                    source,
                    warmup,
                    receiver,
                    worker_ready,
                    worker_threshold,
                    worker_model,
                    worker_cancel,
                );
            })
            .expect("no se pudo crear el hilo ezagent-turn");
        Self {
            threshold,
            ready,
            model,
            sender: Some(sender),
            cancel,
            worker: Some(worker),
        }
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Umbral de decisión; el menú del HUD lo cambia en vivo.
    pub fn threshold(&self) -> f64 {
        *self.threshold.lock()
    }

    pub fn set_threshold(&self, value: f64) {
        *self.threshold.lock() = value;
    }

    /// Probabilidad de que el turno haya terminado (0 a 1).
    pub fn probability(&self, pcm: &[u8], rate: u32) -> Result<f64, String> {
        let model = self
            .model
            .lock()
            .clone()
            .ok_or_else(|| NOT_LOADED.to_string())?;
        let features = prepare(pcm, rate as i32)?;
        model.probability(&features)
    }

    /// Encola una evaluación; `None` si el modelo aún no está listo.
    pub fn submit(&self, pcm: &[u8], rate: u32) -> Option<Probe> {
        if !self.ready() {
            return None;
        }
        let sender = self.sender.as_ref()?;
        let probe = Probe::new();
        let job = Job {
            pcm: pcm.to_vec(),
            rate,
            probe: probe.clone(),
        };
        sender.send(job).ok()?;
        Some(probe)
    }

    pub fn close(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.sender = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for SmartTurn {
    fn drop(&mut self) {
        self.close();
    }
}

fn run_worker(
    source: Source,
    warmup: bool,
    receiver: Receiver<Job>,
    ready: Arc<AtomicBool>,
    threshold: Arc<Mutex<f64>>,
    slot: Arc<Mutex<Option<Arc<dyn TurnModel>>>>,
    cancel: Arc<AtomicBool>,
) {
    let model = match source {
        Source::Model(model) => Some(model),
        Source::Path(path) => load_model(&path),
    };
    let Some(model) = model else {
        return;
    };
    // El Python guarda la sesión antes de precalentar; si el precalentado
    // falla, `probability` sigue funcionando aunque `ready` nunca se encienda.
    *slot.lock() = Some(Arc::clone(&model));
    if warmup {
        let silence = vec![0_u8; SAMPLE_RATE as usize * 2];
        let Ok(features) = prepare(&silence, SAMPLE_RATE as i32) else {
            return;
        };
        if let Err(error) = model.probability(&features) {
            tracing::warn!("Smart Turn no disponible; uso solo el silencio: {error}");
            return;
        }
    }
    ready.store(true, Ordering::SeqCst);
    while let Ok(job) = receiver.recv() {
        if cancel.load(Ordering::SeqCst) {
            job.probe.fail("cancelado");
            continue;
        }
        match prepare(&job.pcm, job.rate as i32).and_then(|features| model.probability(&features)) {
            Ok(value) => job.probe.finish(value >= *threshold.lock()),
            Err(error) => job.probe.fail(error),
        }
    }
}

fn load_model(model_path: &str) -> Option<Arc<dyn TurnModel>> {
    match open_model(model_path) {
        Ok(model) => Some(Arc::new(model)),
        Err(error) => {
            tracing::warn!("Smart Turn no disponible; uso solo el silencio: {error}");
            None
        }
    }
}

fn open_model(model_path: &str) -> Result<OrtTurnModel, String> {
    let path = if model_path.is_empty() {
        resolve_model_path()?
    } else {
        PathBuf::from(model_path)
    };
    OrtTurnModel::open(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Fixed(f64);

    impl TurnModel for Fixed {
        fn probability(&self, _features: &TurnFeatures) -> Result<f64, String> {
            Ok(self.0)
        }
    }

    fn wait(probe: &Probe) -> Option<bool> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if probe.done() {
                return probe.result();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    fn wait_done(probe: &Probe) -> bool {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if probe.done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn unavailable_model_reports_not_loaded() {
        let error = UnavailableTurnModel
            .probability(&TurnFeatures { data: Vec::new() })
            .expect_err("no hay modelo");
        assert_eq!(error, "Smart Turn no está cargado");
    }

    #[test]
    fn missing_model_is_never_ready() {
        let missing = std::env::temp_dir().join("ezagent-turn-sin-modelo.onnx");
        let turn = SmartTurn::new(0.5, &missing.to_string_lossy());
        assert!(!turn.ready());
        assert!(turn.submit(&[0, 0], SAMPLE_RATE).is_none());
        assert_eq!(
            turn.probability(&[0, 0], SAMPLE_RATE)
                .expect_err("sin modelo"),
            "Smart Turn no está cargado"
        );
    }

    #[test]
    fn submitted_probe_uses_the_threshold() {
        let turn = SmartTurn::with_model(0.5, Arc::new(Fixed(0.9)));
        assert!(turn.ready());
        let probe = turn
            .submit(&[0, 0], SAMPLE_RATE)
            .expect("el modelo está listo");
        assert_eq!(wait(&probe), Some(true));
    }

    #[test]
    fn low_probability_does_not_close_the_turn() {
        let turn = SmartTurn::with_model(0.5, Arc::new(Fixed(0.2)));
        let probe = turn
            .submit(&[0, 0], SAMPLE_RATE)
            .expect("el modelo está listo");
        assert_eq!(wait(&probe), Some(false));
    }

    #[test]
    fn threshold_is_live() {
        let turn = SmartTurn::with_model(0.5, Arc::new(Fixed(0.9)));
        assert!((turn.threshold() - 0.5).abs() < 1e-12);
        turn.set_threshold(0.95);
        assert!((turn.threshold() - 0.95).abs() < 1e-12);
    }

    #[test]
    fn failing_model_fails_the_probe() {
        struct Broken;
        impl TurnModel for Broken {
            fn probability(&self, _features: &TurnFeatures) -> Result<f64, String> {
                Err("sin sesión".to_string())
            }
        }
        let turn = SmartTurn::with_model(0.5, Arc::new(Broken));
        let probe = turn
            .submit(&[0, 0], SAMPLE_RATE)
            .expect("el modelo está listo");
        assert!(wait_done(&probe), "la sonda debe completarse");
        assert_eq!(probe.result(), None);
    }
}
