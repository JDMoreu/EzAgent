//! Port of `src/ezagent/agent/policy.py` (F2-04).
//!
//! Es la única puerta que decide si una herramienta puede ejecutarse. La
//! función es total: no valida los argumentos, solo decide.

/// `Decision.action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Allow,
    Confirm,
    ConfirmStrong,
    Deny,
}

impl Action {
    /// Texto tal como lo escriben los fixtures y la auditoría.
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Allow => "allow",
            Action::Confirm => "confirm",
            Action::ConfirmStrong => "confirm_strong",
            Action::Deny => "deny",
        }
    }
}

/// `policy.Decision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub action: Action,
    pub level: i64,
    pub reason: &'static str,
}

/// `bulk_threshold` por defecto de `decide`.
pub const DEFAULT_BULK_THRESHOLD: i64 = 5;

/// `decide(...)` con el umbral de fábrica.
pub fn decide(
    tool_level: i64,
    tainted: bool,
    bulk: i64,
    always_confirm: bool,
    confirm_nondestructive: bool,
) -> Decision {
    decide_with_threshold(
        tool_level,
        tainted,
        bulk,
        always_confirm,
        confirm_nondestructive,
        DEFAULT_BULK_THRESHOLD,
    )
}

/// `decide(..., bulk_threshold)` completo.
pub fn decide_with_threshold(
    tool_level: i64,
    tainted: bool,
    bulk: i64,
    always_confirm: bool,
    confirm_nondestructive: bool,
    bulk_threshold: i64,
) -> Decision {
    // always_confirm: borrado, sobrescritura, scripts y todo lo que salga de
    // write_roots.
    if tool_level >= 3 {
        return Decision {
            action: Action::Deny,
            level: 3,
            reason: "Nivel 3: prohibido",
        };
    }
    if always_confirm {
        let action = if bulk > bulk_threshold {
            Action::ConfirmStrong
        } else {
            Action::Confirm
        };
        return Decision {
            action,
            level: 2,
            reason: "Operación destructiva",
        };
    }
    if bulk > bulk_threshold {
        return Decision {
            action: Action::ConfirmStrong,
            level: 2,
            reason: "Operación masiva",
        };
    }
    if tainted && tool_level >= 1 {
        return Decision {
            action: Action::Confirm,
            level: tool_level,
            reason: "Contenido no confiable en este turno",
        };
    }
    if tool_level == 2 {
        let action = if confirm_nondestructive {
            Action::Confirm
        } else {
            Action::Allow
        };
        return Decision {
            action,
            level: 2,
            reason: "Nivel 2",
        };
    }
    Decision {
        action: Action::Allow,
        level: tool_level,
        reason: "Permitido",
    }
}

/// La política no añade su confirmación a una herramienta que ya pide la suya.
///
/// Solo se usa con `self_confirmed` (10.5): las herramientas de arranque piden
/// la confirmación única del modo autónomo y no deben preguntar dos veces.
pub fn with_self_confirmed(decision: Decision, self_confirmed: bool) -> Decision {
    if !self_confirmed {
        return decision;
    }
    match decision.action {
        Action::Confirm | Action::ConfirmStrong => Decision {
            action: Action::Allow,
            level: decision.level,
            reason: "La herramienta pide su propia confirmación",
        },
        other => Decision {
            action: other,
            ..decision
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_three_always_denies() {
        let decision = decide(3, false, 0, false, true);
        assert_eq!(decision.action, Action::Deny);
        assert_eq!(decision.level, 3);
    }

    #[test]
    fn bulk_at_threshold_is_not_massive() {
        assert_eq!(decide(0, false, 5, false, false).action, Action::Allow);
        assert_eq!(
            decide(0, false, 6, false, false).action,
            Action::ConfirmStrong
        );
    }

    #[test]
    fn self_confirmed_skips_only_the_policy_confirmation() {
        let confirm = decide(2, false, 0, false, true);
        assert_eq!(confirm.action, Action::Confirm);
        assert_eq!(
            with_self_confirmed(confirm, true).action,
            Action::Allow,
            "la herramienta ya pide la suya"
        );
        assert_eq!(with_self_confirmed(confirm, false).action, Action::Confirm);
        let denied = decide(3, false, 0, false, false);
        assert_eq!(
            with_self_confirmed(denied, true).action,
            Action::Deny,
            "el nivel 3 sigue prohibido"
        );
    }
}
