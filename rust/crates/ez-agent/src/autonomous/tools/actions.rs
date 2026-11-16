//! `autonomous_status`, `stop_autonomous_task` y `resume_autonomous_task` (10.5).

use serde_json::{json, Map, Value};

use crate::registry::ToolResult;

use super::super::state::TaskState;
use super::super::texts;
use super::super::AutonomousHost;
use super::{flag, integer, text};

pub fn status(host: &dyn AutonomousHost) -> ToolResult {
    let Some(summary) = host.status() else {
        return ToolResult::ok(json!({"active": false, "message": texts::NO_TASK}));
    };
    let message = summary.describe();
    ToolResult::ok(json!({
        "active": summary.state.unfinished(),
        "id": summary.id,
        "mode": summary.mode.as_str(),
        "state": summary.state.as_str(),
        "round": summary.round,
        "max_rounds": summary.max_rounds,
        "approvals": summary.approvals,
        "clean_streak": summary.clean_streak,
        "workdir": summary.workdir,
        "accepted": summary.accepted,
        "pause_reason": summary.pause_reason,
        "message": message,
    }))
}

pub fn stop(host: &dyn AutonomousHost, args: &Map<String, Value>) -> ToolResult {
    let accept = flag(args, "accept");
    let reason = text(args, "reason");
    let Some(summary) = host.status() else {
        return ToolResult::ok(
            json!({"stopped": false, "accepted": false, "message": texts::NO_TASK}),
        );
    };
    if summary.state == TaskState::Done {
        if !accept {
            return ToolResult::ok(
                json!({"stopped": false, "accepted": summary.accepted, "message": texts::NO_TASK}),
            );
        }
        return match host.stop(&reason, true) {
            Ok(()) => ToolResult::ok(
                json!({"stopped": false, "accepted": true, "message": texts::ACCEPTED}),
            ),
            Err(error) => ToolResult::err(error),
        };
    }
    if !summary.state.unfinished() {
        return ToolResult::ok(
            json!({"stopped": false, "accepted": false, "message": texts::NO_TASK}),
        );
    }
    match host.stop(&reason, false) {
        Ok(()) => {
            ToolResult::ok(json!({"stopped": true, "accepted": false, "message": texts::STOPPED}))
        }
        Err(error) => ToolResult::err(error),
    }
}

pub fn resume(host: &dyn AutonomousHost, args: &Map<String, Value>) -> ToolResult {
    let Some(summary) = host.status() else {
        return ToolResult::err(texts::NO_TASK);
    };
    if summary.state != TaskState::Paused {
        return ToolResult::err(texts::NOT_PAUSED);
    }
    let extend_rounds = integer(args, "extend_rounds", 0);
    let extend_minutes = integer(args, "extend_minutes", 0);
    match host.resume(extend_rounds, extend_minutes) {
        Ok(()) => ToolResult::ok(json!({"resumed": true, "message": texts::RESUMED})),
        Err(error) => ToolResult::err(error),
    }
}
