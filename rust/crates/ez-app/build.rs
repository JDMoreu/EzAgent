//! Recursos de Windows del ejecutable: icono y datos de versión (F8-01).
//!
//! Se escribe el `.res` a mano (formato binario de recursos de Windows) y se
//! pasa al enlazador con `cargo:rustc-link-arg`. Así el icono y la pestaña
//! «Detalles» del `.exe` no dependen de `rc.exe` ni del SDK de Windows: el
//! recurso que también usa `packaging/EzAgent.iss` es el único origen del icono.
//!
//! DECISION: la versión del ejecutable es 0.2.0 (`MyAppVersion` del instalador)
//! y no la del crate (0.1.0 en todo el workspace): cambiarla en `Cargo.toml`
//! alteraría textos ya portados (`ez-tts` publica su versión de crate).

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Idioma de los recursos: en-US (0x0409), página de códigos Unicode (1200).
const LANGUAGE: u16 = 0x0409;
const CODEPAGE: u16 = 1200;
/// `MOVEABLE | PURE`; los iconos añaden `DISCARDABLE`, como los escribe `rc.exe`.
const FLAGS_COMMON: u16 = 0x0030;
const FLAGS_ICON: u16 = 0x1010;
const FLAGS_GROUP_ICON: u16 = 0x1030;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;

const VERSION: &str = "0.2.0";
const COMPANY: &str = "EzAgent contributors";
const PRODUCT: &str = "EzAgent";
const DESCRIPTION: &str = "EzAgent Voice — mensajero para Herdr";
const INTERNAL_NAME: &str = "ezagent-voice";
const ORIGINAL_NAME: &str = "ezagent-voice.exe";

/// Un recurso del `.res`: tipo, nombre, banderas y datos (todo ordinal).
struct Resource {
    kind: u16,
    name: u16,
    flags: u16,
    data: Vec<u8>,
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let icon = icon_path(&manifest);
    emit_rerun(&manifest, &icon);
    let mut resources = Vec::new();
    match icon.as_deref() {
        Some(path) => match std::fs::read(path).map_err(|error| error.to_string()) {
            Ok(bytes) => match icon_resources(&bytes) {
                Ok(mut icons) => resources.append(&mut icons),
                Err(error) => warn(&format!("icono ignorado ({}): {error}", path.display())),
            },
            Err(error) => warn(&format!("icono ignorado ({}): {error}", path.display())),
        },
        None => warn("no hay packaging/icon.ico; el ejecutable sale sin icono"),
    }
    resources.push(Resource {
        kind: RT_VERSION,
        name: 1,
        flags: FLAGS_COMMON,
        data: version_info(),
    });
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it")).join("ezagent.res");
    let bytes = write_res(&resources);
    // Autocomprobación: un `.res` con tamaños incoherentes hace fallar el enlace.
    if let Err(error) = validate_res(&bytes) {
        panic!("el .res generado no cuadra: {error}");
    }
    match std::fs::write(&out, bytes) {
        Ok(()) => {}
        Err(error) => panic!("no pude escribir {}: {error}", out.display()),
    }
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "cargo:rustc-link-arg={}", out.display());
}

/// `packaging/icon.ico` (el mismo del instalador), si existe.
fn icon_path(manifest: &Path) -> Option<PathBuf> {
    let path = manifest
        .join("..")
        .join("..")
        .join("..")
        .join("packaging")
        .join("icon.ico");
    path.is_file().then_some(path)
}

fn emit_rerun(manifest: &Path, icon: &Option<PathBuf>) {
    let mut stdout = std::io::stdout();
    let script = manifest.join("build.rs");
    let _ = writeln!(stdout, "cargo:rerun-if-changed={}", script.display());
    if let Some(path) = icon {
        let _ = writeln!(stdout, "cargo:rerun-if-changed={}", path.display());
    }
}

fn warn(message: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "cargo:warning={message}");
}

/// Un `.res`: registro nulo inicial y, por recurso, tamaño de datos, tamaño de
/// cabecera (que incluye los 8 bytes de esos dos tamaños) y cabecera.
///
/// El registro nulo de `rc.exe` (tipo 0, nombre 0, sin datos) es obligatorio:
/// `link.exe` rechaza el archivo con `LNK1136` si falta.
fn write_res(resources: &[Resource]) -> Vec<u8> {
    let mut out = vec![0_u8; 8];
    out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&[0_u8; 16]);
    let null_header = out.len() as u32;
    out[4..8].copy_from_slice(&null_header.to_le_bytes());
    for resource in resources {
        out.extend_from_slice(&(resource.data.len() as u32).to_le_bytes());
        let mut header = Vec::new();
        push_name(&mut header, resource.kind);
        push_name(&mut header, resource.name);
        header.extend_from_slice(&0_u32.to_le_bytes()); // DataVersion
        header.extend_from_slice(&resource.flags.to_le_bytes());
        header.extend_from_slice(&LANGUAGE.to_le_bytes());
        header.extend_from_slice(&0_u32.to_le_bytes()); // Version
        header.extend_from_slice(&0_u32.to_le_bytes()); // Characteristics
        out.extend_from_slice(&((header.len() + 8) as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&resource.data);
        align4(&mut out);
    }
    out
}

/// Recorre el `.res` y comprueba que cada registro (cabecera y datos) cabe.
fn validate_res(bytes: &[u8]) -> Result<(), String> {
    let mut offset = 0_usize;
    let mut records = 0_usize;
    while offset < bytes.len() {
        let size = read_u32(bytes, offset)? as usize;
        let header = read_u32(bytes, offset + 4)? as usize;
        if header < 24 {
            return Err(format!("cabecera de {header} bytes en {offset}"));
        }
        let end = offset + header + size;
        if end > bytes.len() {
            return Err(format!("registro en {offset} se pasa del archivo"));
        }
        records += 1;
        offset = end.div_ceil(4) * 4;
    }
    if offset > bytes.len() || bytes.len() - offset > 3 {
        return Err(format!("cierre en {offset} de {} bytes", bytes.len()));
    }
    if records < 2 {
        return Err("faltan recursos en el archivo".to_string());
    }
    Ok(())
}

fn push_name(out: &mut Vec<u8>, name: u16) {
    out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
    out.extend_from_slice(&name.to_le_bytes());
}

fn align4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Texto UTF-16LE terminado en NUL.
fn utf16z(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

/// Bloque `VS_VERSIONINFO` genérico: cabecera, clave, valor y hijos.
fn block(key: &str, value: &[u8], value_len: u16, kind: u16, children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0_u16.to_le_bytes()); // wLength, al final
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&utf16z(key));
    align4(&mut out);
    out.extend_from_slice(value);
    align4(&mut out);
    for child in children {
        out.extend_from_slice(child);
    }
    let length = u16::try_from(out.len()).unwrap_or(u16::MAX);
    out[..2].copy_from_slice(&length.to_le_bytes());
    out
}

/// `VS_FIXEDFILEINFO` con la versión del producto.
fn fixed_file_info() -> Vec<u8> {
    let parts: Vec<u32> = VERSION
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    let major = parts.first().copied().unwrap_or(0);
    let minor = parts.get(1).copied().unwrap_or(0);
    let build = parts.get(2).copied().unwrap_or(0);
    let revision = parts.get(3).copied().unwrap_or(0);
    let version_ms = (major << 16) | minor;
    let version_ls = (build << 16) | revision;
    let fields: [u32; 13] = [
        0xFEEF_04BD, // dwSignature
        0x0001_0000, // dwStrucVersion
        version_ms,
        version_ls,
        version_ms, // dwProductVersionMS
        version_ls,
        0x3F,        // dwFileFlagsMask
        0,           // dwFileFlags
        0x0004_0004, // dwFileOS = VOS_NT_WINDOWS32
        1,           // dwFileType = VFT_APP
        0,           // dwFileSubtype
        0,           // dwFileDateMS
        0,           // dwFileDateLS
    ];
    let mut out = Vec::with_capacity(52);
    for field in fields {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out
}

/// Un `String` de la tabla: clave y valor UTF-16 con su longitud en palabras.
fn version_string(key: &str, value: &str) -> Vec<u8> {
    let encoded = utf16z(value);
    let words = u16::try_from(encoded.len() / 2).unwrap_or(0);
    block(key, &encoded, words, 1, &[])
}

fn string_table(entries: &[(&str, &str)]) -> Vec<u8> {
    let children: Vec<Vec<u8>> = entries
        .iter()
        .map(|(key, value)| version_string(key, value))
        .collect();
    block(
        &format!("{LANGUAGE:04X}{CODEPAGE:04X}"),
        &[],
        0,
        1,
        &children,
    )
}

fn var_file_info() -> Vec<u8> {
    let mut translation = Vec::new();
    translation.extend_from_slice(&LANGUAGE.to_le_bytes());
    translation.extend_from_slice(&CODEPAGE.to_le_bytes());
    let var = block("Translation", &translation, 4, 0, &[]);
    block("VarFileInfo", &[], 0, 1, &[var])
}

/// Recurso `RT_VERSION` completo (`VS_VERSION_INFO`).
fn version_info() -> Vec<u8> {
    let entries = [
        ("CompanyName", COMPANY),
        ("FileDescription", DESCRIPTION),
        ("FileVersion", VERSION),
        ("InternalName", INTERNAL_NAME),
        ("LegalCopyright", "Apache-2.0"),
        ("OriginalFilename", ORIGINAL_NAME),
        ("ProductName", PRODUCT),
        ("ProductVersion", VERSION),
    ];
    let strings = block("StringFileInfo", &[], 0, 1, &[string_table(&entries)]);
    block(
        "VS_VERSION_INFO",
        &fixed_file_info(),
        52,
        0,
        &[strings, var_file_info()],
    )
}

/// Un `(ancho, alto, colores, reservado, planos, bits, bytes, offset)` del `.ico`.
struct IconEntry {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    bytes: u32,
    offset: u32,
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let slice = bytes.get(at..at + 2).ok_or("icono truncado")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes.get(at..at + 4).ok_or("icono truncado")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Entradas del `.ico` (formato `ICONDIR` + `ICONDIRENTRY`).
fn icon_entries(bytes: &[u8]) -> Result<Vec<IconEntry>, String> {
    if read_u16(bytes, 0)? != 0 || read_u16(bytes, 2)? != 1 {
        return Err("no es un archivo .ico".to_string());
    }
    let count = read_u16(bytes, 4)? as usize;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let at = 6 + index * 16;
        let fields = bytes.get(at..at + 16).ok_or("icono truncado")?;
        entries.push(IconEntry {
            width: fields[0],
            height: fields[1],
            colors: fields[2],
            planes: u16::from_le_bytes([fields[4], fields[5]]),
            bits: u16::from_le_bytes([fields[6], fields[7]]),
            bytes: read_u32(bytes, at + 8)?,
            offset: read_u32(bytes, at + 12)?,
        });
    }
    Ok(entries)
}

/// `GRPICONDIR` con los mismos campos que el `.ico`, más el id de cada imagen.
fn group_icon(entries: &[IconEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0_u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1_u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (index, entry) in entries.iter().enumerate() {
        out.push(entry.width);
        out.push(entry.height);
        out.push(entry.colors);
        out.push(0);
        out.extend_from_slice(&entry.planes.to_le_bytes());
        out.extend_from_slice(&entry.bits.to_le_bytes());
        out.extend_from_slice(&entry.bytes.to_le_bytes());
        out.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }
    out
}

/// Recursos `RT_ICON` (una por tamaño) y `RT_GROUP_ICON` (id 1).
fn icon_resources(bytes: &[u8]) -> Result<Vec<Resource>, String> {
    let entries = icon_entries(bytes)?;
    if entries.is_empty() {
        return Err("el .ico está vacío".to_string());
    }
    let mut resources = Vec::with_capacity(entries.len() + 1);
    for (index, entry) in entries.iter().enumerate() {
        let start = entry.offset as usize;
        let end = start + entry.bytes as usize;
        let data = bytes.get(start..end).ok_or("icono truncado")?.to_vec();
        resources.push(Resource {
            kind: RT_ICON,
            name: (index + 1) as u16,
            flags: FLAGS_ICON,
            data,
        });
    }
    resources.push(Resource {
        kind: RT_GROUP_ICON,
        name: 1,
        flags: FLAGS_GROUP_ICON,
        data: group_icon(&entries),
    });
    Ok(resources)
}
