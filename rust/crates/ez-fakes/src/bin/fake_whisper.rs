//! Binario `fake-whisper`: imita `whisper-server` en 127.0.0.1 (F1-11).

use std::process::ExitCode;

const USAGE: &str = "\
fake-whisper: imita whisper-server (POST /inference y /v1/audio/transcriptions).

Uso:
  fake-whisper [--scenario RUTA] [--port N]

Opciones:
  --scenario RUTA  Escenario JSON con «transcripts» (en orden) y «by_hash».
  --port N         Puerto en 127.0.0.1 (defecto: 8088).

La app Python se apunta con WHISPER_URL=http://127.0.0.1:8088/inference
(o el puerto elegido) y auto_start_whisper=false en voice.json.
GET /health responde {\"status\":\"ok\"}.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = ez_fakes::fake_whisper::Options::parse(&args).and_then(|options| {
        let scenario = match options.scenario.as_deref() {
            Some(path) => ez_fakes::fake_whisper::Scenario::load(path)?,
            None => ez_fakes::fake_whisper::Scenario::default(),
        };
        let server = ez_fakes::fake_whisper::bind(options.port)?;
        let port = server
            .server_addr()
            .to_ip()
            .map_or(options.port, |address| address.port());
        println!("fake-whisper escuchando en http://127.0.0.1:{port}/inference");
        ez_fakes::fake_whisper::serve(
            &server,
            &mut ez_fakes::fake_whisper::Transcripts::new(scenario),
        )
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fake-whisper: {error}");
            ExitCode::FAILURE
        }
    }
}
