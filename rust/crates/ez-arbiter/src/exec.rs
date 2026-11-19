//! Subprocesos sin shell y sin ventana de consola.

use std::process::Command;

/// Comando con argumentos en lista; en Windows sin consola (`CREATE_NO_WINDOW`).
pub fn command(program: &str) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
