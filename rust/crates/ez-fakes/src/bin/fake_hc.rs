//! Binario `fake-hc`: imita `hc`/`herdr-app` con estado en un JSON (F1-11).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", ez_fakes::fake_hc::usage());
        return ExitCode::SUCCESS;
    }
    if args.is_empty() {
        eprint!("{}", ez_fakes::fake_hc::usage());
        return ExitCode::from(2);
    }
    let reply = ez_fakes::fake_hc::dispatch(&args);
    print!("{}", reply.stdout);
    eprint!("{}", reply.stderr);
    if reply.code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(reply.code.clamp(1, 255) as u8)
    }
}
