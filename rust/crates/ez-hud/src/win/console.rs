//! Consola de la app instalada (F8).
//!
//! El ejecutable de la versión Python se empaqueta sin consola (`console=False`
//! en `EzAgent.spec`): abierto desde el menú Inicio no muestra ninguna ventana
//! negra. El de Rust se compila con el subsistema de ventanas en `release` y,
//! para que las órdenes de terminal (`--list-mics`, `--help`, `downloads`…)
//! sigan mostrando su salida, se engancha a la consola de quien lo lanzó.

use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};

/// Se engancha a la consola del proceso padre si existe (terminal, `.bat`).
///
/// Lanzada desde el Explorador o un acceso directo no hay consola padre y no se
/// crea ninguna. Si las salidas ya vienen redirigidas (tuberías de `subprocess`
/// o de las pruebas), Windows las conserva: enganchar la consola no las cambia.
/// Con `EZAGENT_CONSOLE=1` se abre una consola propia para ver los registros
/// (lo usa `EzAgent.bat` en desarrollo).
pub fn attach_parent_console() -> bool {
    // SAFETY: llamada Win32 sin punteros; falla sin efectos si no hay consola
    // padre o si el proceso ya tiene una.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok();
    if attached {
        return true;
    }
    if std::env::var("EZAGENT_CONSOLE").is_ok_and(|value| value == "1") {
        // SAFETY: llamada Win32 sin punteros; crea una consola si no hay otra.
        return unsafe { AllocConsole() }.is_ok();
    }
    false
}
