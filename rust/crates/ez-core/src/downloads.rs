//! Port of `src/ezagent/core/downloads.py` (F2-05).
//!
//! Recursos externos verificados por SHA256. Cada descarga va a un `.part`, se
//! reanuda con `Range`, se verifica el hash completo y solo entonces se
//! renombra. Un hash incorrecto descarta el `.part`.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use crate::dirs;

// DECISION: la version Rust tiene su propio manifiesto (PYOUT-1) para no
// depender del arbol Python, que se retira en el paso siguiente; incluye
// onnxruntime y espeak-ng, que antes salian del `.venv`.
const MANIFEST: &str = include_str!("manifest.toml");
const CHUNK: usize = 256 * 1024;

/// `DownloadError`: fallo de descarga, verificación o extracción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadError(pub String);

impl std::fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DownloadError {}

fn fail<T>(message: String) -> Result<T, DownloadError> {
    Err(DownloadError(message))
}

/// `(asset_id, hecho, total)`.
pub type ProgressFn<'a> = &'a mut dyn FnMut(&str, u64, u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub id: String,
    pub label: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub dest: String,
    pub unzip: bool,
    pub strip_prefix: String,
    pub license: String,
    pub optional: bool,
}

/// Carpeta de datos (`EZAGENT_HOME` o `%LOCALAPPDATA%\EzAgent`); se crea si falta.
pub fn data_dir() -> PathBuf {
    let root = match std::env::var("EZAGENT_HOME").ok().filter(|v| !v.is_empty()) {
        Some(explicit) => expand_home(&explicit),
        None => {
            let base = std::env::var("LOCALAPPDATA")
                .ok()
                .filter(|v| !v.is_empty())
                .map_or_else(|| dirs::home().join("AppData").join("Local"), PathBuf::from);
            base.join("EzAgent")
        }
    };
    // Como `mkdir(exist_ok=True)`: si no se puede crear, el error sale al usarla.
    let _ = std::fs::create_dir_all(&root);
    root
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            dirs::home().join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    }
}

/// `load_manifest()` con el manifiesto incrustado.
pub fn load_manifest() -> Result<IndexMap<String, Asset>, DownloadError> {
    parse_manifest(MANIFEST)
}

/// `load_manifest(path)`.
pub fn load_manifest_from(path: &Path) -> Result<IndexMap<String, Asset>, DownloadError> {
    let text = std::fs::read_to_string(path).map_err(|error| DownloadError(error.to_string()))?;
    parse_manifest(&text)
}

fn parse_manifest(text: &str) -> Result<IndexMap<String, Asset>, DownloadError> {
    let raw: toml::Table = text
        .parse()
        .map_err(|error: toml::de::Error| DownloadError(error.to_string()))?;
    let mut assets = IndexMap::new();
    let entries = raw.get("asset").and_then(toml::Value::as_array);
    for entry in entries.into_iter().flatten() {
        let asset = asset_from(entry)?;
        if !asset.url.starts_with("https://") {
            return fail(format!("{}: solo se permiten URLs https", asset.id));
        }
        assets.insert(asset.id.clone(), asset);
    }
    Ok(assets)
}

fn asset_from(entry: &toml::Value) -> Result<Asset, DownloadError> {
    let text = |key: &str| entry.get(key).and_then(toml::Value::as_str);
    let flag = |key: &str| {
        entry
            .get(key)
            .and_then(toml::Value::as_bool)
            .unwrap_or(false)
    };
    let required = |key: &str| {
        text(key)
            .map(str::to_string)
            .ok_or_else(|| DownloadError(format!("KeyError: '{key}'")))
    };
    let id = required("id")?;
    let size = entry
        .get("size")
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| DownloadError("KeyError: 'size'".into()))?;
    Ok(Asset {
        label: text("label").map_or_else(|| id.clone(), str::to_string),
        url: required("url")?,
        sha256: required("sha256")?.to_lowercase(),
        size: u64::try_from(size).map_err(|_| DownloadError("size negativo".into()))?,
        dest: required("dest")?,
        unzip: flag("unzip"),
        strip_prefix: text("strip_prefix").unwrap_or("").to_string(),
        license: text("license").unwrap_or("").to_string(),
        optional: flag("optional"),
        id,
    })
}

pub fn asset_path(asset: &Asset, root: Option<&Path>) -> PathBuf {
    root.map_or_else(data_dir, Path::to_path_buf)
        .join(&asset.dest)
}

fn marker(asset: &Asset, root: Option<&Path>) -> PathBuf {
    let short: String = asset.sha256.chars().take(12).collect();
    asset_path(asset, root).join(format!(".installed-{short}"))
}

pub fn is_installed(asset: &Asset, root: Option<&Path>) -> bool {
    if asset.unzip {
        return marker(asset, root).is_file();
    }
    std::fs::metadata(asset_path(asset, root))
        .is_ok_and(|meta| meta.is_file() && meta.len() == asset.size)
}

fn sha256_of(path: &Path) -> Result<String, DownloadError> {
    let io = |error: std::io::Error| DownloadError(error.to_string());
    let mut file = File::open(path).map_err(io)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        let read = file.read(&mut buffer).map_err(io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

fn http_error(asset: &Asset, error: impl std::fmt::Display) -> DownloadError {
    DownloadError(format!("{}: {error}", asset.id))
}

fn fetch(
    asset: &Asset,
    part: &Path,
    client: &reqwest::blocking::Client,
    mut progress: Option<ProgressFn<'_>>,
) -> Result<(), DownloadError> {
    let mut start = std::fs::metadata(part).map_or(0, |meta| meta.len());
    if start >= asset.size {
        // Parte inválida o completa: se descarta y se re-verifica.
        let _ = std::fs::remove_file(part);
        start = 0;
    }
    let mut request = client.get(&asset.url);
    if start > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={start}-"));
    }
    let mut response = request.send().map_err(|error| http_error(asset, error))?;
    match response.status().as_u16() {
        200 => start = 0, // el servidor ignoró Range: empezar de cero
        206 => {}
        other => return fail(format!("{}: HTTP {other}", asset.id)),
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(start > 0)
        .truncate(start == 0)
        .open(part)
        .map_err(|error| http_error(asset, error))?;
    let mut done = start;
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| http_error(asset, error))?;
        if read == 0 {
            return Ok(());
        }
        out.write_all(&buffer[..read])
            .map_err(|error| http_error(asset, error))?;
        done += read as u64;
        if let Some(report) = progress.as_mut() {
            report(&asset.id, done, asset.size);
        }
    }
}

/// Ruta de `name` dentro de `target`, o `None` si se sale (zip slip).
fn safe_join(target: &Path, name: &str) -> Option<PathBuf> {
    let mut out = target.to_path_buf();
    let depth = target.components().count();
    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if out.components().count() <= depth {
                    return None;
                }
                out.pop();
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (out.components().count() > depth).then_some(out)
}

fn extract(asset: &Asset, archive: &Path, root: Option<&Path>) -> Result<(), DownloadError> {
    let io = |error: std::io::Error| DownloadError(format!("{}: {error}", asset.id));
    let target = asset_path(asset, root);
    std::fs::create_dir_all(&target).map_err(io)?;
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(io)?)
        .map_err(|error| http_error(asset, error))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| http_error(asset, error))?;
        let original = entry
            .name()
            .map_err(|error| http_error(asset, error))?
            .into_owned();
        let mut name = original.as_str();
        if !asset.strip_prefix.is_empty() {
            match name.strip_prefix(asset.strip_prefix.as_str()) {
                Some(rest) => name = rest,
                None => continue,
            }
        }
        if name.is_empty() || name.ends_with('/') {
            continue;
        }
        let Some(out) = safe_join(&target, name) else {
            return fail(format!("{}: ruta insegura en el zip: {original}", asset.id));
        };
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut dst = File::create(&out).map_err(io)?;
        std::io::copy(&mut entry, &mut dst).map_err(io)?;
    }
    std::fs::write(marker(asset, root), &asset.sha256).map_err(io)
}

/// Opciones de `download`.
#[derive(Default)]
pub struct DownloadOptions<'a> {
    pub root: Option<&'a Path>,
    pub client: Option<&'a reqwest::blocking::Client>,
    pub force: bool,
}

fn default_client() -> Result<reqwest::blocking::Client, DownloadError> {
    reqwest::blocking::Client::builder()
        // DECISION: `reqwest::blocking` no tiene `read_timeout`; en su lugar hay un
        // tope total de 6 h por descarga (el Python solo vigila 60 s sin datos).
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(6 * 3600))
        .build()
        .map_err(|error| DownloadError(error.to_string()))
}

/// Descarga y verifica `asset`; devuelve su ruta. Reintenta una vez.
pub fn download(
    asset: &Asset,
    options: &DownloadOptions<'_>,
    mut progress: Option<ProgressFn<'_>>,
) -> Result<PathBuf, DownloadError> {
    if is_installed(asset, options.root) && !options.force {
        return Ok(asset_path(asset, options.root));
    }
    let base = options.root.map_or_else(data_dir, Path::to_path_buf);
    let owned;
    let client = match options.client {
        Some(client) => client,
        None => {
            owned = default_client()?;
            &owned
        }
    };
    let dest = if asset.unzip {
        format!("{}.zip", asset.dest)
    } else {
        asset.dest.clone()
    };
    let final_path = base.join(dest);
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| http_error(asset, error))?;
    }
    let mut part_name = final_path.file_name().unwrap_or_default().to_os_string();
    part_name.push(".part");
    let part = final_path.with_file_name(part_name);
    for attempt in 1..=2 {
        fetch(
            asset,
            &part,
            client,
            progress.as_deref_mut().map(|p| p as ProgressFn<'_>),
        )?;
        if sha256_of(&part)? == asset.sha256 {
            break;
        }
        let _ = std::fs::remove_file(&part);
        tracing::warn!("{}: hash incorrecto (intento {attempt})", asset.id);
        if attempt == 2 {
            return fail(format!(
                "{}: el hash no coincide; descarga descartada",
                asset.id
            ));
        }
    }
    std::fs::rename(&part, &final_path).map_err(|error| http_error(asset, error))?;
    if asset.unzip {
        let result = extract(asset, &final_path, options.root);
        let _ = std::fs::remove_file(&final_path);
        result?;
    }
    Ok(asset_path(asset, options.root))
}

/// Copia un archivo existente si su hash es el oficial; no toca `source`.
pub fn adopt(asset: &Asset, source: &Path, root: Option<&Path>) -> bool {
    let Ok(meta) = std::fs::metadata(source) else {
        return false;
    };
    if asset.unzip || !meta.is_file() || meta.len() != asset.size {
        return false;
    }
    if sha256_of(source).map_or(true, |hash| hash != asset.sha256) {
        return false;
    }
    let final_path = asset_path(asset, root);
    if let Some(parent) = final_path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let mut part_name = final_path.file_name().unwrap_or_default().to_os_string();
    part_name.push(".part");
    let part = final_path.with_file_name(part_name);
    std::fs::copy(source, &part).is_ok() && std::fs::rename(&part, &final_path).is_ok()
}

/// `ensure(asset_id)`.
pub fn ensure(
    asset_id: &str,
    options: &DownloadOptions<'_>,
    progress: Option<ProgressFn<'_>>,
) -> Result<PathBuf, DownloadError> {
    let assets = load_manifest()?;
    match assets.get(asset_id) {
        Some(asset) => download(asset, options, progress),
        None => fail(format!("Recurso desconocido: {asset_id}")),
    }
}

/// `python -m ezagent.core.downloads`: devuelve el código de salida.
pub fn cli_main(args: &[String], out: &mut dyn Write) -> i32 {
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let ids: Vec<&String> = args.iter().filter(|arg| !arg.starts_with("--")).collect();
    let Ok(assets) = load_manifest() else {
        return 1;
    };
    if flag("--list") || !(flag("--all") || !ids.is_empty()) {
        for asset in assets.values() {
            let state = if is_installed(asset, None) {
                "instalado"
            } else {
                "falta"
            };
            let extra = if asset.optional { " (opcional)" } else { "" };
            let _ = writeln!(
                out,
                "{:22} {:10} {:8.1} MB  {}{}",
                asset.id,
                state,
                asset.size as f64 / 1e6,
                asset.label,
                extra
            );
        }
        return 0;
    }
    let wanted: Vec<String> = if ids.is_empty() {
        assets
            .values()
            .filter(|asset| flag("--with-optional") || !asset.optional)
            .map(|asset| asset.id.clone())
            .collect()
    } else {
        ids.into_iter().cloned().collect()
    };
    for asset_id in wanted {
        let _ = writeln!(out, "{asset_id}…");
        let mut last = -1_i64;
        let mut lines: Vec<String> = Vec::new();
        let mut show = |id: &str, done: u64, total: u64| {
            let pct = (done * 100 / total.max(1)) as i64;
            if pct != last && pct % 10 == 0 {
                last = pct;
                lines.push(format!("  {id}: {pct}%"));
            }
        };
        let result = ensure(&asset_id, &DownloadOptions::default(), Some(&mut show));
        for line in lines {
            let _ = writeln!(out, "{line}");
        }
        match result {
            Ok(path) => {
                let _ = writeln!(out, "  listo: {}", path.display());
            }
            Err(error) => {
                let _ = writeln!(out, "ERROR: {error}");
                return 1;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_every_asset() {
        let assets = load_manifest().expect("manifiesto");
        assert_eq!(assets.len(), 13);
        assert!(assets["whisper-vulkan"].unzip);
        assert_eq!(assets["whisper-cpu"].strip_prefix, "Release/");
        assert!(assets["kokoro-fp32"].optional);
        let runtime = &assets["onnxruntime"];
        assert_eq!(
            runtime.dest, "onnxruntime",
            "ez-tts y ez-audio buscan la DLL ahí"
        );
        assert_eq!(runtime.strip_prefix, "runtimes/win-x64/native/");
        let espeak = &assets["espeak-ng"];
        assert_eq!(espeak.dest, "espeak-ng");
        assert_eq!(espeak.strip_prefix, "espeakng_loader/");
    }

    #[test]
    fn rejects_zip_slip() {
        let base = Path::new("base");
        assert!(safe_join(base, "../x").is_none());
        assert!(safe_join(base, "a/../../x").is_none());
        assert_eq!(safe_join(base, "a/../b"), Some(base.join("b")));
        assert_eq!(
            safe_join(base, "a/b.dll"),
            Some(base.join("a").join("b.dll"))
        );
    }

    #[test]
    fn http_urls_are_refused() {
        let text = "[[asset]]\nid='x'\nurl='http://a'\nsha256='AB'\nsize=1\ndest='d'\n";
        assert_eq!(
            parse_manifest(text),
            Err(DownloadError("x: solo se permiten URLs https".into()))
        );
    }
}
