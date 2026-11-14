//! `is_local_endpoint`: `urlsplit(...).hostname` de Python 3.12, lo justo.

use std::net::Ipv6Addr;

const LOCAL_HOSTS: [&str; 4] = ["127.0.0.1", "localhost", "::1", "0.0.0.0"];

/// True si la URL apunta a esta máquina (Ollama, LM Studio, vLLM…).
///
/// DECISION: no se porta `_checknetloc` (normalización NFKC de hosts no ASCII):
/// ningún host local lo necesita y un host así no estaría en `LOCAL_HOSTS`.
pub fn is_local_endpoint(base_url: &str) -> bool {
    match hostname(base_url) {
        Ok(Some(host)) => LOCAL_HOSTS.contains(&host.to_lowercase().as_str()),
        Ok(None) | Err(InvalidUrl) => false,
    }
}

/// `ValueError` de `urlsplit` (por ejemplo, un IPv6 entre corchetes inválido).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidUrl;

/// Lo que el resto del código necesita de `urlsplit(url)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts {
    /// En minúsculas (`""` si no hay).
    pub scheme: String,
    pub netloc: String,
    /// `hostname` de Python (sin minúsculas; `None` si está vacío).
    pub hostname: Option<String>,
    /// `username is not None`: hay un `@` en el netloc.
    pub has_userinfo: bool,
}

/// `urlsplit(url)`; `Err` donde Python lanza `ValueError`.
pub fn urlsplit(url: &str) -> Result<UrlParts, InvalidUrl> {
    let url = url.trim_start_matches(|c: char| c <= ' ');
    let url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let (scheme, rest) = split_scheme(&url);
    let empty = UrlParts {
        scheme: scheme.to_lowercase(),
        netloc: String::new(),
        hostname: None,
        has_userinfo: false,
    };
    let Some(after) = rest.strip_prefix("//") else {
        return Ok(empty);
    };
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let netloc = &after[..end];
    let has_open = netloc.contains('[');
    let has_close = netloc.contains(']');
    if has_open != has_close {
        return Err(InvalidUrl);
    }
    if has_open {
        let bracketed = netloc.split_once('[').map_or("", |(_, tail)| tail);
        let bracketed = bracketed
            .split_once(']')
            .map_or(bracketed, |(head, _)| head);
        check_bracketed_host(bracketed)?;
    }
    let hostinfo = netloc.rsplit_once('@').map_or(netloc, |(_, tail)| tail);
    let host = match hostinfo.split_once('[') {
        Some((_, bracketed)) => bracketed
            .split_once(']')
            .map_or(bracketed, |(head, _)| head),
        None => hostinfo.split_once(':').map_or(hostinfo, |(head, _)| head),
    };
    Ok(UrlParts {
        netloc: netloc.to_string(),
        hostname: (!host.is_empty()).then(|| host.to_string()),
        has_userinfo: netloc.contains('@'),
        ..empty
    })
}

/// `urlsplit(url).hostname`; `Err` donde Python lanza `ValueError`.
fn hostname(url: &str) -> Result<Option<String>, InvalidUrl> {
    Ok(urlsplit(url)?.hostname)
}

/// `(scheme, resto)`: el esquema solo cuenta si es ASCII válido antes de `:`.
fn split_scheme(url: &str) -> (&str, &str) {
    let Some(colon) = url.find(':') else {
        return ("", url);
    };
    let head = &url[..colon];
    let starts_alpha = head.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    let valid = head
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if colon > 0 && starts_alpha && valid {
        (head, &url[colon + 1..])
    } else {
        ("", url)
    }
}

/// `_check_bracketed_host`: IPv6 (con zona opcional) o IPvFuture.
fn check_bracketed_host(host: &str) -> Result<(), InvalidUrl> {
    if let Some(rest) = host.strip_prefix('v') {
        let (hex, tail) = rest.split_once('.').ok_or(InvalidUrl)?;
        let hex_ok = !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
        return if hex_ok && !tail.is_empty() {
            Ok(())
        } else {
            Err(InvalidUrl)
        };
    }
    let (address, zone) = match host.split_once('%') {
        Some((address, zone)) => (address, Some(zone)),
        None => (host, None),
    };
    if zone.is_some_and(|zone| zone.is_empty() || zone.contains('%')) {
        return Err(InvalidUrl);
    }
    address
        .parse::<Ipv6Addr>()
        .map(|_| ())
        .map_err(|_| InvalidUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert!(is_local_endpoint("http://[::1]:8080/v1"));
        assert!(is_local_endpoint("http://LOCALHOST:11434/v1"));
        assert!(!is_local_endpoint("http://[oops"));
        assert!(!is_local_endpoint("file:///tmp/x"));
        assert!(!is_local_endpoint("no es una url"));
        assert!(!is_local_endpoint("localhost:11434"));
    }
}
