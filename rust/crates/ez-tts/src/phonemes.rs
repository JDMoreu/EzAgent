//! Port of `kokoro_onnx.tokenizer` and `phonemizer` (espeak-ng) (F4-04).
//!
//! `espeak-ng` is a C library: it is loaded with `LoadLibraryW`/`GetProcAddress`
//! (see `ffi.rs`) and every call runs under a lock, like the module-level
//! `_espeak_lock` of the Python. The FFI is hidden behind [`Phonemizer`] so the
//! rest of the crate (and the tests) can work without the DLL.

mod ffi;
mod text;

use std::path::{Path, PathBuf};

use std::sync::OnceLock;

use parking_lot::Mutex;

/// Failure to load or call the espeak library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonemeError(pub String);

impl std::fmt::Display for PhonemeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PhonemeError {}

impl From<String> for PhonemeError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for PhonemeError {
    fn from(message: &str) -> Self {
        Self(message.to_string())
    }
}

/// `EspeakConfig`: the shared library and its data directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EspeakConfig {
    pub lib_path: Option<PathBuf>,
    pub data_path: Option<PathBuf>,
}

impl EspeakConfig {
    pub fn new(lib_path: impl Into<PathBuf>, data_path: impl Into<PathBuf>) -> Self {
        Self {
            lib_path: Some(lib_path.into()),
            data_path: Some(data_path.into()),
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Folder `espeak-ng` is unpacked into (`ez_core::downloads` decides where).
pub fn default_install_dir() -> PathBuf {
    ez_core::downloads::data_dir().join("espeak-ng")
}

/// Folder of the running executable: the packaging puts the DLLs there (F8-01).
fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

/// Library escogida por configuración o por el entorno; `None` si no hay.
pub fn default_library_path() -> Option<PathBuf> {
    env_path("EZAGENT_ESPEAK_LIBRARY")
        .or_else(|| env_path("PHONEMIZER_ESPEAK_LIBRARY"))
        // DECISION: primero junto al ejecutable (el stage instalado) y después
        // la carpeta de descargas, como la DLL de ONNX Runtime.
        .or_else(|| library_in(&[executable_dir()?, default_install_dir()]))
}

/// Data directory (`phontab` inside) elegida por configuración o entorno.
pub fn default_data_path() -> Option<PathBuf> {
    env_path("EZAGENT_ESPEAK_DATA_PATH")
        .or_else(|| env_path("PHONEMIZER_ESPEAK_DATA_PATH"))
        .or_else(|| {
            let mut folders = Vec::new();
            folders.extend(executable_dir());
            folders.push(default_install_dir());
            folders
                .into_iter()
                .map(|folder| folder.join("espeak-ng-data"))
                .find(|folder| folder.is_dir())
        })
}

/// Primera carpeta que contiene `espeak-ng.dll` o `libespeak-ng.dll`.
fn library_in(folders: &[PathBuf]) -> Option<PathBuf> {
    folders
        .iter()
        .flat_map(|folder| {
            ["espeak-ng.dll", "libespeak-ng.dll"]
                .iter()
                .map(move |name| folder.join(name))
        })
        .find(|path| path.is_file())
}

/// `phonemizer.phonemize(text, lang, preserve_punctuation=True, with_stress=True)`.
pub trait Phonemizer: Send + Sync {
    fn phonemize(&self, text: &str, lang: &str) -> Result<String, PhonemeError>;
}

/// espeak-ng with the process-global state espeak keeps behind a lock.
///
/// espeak is a single C library instance per process: the Python loads a copy
/// per backend, but `LoadLibraryW` on the same file shares the module and its
/// globals, so a second instance must reuse it. The first configuration wins.
pub struct EspeakPhonemizer {
    version: (i32, i32),
}

static ESPEAK: OnceLock<Mutex<ffi::Espeak>> = OnceLock::new();
static ESPEAK_INIT: Mutex<()> = Mutex::new(());

impl EspeakPhonemizer {
    /// Loads the configured library, or the system-wide one as the Python does.
    pub fn new(config: &EspeakConfig) -> Result<Self, PhonemeError> {
        if let Some(espeak) = ESPEAK.get() {
            return Ok(Self {
                version: espeak.lock().version(),
            });
        }
        let _guard = ESPEAK_INIT.lock();
        if let Some(espeak) = ESPEAK.get() {
            return Ok(Self {
                version: espeak.lock().version(),
            });
        }
        let espeak = open_espeak(config)?;
        let version = espeak.version();
        tracing::debug!("espeak {}.{} loaded", version.0, version.1);
        let _ = ESPEAK.set(Mutex::new(espeak));
        Ok(Self { version })
    }

    /// espeak version as `(major, minor)`.
    pub fn version(&self) -> (i32, i32) {
        self.version
    }
}

fn open_espeak(config: &EspeakConfig) -> Result<ffi::Espeak, PhonemeError> {
    let configured = config.lib_path.clone().or_else(default_library_path);
    let mut library = None;
    if let Some(path) = &configured {
        match ffi::Library::open(path) {
            Ok(loaded) => library = Some(loaded),
            Err(error) => {
                tracing::error!("Failed to load espeak shared library: {error}");
                tracing::warn!("Falling back to system wide espeak-ng library");
            }
        }
    } else {
        tracing::error!("Failed to load espeak shared library: no library found");
        tracing::warn!("Falling back to system wide espeak-ng library");
    }
    let library = match library {
        Some(library) => library,
        None => [
            "espeak-ng.dll",
            "espeak.dll",
            "libespeak-ng.dll",
            "libespeak.dll",
        ]
        .iter()
        .find_map(|name| ffi::Library::open_by_name(name).ok())
        .ok_or_else(|| PhonemeError(load_error_info()))?,
    };
    let data = config
        .data_path
        .clone()
        .or_else(default_data_path)
        .or_else(|| {
            configured
                .as_deref()
                .and_then(|path| path.parent())
                .map(|parent| parent.join("espeak-ng-data"))
        });
    ffi::Espeak::open(library, data.as_deref()).map_err(PhonemeError)
}

impl Phonemizer for EspeakPhonemizer {
    fn phonemize(&self, text: &str, lang: &str) -> Result<String, PhonemeError> {
        let espeak = ESPEAK
            .get()
            .ok_or_else(|| PhonemeError("espeak is not initialized".to_string()))?;
        let mut espeak = espeak.lock();
        // El mensaje público de `phonemizer` cuando el idioma no existe.
        espeak.set_voice(lang).map_err(|_| {
            PhonemeError(format!(
                "language {lang:?} is not supported by the espeak backend"
            ))
        })?;
        text::phonemize_text(text, |chunk| {
            let raw = espeak.text_to_phonemes(chunk).map_err(PhonemeError)?;
            Ok(text::postprocess(&raw))
        })
    }
}

/// Text of the Python when neither the configured nor the system library loads.
fn load_error_info() -> String {
    format!(
        "Failed to load espeak-ng from fallback. Please install espeak-ng system wide.\n\tSee https://github.com/espeak-ng/espeak-ng/blob/master/docs/guide.md\n\tNote: you can specify shared library path using PHONEMIZER_ESPEAK_LIBRARY environment variable.\nEnvironment:\n\t{} ({}) | ez-tts {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION")
    )
}

/// Answer of the fake phonemizer for one `(text, lang)` pair.
type Answer = Box<dyn Fn(&str, &str) -> String + Send + Sync>;

/// A phonemizer with fixed answers, for tests and for callers without espeak.
pub struct FakePhonemizer {
    answer: Answer,
    calls: Mutex<Vec<(String, String)>>,
}

impl FakePhonemizer {
    pub fn new(answer: impl Fn(&str, &str) -> String + Send + Sync + 'static) -> Self {
        Self {
            answer: Box::new(answer),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Returns `text` unchanged whatever the language.
    pub fn identity() -> Self {
        Self::new(|text, _lang| text.to_string())
    }

    /// Every `(text, lang)` pair received so far.
    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().clone()
    }
}

impl Phonemizer for FakePhonemizer {
    fn phonemize(&self, text: &str, lang: &str) -> Result<String, PhonemeError> {
        self.calls.lock().push((text.to_string(), lang.to_string()));
        Ok((self.answer)(text, lang))
    }
}

#[path = "phonemes/tests.rs"]
#[cfg(test)]
mod tests;
