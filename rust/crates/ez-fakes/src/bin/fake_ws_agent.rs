//! Binario `fake-ws-agent`: agente falso sobre una terminal de hc (ECO-8).

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(ez_fakes::fake_ws_agent::USAGE.as_bytes());
        return ExitCode::SUCCESS;
    }
    match ez_fakes::fake_ws_agent::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "fake-ws-agent: {error}");
            ExitCode::FAILURE
        }
    }
}
