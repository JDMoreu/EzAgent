//! Port of `kokoro_onnx.session` and the ONNX inference (F4-04).
//!
//! `ort` is only touched when `onnxruntime.dll` exists: a missing runtime
//! returns an error instead of panicking, and the tests never load a model.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ort::environment::Environment;
use ort::session::Session;
use ort::session::SessionInputValue;
use ort::value::{Tensor, TensorElementType, ValueType};

/// Where `scripts/export.py` stores the vocabulary inside the graph.
pub const CONFIG_KEY: &str = "kokoro_config";

/// `_NUMPY_TYPES`: model input type to the dtype its tensor must use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    Float32,
    Float16,
    Float64,
    Int32,
    Int64,
}

impl DType {
    fn from_element(ty: TensorElementType) -> Self {
        match ty {
            TensorElementType::Float16 => Self::Float16,
            TensorElementType::Float64 => Self::Float64,
            TensorElementType::Int32 => Self::Int32,
            TensorElementType::Int64 => Self::Int64,
            _ => Self::Float32,
        }
    }

    fn is_integer(self) -> bool {
        matches!(self, Self::Int32 | Self::Int64)
    }
}

/// The parts of the ONNX session `Kokoro` needs, so tests can fake them.
pub trait Model: Send {
    fn input_dtypes(&self) -> &HashMap<String, DType>;
    fn tokens_input(&self) -> &str;
    fn output_names(&self) -> &[String];
    fn metadata(&self, key: &str) -> Option<&str>;

    /// `_infer`: audio for one window of tokens and its frame durations.
    fn infer(
        &mut self,
        tokens: &[i64],
        style: &[f32],
        speed: f64,
    ) -> Result<(Vec<f32>, Option<Vec<i64>>), String>;
}

/// `resolve_providers`: `ONNX_PROVIDER` wins; si no, solo CPU.
///
/// `ort` no tiene búsqueda de proveedores por nombre: se acepta el valor del
/// entorno como el Python, y la sesión sigue siendo de CPU.
pub fn resolve_providers() -> Vec<String> {
    if let Some(provider) = std::env::var("ONNX_PROVIDER")
        .ok()
        .filter(|value| !value.is_empty())
    {
        tracing::warn!(
            "ONNX_PROVIDER={provider} no soportado por ez-tts; la sesión usa CPUExecutionProvider"
        );
        return vec![provider];
    }
    vec!["CPUExecutionProvider".to_string()]
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

/// `ort` solo admite un entorno por proceso: se comparte con Smart Turn.
fn environment(library: &Path) -> Result<&'static Environment, String> {
    ez_audio::turn::shared_environment(library)
}

/// A real ONNX Runtime session.
pub struct OrtModel {
    session: Session,
    dtypes: HashMap<String, DType>,
    tokens_input: String,
    outputs: Vec<String>,
    metadata: HashMap<String, String>,
    has_timings: bool,
}

impl OrtModel {
    /// `create_session(model_path)`, only with the runtime available.
    pub fn open(model_path: &Path) -> Result<Self, String> {
        let library = onnxruntime_path().ok_or_else(|| {
            "onnxruntime.dll no está disponible; ejecuta `ezagent-voice downloads onnxruntime`, \
             colócalo junto al ejecutable o define ORT_DYLIB_PATH"
                .to_string()
        })?;
        let environment = environment(&library)?;
        tracing::debug!("Providers: {:?}", resolve_providers());
        let mut builder = Session::builder(environment).map_err(|error| error.to_string())?;
        let session = builder
            .commit_from_file(model_path)
            .map_err(|error| error.to_string())?;
        let mut dtypes = HashMap::new();
        for input in session.inputs() {
            let dtype = match input.dtype() {
                ValueType::Tensor { ty, .. } => DType::from_element(*ty),
                _ => DType::Float32,
            };
            dtypes.insert(input.name().to_string(), dtype);
        }
        let outputs: Vec<String> = session
            .outputs()
            .iter()
            .map(|output| output.name().to_string())
            .collect();
        let has_timings = outputs.iter().any(|name| name == "duration");
        let tokens_input = if dtypes.contains_key("input_ids") {
            "input_ids"
        } else {
            "tokens"
        }
        .to_string();
        let mut metadata = HashMap::new();
        if let Ok(values) = session.metadata() {
            for key in values.custom_keys().unwrap_or_default() {
                if let Some(value) = values.custom(&key) {
                    metadata.insert(key, value);
                }
            }
        }
        Ok(Self {
            session,
            dtypes,
            tokens_input,
            outputs,
            metadata,
            has_timings,
        })
    }
}

impl Model for OrtModel {
    fn input_dtypes(&self) -> &HashMap<String, DType> {
        &self.dtypes
    }

    fn tokens_input(&self) -> &str {
        &self.tokens_input
    }

    fn output_names(&self) -> &[String] {
        &self.outputs
    }

    fn metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    fn infer(
        &mut self,
        tokens: &[i64],
        style: &[f32],
        speed: f64,
    ) -> Result<(Vec<f32>, Option<Vec<i64>>), String> {
        let mut window = Vec::with_capacity(tokens.len() + 2);
        window.push(0);
        window.extend_from_slice(tokens);
        window.push(0);
        let tokens_value = tokens_tensor(dtype(&self.dtypes, self.tokens_input()), &window)?;
        let style_value = style_tensor(dtype(&self.dtypes, "style"), style)?;
        let speed_value = speed_tensor(dtype(&self.dtypes, "speed"), speed)?;
        let inputs: Vec<(Cow<'_, str>, SessionInputValue<'static>)> = vec![
            (Cow::Borrowed(self.tokens_input.as_str()), tokens_value),
            (Cow::Borrowed("style"), style_value),
            (Cow::Borrowed("speed"), speed_value),
        ];
        let outputs = self
            .session
            .run(inputs)
            .map_err(|error| error.to_string())?;
        let (_, audio) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| error.to_string())?;
        let audio = audio.to_vec();
        let duration = if self.has_timings {
            let (_, duration) = outputs[1]
                .try_extract_tensor::<i64>()
                .map_err(|error| error.to_string())?;
            Some(duration.to_vec())
        } else {
            None
        };
        Ok((audio, duration))
    }
}

fn dtype(dtypes: &HashMap<String, DType>, name: &str) -> DType {
    dtypes.get(name).copied().unwrap_or(DType::Float32)
}

fn unsupported(name: &str, dtype: DType) -> String {
    format!("input {name} with type {dtype:?} is not supported")
}

fn tokens_tensor(dtype: DType, tokens: &[i64]) -> Result<SessionInputValue<'static>, String> {
    let shape = [1_usize, tokens.len()];
    match dtype {
        DType::Int64 => Tensor::from_array((shape, tokens.to_vec()))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        DType::Int32 => {
            let data: Vec<i32> = tokens.iter().map(|value| *value as i32).collect();
            Tensor::from_array((shape, data))
                .map(SessionInputValue::from)
                .map_err(|error| error.to_string())
        }
        other => Err(unsupported("tokens", other)),
    }
}

fn style_tensor(dtype: DType, style: &[f32]) -> Result<SessionInputValue<'static>, String> {
    let shape = [1_usize, style.len()];
    match dtype {
        DType::Float32 => Tensor::from_array((shape, style.to_vec()))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        DType::Float64 => {
            let data: Vec<f64> = style.iter().map(|value| f64::from(*value)).collect();
            Tensor::from_array((shape, data))
                .map(SessionInputValue::from)
                .map_err(|error| error.to_string())
        }
        other => Err(unsupported("style", other)),
    }
}

fn speed_tensor(dtype: DType, speed: f64) -> Result<SessionInputValue<'static>, String> {
    let shape = [1_usize];
    match dtype {
        DType::Float32 => Tensor::from_array((shape, vec![speed as f32]))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        DType::Float64 => Tensor::from_array((shape, vec![speed]))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        DType::Int32 => Tensor::from_array((shape, vec![speed as i32]))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        DType::Int64 => Tensor::from_array((shape, vec![speed as i64]))
            .map(SessionInputValue::from)
            .map_err(|error| error.to_string()),
        other => Err(unsupported("speed", other)),
    }
}

impl DType {
    /// `dtype.kind in "iu"` of the Python.
    pub fn kind_is_integer(self) -> bool {
        self.is_integer()
    }
}
