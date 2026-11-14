//! Unit tests for `voices.rs` (F4-04).

use super::*;

fn npy(shape: &[usize], values: &[f32]) -> Vec<u8> {
    let shape_text = if shape.len() == 1 {
        format!("({},)", shape[0])
    } else {
        let parts: Vec<String> = shape.iter().map(usize::to_string).collect();
        format!("({})", parts.join(", "))
    };
    let dict = format!("{{'descr': '<f4', 'fortran_order': False, 'shape': {shape_text}, }}");
    let mut header = dict.into_bytes();
    let padding = (64 - (10 + header.len() + 1) % 64) % 64;
    header.extend(std::iter::repeat_n(b' ', padding));
    header.push(b'\n');

    let mut out = Vec::new();
    out.extend_from_slice(NPY_MAGIC);
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&(header.len() as u16).to_le_bytes());
    out.extend_from_slice(&header);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn zip(entries: &[(&str, Vec<u8>)], method: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        out.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        out.extend_from_slice(&20_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    let central_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&END_OF_CENTRAL.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

fn sample_voices() -> Voices {
    let bytes = zip(
        &[
            (
                "ef_dora.npy",
                npy(&[3, 1, 2], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            ),
            (
                "em_alex.npy",
                npy(&[3, 1, 2], &[3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
            ),
        ],
        0,
    );
    Voices::from_npz(&bytes).expect("npz sintético válido")
}

#[test]
fn reads_shape_and_values() {
    let voices = sample_voices();
    assert_eq!(voices.len(), 2);
    assert_eq!(voices.names(), vec!["ef_dora", "em_alex"]);
    let style = voices.get("ef_dora").expect("voz presente");
    assert_eq!(style.rows(), 3);
    assert_eq!(style.cols(), 2);
    assert_eq!(style.row(1), Some(&[3.0_f32, 4.0][..]));
    assert_eq!(style.row(3), None);
    assert!(!voices.is_empty());
}

#[test]
fn single_voice_is_returned_as_is() {
    let voices = sample_voices();
    let style = voices.voice_style("ef_dora").expect("voz válida");
    assert_eq!(style.data(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn combined_voices_are_averaged() {
    let voices = sample_voices();
    let style = voices.voice_style("ef_dora + em_alex").expect("dos voces");
    assert_eq!(style.data(), &[2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    let partial = voices
        .voice_style("ef_dora+desconocida")
        .expect("una válida");
    assert_eq!(partial.data(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn no_valid_voice_uses_the_python_message() {
    let voices = sample_voices();
    let error = voices.voice_style("nadie+tu").expect_err("sin voces");
    assert_eq!(error, "Ninguna voz de Kokoro válida en 'nadie+tu'");
    let error = voices.voice_style("   ").expect_err("sin voces");
    assert_eq!(error, "Ninguna voz de Kokoro válida en '   '");
}

#[test]
fn compressed_entries_are_rejected() {
    let bytes = zip(&[("v.npy", npy(&[1, 1, 1], &[0.0]))], 8);
    let error = Voices::from_npz(&bytes).expect_err("comprimido");
    assert!(error.contains("compression method 8"));
}

#[test]
fn foreign_dtypes_and_order_are_rejected() {
    let mut bytes = zip(&[("v.npy", npy(&[1, 1, 1], &[0.0]))], 0);
    let position = bytes
        .windows(3)
        .position(|window| window == b"<f4")
        .expect("descr en la cabecera");
    bytes[position] = b'<';
    bytes[position + 1] = b'f';
    bytes[position + 2] = b'8';
    assert!(Voices::from_npz(&bytes)
        .expect_err("f8")
        .contains("not supported"));

    let mut bytes = zip(&[("v.npy", npy(&[1, 1, 1], &[0.0]))], 0);
    let position = bytes
        .windows(6)
        .position(|window| window == b"False,")
        .expect("fortran_order en la cabecera");
    bytes[position] = b'T';
    bytes[position + 1] = b'r';
    bytes[position + 2] = b'u';
    bytes[position + 3] = b'e';
    bytes[position + 4] = b' ';
    assert!(Voices::from_npz(&bytes)
        .expect_err("fortran")
        .contains("Fortran"));
}

#[test]
fn truncated_and_empty_files_are_rejected() {
    assert!(Voices::from_npz(b"nada").is_err());
    assert!(Voices::from_npz(&zip(&[], 0)).is_err());
    let bytes = zip(&[("v.npy", npy(&[1, 1, 1], &[0.0]))], 0);
    assert!(Voices::from_npz(&bytes[..20]).is_err());
}

#[test]
fn reads_the_installed_voices_file_when_present() {
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let path = std::path::PathBuf::from(base)
        .join("EzAgent")
        .join("models")
        .join("kokoro")
        .join("voices-v1.0.bin");
    if !path.is_file() {
        return;
    }
    let voices = Voices::load(&path).expect("voces instaladas");
    assert_eq!(voices.len(), 54);
    let style = voices.get("ef_dora").expect("ef_dora");
    assert_eq!((style.rows(), style.cols()), (510, 256));
}
