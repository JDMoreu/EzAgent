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
    fn matches_the_contract_oracle_over_the_whole_space() {
        fn oracle(
            level: i64,
            tainted: bool,
            bulk: i64,
            always: bool,
            nondestructive: bool,
        ) -> &'static str {
            if level >= 3 {
                "deny"
            } else if always {
                if bulk > 5 {
                    "confirm_strong"
                } else {
                    "confirm"
                }
            } else if bulk > 5 {
                "confirm_strong"
            } else if (tainted && level >= 1) || (level == 2 && nondestructive) {
                "confirm"
            } else {
                "allow"
            }
        }

        let mut count = 0;
        for level in 0..5 {
            for tainted in [false, true] {
                for bulk in [0, 1, 5, 6, 50] {
                    for always in [false, true] {
                        for nondestructive in [false, true] {
                            let decision = decide(level, tainted, bulk, always, nondestructive);
                            assert_eq!(
                                decision.action.as_str(),
                                oracle(level, tainted, bulk, always, nondestructive),
                                "{level}/{tainted}/{bulk}/{always}/{nondestructive}"
                            );
                            assert!(!decision.reason.is_empty());
                            count += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(count, 200);
        for level in [3, 4, 99] {
            for tainted in [false, true] {
                for bulk in [0, 100] {
                    for always in [false, true] {
                        for nondestructive in [false, true] {
                            assert_eq!(
                                decide(level, tainted, bulk, always, nondestructive).action,
                                Action::Deny
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn destructive_operations_always_confirm() {
        for level in 0..3 {
            for tainted in [false, true] {
                for bulk in [0, 1, 5, 6, 1000] {
                    for nondestructive in [false, true] {
                        let action = decide(level, tainted, bulk, true, nondestructive).action;
                        assert!(
                            action == Action::Confirm || action == Action::ConfirmStrong,
                            "{level}/{tainted}/{bulk}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn threshold_taint_and_level_two_follow_the_contract() {
        assert_eq!(decide(1, false, 5, false, false).action, Action::Allow);
        assert_eq!(
            decide(1, false, 6, false, false).action,
            Action::ConfirmStrong
        );
        assert_eq!(
            decide_with_threshold(1, false, 3, false, false, 2).action,
            Action::ConfirmStrong
        );
        assert_eq!(
            decide_with_threshold(1, false, 2, false, false, 2).action,
            Action::Allow
        );
        assert_eq!(
            decide_with_threshold(0, false, 1, false, false, 0).action,
            Action::ConfirmStrong
        );
        assert_eq!(decide(0, true, 0, false, false).action, Action::Allow);
        assert_eq!(decide(1, true, 0, false, false).action, Action::Confirm);
        assert_eq!(decide(2, true, 0, false, false).action, Action::Confirm);
        assert_eq!(decide(2, false, 0, false, false).action, Action::Allow);
        assert_eq!(decide(2, false, 0, false, true).action, Action::Confirm);
        assert_eq!(decide(1, false, 0, false, true).action, Action::Allow);
    }

    #[test]
    fn the_returned_level_names_the_tool_or_two_when_it_confirms() {
        assert_eq!(
            decide(0, false, 0, false, false),
            Decision {
                action: Action::Allow,
                level: 0,
                reason: "Permitido"
            }
        );
        assert_eq!(decide(1, true, 0, false, false).level, 1);
        assert_eq!(decide(0, false, 99, false, false).level, 2);
        assert_eq!(decide(1, false, 0, true, false).level, 2);
        assert_eq!(decide(3, false, 0, false, false).level, 3);
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
