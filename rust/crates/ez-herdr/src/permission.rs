//! Clasificador de diálogos de permiso y lista de denegación (AUTO-A2b).
//!
//! Implementa §10.7, §10.8 y §10.15 del diseño del modo autónomo: el filtro
//! de opciones es por palabra completa, `approve_once` es una secuencia de
//! teclas y `decide` recibe el papel con sus carpetas canónicas. Módulo puro:
//! no habla con Herdr, no abre hilos y no guarda pantallas.

use std::path::PathBuf;

mod audit;
mod deny;
mod dialogs;
#[cfg(test)]
mod tests;
mod transaction;

pub use audit::audit_summary;
pub use deny::{denied, denied_path};
pub use dialogs::classify;
pub use transaction::{
    approval_granted, same_dialog, screen_fingerprint, APPROVE_SETTLE_S, FINGERPRINT_LINES,
};

/// Lo que pide un diálogo de permiso.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DialogKind {
    /// Ejecutar un comando.
    Exec,
    /// Editar o escribir un archivo.
    Edit,
    /// Confiar en una carpeta (`_BLOCKING_DIALOG` del Python).
    Trust,
    /// Cualquier otro diálogo: se pregunta.
    Other,
    /// La pantalla no muestra un diálogo.
    #[default]
    None,
}

/// Diálogo leído de la pantalla de una terminal bloqueada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialog {
    pub kind: DialogKind,
    /// Comando pedido (`Exec`).
    pub command: Option<String>,
    /// Archivo destino (`Edit`).
    pub path: Option<String>,
    /// Carpeta del diálogo de confianza (`Trust`).
    pub folder: Option<String>,
    /// `(tecla, texto)` de las opciones numeradas o con flecha.
    pub options: Vec<(String, String)>,
    /// Secuencia de teclas que aprueba solo esta vez, si es única: el número
    /// de la opción o las flechas desde la marcada, más `enter`.
    pub approve_once: Option<Vec<String>>,
}

/// Papel de la terminal dentro de la tarea autónoma (§10.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Executor,
    Verifier,
    Member,
}

/// Alcance de una terminal, ya calculado por el bucle: si está autorizada, su
/// papel, la carpeta de trabajo y las carpetas donde puede editar (canónicas).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// La dirección está en `scope` y la tarea no está en pausa.
    pub in_scope: bool,
    pub role: Role,
    pub workdir: Option<PathBuf>,
    pub edit_dirs: Vec<PathBuf>,
}

/// Qué hacer con el diálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Enviar esta secuencia (`keys_agent(address, secuencia)`).
    Approve(Vec<String>),
    /// Pausar el modo autónomo y preguntar al usuario, con este motivo.
    Ask(String),
    /// El diálogo ya no está en pantalla: no hay nada que hacer.
    Ignore,
}

fn ask(reason: &str) -> Decision {
    Decision::Ask(reason.to_string())
}

/// Secuencia de la opción de una sola vez, o `Ask` si no es única.
fn approve(dialog: &Dialog) -> Decision {
    if let Some(sequence) = &dialog.approve_once {
        if !sequence.is_empty() {
            return Decision::Approve(sequence.clone());
        }
    }
    if dialogs::has_permanent(&dialog.options) {
        return ask("hay una opción permanente y no pude identificar la de una sola vez");
    }
    ask("no pude deducir una opción de permiso para esta vez")
}

fn decide_exec(dialog: &Dialog, scope: &Scope) -> Decision {
    let Some(command) = dialog.command.as_deref() else {
        return ask("no pude leer el comando del diálogo");
    };
    if let Some(reason) = denied(command) {
        return ask(reason);
    }
    let workdir = scope.workdir.as_deref();
    if let Some(reason) = deny::deletes_outside(command, &scope.edit_dirs, workdir) {
        return ask(reason);
    }
    approve(dialog)
}

fn decide_edit(dialog: &Dialog, scope: &Scope) -> Decision {
    let Some(path) = dialog.path.as_deref() else {
        return ask("no pude leer la ruta del archivo");
    };
    if let Some(reason) = denied_path(path) {
        return ask(reason);
    }
    let workdir = scope.workdir.as_deref();
    if !deny::path_allowed(path, &scope.edit_dirs, workdir) {
        return ask("la ruta está fuera de las carpetas permitidas para esta tarea");
    }
    approve(dialog)
}

fn decide_trust(dialog: &Dialog, scope: &Scope) -> Decision {
    if scope.role == Role::Verifier {
        return ask("un verificador no puede confiar carpetas");
    }
    let Some(folder) = dialog.folder.as_deref() else {
        return ask("no pude leer la carpeta del diálogo");
    };
    let Some(workdir) = scope.workdir.as_deref() else {
        return ask("no hay carpeta de trabajo para comparar la confianza");
    };
    if !deny::resolves_inside(folder, workdir, Some(workdir)) {
        return ask("la carpeta del diálogo no está dentro de la carpeta de trabajo");
    }
    approve(dialog)
}

/// Aplica las reglas de «Aprobar» de §10.7 y §10.8 a un diálogo clasificado.
///
/// `scope` ya viene calculado: el antirrebote (S1/S2) y la comprobación de S3
/// los hace el bucle con [`same_dialog`] y [`approval_granted`].
pub fn decide(dialog: &Dialog, scope: &Scope) -> Decision {
    match dialog.kind {
        DialogKind::None => Decision::Ignore,
        _ if !scope.in_scope => ask("la terminal no está en el alcance de la tarea"),
        DialogKind::Exec => decide_exec(dialog, scope),
        DialogKind::Edit => decide_edit(dialog, scope),
        DialogKind::Trust => decide_trust(dialog, scope),
        DialogKind::Other => ask("diálogo de permiso no reconocido"),
    }
}
