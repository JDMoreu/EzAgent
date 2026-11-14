//! Port of `src/ezagent/tts.py` (F2-05): archivos del modelo Kokoro.
//!
//! Por defecto usa la variante int8. Si existen en la caché heredada
//! (`~/.cache/pipecat/kokoro-onnx`) se copian una sola vez tras comprobar su
//! hash; si no, los descarga el gestor.

use std::path::{Path, PathBuf};

use crate::dirs;
use crate::downloads::{
    adopt, asset_path, download, is_installed, load_manifest, Asset, DownloadError, DownloadOptions,
};

pub const VOICES_ASSET: &str = "kokoro-voices";

/// `MODEL_ASSETS`: variante → id del recurso.
pub fn model_asset(variant: &str) -> Option<&'static str> {
    match variant {
        "int8" => Some("kokoro-int8"),
        "fp32" => Some("kokoro-fp32"),
        _ => None,
    }
}

/// Caché de una versión anterior con Pipecat: solo se lee, nunca se escribe.
pub fn legacy_dir() -> PathBuf {
    dirs::home()
        .join(".cache")
        .join("pipecat")
        .join("kokoro-onnx")
}

fn ensure_asset(asset: &Asset) -> Result<PathBuf, DownloadError> {
    if !is_installed(asset, None) {
        let name = Path::new(&asset.dest).file_name().unwrap_or_default();
        if adopt(asset, &legacy_dir().join(name), None) {
            tracing::info!("{}: copiado desde la caché anterior", asset.id);
        } else {
            download(asset, &DownloadOptions::default(), None)?;
        }
    }
    Ok(asset_path(asset, None))
}

/// `resolve_kokoro_files`: `(modelo, voces)` para Kokoro.
pub fn resolve_kokoro_files(
    variant: &str,
    model_path: Option<&str>,
    voices_path: Option<&str>,
) -> Result<(Option<String>, Option<String>), DownloadError> {
    let manifest = load_manifest()?;
    let asset = |id: &str| {
        manifest
            .get(id)
            .ok_or_else(|| DownloadError(format!("KeyError: '{id}'")))
    };
    let voices = match voices_path.filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => ensure_asset(asset(VOICES_ASSET)?)?,
    };
    let model = if let Some(path) = model_path.filter(|path| !path.is_empty()) {
        Some(PathBuf::from(path))
    } else if let Some(id) = model_asset(variant) {
        Some(ensure_asset(asset(id)?)?)
    } else {
        tracing::warn!("KOKORO_MODEL_VARIANT='{variant}' no reconocido; usa KOKORO_MODEL_PATH");
        None
    };
    Ok((
        model.map(|path| path.display().to_string()),
        Some(voices.display().to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_map_to_assets() {
        assert_eq!(model_asset("int8"), Some("kokoro-int8"));
        assert_eq!(model_asset("fp32"), Some("kokoro-fp32"));
        assert_eq!(model_asset("x"), None);
    }

    #[test]
    fn explicit_paths_skip_downloads() {
        let (model, voices) =
            resolve_kokoro_files("int8", Some("m.onnx"), Some("v.bin")).expect("rutas dadas");
        assert_eq!(model.as_deref(), Some("m.onnx"));
        assert_eq!(voices.as_deref(), Some("v.bin"));
    }
}
