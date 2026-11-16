//! La aprobación como transacción (10.7 y 10.8).
//!
//! S1 = leer y clasificar; esperar `APPROVE_SETTLE_S`; S2 = leer y comprobar que
//! la huella no cambió; comprobar la generación; enviar las teclas; S3 = leer
//! en menos de 2 s y confirmar que el diálogo desapareció. Cualquier fallo
//! pausa el modo; nunca se aprueba de más. Todo se hace sin cerrojos.

use std::path::PathBuf;

use ez_agent::autonomous::audit::{log_approval, ApprovalRecord};
use ez_agent::autonomous::texts;
use ez_agent::autonomous::ScopeRole;
use ez_herdr::permission::{
    approval_granted, classify, decide, same_dialog, screen_fingerprint, Decision, Dialog,
    DialogKind, Role, Scope, APPROVE_SETTLE_S,
};

use super::{AutonomousLoop, Session, FINGERPRINTS, GRANT_READ_S};

/// Resultado de intentar aprobar un diálogo.
pub enum Approval {
    Approved,
    /// No hay nada que aprobar o ya se aprobó ese diálogo.
    Ignored,
    /// Hay que pausar con este motivo.
    Paused(String),
    /// La generación cambió (stop o pausa): no se envió nada.
    Aborted,
}

/// Un diálogo bloqueado de una dirección de `scope`.
pub fn approve_blocked(loop_: &AutonomousLoop, session: &mut Session, address: &str) -> Approval {
    let screen = match loop_.port.read_agent(address, Some(60)) {
        Ok(screen) => screen,
        Err(_) => return Approval::Paused(texts::REASON_READ.to_string()),
    };
    let dialog = classify(&screen);
    if dialog.kind == DialogKind::None {
        return Approval::Ignored;
    }
    let Some(scope) = scope_for(session, address, true) else {
        return Approval::Ignored;
    };
    let decision = decide(&dialog, &scope);
    match decision {
        Decision::Ignore => Approval::Ignored,
        Decision::Ask(reason) => {
            deny(loop_, session, address, &dialog, &reason);
            Approval::Paused(reason)
        }
        Decision::Approve(keys) => perform(loop_, session, address, &screen, &dialog, &keys),
    }
}

/// 10.2/10.9: si la terminal quedó en el diálogo de confianza, se aprueba solo
/// cuando la carpeta está dentro de `workdir`; si no, pausa con el motivo.
///
/// La usan el lanzamiento del ejecutor, el del verificador y los miembros del
/// equipo autónomo (que se abren desde el enjambre).
pub fn approve_launch(loop_: &AutonomousLoop, session: &mut Session, address: &str) -> Approval {
    let Ok(screen) = loop_.port.read_agent(address, Some(60)) else {
        return Approval::Ignored;
    };
    let dialog = classify(&screen);
    if dialog.kind != DialogKind::Trust {
        return Approval::Ignored;
    }
    let Some(scope) = scope_for(session, address, false) else {
        return Approval::Ignored;
    };
    match decide(&dialog, &scope) {
        Decision::Ignore => Approval::Ignored,
        Decision::Ask(reason) => {
            deny(loop_, session, address, &dialog, &reason);
            Approval::Paused(reason)
        }
        Decision::Approve(keys) => perform(loop_, session, address, &screen, &dialog, &keys),
    }
}

/// 10.9: si el lanzamiento falló por el diálogo de confianza, se aprueba.
pub fn approve_trust(loop_: &AutonomousLoop, session: &mut Session, address: &str) -> bool {
    matches!(approve_launch(loop_, session, address), Approval::Approved)
}

/// Alcance del papel, con las rutas ya canónicas (10.1).
fn scope_for(session: &Session, address: &str, require_running: bool) -> Option<Scope> {
    let entry = if require_running {
        session.state.approval_scope(address)?
    } else {
        session
            .state
            .scope
            .iter()
            .find(|entry| entry.address == address)?
    };
    Some(Scope {
        in_scope: true,
        role: role_of(entry.role),
        workdir: Some(PathBuf::from(&session.state.workdir)),
        edit_dirs: entry
            .edit_dirs
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    })
}

fn role_of(role: ScopeRole) -> Role {
    match role {
        ScopeRole::Executor => Role::Executor,
        ScopeRole::Verifier => Role::Verifier,
        ScopeRole::Member => Role::Member,
    }
}

/// Los cinco pasos de 10.7.
fn perform(
    loop_: &AutonomousLoop,
    session: &mut Session,
    address: &str,
    first: &str,
    dialog: &Dialog,
    keys: &[String],
) -> Approval {
    let fingerprint = screen_fingerprint(first);
    if session.fingerprints.contains(&fingerprint) {
        return Approval::Ignored;
    }
    let generation = loop_.generation();
    loop_.clock.sleep(APPROVE_SETTLE_S);
    let Ok(second) = loop_.port.read_agent(address, Some(60)) else {
        return Approval::Paused(texts::REASON_READ.to_string());
    };
    if !same_dialog(first, &second) {
        return Approval::Paused(texts::REASON_DIALOG_CHANGED.to_string());
    }
    if loop_.generation() != generation {
        return Approval::Aborted;
    }
    if loop_.port.keys_agent(address, keys).is_err() {
        return Approval::Paused(texts::REASON_KEYS.to_string());
    }
    if let Some(reason) = confirm_granted(loop_, address) {
        return Approval::Paused(reason);
    }
    remember(session, fingerprint);
    record(
        loop_,
        session,
        address,
        dialog,
        &Decision::Approve(keys.to_vec()),
        "approve",
    );
    let action = session.state.record_approval(loop_.now());
    match action {
        ez_agent::autonomous::Action::Pause(reason) => Approval::Paused(reason),
        _ => Approval::Approved,
    }
}

/// S3: el diálogo desapareció y no quedó una concesión permanente (≤ 2 s).
fn confirm_granted(loop_: &AutonomousLoop, address: &str) -> Option<String> {
    let deadline = loop_.now() + GRANT_READ_S;
    loop {
        let screen = match loop_.port.read_agent(address, Some(60)) {
            Ok(screen) => screen,
            Err(_) => return Some(texts::REASON_READ.to_string()),
        };
        match approval_granted(&screen) {
            Ok(()) => return None,
            Err(reason) => {
                if loop_.now() >= deadline {
                    return Some(reason.to_string());
                }
            }
        }
        loop_.clock.sleep(0.1);
    }
}

fn remember(session: &mut Session, fingerprint: String) {
    session.fingerprints.push(fingerprint);
    if session.fingerprints.len() > FINGERPRINTS {
        session.fingerprints.remove(0);
    }
}

/// Auditoría sin pantallas y bitácora con la misma redacción (10.15).
fn record(
    loop_: &AutonomousLoop,
    session: &Session,
    address: &str,
    dialog: &Dialog,
    decision: &Decision,
    label: &str,
) {
    let record = ApprovalRecord {
        kind: kind_label(dialog.kind),
        option: chosen_option(dialog),
        command: dialog.command.as_deref(),
        path: dialog.path.as_deref(),
        address,
        decision: label,
    };
    let audit = loop_.audit.lock().clone();
    if let Some(audit) = audit {
        log_approval(&audit, session.id(), &record);
    }
    let event = if label == "approve" {
        ez_agent::autonomous::artifacts::Event::Approve
    } else {
        ez_agent::autonomous::artifacts::Event::Deny
    };
    loop_.log(session, event, bitacora_detail(dialog, decision, address));
}

/// `audit_summary` de `ez-herdr` con la dirección añadida (10.15).
fn bitacora_detail(dialog: &Dialog, decision: &Decision, address: &str) -> serde_json::Value {
    let text = ez_herdr::permission::audit_summary(dialog, decision);
    let mut value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "address".to_string(),
            serde_json::Value::String(address.to_string()),
        );
    }
    value
}

fn deny(loop_: &AutonomousLoop, session: &Session, address: &str, dialog: &Dialog, reason: &str) {
    record(
        loop_,
        session,
        address,
        dialog,
        &Decision::Ask(reason.to_string()),
        "deny",
    );
}

/// Texto de la opción elegida, si la secuencia empieza por su número.
fn chosen_option(dialog: &Dialog) -> Option<&str> {
    let first = dialog.approve_once.as_ref()?.first()?;
    dialog
        .options
        .iter()
        .find(|(key, _)| key == first)
        .map(|(_, text)| text.as_str())
}

fn kind_label(kind: DialogKind) -> &'static str {
    match kind {
        DialogKind::Exec => "exec",
        DialogKind::Edit => "edit",
        DialogKind::Trust => "trust",
        DialogKind::Other => "other",
        DialogKind::None => "none",
    }
}
