//! Result table of the end-to-end inventory test.
//!
//! Every check prints `OK` or `FALLO`; the last line is the `N/N OK` count the
//! brief asks for. Names and details are user-facing and therefore in neutral
//! Spanish.

use serde_json::{json, Map, Value};

/// One check of the test.
#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// Everything the test checked, plus the data copied to the result JSON.
#[derive(Debug, Default)]
pub struct Report {
    pub checks: Vec<Check>,
    pub extra: Map<String, Value>,
}

impl Report {
    /// Records a check with its result.
    pub fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.to_string(),
            ok,
            detail: detail.into(),
        });
    }

    /// A check that passed.
    pub fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.check(name, true, detail);
    }

    /// A check that failed.
    pub fn fail(&mut self, name: &str, detail: impl Into<String>) {
        self.check(name, false, detail);
    }

    /// Whether every check passed.
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|check| check.ok)
    }

    /// How many checks failed.
    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|check| !check.ok).count()
    }

    /// The human-readable table, with the final `N/N OK` line.
    pub fn table(&self) -> String {
        let mut lines = Vec::with_capacity(self.checks.len() + 1);
        for check in &self.checks {
            lines.push(format!(
                "{:<5}  {:<56}  {}",
                if check.ok { "OK" } else { "FALLO" },
                check.name,
                check.detail
            ));
        }
        let total = self.checks.len();
        let passed = total - self.failed();
        let mut summary = format!("{passed}/{total} OK");
        if self.failed() > 0 {
            summary.push_str(&format!(" ({} FALLO)", self.failed()));
        }
        lines.push(summary);
        lines.join("\n")
    }

    /// The JSON with every check and the extra data.
    pub fn to_value(&self) -> Value {
        let checks: Vec<Value> = self
            .checks
            .iter()
            .map(|check| json!({"name": check.name, "ok": check.ok, "detail": check.detail}))
            .collect();
        let mut value = Map::new();
        value.insert("ok".to_string(), Value::Bool(self.ok()));
        value.insert("checks".to_string(), Value::Array(checks));
        value.insert("failed".to_string(), json!(self.failed()));
        for (key, item) in &self.extra {
            value.insert(key.clone(), item.clone());
        }
        Value::Object(value)
    }
}
