//! Port of `src/ezagent/voice/redact.py` (F2-06).
//!
//! Un solo filtro para todos los canales de diagnóstico: claves de API, tokens
//! bearer, credenciales en cabeceras o consultas, contraseñas en URLs y tokens
//! largos en hex/base64.

use std::sync::OnceLock;

use fancy_regex::{Captures, Regex};

type Caps<'t> = Captures<'t, str>;

const REDACTED: &str = "[REDACTED]";

const KEY_NAME: &str =
    r"((?:x[-_])?api[-_]?key|access[-_]?token|auth[-_]?token|token|password|passwd|secret)";

macro_rules! pattern {
    ($name:ident, $source:expr) => {
        fn $name() -> &'static Regex {
            static SLOT: OnceLock<Regex> = OnceLock::new();
            SLOT.get_or_init(|| Regex::new($source).expect("patrón interno válido"))
        }
    };
}

pattern!(
    url_credentials,
    r"(?i)\b([a-z][a-z0-9+.-]*://)[^\s/:@]+:[^\s/@]+@"
);
pattern!(sk_token, r"\bsk-[A-Za-z0-9][A-Za-z0-9_-]{2,}");
pattern!(bearer, r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+");
pattern!(long_hex, r"\b[A-Fa-f0-9]{32,}\b");
pattern!(
    long_token,
    r"(?<![A-Za-z0-9+/_-])(?=[A-Za-z0-9+/_-]*\d)(?=[A-Za-z0-9+/_-]*[A-Za-z])[A-Za-z0-9+/_-]{32,}={0,2}(?![A-Za-z0-9+/_=-])"
);

fn key_value_quoted() -> &'static Regex {
    static SLOT: OnceLock<Regex> = OnceLock::new();
    SLOT.get_or_init(|| {
        let source = format!(r#"(?i)\b{KEY_NAME}(\s*[:=]\s*)(["'])(.*?)\3"#);
        Regex::new(&source).expect("patrón interno válido")
    })
}

fn key_value() -> &'static Regex {
    static SLOT: OnceLock<Regex> = OnceLock::new();
    SLOT.get_or_init(|| {
        let source = format!(r#"(?i)\b{KEY_NAME}(\s*[:=]\s*)(["']?)(?!\[REDACTED\])([^\s"',;&]+)"#);
        Regex::new(&source).expect("patrón interno válido")
    })
}

fn group<'t>(caps: &Caps<'t>, index: usize) -> &'t str {
    caps.get(index).map_or("", |capture| capture.as_str())
}

fn redact_pair(caps: &Caps<'_>) -> String {
    let (name, separator, quote) = (group(caps, 1), group(caps, 2), group(caps, 3));
    format!("{name}{separator}{quote}{REDACTED}{quote}")
}

/// Devuelve `text` con las credenciales sustituidas por `[REDACTED]`.
pub fn redact(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let text = url_credentials().replace_all(text, |caps: &Caps<'_>| {
        format!("{}{REDACTED}@", group(caps, 1))
    });
    let text = sk_token().replace_all(&text, REDACTED);
    let text = bearer().replace_all(&text, concat!("Bearer ", "[REDACTED]"));
    let text = key_value_quoted().replace_all(&text, redact_pair);
    let text = key_value().replace_all(&text, redact_pair);
    let text = long_hex().replace_all(&text, REDACTED);
    long_token().replace_all(&text, REDACTED).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_text_without_secrets() {
        assert_eq!(redact(""), "");
        assert_eq!(redact("texto normal"), "texto normal");
    }

    #[test]
    fn redacts_known_channels() {
        assert_eq!(redact("sk-abc123def456"), REDACTED);
        assert_eq!(redact("api_key=abc123"), "api_key=[REDACTED]");
        assert_eq!(
            redact("https://user:pass@example.com/x"),
            "https://[REDACTED]@example.com/x"
        );
        assert_eq!(redact("api_key=[REDACTED]"), "api_key=[REDACTED]");
    }
}
