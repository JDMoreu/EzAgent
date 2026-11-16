//! Sesión de ONNX Runtime para Smart Turn (P-TURN).
//!
//! Carga perezosa de `smart-turn-v3.2-cpu.onnx` con las mismas opciones que
//! `turn.py`: modo secuencial, 1 hilo entre operaciones y 2 dentro, CPU. La
//! DLL se toma de `ORT_DYLIB_PATH`, junto al ejecutable o de la carpeta de
//! datos, igual que `ez-tts` (`kokoro/session.rs`).
//!
//! DECISION: `ort` solo admite un entorno por proceso; este crate lo crea y
//! `ez-tts` lo reutiliza con [`shared_environment`] (`ez-tts` depende de
//! `ez-audio`, no al revés).

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ort::environment::Environment;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use parking_lot::Mutex as SessionMutex;

use super::{TurnFeatures, TurnModel, N_FRAMES, N_MELS};

/// Nombre de la entrada del modelo, como en `turn.py`.
const INPUT_NAME: &str = "input_features";

/// Sesión real de Smart Turn; [`TurnModel::probability`] toma el lock.
pub struct OrtTurnModel {
    session: SessionMutex<Session>,
}

impl OrtTurnModel {
    /// Crea la sesión con el modelo y las opciones de `SmartTurn._load`.
    pub fn open(model_path: &Path) -> Result<Self, String> {
        let library = onnxruntime_path().ok_or_else(|| {
            "onnxruntime.dll no está disponible; ejecuta `ezagent-voice downloads onnxruntime`, \
             colócalo junto al ejecutable o define ORT_DYLIB_PATH"
                .to_string()
        })?;
        let environment = shared_environment(&library)?;
        let session = Session::builder(environment)
            .map_err(ort_error)?
            .with_parallel_execution(false)
            .map_err(ort_error)?
            .with_inter_threads(1)
            .map_err(ort_error)?
            .with_intra_threads(2)
            .map_err(ort_error)?
            .commit_from_file(model_path)
            .map_err(ort_error)?;
        Ok(Self {
            session: SessionMutex::new(session),
        })
    }
}

impl TurnModel for OrtTurnModel {
    /// `SmartTurn.probability`: `input_features` (1 x 80 x 800) -> `logits[0]`.
    fn probability(&self, features: &TurnFeatures) -> Result<f64, String> {
        let shape = [1_usize, N_MELS, N_FRAMES];
        let input = Tensor::from_array((shape, features.data.clone())).map_err(ort_error)?;
        let inputs: Vec<(Cow<'_, str>, SessionInputValue<'static>)> =
            vec![(Cow::Borrowed(INPUT_NAME), input.into())];
        let mut session = self.session.lock();
        let outputs = session.run(inputs).map_err(ort_error)?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(ort_error)?;
        let first = data
            .first()
            .copied()
            .ok_or_else(|| "la salida del modelo está vacía".to_string())?;
        Ok(f64::from(first))
    }
}

fn ort_error<T>(error: ort::Error<T>) -> String {
    error.to_string()
}

/// `onnxruntime.dll`: `ORT_DYLIB_PATH`, junto al ejecutable o en la carpeta de
/// descargas (`ezagent-voice downloads onnxruntime`).
pub fn onnxruntime_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("ORT_DYLIB_PATH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(explicit);
    }
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let mut candidates = Vec::new();
    if let Some(directory) = executable {
        candidates.push(directory.join("onnxruntime.dll"));
    }
    let data = ez_core::downloads::data_dir();
    candidates.push(data.join("onnxruntime.dll"));
    candidates.push(data.join("onnxruntime").join("onnxruntime.dll"));
    candidates.into_iter().find(|path| path.is_file())
}

/// Entorno de ONNX Runtime **único del proceso**: `ort` solo admite uno, así que
/// Smart Turn y el TTS (`ez-tts`) lo comparten desde aquí.
pub fn shared_environment(library: &Path) -> Result<&'static Environment, String> {
    static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();
    static INIT: Mutex<()> = Mutex::new(());
    if let Some(environment) = ENVIRONMENT.get() {
        return Ok(environment);
    }
    let _guard = INIT
        .lock()
        .map_err(|_| "el entorno de ONNX Runtime quedó envenenado".to_string())?;
    if let Some(environment) = ENVIRONMENT.get() {
        return Ok(environment);
    }
    let environment = ort::init_from(library)
        .map_err(|error| error.to_string())?
        .with_name("ezagent")
        .build()
        .map_err(|error| error.to_string())?;
    Ok(ENVIRONMENT.get_or_init(|| environment))
}
