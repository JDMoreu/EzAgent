//! `LoadLibraryW`/`GetProcAddress` bindings for the espeak-ng C library.
//!
//! The only FFI of `ez-tts` lives here. Every block states why it is sound;
//! the functions come from the loaded library and their signatures are the
//! ones in `speak_lib.h`.

use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};

use windows::core::PCSTR;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

// SAFETY: signatures copied from `speak_lib.h`; calls happen inside `unsafe`.
type Procedure = unsafe extern "system" fn() -> isize;

/// `AUDIO_OUTPUT_SYNCHRONOUS`: espeak only computes, `phonemizer` uses it too.
const AUDIO_OUTPUT_SYNCHRONOUS: i32 = 0x02;
/// `espeakCHARS_UTF8`.
const TEXT_MODE_UTF8: i32 = 1;
/// IPA output with `_` as the phoneme separator: `ord('_') << 8 | 0x02`.
const PHONEMES_MODE: i32 = (b'_' as i32) << 8 | 0x02;
/// `_N_PATH_HOME` from `speech.h`, 230 on Windows.
const PATH_HOME: usize = 230;

// SAFETY: signatures copied from `speak_lib.h`; calls happen inside `unsafe`.
type Initialize = unsafe extern "C" fn(i32, i32, *const c_char, i32) -> i32;
type Terminate = unsafe extern "C" fn() -> i32;
type Info = unsafe extern "C" fn(*mut *const c_char) -> *const c_char;
// SAFETY: same origin as the signatures above.
type SetVoiceByName = unsafe extern "C" fn(*const c_char) -> i32;
type TextToPhonemes = unsafe extern "C" fn(*mut *mut c_char, i32, i32) -> *const c_char;

/// One loaded module. Function pointers stay valid while the library is alive.
pub struct Library {
    module: HMODULE,
}

// SAFETY: the module handle is opaque; espeak calls are serialized by the
// `EspeakPhonemizer` mutex.
unsafe impl Send for Library {}

impl Library {
    pub fn open(path: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt as _;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` is NUL terminated and outlives the call.
        let module = unsafe { LoadLibraryW(wide_str(&wide)) }.map_err(|error| error.to_string())?;
        Ok(Self { module })
    }

    pub fn open_by_name(name: &str) -> Result<Self, String> {
        Self::open(Path::new(name))
    }

    fn symbol(&self, name: &str) -> Result<Procedure, String> {
        let symbol = CString::new(name).map_err(|error| error.to_string())?;
        // SAFETY: the module is loaded and the symbol name is NUL terminated.
        let procedure =
            unsafe { GetProcAddress(self.module, PCSTR::from_raw(symbol.as_ptr().cast())) };
        procedure.ok_or_else(|| format!("the espeak library does not export {name}"))
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the module was returned by `LoadLibraryW` and is dropped once.
        unsafe {
            let _ = FreeLibrary(self.module);
        }
    }
}

fn wide_str(wide: &[u16]) -> windows::core::PCWSTR {
    windows::core::PCWSTR::from_raw(wide.as_ptr())
}

macro_rules! bind {
    ($library:expr, $name:literal, $ty:ty) => {{
        let procedure = $library.symbol($name)?;
        // SAFETY: a function pointer has the size of `Procedure` and the
        // exported function has the signature of `$ty` from `speak_lib.h`.
        unsafe { std::mem::transmute::<Procedure, $ty>(procedure) }
    }};
}

struct Api {
    initialize: Initialize,
    terminate: Terminate,
    info: Info,
    set_voice_by_name: SetVoiceByName,
    text_to_phonemes: TextToPhonemes,
}

/// A ready espeak-ng session with the mutable state espeak keeps globally.
pub struct Espeak {
    api: Api,
    #[allow(dead_code)] // keeps the module loaded while `api` points into it
    library: Library,
    version: (i32, i32),
    voice: Option<String>,
}

impl Espeak {
    /// Loads `library` and initializes espeak with `data_path`.
    pub fn open(library: Library, data_path: Option<&Path>) -> Result<Self, String> {
        let api = Api {
            initialize: bind!(library, "espeak_Initialize", Initialize),
            terminate: bind!(library, "espeak_Terminate", Terminate),
            info: bind!(library, "espeak_Info", Info),
            set_voice_by_name: bind!(library, "espeak_SetVoiceByName", SetVoiceByName),
            text_to_phonemes: bind!(library, "espeak_TextToPhonemes", TextToPhonemes),
        };
        let version = version_of(&api)?;
        if version < (1, 48) {
            return Err(format!(
                "espeak-{}.{} is not supported, phonemizer requires espeak>=1.48",
                version.0, version.1
            ));
        }
        let data = match data_path {
            Some(path) => data_path_argument(path, version)?,
            None => PathBuf::new(),
        };
        let path =
            CString::new(data.to_string_lossy().as_bytes()).map_err(|error| error.to_string())?;
        // SAFETY: the path is NUL terminated; a null path means espeak's default.
        let rate = unsafe {
            (api.initialize)(
                AUDIO_OUTPUT_SYNCHRONOUS,
                0,
                if data.as_os_str().is_empty() {
                    std::ptr::null()
                } else {
                    path.as_ptr()
                },
                0,
            )
        };
        if rate <= 0 {
            return Err("failed to initialize espeak shared library".to_string());
        }
        Ok(Self {
            api,
            library,
            version,
            voice: None,
        })
    }

    pub fn version(&self) -> (i32, i32) {
        self.version
    }

    /// `EspeakWrapper.set_voice("es")`: the voice name is the language code.
    pub fn set_voice(&mut self, code: &str) -> Result<(), String> {
        if self.voice.as_deref() == Some(code) {
            return Ok(());
        }
        if self.try_voice(code)? || self.try_voice(&code.to_lowercase())? {
            self.voice = Some(code.to_string());
            return Ok(());
        }
        Err(format!("invalid voice code \"{code}\""))
    }

    fn try_voice(&self, code: &str) -> Result<bool, String> {
        let name = CString::new(code).map_err(|error| error.to_string())?;
        // SAFETY: the name is NUL terminated and espeak copies what it needs.
        let status = unsafe { (self.api.set_voice_by_name)(name.as_ptr()) };
        Ok(status == 0)
    }

    /// `text_to_phonemes`: every returned clause joined with a space.
    pub fn text_to_phonemes(&self, text: &str) -> Result<String, String> {
        let text = text.replace('\0', "");
        let source = CString::new(text).map_err(|error| error.to_string())?;
        let mut pointer = source.as_ptr() as *mut c_char;
        let mut pieces = Vec::new();
        while !pointer.is_null() {
            let previous = pointer;
            // SAFETY: espeak advances `pointer` inside `source`, which stays
            // alive for the loop; the string is read before the next call.
            let result =
                unsafe { (self.api.text_to_phonemes)(&mut pointer, TEXT_MODE_UTF8, PHONEMES_MODE) };
            if !result.is_null() {
                // SAFETY: espeak returns a NUL-terminated C string or null.
                pieces.push(
                    unsafe { CStr::from_ptr(result) }
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            if pointer == previous {
                // DECISION: espeak always advances; stopping avoids a hang if
                // a build returns the same pointer forever.
                break;
            }
        }
        Ok(pieces.join(" "))
    }
}

impl Drop for Espeak {
    fn drop(&mut self) {
        // SAFETY: only called once, before the library is unloaded.
        unsafe {
            let _ = (self.api.terminate)();
        }
    }
}

fn version_of(api: &Api) -> Result<(i32, i32), String> {
    let mut data_path: *const c_char = std::ptr::null();
    // SAFETY: `espeak_Info` expects a place to store the data path.
    let text = unsafe { (api.info)(&mut data_path) };
    if text.is_null() {
        return Err("failed to load espeak library".to_string());
    }
    // SAFETY: the version string is NUL terminated.
    let text = unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned();
    let first = text
        .trim()
        .split(' ')
        .next()
        .unwrap_or_default()
        .replace("-dev", "");
    let mut parts = first.split('.').filter_map(|part| part.parse::<i32>().ok());
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => Ok((major, minor)),
        _ => Err(format!("unexpected espeak version {text:?}")),
    }
}

/// `_data_path_argument`: which directory to hand to `espeak_Initialize`.
pub fn data_path_argument(data_path: &Path, version: (i32, i32)) -> Result<PathBuf, String> {
    let mut passed = data_path.to_path_buf();
    let data = if version < (1, 49) {
        if passed.file_name().is_some_and(|name| name == "espeak-data") {
            passed = passed.parent().unwrap_or(Path::new("")).to_path_buf();
        }
        passed.join("espeak-data")
    } else if passed.join("phontab").is_file() {
        passed.clone()
    } else {
        passed.join("espeak-ng-data")
    };
    if !data.join("phontab").is_file() {
        return Err(format!("no espeak data found in {}", data_path.display()));
    }
    if data.to_string_lossy().len() >= PATH_HOME {
        // DECISION: Python tries a symlink in a temp dir; Windows needs a
        // privilege, so the same error is reported instead.
        return Err(format!(
            "the path to the espeak data is too long for espeak ({} bytes at most) and a shorter link to it cannot be made: symlinks are not supported",
            PATH_HOME - 1
        ));
    }
    Ok(passed)
}
