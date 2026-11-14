//! Port of `kokoro_onnx` voice styles and `tts._voice_style` (F4-04).
//!
//! `voices-v1.0.bin` is a `.npz`: a zip of `.npy` arrays with shape
//! `(phonemes, 1, 256)` in C order and `float32`. The Python side reads it with
//! `numpy.load` and indexes `voices[name]`; here the zip and the `.npy` header
//! are parsed by hand because `ez-tts` does not declare the `zip` crate.
// DECISION: every entry is read eagerly (about 28 MB); Python's `NpzFile`
// loads each array on demand.
// DECISION: no new dependency: the zip entries are stored (method 0) and the
// `.npy` header is a short dict, so a minimal reader covers the real file.
// Compressed or Fortran-ordered entries are reported as errors instead of
// guessed.

use std::collections::HashMap;
use std::path::Path;

use ez_core::pyvalue::{py_strip, repr_str};

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL: u32 = 0x0605_4b50;
const NPY_MAGIC: &[u8] = b"\x93NUMPY";

/// One voice style: `float32` rows of `cols` weights in C order.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    rows: usize,
    cols: usize,
    data: Vec<f32>,
}

impl Style {
    /// Builds a style from its rows; `data.len()` must be `rows * cols`.
    pub fn new(rows: usize, cols: usize, data: Vec<f32>) -> Result<Self, String> {
        if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(data.len()) {
            return Err(format!(
                "style shape ({rows}, {cols}) does not match {} values",
                data.len()
            ));
        }
        Ok(Self { rows, cols, data })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// `voice[index]`, i.e. one style vector per phoneme count.
    pub fn row(&self, index: usize) -> Option<&[f32]> {
        if index >= self.rows {
            return None;
        }
        Some(&self.data[index * self.cols..(index + 1) * self.cols])
    }

    fn mean(styles: &[&Style]) -> Result<Style, String> {
        let first = styles.first().ok_or("no styles to average")?;
        if styles
            .iter()
            .any(|style| style.rows != first.rows || style.cols != first.cols)
        {
            return Err("voice styles with different shapes".to_string());
        }
        let mut data = vec![0.0_f32; first.data.len()];
        for style in styles {
            for (slot, value) in data.iter_mut().zip(&style.data) {
                *slot += *value;
            }
        }
        let count = styles.len() as f32;
        for value in &mut data {
            *value /= count;
        }
        Ok(Style {
            rows: first.rows,
            cols: first.cols,
            data,
        })
    }
}

/// All styles of one `voices-v1.0.bin`.
#[derive(Debug, Clone, Default)]
pub struct Voices {
    styles: HashMap<String, Style>,
}

impl Voices {
    /// `numpy.load(voices_path)`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        Self::from_npz(&bytes)
    }

    /// Parses the `.npz` bytes, so tests do not need a file on disk.
    pub fn from_npz(bytes: &[u8]) -> Result<Self, String> {
        let mut styles = HashMap::new();
        for (name, payload) in zip_entries(bytes)? {
            let stem = name.strip_suffix(".npy").unwrap_or(&name);
            let style = parse_npy(&payload)?;
            styles.insert(stem.to_string(), style);
        }
        Ok(Self { styles })
    }

    pub fn len(&self) -> usize {
        self.styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }

    /// Builds the set from already parsed styles (tests and callers).
    pub fn from_styles(styles: HashMap<String, Style>) -> Self {
        Self { styles }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.styles.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&Style> {
        self.styles.get(name)
    }

    /// `sorted(self.voices.keys())`: Python sorts by code point.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.styles.keys().cloned().collect();
        names.sort();
        names
    }

    /// `_voice_style`: `a+b` averages the styles of the names that exist.
    pub fn voice_style(&self, voice: &str) -> Result<Style, String> {
        let names: Vec<&str> = voice
            .split('+')
            .map(py_strip)
            .filter(|name| !name.is_empty())
            .collect();
        let styles: Vec<&Style> = names
            .iter()
            .filter_map(|name| self.styles.get(*name))
            .collect();
        match styles.len() {
            0 => Err(format!(
                "Ninguna voz de Kokoro válida en {}",
                repr_str(voice)
            )),
            1 => Ok(styles[0].clone()),
            _ => Style::mean(&styles),
        }
    }
}

/// `_voice_style(kokoro, voice)` over an already loaded set.
pub fn voice_style(voices: &Voices, voice: &str) -> Result<Style, String> {
    voices.voice_style(voice)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "truncated zip".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated zip".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// `(name, data)` of every stored entry, walking the local headers in order.
fn zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut entries = Vec::new();
    let mut offset = 0_usize;
    while offset + 4 <= bytes.len() {
        let signature = u32_at(bytes, offset)?;
        if signature == CENTRAL_HEADER || signature == END_OF_CENTRAL {
            break;
        }
        if signature != LOCAL_HEADER {
            return Err(format!("unexpected zip signature at {offset}"));
        }
        let flags = u16_at(bytes, offset + 6)?;
        let method = u16_at(bytes, offset + 8)?;
        let mut compressed = u64::from(u32_at(bytes, offset + 18)?);
        let mut uncompressed = u64::from(u32_at(bytes, offset + 22)?);
        let name_len = u16_at(bytes, offset + 26)? as usize;
        let extra_len = u16_at(bytes, offset + 28)? as usize;
        if flags & 0x08 != 0 {
            return Err("zip entries with a data descriptor are not supported".to_string());
        }
        if method != 0 {
            return Err(format!("zip compression method {method} is not supported"));
        }
        let name_start = offset + 30;
        let name = bytes
            .get(name_start..name_start + name_len)
            .ok_or_else(|| "truncated zip name".to_string())?;
        let name = String::from_utf8_lossy(name).into_owned();
        let extra_start = name_start + name_len;
        let extra = bytes
            .get(extra_start..extra_start + extra_len)
            .ok_or_else(|| "truncated zip extra field".to_string())?;
        if compressed == u64::from(u32::MAX) || uncompressed == u64::from(u32::MAX) {
            // numpy forces zip64 in `savez`: the real sizes are in the extra.
            let sizes = zip64_sizes(extra)?;
            if uncompressed == u64::from(u32::MAX) {
                uncompressed = sizes.0;
            }
            if compressed == u64::from(u32::MAX) {
                compressed = sizes.1;
            }
        }
        let data_start = extra_start + extra_len;
        let data_end = data_start
            .checked_add(usize::try_from(compressed).map_err(|_| "zip entry too large")?)
            .ok_or_else(|| "zip entry too large".to_string())?;
        let data = bytes
            .get(data_start..data_end)
            .ok_or_else(|| "truncated zip entry".to_string())?;
        if data.len() as u64 != uncompressed {
            return Err(format!("zip entry {name} has an unexpected size"));
        }
        entries.push((name, data.to_vec()));
        offset = data_end;
    }
    if entries.is_empty() {
        return Err("no entries in the npz".to_string());
    }
    Ok(entries)
}

/// `(uncompressed, compressed)` of the zip64 extra field.
fn zip64_sizes(extra: &[u8]) -> Result<(u64, u64), String> {
    let mut offset = 0_usize;
    while offset + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[offset], extra[offset + 1]]);
        let size = u16::from_le_bytes([extra[offset + 2], extra[offset + 3]]) as usize;
        let body = extra
            .get(offset + 4..offset + 4 + size)
            .ok_or_else(|| "truncated zip extra field".to_string())?;
        if id == 1 {
            let size_of = |index: usize| -> Result<u64, String> {
                let slice = body
                    .get(index..index + 8)
                    .ok_or_else(|| "truncated zip64 sizes".to_string())?;
                let mut bytes = [0_u8; 8];
                bytes.copy_from_slice(slice);
                Ok(u64::from_le_bytes(bytes))
            };
            return Ok((size_of(0)?, size_of(8)?));
        }
        offset += 4 + size;
    }
    Err("zip64 sizes are missing".to_string())
}

fn parse_npy(bytes: &[u8]) -> Result<Style, String> {
    if bytes.len() < 10 || &bytes[..6] != NPY_MAGIC {
        return Err("not a npy array".to_string());
    }
    let version = bytes[6];
    let (header_len, header_start) = if version == 1 {
        (u16_at(bytes, 8)? as usize, 10)
    } else {
        (u32_at(bytes, 8)? as usize, 12)
    };
    let header = bytes
        .get(header_start..header_start + header_len)
        .ok_or_else(|| "truncated npy header".to_string())?;
    let header = String::from_utf8_lossy(header);

    let descr = dict_value(&header, "'descr'").ok_or("npy header without descr")?;
    if descr != "<f4" && descr != "|f4" {
        return Err(format!("npy element type {descr} is not supported"));
    }
    let fortran = dict_value(&header, "'fortran_order'").ok_or("npy header without order")?;
    if fortran != "False" {
        return Err("Fortran-ordered npy arrays are not supported".to_string());
    }
    let shape_text = dict_value(&header, "'shape'").ok_or("npy header without shape")?;
    let shape = parse_shape(&shape_text)?;
    let total = shape
        .iter()
        .try_fold(1_usize, |total, dimension| total.checked_mul(*dimension));
    let Some(total) = total.filter(|total| *total > 0) else {
        return Err("npy array is empty or too large".to_string());
    };
    let rows = shape.first().copied().unwrap_or(1).max(1);
    if !total.is_multiple_of(rows) {
        return Err("npy shape is not a multiple of its first dimension".to_string());
    }
    let cols = total / rows;

    let payload = bytes
        .get(header_start + header_len..)
        .ok_or_else(|| "truncated npy payload".to_string())?;
    let expected = total
        .checked_mul(4)
        .ok_or_else(|| "npy array is too large".to_string())?;
    if payload.len() != expected {
        return Err(format!(
            "npy payload is {} bytes, expected {expected}",
            payload.len()
        ));
    }
    let data = payload
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    Ok(Style { rows, cols, data })
}

/// Value of a `'key': value` pair of the npy header dict.
fn dict_value(header: &str, key: &str) -> Option<String> {
    let start = header.find(key)? + key.len();
    let rest = header.get(start..)?;
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    if let Some(rest) = rest.strip_prefix('\'') {
        return rest.find('\'').map(|end| rest[..end].to_string());
    }
    if let Some(rest) = rest.strip_prefix('(') {
        // A tuple: commas inside it do not end the dict entry.
        return rest.find(')').map(|end| format!("({}", &rest[..end]));
    }
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn parse_shape(text: &str) -> Result<Vec<usize>, String> {
    let mut shape = Vec::new();
    let mut digits = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
        } else if !digits.is_empty() {
            shape.push(digits.parse::<usize>().map_err(|_| "bad npy shape")?);
            digits.clear();
        }
    }
    if !digits.is_empty() {
        shape.push(digits.parse::<usize>().map_err(|_| "bad npy shape")?);
    }
    Ok(shape)
}

#[path = "voices/tests.rs"]
#[cfg(test)]
mod tests;
