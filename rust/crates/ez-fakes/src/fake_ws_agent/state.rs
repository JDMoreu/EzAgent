//! Registro de estado OSC 7501 que entiende `hc` (ECO-8).

use std::io::Write;

/// Anuncia el estado del agente falso para que `hc` lo muestre y `terminal.send`
/// pueda saber si la primera línea fue recibida (diseño §14.2).
pub(crate) fn announce(out: &mut dyn Write, state: &str) -> Result<(), String> {
    let record = format!("\x1b]7501;:state={state}:app=ez-fake\x1b\\");
    out.write_all(record.as_bytes())
        .map_err(|error| format!("no pude anunciar el estado: {error}"))?;
    out.flush()
        .map_err(|error| format!("no pude vaciar stdout: {error}"))
}

#[cfg(test)]
mod tests {
    use super::announce;

    #[test]
    fn the_record_is_the_format_confirmed_in_hc() {
        let mut out = Vec::new();
        announce(&mut out, "idle").expect("anuncio");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "\x1b]7501;:state=idle:app=ez-fake\x1b\\"
        );
    }
}
