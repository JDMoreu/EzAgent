//! Esquemas de las herramientas del modo autónomo (10.2, 10.3 y 10.5).

use serde_json::{json, Value};

use super::super::texts;
use super::{
    MAX_CRITERIA, MAX_CRITERION_CHARS, MAX_MINUTES, MAX_ROLES, MAX_ROUNDS, MAX_TASK_CHARS,
    MIN_MINUTES, MIN_ROUNDS,
};

fn criterion_items() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_CRITERION_CHARS,
                "description": texts::CRITERION_TEXT_DESCRIPTION
            },
            "check": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_CRITERION_CHARS,
                "description": texts::CRITERION_CHECK_DESCRIPTION
            }
        },
        "required": ["text", "check"]
    })
}

fn task_field() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": MAX_TASK_CHARS,
        "description": texts::TASK_DESCRIPTION
    })
}

fn criteria_field() -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "maxItems": MAX_CRITERIA,
        "description": texts::CRITERIA_DESCRIPTION,
        "items": criterion_items()
    })
}

fn rounds_field() -> Value {
    json!({
        "type": "integer",
        "minimum": MIN_ROUNDS,
        "maximum": MAX_ROUNDS,
        "description": texts::ROUNDS_DESCRIPTION
    })
}

fn minutes_field() -> Value {
    json!({
        "type": "integer",
        "minimum": MIN_MINUTES,
        "maximum": MAX_MINUTES,
        "description": texts::MINUTES_DESCRIPTION
    })
}

fn workdir_field() -> Value {
    json!({"type": "string", "description": texts::WORKDIR_DESCRIPTION})
}

pub fn task_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": task_field(),
            "done_criteria": criteria_field(),
            "agent": {"type": "string", "description": texts::AGENT_DESCRIPTION},
            "space": {"type": "string", "description": texts::SPACE_DESCRIPTION},
            "workdir": workdir_field(),
            "terminal": {"type": "string", "description": texts::TERMINAL_DESCRIPTION},
            "verifier_agent": {
                "type": "string",
                "description": texts::VERIFIER_DESCRIPTION
            },
            "verifier_model": {
                "type": "string",
                "description": texts::VERIFIER_MODEL_DESCRIPTION
            },
            "max_rounds": rounds_field(),
            "max_minutes": minutes_field()
        },
        "required": ["task", "done_criteria", "agent", "space"]
    })
}

pub fn team_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": task_field(),
            "done_criteria": criteria_field(),
            "roles": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_ROLES,
                "description": texts::ROLES_DESCRIPTION,
                "items": {
                    "type": "object",
                    "properties": {
                        "role": {"type": "string"},
                        "agent": {"type": "string"},
                        "model": {"type": "string"}
                    },
                    "required": ["role", "agent"]
                }
            },
            "space": {"type": "string", "description": texts::SPACE_DESCRIPTION},
            "workdir": workdir_field(),
            "verifier_agent": {
                "type": "string",
                "description": texts::TEAM_VERIFIER_DESCRIPTION
            },
            "verifier_model": {
                "type": "string",
                "description": texts::VERIFIER_MODEL_DESCRIPTION
            },
            "max_rounds": rounds_field(),
            "max_minutes": minutes_field()
        },
        "required": ["task", "done_criteria", "roles", "space"]
    })
}

pub fn stop_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reason": {"type": "string", "description": texts::REASON_DESCRIPTION},
            "accept": {"type": "boolean", "description": texts::ACCEPT_DESCRIPTION}
        }
    })
}

pub fn resume_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "extend_rounds": {
                "type": "integer",
                "minimum": MIN_ROUNDS,
                "maximum": MAX_ROUNDS,
                "description": texts::EXTEND_ROUNDS_DESCRIPTION
            },
            "extend_minutes": {
                "type": "integer",
                "minimum": MIN_MINUTES,
                "maximum": MAX_MINUTES,
                "description": texts::EXTEND_MINUTES_DESCRIPTION
            }
        }
    })
}
