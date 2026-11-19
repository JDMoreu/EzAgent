//! La tabla de resultados: cada comprobación con su PASS o FAIL.

use serde_json::{json, Map, Value};

/// Una comprobación de la prueba.
#[derive(Debug, Clone)]
pub struct Check {
    /// Qué se comprobó.
    pub name: String,
    /// Si pasó.
    pub ok: bool,
    /// La evidencia (o el motivo del fallo).
    pub detail: String,
}

/// Todo lo que la prueba Miranda.
#[derive(Debug, Default)]
pub struct Report {
    /// Las comprobaciones, en orden.
    pub checks: Vec<Check>,
    /// Los datos que se copian al JSON final.
    pub extra: Map<String, Value>,
}

impl Report {
    /// Una comprobación nueva.
    pub fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.to_string(),
            ok,
            detail: detail.into(),
        });
    }

    /// Una comprobación que pasó.
    pub fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.check(name, true, detail);
    }

    /// Una comprobación que falló.
    pub fn fail(&mut self, name: &str, detail: impl Into<String>) {
        self.check(name, false, detail);
    }

    /// ¿Pasó todo?
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|check| check.ok)
    }

    /// Cuántas comprobaciones fallaron.
    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|check| !check.ok).count()
    }

    /// La tabla legible.
    pub fn table(&self) -> String {
        let mut lines = Vec::new();
        for check in &self.checks {
            lines.push(format!(
                "{:<4}  {:<52}  {}",
                if check.ok { "PASS" } else { "FAIL" },
                check.name,
                check.detail
            ));
        }
        lines.push(format!(
            "{} comprobaciones: {} pasan, {} fallan.",
            self.checks.len(),
            self.checks.len() - self.failed(),
            self.failed()
        ));
        lines.join("\n")
    }

    /// El JSON con todo lo que se comprobó.
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
