//! Cliente HTTP de `ez` (diseño §7): `reqwest::blocking`, sin reintentos.

use std::time::Duration;

use crate::protocol::{Reply, Request};

const BAD_ANSWER: &str = "EzAgent devolvió una respuesta que no entiendo.";
const UNAUTHORIZED: &str = "Token inválido o ausente.";

/// Por qué no hubo una [`Reply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    /// No se pudo abrir la conexión con EzAgent.
    Unreachable,
    /// El servidor rechazó el token (401) con este mensaje.
    Unauthorized(String),
    /// Otro rechazo del servidor, con el mensaje que lo explica.
    Failed(String),
}

/// Envía `request` a `url` con `token` y devuelve la respuesta del árbitro.
pub fn send(
    url: &str,
    token: &str,
    request: &Request,
    timeout: Duration,
) -> Result<Reply, ClientError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|_| ClientError::Unreachable)?;
    let endpoint = format!("{}/v1/command", url.trim_end_matches('/'));
    let response = client
        .post(endpoint)
        .bearer_auth(token)
        .json(request)
        .send()
        .map_err(|_| ClientError::Unreachable)?;
    let status = response.status().as_u16();
    let body = response.text().map_err(|_| ClientError::Unreachable)?;
    let reply = serde_json::from_str::<Reply>(&body).ok();
    match status {
        200 => reply.ok_or_else(|| ClientError::Failed(BAD_ANSWER.to_string())),
        401 => Err(ClientError::Unauthorized(
            reply.map_or_else(|| UNAUTHORIZED.to_string(), |reply| reply.message),
        )),
        _ => Err(ClientError::Failed(reply.map_or_else(
            || format!("EzAgent respondió con el estado {status}."),
            |reply| reply.message,
        ))),
    }
}
