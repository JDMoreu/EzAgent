//! Comando `ez` (lo implementa ECO-3).

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let env = |name: &str| std::env::var(name).ok();
    let code = ez_ctl::main_with(
        &argv,
        &env,
        &cwd,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    ExitCode::from(code as u8)
}
