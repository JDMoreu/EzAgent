//! Validación de destino de las URLs web (ST-21) y filtro de IPs.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;

use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{is_py_space, py_strip};
use reqwest::Url;

use super::WebToolError;

const NAT64_96: u128 = 0x0064_ff9b_0000_0000_0000_0000_0000_0000;
const NAT64_48: u128 = 0x0064_ff9b_0001_0000_0000_0000_0000_0000;
const SIX_TO_FOUR: u128 = 0x2002_0000_0000_0000_0000_0000_0000_0000;
const TEREDO: u128 = 0x2001_0000_0000_0000_0000_0000_0000_0000;

/// Destino ya validado: URL original, host a fijar (si lo hay), puerto e IPs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTarget {
    pub url: String,
    pub host: Option<String>,
    pub port: u16,
    pub addresses: Vec<IpAddr>,
}

pub(super) fn blocks_private_ips(config: &AgentConfig) -> bool {
    config.web.block_private_ips
}

/// `True` si la IP no es global o encapsula una IPv4 no global.
pub fn is_blocked_ip(address: IpAddr) -> bool {
    if !is_global(address) || is_multicast(address) {
        return true;
    }
    let IpAddr::V6(ipv6) = address else {
        return false;
    };
    if let Some(mapped) = ipv6.to_ipv4_mapped() {
        if !is_global_v4(mapped) {
            return true;
        }
    }
    let value = u128::from(ipv6);
    // DECISION: NAT64 y 6to4 pueden encapsular una IPv4 privada aunque la
    // IPv6 tenga pinta de global.
    if in_v6(value, NAT64_96, 96) || in_v6(value, NAT64_48, 48) {
        let embedded = Ipv4Addr::from((value & 0xffff_ffff) as u32);
        if !is_global_v4(embedded) {
            return true;
        }
    }
    if in_v6(value, SIX_TO_FOUR, 16) {
        let embedded = Ipv4Addr::from(((value >> 80) & 0xffff_ffff) as u32);
        if !is_global_v4(embedded) {
            return true;
        }
    }
    // Teredo ofusca la IPv4 del cliente en los últimos 32 bits (XOR).
    if in_v6(value, TEREDO, 32) {
        let embedded = Ipv4Addr::from(((value & 0xffff_ffff) ^ 0xffff_ffff) as u32);
        if !is_global_v4(embedded) {
            return true;
        }
    }
    false
}

fn is_multicast(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ipv4) => ipv4.is_multicast(),
        IpAddr::V6(ipv6) => ipv6
            .to_ipv4_mapped()
            .map_or_else(|| ipv6.is_multicast(), |mapped| mapped.is_multicast()),
    }
}

fn is_global(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ipv4) => is_global_v4(ipv4),
        IpAddr::V6(ipv6) => match ipv6.to_ipv4_mapped() {
            Some(mapped) => is_global_v4(mapped),
            None => !is_private_v6(ipv6),
        },
    }
}

fn is_global_v4(address: Ipv4Addr) -> bool {
    !in_v4(address, [100, 64, 0, 0], 10) && !is_private_v4(address)
}

/// Redes no globales de `ipaddress` (CPython 3.11).
fn is_private_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, d] = address.octets();
    if a == 192 && b == 0 && c == 0 && (d == 9 || d == 10) {
        return false;
    }
    a == 0
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 240
}

fn is_private_v6(address: Ipv6Addr) -> bool {
    let value = u128::from(address);
    if in_v6(value, 0, 128) || in_v6(value, 1, 128) {
        return true;
    }
    if in_v6(value, NAT64_48, 48) || in_v6(value, 0x0100_u128 << 112, 64) {
        return true;
    }
    if in_v6(value, 0x2001_0000_u128 << 96, 23) {
        let exceptions = [
            (0x2001_0001_0000_0001_u128 << 64, 128),
            (0x2001_0001_0000_0002_u128 << 64, 128),
            (0x2001_0003_u128 << 96, 32),
            (0x2001_0004_0112_u128 << 80, 48),
            (0x2001_0020_u128 << 96, 28),
            (0x2001_0030_u128 << 96, 28),
        ];
        if !exceptions
            .iter()
            .any(|(network, prefix)| in_v6(value, *network, *prefix))
        {
            return true;
        }
    }
    in_v6(value, 0x2001_0db8_u128 << 96, 32)
        || in_v6(value, SIX_TO_FOUR, 16)
        || in_v6(value, 0x3fff_u128 << 112, 20)
        || in_v6(value, 0xfc00_u128 << 112, 7)
        || in_v6(value, 0xfe80_u128 << 112, 10)
}

fn in_v4(address: Ipv4Addr, network: [u8; 4], prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32))
    };
    (u32::from(address) & mask) == (u32::from(Ipv4Addr::from(network)) & mask)
}

fn in_v6(address: u128, network: u128, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix.min(128))
    };
    (address & mask) == (network & mask)
}

/// Valida esquema y destino; devuelve la URL original y las IPs comprobadas.
pub fn validate_public_target(
    url: &str,
    config: &AgentConfig,
) -> Result<PublicTarget, WebToolError> {
    let raw_url = py_strip(url);
    if raw_url.is_empty() {
        return Err(WebToolError::unsafe_url("La URL no puede estar vacía."));
    }
    if raw_url
        .chars()
        .any(|character| is_py_space(character) || (character as u32) < 0x20)
    {
        return Err(WebToolError::unsafe_url(
            "La URL contiene espacios o caracteres de control.",
        ));
    }
    let scheme = url_scheme(raw_url);
    if scheme != "http" && scheme != "https" {
        return Err(WebToolError::unsafe_url(
            "Solo se permiten URL con esquema http:// o https://.",
        ));
    }
    let Some(authority) = url_authority(raw_url, &scheme) else {
        return Err(WebToolError::unsafe_url(
            "La URL no incluye un nombre de host válido.",
        ));
    };
    if authority.is_empty() {
        return Err(WebToolError::unsafe_url(
            "La URL no incluye un nombre de host válido.",
        ));
    }
    // DECISION: credenciales embebidas se rechazan; no hacen falta para leer
    // páginas y facilitan el engaño. Se mira la autoridad cruda porque
    // `Url::username()` no distingue `http://@host/` de una URL sin usuario.
    if authority.contains('@') {
        return Err(WebToolError::unsafe_url(
            "No se permiten credenciales dentro de la URL.",
        ));
    }
    let parts = Url::parse(raw_url).map_err(|error| url_error_text(&error.to_string()))?;
    let raw_host = parts.host_str().unwrap_or_default();
    let host = raw_host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(raw_host);
    if host.is_empty() {
        return Err(WebToolError::unsafe_url(
            "La URL no incluye un nombre de host válido.",
        ));
    }
    let default_port = if scheme == "https" { 443 } else { 80 };
    let port = parts.port().unwrap_or(default_port);
    if !blocks_private_ips(config) {
        // DECISION: block_private_ips permite desactivar el filtro de hosts;
        // el esquema http(s) se sigue exigiendo siempre.
        return Ok(PublicTarget {
            url: raw_url.to_string(),
            host: None,
            port,
            addresses: Vec::new(),
        });
    }
    validate_public_host(raw_url, host, port)
}

fn validate_public_host(
    raw_url: &str,
    host: &str,
    port: u16,
) -> Result<PublicTarget, WebToolError> {
    let host_clean = host.trim_end_matches('.').to_lowercase();
    if host_clean == "localhost" || host_clean.ends_with(".localhost") {
        return Err(WebToolError::unsafe_url(format!(
            "No se permite acceder a direcciones locales: «{host}»."
        )));
    }
    let literal = host_clean.split('%').next().unwrap_or_default();
    if let Ok(address) = IpAddr::from_str(literal) {
        ensure_public_ip(address, host)?;
        return Ok(PublicTarget {
            url: raw_url.to_string(),
            host: None,
            port,
            addresses: vec![address],
        });
    }
    let addresses = resolve_ips(&host_clean, port, host)?;
    for address in &addresses {
        ensure_public_ip(*address, host)?;
    }
    Ok(PublicTarget {
        url: raw_url.to_string(),
        host: Some(host_clean),
        port,
        addresses,
    })
}

/// Autoridad cruda (`[user@]host[:port]`) entre `://` y `/`, `?` o `#`.
fn url_authority<'a>(raw_url: &'a str, scheme: &str) -> Option<&'a str> {
    let rest = raw_url.get(scheme.len()..)?.strip_prefix("://")?;
    Some(rest.split(['/', '?', '#']).next().unwrap_or_default())
}

fn url_scheme(raw_url: &str) -> String {
    let Some((candidate, _)) = raw_url.split_once(':') else {
        return String::new();
    };
    let mut characters = candidate.chars();
    let valid = characters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if valid {
        candidate.to_ascii_lowercase()
    } else {
        String::new()
    }
}

fn url_error_text(text: &str) -> WebToolError {
    if text == "invalid port number" {
        return WebToolError::unsafe_url("La URL contiene un puerto no válido.");
    }
    if text == "empty host" {
        return WebToolError::unsafe_url("La URL no incluye un nombre de host válido.");
    }
    WebToolError::unsafe_url("La URL no es válida.")
}

/// Sitio al que se conecta la petición, con la IP ya validada cuando toca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestTarget {
    pub url: String,
    pub pin: Option<(String, SocketAddr)>,
}

pub(super) fn request_target(
    url: &str,
    config: &AgentConfig,
) -> Result<RequestTarget, WebToolError> {
    let target = validate_public_target(url, config)?;
    let Some(host) = target.host else {
        return Ok(RequestTarget {
            url: target.url,
            pin: None,
        });
    };
    if target.addresses.is_empty() {
        return Ok(RequestTarget {
            url: target.url,
            pin: None,
        });
    }
    // DECISION: se prefiere una IPv4 validada para no perder conectividad en
    // equipos sin IPv6; cualquiera de la lista ya pasó el filtro.
    let address = target
        .addresses
        .iter()
        .find(|address| address.is_ipv4())
        .copied()
        .unwrap_or(target.addresses[0]);
    Ok(RequestTarget {
        url: target.url,
        // DECISION: en vez de reescribir la URL a la IP y añadir Host/SNI, se
        // fija la resolución del cliente a la IP validada; reqwest conserva
        // Host y SNI del nombre original (equivalente a F36).
        pin: Some((host, SocketAddr::new(address, target.port))),
    })
}

fn ensure_public_ip(address: IpAddr, host: &str) -> Result<(), WebToolError> {
    if is_blocked_ip(address) {
        return Err(WebToolError::unsafe_url(format!(
            "El host «{host}» apunta a una dirección local, privada o reservada ({address}); acceso bloqueado."
        )));
    }
    Ok(())
}

fn resolve_ips(host: &str, port: u16, original: &str) -> Result<Vec<IpAddr>, WebToolError> {
    let Ok(infos) = (host, port).to_socket_addrs() else {
        return Err(unresolved(original));
    };
    let addresses: Vec<IpAddr> = infos.map(|info| info.ip()).collect();
    if addresses.is_empty() {
        return Err(unresolved(original));
    }
    Ok(addresses)
}

fn unresolved(host: &str) -> WebToolError {
    WebToolError::unsafe_url(format!("No se pudo resolver el nombre de host «{host}»."))
}

/// Valida esquema y destino; resuelve DNS y rechaza IPs no globales (ST-21).
pub fn validate_public_url(url: &str, config: &AgentConfig) -> Result<String, WebToolError> {
    validate_public_target(url, config).map(|target| target.url)
}
