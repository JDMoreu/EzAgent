//! Plan de arranque de `whisper-server`, sin lanzar nada (DX-3b).
//!
//! Uso: `cargo run -p ez-stt --example whisper_plan [-- <puerto>]`
//!
//! Lee `%APPDATA%\EzAgent\voice.json` (motor y modelo) y la instalación real de
//! whisper. El puerto por omisión es 0, un marcador: `ensure_whisper` elige uno
//! libre al lanzar. Con un puerto explícito, la salida debe coincidir con el
//! proceso real salvo por ese número.

use std::io::{stderr, stdout, Write};
use std::process::ExitCode;

use ez_core::config::VoiceConfig;

fn main() -> ExitCode {
    let port = match std::env::args().nth(1) {
        Some(text) => match text.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                let _ = writeln!(stderr(), "puerto no válido: {text}");
                return ExitCode::FAILURE;
            }
        },
        None => 0,
    };
    let config = match VoiceConfig::load(None) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(stderr(), "No pude leer voice.json: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(plan) = ez_stt::server::launch_plan(&config, port) else {
        let _ = writeln!(
            stderr(),
            "No puedo planear el arranque: falta whisper-server.exe o el modelo. \
             Revisa whisper_home y stt_model en voice.json."
        );
        return ExitCode::FAILURE;
    };
    let mut out = stdout();
    let _ = writeln!(out, "ejecutable: {}", plan.executable.display());
    let _ = writeln!(out, "argumentos: {}", plan.args.join(" "));
    ExitCode::SUCCESS
}
