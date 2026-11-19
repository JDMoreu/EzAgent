//! Procesos `ez` del agente falso: sin shell y sin consola en Windows.

use std::process::Command;

/// Comando `ez` listo para recibir argumentos en lista.
pub(crate) fn ez() -> Command {
    let mut command = Command::new("ez");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
