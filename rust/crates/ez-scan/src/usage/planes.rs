//! `planes.toml`: lo que la persona declara sobre el plan y el límite de cada
//! agente (§5). Lo declarado gana sobre lo detectado.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use super::{UsageLimit, UsageReport};

pub const FILE_NAME: &str = "planes.toml";

/// Prioridad declarada frente a los demás agentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Primero,
    Normal,
    Reservar,
}

/// Lo declarado para un tipo de agente.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plane {
    pub plan: Option<String>,
    pub window_hours: Option<f64>,
    pub limit: Option<UsageLimit>,
    pub priority: Option<Priority>,
}

/// `[novedades]` del archivo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Novedades {
    pub automatic_retry: bool,
}

/// El contenido de `planes.toml`; sin archivo, todo vacío.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Planes {
    pub agents: BTreeMap<String, Plane>,
    pub novedades: Novedades,
    pub warnings: Vec<String>,
}

/// Lee `planes.toml`; un archivo ausente no es un error.
pub fn load(path: &Path) -> Planes {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Planes::default(),
        Err(error) => Planes {
            warnings: vec![format!("No pude leer {}: {error}", path.display())],
            ..Planes::default()
        },
    }
}

/// Analiza el TOML tolerando campos que falten o no se entiendan.
pub fn parse(text: &str) -> Planes {
    let mut planes = Planes::default();
    let table: toml::Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(error) => {
            planes
                .warnings
                .push(format!("No pude leer planes.toml: {error}"));
            return planes;
        }
    };
    for (name, item) in &table {
        if name == "novedades" {
            planes.novedades = parse_novedades(item, &mut planes.warnings);
            continue;
        }
        let Some(agent) = item.as_table() else {
            planes
                .warnings
                .push(format!("planes.toml: «{name}» no es una tabla; lo salto."));
            continue;
        };
        let plane = parse_plane(agent, name, &mut planes.warnings);
        planes.agents.insert(name.clone(), plane);
    }
    planes
}

fn parse_novedades(item: &toml::Value, warnings: &mut Vec<String>) -> Novedades {
    let Some(table) = item.as_table() else {
        warnings.push("planes.toml: [novedades] no es una tabla.".to_string());
        return Novedades::default();
    };
    Novedades {
        automatic_retry: table
            .get("reintento_automatico")
            .map(|value| match value.as_bool() {
                Some(flag) => flag,
                None => {
                    warnings.push(
                        "planes.toml: reintento_automatico debe ser true o false.".to_string(),
                    );
                    false
                }
            })
            .unwrap_or(false),
    }
}

fn parse_plane(table: &toml::Table, name: &str, warnings: &mut Vec<String>) -> Plane {
    let plan = table
        .get("plan")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(str::to_string);
    let window_hours = table.get("ventana_horas").and_then(number).filter(|hours| {
        let valid = *hours > 0.0 && *hours <= 24.0 * 366.0;
        if !valid {
            warnings.push(format!(
                "planes.toml: la ventana de {name} debe estar entre 0 y 8784 horas; la ignoro."
            ));
        }
        valid
    });
    let priority = table.get("prioridad").and_then(toml::Value::as_str).and_then(|text| {
        match text.trim().to_lowercase().as_str() {
            "primero" => Some(Priority::Primero),
            "normal" => Some(Priority::Normal),
            "reservar" => Some(Priority::Reservar),
            _ => {
                warnings.push(format!(
                    "planes.toml: la prioridad de {name} debe ser primero, normal o reservar; la ignoro."
                ));
                None
            }
        }
    });
    let limit = parse_limit(table, name, warnings);
    Plane {
        plan,
        window_hours,
        limit,
        priority,
    }
}

fn parse_limit(table: &toml::Table, name: &str, warnings: &mut Vec<String>) -> Option<UsageLimit> {
    let value = table.get("limite")?;
    let amount = match number(value).filter(|value| *value > 0.0) {
        Some(amount) => amount,
        None => {
            warnings.push(format!(
                "planes.toml: el límite de {name} debe ser un número mayor que cero; lo ignoro."
            ));
            return None;
        }
    };
    let unit = table
        .get("unidad")
        .and_then(toml::Value::as_str)
        .map(|text| text.trim().to_lowercase());
    match unit.as_deref() {
        None | Some("tokens") => Some(UsageLimit::tokens(amount)),
        Some("mensajes") | Some("messages") => Some(UsageLimit::messages(amount)),
        Some(other) => {
            warnings.push(format!(
                "planes.toml: la unidad «{other}» de {name} no es tokens ni mensajes; ignoro el límite."
            ));
            None
        }
    }
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|number| number as f64))
}

/// El plane que corresponde a `key` (admite el alias `minimax` de `mcode`).
pub fn for_agent<'a>(planes: &'a Planes, key: &str) -> Option<&'a Plane> {
    planes.agents.get(key).or_else(|| {
        if key == "mcode" {
            planes.agents.get("minimax")
        } else {
            None
        }
    })
}

/// Crea `planes.toml` con lo detectado si no existe. Devuelve si lo creó.
///
/// DECISION: se abre con `create_new` para no sobrescribir lo que la persona
/// ya escribió; si el archivo aparece entre la comprobación y la escritura,
/// la creación falla sin tocarlo.
pub fn propose(path: &Path, report: &UsageReport) -> Result<bool, String> {
    if path.exists() {
        return Ok(false);
    }
    let text = proposal_text(report);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("No pude crear {}: {error}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("No pude escribir {}: {error}", path.display()))?;
    if let Err(error) = file.write_all(text.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(format!("No pude escribir {}: {error}", path.display()));
    }
    Ok(true)
}

fn proposal_text(report: &UsageReport) -> String {
    let mut text = String::from(
        "# Planes y límites de uso por tipo de agente (docs/designs/inventario-recursos.md §5).\n\
         # Lo declarado aquí gana sobre lo detectado. Revísalo y confírmalo.\n\
         # limite: tokens en la ventana, o mensajes con unidad = \"mensajes\".\n\
         # prioridad: \"primero\", \"normal\" o \"reservar\".\n",
    );
    for agent in report.agents.iter().filter(|agent| agent.installed) {
        text.push_str(&format!("\n[{}]\n", agent.key));
        if let Some(plan) = agent.plan.as_deref() {
            text.push_str(&format!("plan = {}\n", quote(plan)));
        }
        let hours = agent
            .observed_window_hours
            .filter(|hours| *hours > 0.0)
            .unwrap_or(agent.window_hours);
        text.push_str(&format!("ventana_horas = {}\n", decimal(hours)));
        text.push_str("prioridad = \"normal\"\n");
        text.push_str("# limite = 2000000\n# unidad = \"tokens\"\n");
    }
    text.push_str("\n[novedades]\nreintento_automatico = false\n");
    text
}

fn quote(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn decimal(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests;
