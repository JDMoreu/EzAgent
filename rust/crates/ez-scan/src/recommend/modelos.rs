//! `modelos.toml`: nivel, costo y familia de cada modelo (§6).
//!
//! Formato, una tabla por modelo (el nombre es la clave):
//! ```text
//! ["deepseek-v4.1-flash"]
//! agente = "opencode"   # opcional si el modelo está en los de fábrica
//! nivel = "medio"       # alto | medio | bajo
//! costo = "plan"        # gratis | plan | pago
//! familia = "deepseek"
//! ```
//!
//! El nombre va entre comillas: sin ellas, TOML lee `[deepseek-v4.1-flash]`
//! como la tabla `deepseek-v4` con la subclave `1-flash`. Esa forma anidada
//! también se acepta y se reconstruye como `deepseek-v4.1-flash`.
//!
//! Sin archivo mandan los valores de fábrica; un archivo con entradas válidas
//! los reemplaza (la persona manda). Un archivo ilegible o sin entradas deja
//! los de fábrica y se avisa.

use std::path::Path;

use serde::Serialize;

pub const FILE_NAME: &str = "modelos.toml";

/// Capacidad de un modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Bajo,
    Medio,
    Alto,
}

impl Level {
    /// `alto`, `medio` o `bajo` (sin distinguir mayúsculas).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "alto" => Some(Self::Alto),
            "medio" => Some(Self::Medio),
            "bajo" => Some(Self::Bajo),
            _ => None,
        }
    }

    /// El nombre que se muestra.
    pub fn name(self) -> &'static str {
        match self {
            Self::Alto => "alto",
            Self::Medio => "medio",
            Self::Bajo => "bajo",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Bajo => 0,
            Self::Medio => 1,
            Self::Alto => 2,
        }
    }
}

/// Lo que cuesta usar un modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cost {
    Gratis,
    Plan,
    Pago,
}

impl Cost {
    /// `gratis`, `plan` o `pago` (sin distinguir mayúsculas).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "gratis" => Some(Self::Gratis),
            "plan" => Some(Self::Plan),
            "pago" => Some(Self::Pago),
            _ => None,
        }
    }

    /// El nombre que se muestra.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gratis => "gratis",
            Self::Plan => "plan",
            Self::Pago => "pago",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Gratis => 0,
            Self::Plan => 1,
            Self::Pago => 2,
        }
    }
}

/// Un modelo del catálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    pub agent: String,
    pub model: String,
    pub level: Level,
    pub cost: Cost,
    pub family: String,
}

/// Los modelos que se pueden recomendar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub entries: Vec<ModelEntry>,
    pub warnings: Vec<String>,
}

/// Los valores de fábrica: los del catálogo de `voice.json` (2026-11-19).
///
/// DECISION: el nivel, el costo y la familia de cada uno son una primera
/// propuesta para que la persona los edite en `modelos.toml`. Los modelos de
/// OpenCode y Claude Code van por su plan de suscripción (`plan`); Qwen Code es
/// el único gratuito del catálogo. Los modelos gratuitos de OpenCode (`*-free`)
/// no están en el catálogo del perfil: quien los use los declara en su archivo.
const FACTORY: [(&str, &str, Level, Cost, &str); 11] = [
    (
        "opencode",
        "deepseek-v4.1-flash",
        Level::Medio,
        Cost::Plan,
        "deepseek",
    ),
    (
        "opencode",
        "deepseek-v4-pro",
        Level::Alto,
        Cost::Plan,
        "deepseek",
    ),
    ("opencode", "glm-5.3", Level::Medio, Cost::Plan, "zhipu"),
    ("opencode", "kimi-k3", Level::Alto, Cost::Pago, "moonshot"),
    (
        "opencode",
        "minimax-m3.1-flash",
        Level::Medio,
        Cost::Plan,
        "minimax",
    ),
    (
        "opencode",
        "claude-haiku-5-5",
        Level::Bajo,
        Cost::Plan,
        "anthropic",
    ),
    ("claude", "opus", Level::Alto, Cost::Plan, "anthropic"),
    ("claude", "sonnet", Level::Medio, Cost::Plan, "anthropic"),
    ("claude", "haiku", Level::Bajo, Cost::Plan, "anthropic"),
    ("claude", "fable", Level::Alto, Cost::Plan, "anthropic"),
    (
        "qwen",
        "qwen3-coder-plus",
        Level::Medio,
        Cost::Gratis,
        "qwen",
    ),
];

impl Catalog {
    /// Los valores de fábrica.
    pub fn factory() -> Self {
        Self {
            entries: FACTORY
                .iter()
                .map(|(agent, model, level, cost, family)| ModelEntry {
                    agent: (*agent).to_string(),
                    model: (*model).to_string(),
                    level: *level,
                    cost: *cost,
                    family: (*family).to_string(),
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    /// Lee el archivo; sin archivo o sin entradas válidas, los de fábrica.
    ///
    /// DECISION: un archivo con entradas válidas reemplaza por completo la tabla
    /// de fábrica (la persona manda); no se mezclan las dos.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let parsed = Self::parse(&text);
                if parsed.entries.is_empty() {
                    // DECISION: un archivo vacío o solo de comentarios no deja
                    // la recomendación sin datos; se usan los de fábrica.
                    let mut factory = Self::factory();
                    factory.warnings.push(
                        "modelos.toml no tiene ninguna entrada válida; uso los valores de fábrica."
                            .to_string(),
                    );
                    factory.warnings.extend(parsed.warnings);
                    factory
                } else {
                    parsed
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::factory(),
            Err(error) => {
                let mut factory = Self::factory();
                factory
                    .warnings
                    .push(format!("No pude leer {}: {error}", path.display()));
                factory
            }
        }
    }

    /// Analiza el TOML tolerando entradas que falten o no se entiendan.
    ///
    /// Una tabla sin `nivel`/`costo` cuyas subtablas sí los tienen es el
    /// nombre con punto sin citar (`[deepseek-v4.1-flash]`): se reconstruye
    /// como `padre.hijo` para no perder la preferencia de la persona.
    pub fn parse(text: &str) -> Self {
        let mut catalog = Self::default();
        let table: toml::Table = match toml::from_str(text) {
            Ok(table) => table,
            Err(error) => {
                catalog
                    .warnings
                    .push(format!("No pude leer modelos.toml: {error}"));
                return catalog;
            }
        };
        for (name, value) in table {
            catalog.take_entry(&name, &value);
        }
        catalog
    }

    /// Una tabla de primer nivel o una subtabla de un nombre con punto.
    fn take_entry(&mut self, name: &str, value: &toml::Value) {
        let Some(fields) = value.as_table() else {
            self.warnings
                .push(format!("modelos.toml: «{name}» no es una tabla; lo salto."));
            return;
        };
        if fields.contains_key("nivel") || fields.contains_key("costo") {
            self.push_entry(name, fields);
            return;
        }
        let mut nested = 0;
        for (child, child_value) in fields {
            if child_value.is_table() {
                nested += 1;
                self.take_entry(&format!("{name}.{child}"), child_value);
            }
        }
        if nested == 0 {
            self.warnings.push(format!(
                "modelos.toml: «{name}» necesita nivel alto, medio o bajo; lo salto."
            ));
        }
    }

    fn push_entry(&mut self, name: &str, fields: &toml::Table) {
        let Some(level) = field(fields, "nivel").and_then(Level::parse) else {
            self.warnings.push(format!(
                "modelos.toml: «{name}» necesita nivel alto, medio o bajo; lo salto."
            ));
            return;
        };
        let Some(cost) = field(fields, "costo").and_then(Cost::parse) else {
            self.warnings.push(format!(
                "modelos.toml: «{name}» necesita costo gratis, plan o pago; lo salto."
            ));
            return;
        };
        let agent = field(fields, "agente")
            .map(str::to_string)
            .or_else(|| factory_agent(name))
            .unwrap_or_default();
        if agent.is_empty() {
            self.warnings.push(format!(
                "modelos.toml: «{name}» no dice su agente; lo salto."
            ));
            return;
        }
        let family = field(fields, "familia")
            .map(str::to_string)
            .unwrap_or_else(|| {
                // DECISION: sin familia no se descarta el modelo; queda como
                // «desconocida» y no cuenta para la regla de familia distinta.
                self.warnings.push(format!(
                    "modelos.toml: «{name}» no dice su familia; uso «desconocida»."
                ));
                "desconocida".to_string()
            });
        if self.entries.iter().any(|entry| {
            entry.agent.eq_ignore_ascii_case(&agent) && entry.model.eq_ignore_ascii_case(name)
        }) {
            self.warnings.push(format!(
                "modelos.toml: «{name}» está dos veces para {agent}; uso la primera."
            ));
            return;
        }
        self.entries.push(ModelEntry {
            agent,
            model: name.to_string(),
            level,
            cost,
            family,
        });
    }

    /// La familia de un modelo (por su nombre, con o sin proveedor).
    pub fn family_of(&self, model: &str) -> Option<&str> {
        let tail = model_tail(model);
        self.entries
            .iter()
            .find(|entry| entry.model.eq_ignore_ascii_case(tail))
            .map(|entry| entry.family.as_str())
    }
}

fn field<'a>(fields: &'a toml::Table, name: &str) -> Option<&'a str> {
    fields
        .get(name)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// El agente de fábrica de un modelo, si está en la tabla.
fn factory_agent(model: &str) -> Option<String> {
    let tail = model_tail(model);
    FACTORY
        .iter()
        .find(|(_, known, ..)| known.eq_ignore_ascii_case(tail))
        .map(|(agent, ..)| (*agent).to_string())
}

/// El nombre sin `proveedor/` ni `#nivel`.
fn model_tail(model: &str) -> &str {
    let model = model.split('#').next().unwrap_or(model);
    model.rsplit('/').next().unwrap_or(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_archivo_valen_los_de_fabrica() {
        let path =
            std::env::temp_dir().join(format!("ez-scan-modelos-sin-{}.toml", std::process::id()));
        let catalog = Catalog::load(&path);
        assert!(catalog.entries.len() >= 10);
        assert_eq!(
            catalog.family_of("opencode-go/deepseek-v4.1-flash"),
            Some("deepseek")
        );
        assert!(catalog.warnings.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn el_archivo_reemplaza_la_fabrica_y_tolera_errores() {
        let path =
            std::env::temp_dir().join(format!("ez-scan-modelos-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[mio]\nagente = \"opencode\"\nnivel = \"alto\"\ncosto = \"pago\"\nfamilia = \"otra\"\n\
             [roto]\nnivel = \"altísimo\"\n",
        )
        .expect("modelos.toml temporal");
        let catalog = Catalog::load(&path);
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].level, Level::Alto);
        assert_eq!(catalog.entries[0].cost, Cost::Pago);
        assert!(
            catalog
                .warnings
                .iter()
                .any(|warning| warning.contains("roto")),
            "{:?}",
            catalog.warnings
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_archivo_vacio_deja_la_fabrica() {
        let path =
            std::env::temp_dir().join(format!("ez-scan-modelos-vacio-{}.toml", std::process::id()));
        std::fs::write(&path, "# solo un comentario\n").expect("modelos.toml temporal");
        assert!(Catalog::load(&path).entries.len() >= 10);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_nombre_con_punto_sin_comillas_se_reconstruye() {
        let path =
            std::env::temp_dir().join(format!("ez-scan-modelos-punto-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "[deepseek-v4.1-flash]\nagente = \"opencode\"\nnivel = \"medio\"\n\
             costo = \"plan\"\nfamilia = \"deepseek\"\n",
        )
        .expect("modelos.toml temporal");
        let catalog = Catalog::load(&path);
        assert_eq!(catalog.entries.len(), 1, "{:?}", catalog.warnings);
        assert_eq!(catalog.entries[0].model, "deepseek-v4.1-flash");
        assert_eq!(catalog.entries[0].level, Level::Medio);
        assert_eq!(catalog.entries[0].cost, Cost::Plan);
        assert!(catalog.warnings.is_empty(), "{:?}", catalog.warnings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn la_forma_citada_da_el_mismo_nombre() {
        let catalog = Catalog::parse(
            "[\"deepseek-v4.1-flash\"]\nagente = \"opencode\"\nnivel = \"medio\"\n\
             costo = \"plan\"\nfamilia = \"deepseek\"\n",
        );
        assert_eq!(catalog.entries.len(), 1, "{:?}", catalog.warnings);
        assert_eq!(catalog.entries[0].model, "deepseek-v4.1-flash");
        assert!(catalog.warnings.is_empty(), "{:?}", catalog.warnings);
    }

    #[test]
    fn sin_entradas_validas_avisa_de_la_fabrica_y_por_que() {
        let path = std::env::temp_dir().join(format!(
            "ez-scan-modelos-invalido-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "[roto]\nnivel = \"altísimo\"\n").expect("modelos.toml temporal");
        let catalog = Catalog::load(&path);
        assert!(catalog.entries.len() >= 10);
        assert!(
            catalog
                .warnings
                .iter()
                .any(|warning| warning.contains("valores de fábrica")),
            "{:?}",
            catalog.warnings
        );
        assert!(
            catalog
                .warnings
                .iter()
                .any(|warning| warning.contains("roto") && warning.contains("nivel")),
            "{:?}",
            catalog.warnings
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn niveles_y_costos_se_ordenan_de_menos_a_mas() {
        assert!(Level::Bajo < Level::Medio && Level::Medio < Level::Alto);
        assert!(Cost::Gratis < Cost::Plan && Cost::Plan < Cost::Pago);
    }
}
