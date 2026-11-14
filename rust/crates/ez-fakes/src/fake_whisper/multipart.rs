//! Lector mínimo de `multipart/form-data`, el formato que manda `httpx` a
//! `whisper-server` (`files={"file": ("utterance.wav", bytes, "audio/wav")}`).

/// Una parte del formulario: nombre, nombre de archivo y bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

/// `boundary` del `Content-Type`, si es un formulario multipart.
pub fn boundary(content_type: &str) -> Option<String> {
    let mut pieces = content_type.split(';');
    let kind = pieces.next()?.trim().to_lowercase();
    if kind != "multipart/form-data" {
        return None;
    }
    for piece in pieces {
        let (key, value) = piece.split_once('=')?;
        if key.trim().eq_ignore_ascii_case("boundary") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn split_headers(part: &[u8]) -> Option<(&[u8], &[u8])> {
    let separator = find(part, b"\r\n\r\n")?;
    Some((&part[..separator], &part[separator + 4..]))
}

fn param(header: &str, key: &str) -> Option<String> {
    for piece in header.split(';').skip(1) {
        let Some((name, value)) = piece.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

fn parse_part(raw: &[u8]) -> Result<Part, String> {
    let (head, data) = split_headers(raw).ok_or_else(|| "parte sin cabeceras".to_string())?;
    let head = String::from_utf8_lossy(head);
    let mut name = String::new();
    let mut filename = None;
    let mut content_type = None;
    for line in head.split("\r\n") {
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        if field.trim().eq_ignore_ascii_case("content-disposition") {
            name = param(value, "name").unwrap_or_default();
            filename = param(value, "filename");
        } else if field.trim().eq_ignore_ascii_case("content-type") {
            content_type = Some(value.trim().to_string());
        }
    }
    Ok(Part {
        name,
        filename,
        content_type,
        data: data.to_vec(),
    })
}

/// Divide el cuerpo en partes; un `boundary` ausente o un cierre roto es error.
pub fn parse(body: &[u8], boundary: &str) -> Result<Vec<Part>, String> {
    let delimiter = format!("--{boundary}").into_bytes();
    if !body.starts_with(&delimiter) {
        return Err("el cuerpo no empieza por el boundary".to_string());
    }
    let boundary_bytes = boundary.as_bytes();
    let mut cursor = delimiter.len();
    let mut parts = Vec::new();
    loop {
        if body[cursor..].starts_with(b"--") {
            break;
        }
        if body[cursor..].starts_with(b"\r\n") {
            cursor += 2;
        }
        let marker = b"\r\n--";
        let end = find(&body[cursor..], marker)
            .ok_or_else(|| "el formulario no tiene cierre".to_string())?;
        parts.push(parse_part(&body[cursor..cursor + end])?);
        cursor += end + 2;
        if !body[cursor..].starts_with(b"--") {
            return Err("boundary intermedio inválido".to_string());
        }
        cursor += 2;
        if !body[cursor..].starts_with(boundary_bytes) {
            return Err("boundary intermedio inválido".to_string());
        }
        cursor += boundary_bytes.len();
    }
    Ok(parts)
}

/// La parte del archivo: la que se llama `file` o la primera con `filename`.
pub fn file_part(parts: &[Part]) -> Option<&Part> {
    parts
        .iter()
        .find(|part| part.name == "file")
        .or_else(|| parts.iter().find(|part| part.filename.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(boundary: &str) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"model\"\r\n\r\nggml-large-v3\r\n",
        );
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"utterance.wav\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
        body.extend_from_slice(b"RIFF\x00\x01\xffWAVE");
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[test]
    fn reads_boundary_from_content_type() {
        let value = "multipart/form-data; boundary=\"abc-123\"";
        assert_eq!(boundary(value).as_deref(), Some("abc-123"));
        assert_eq!(boundary("application/json"), None);
        assert_eq!(boundary("multipart/form-data"), None);
    }

    #[test]
    fn splits_fields_and_preserves_binary_file() {
        let parts = parse(&form("xyz"), "xyz").expect("formulario");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "model");
        assert_eq!(parts[0].data, b"ggml-large-v3");
        let file = file_part(&parts).expect("archivo");
        assert_eq!(file.name, "file");
        assert_eq!(file.filename.as_deref(), Some("utterance.wav"));
        assert_eq!(file.content_type.as_deref(), Some("audio/wav"));
        assert_eq!(file.data, b"RIFF\x00\x01\xffWAVE");
    }

    #[test]
    fn rejects_broken_forms() {
        assert!(parse(b"sin boundary", "xyz").is_err());
        let mut body = form("xyz");
        body.truncate(body.len() - 6);
        assert!(parse(&body, "xyz").is_err());
        assert!(parse(&form("xyz"), "otro").is_err());
    }

    #[test]
    fn content_type_boundary_parameter_is_optional() {
        assert_eq!(boundary("multipart/form-data; charset=utf-8"), None);
        assert_eq!(
            boundary("Multipart/Form-Data; boundary=zzz").as_deref(),
            Some("zzz")
        );
    }
}
