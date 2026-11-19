//! Subagentes en Herdr: lanzar un agente como hijo de otro y ubicarlo (§8 de
//! `docs/designs/subagentes-herdr.md`).
//!
//! Con `parent`, el lanzamiento va siempre por `hc run --parent` (es la única
//! ruta que puede dejar la terminal colgando de su padre) y después se ubica
//! el subagente en una pestaña o en un panel. La ubicación nunca hace fallar
//! el lanzamiento: si Herdr no responde, el subagente queda en el árbol bajo
//! su agente y el resultado lo avisa.

use std::collections::BTreeMap;
use std::time::Duration;

use ez_core::pyvalue::{head_chars, py_strip, python_str, truthy};
use serde_json::{Map, Value};

use super::actions::{wrapped_argv, LaunchPlan, LaunchStep};
use super::{error_row, python_text, value_or_text, Herdr, HerdrError};

/// Máximo de caracteres de un nombre de `hc`.
const MAX_HC_NAME: usize = 32;

/// Espera entre reintentos de `terminal.keep`: Herdr tarda en ver la terminal
/// nueva (verificado en vivo).
const PLACE_RETRY_STEP_S: f64 = 0.5;

/// Tope de espera para ubicar el subagente.
const PLACE_RETRY_LIMIT_S: f64 = 15.0;

/// Aviso que aparece cuando el padre no tiene panel en el que abrirlo.
const NO_PANE_WARNING: &str = "El agente padre no tiene panel; abrí el subagente en una pestaña.";

/// Dónde se ve un subagente en Herdr (§8 del diseño).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// En el árbol, bajo su agente (no se toca Herdr).
    #[default]
    Tree,
    /// En una pestaña propia.
    Tab,
    /// En un panel a la derecha del panel enfocado.
    Right,
    /// En un panel debajo del panel enfocado.
    Down,
}

impl Placement {
    /// "tree" | "tab" | "right" | "down" (sin distinguir mayúsculas); otro → None.
    pub fn parse(text: &str) -> Option<Placement> {
        match text.trim().to_lowercase().as_str() {
            "tree" => Some(Placement::Tree),
            "tab" => Some(Placement::Tab),
            "right" => Some(Placement::Right),
            "down" => Some(Placement::Down),
            _ => None,
        }
    }

    /// El nombre de la ubicación (`Placement::as_str` del contrato).
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Tree => "tree",
            Placement::Tab => "tab",
            Placement::Right => "right",
            Placement::Down => "down",
        }
    }
}

/// Lo que quedó de la ubicación de un subagente.
struct Placed {
    placement: Placement,
    warnings: Vec<String>,
    /// `tab`/`pane` que devolvió `terminal.keep`.
    extra: Map<String, Value>,
}

impl Placed {
    fn tree() -> Self {
        Self {
            placement: Placement::Tree,
            warnings: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// Nombre válido para `hc`: minúsculas, `[a-z0-9_.-]`, empieza por letra o
/// número, máx. 32; lo demás se vuelve `-` (vocales con tilde → sin tilde,
/// «ñ» → «n»), sin `-` repetidos ni al borde; vacío → "agente".
///
/// `hc` además rechaza los nombres con forma de dirección (`t7`, `t9n`:
/// `^t[0-9][0-9a-z]*$`): a esos se les antepone `a-`.
pub fn hc_slug(text: &str) -> String {
    let mut raw = String::with_capacity(text.len());
    for character in text.chars() {
        raw.push(slug_char(character));
    }
    let collapsed = collapse_dashes(&raw);
    let trimmed = collapsed.trim_matches('-');
    let start = trimmed
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphanumeric())
        .map(|(index, _)| index)
        .unwrap_or(trimmed.len());
    let mut name: String = trimmed[start..].chars().take(MAX_HC_NAME).collect();
    while name.ends_with('-') {
        name.pop();
    }
    if name.is_empty() {
        return "agente".to_string();
    }
    if looks_like_address(&name) {
        let mut base: String = name.chars().take(MAX_HC_NAME - 2).collect();
        while base.ends_with('-') {
            base.pop();
        }
        name = format!("a-{base}");
    }
    name
}

/// Una letra de `hc`: minúscula, ASCII alfanumérico o `_`, `.`, `-`.
fn slug_char(character: char) -> char {
    let lower = character.to_lowercase().next().unwrap_or(character);
    match lower {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'ē' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'ī' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ō' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'ū' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other if other.is_ascii_alphanumeric() || other == '_' || other == '.' || other == '-' => {
            other
        }
        _ => '-',
    }
}

/// Junta los `-` repetidos.
fn collapse_dashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = false;
    for character in text.chars() {
        if character == '-' {
            if !last_dash {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(character);
            last_dash = false;
        }
    }
    out
}

/// ¿El nombre tiene forma de dirección (`t7`, `t9n`)? `hc` los rechaza.
fn looks_like_address(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if first != 't' {
        return false;
    }
    let Some(second) = characters.next() else {
        return false;
    };
    if !second.is_ascii_digit() {
        return false;
    }
    characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
}

/// ¿Es un nombre que `hc` acepta tal cual (`^[a-z0-9][a-z0-9_.-]{0,31}$` y sin
/// forma de dirección)?
fn hc_valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    if name.chars().count() > MAX_HC_NAME {
        return false;
    }
    if !characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'
            || character == '.'
            || character == '-'
    }) {
        return false;
    }
    !looks_like_address(name)
}

/// Nombre de `hc` para el subagente: el pedido si `hc` lo acepta, si no su
/// versión válida.
fn child_name(plan: &LaunchPlan<'_>) -> String {
    if let Some(name) = plan.hc_name.map(str::trim).filter(|name| !name.is_empty()) {
        return if hc_valid_name(name) {
            name.to_string()
        } else {
            hc_slug(name)
        };
    }
    hc_slug(&plan.label)
}

/// Etiquetas extra como pares `clave=valor`, en orden alfabético.
fn label_pairs(labels: Option<&Map<String, Value>>) -> Vec<(String, String)> {
    let Some(labels) = labels else {
        return Vec::new();
    };
    let mut sorted: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in labels {
        sorted.insert(key.clone(), python_str(value));
    }
    sorted.into_iter().collect()
}

/// Las etiquetas que se piden a `hc`: la del tipo y las extra.
fn requested_labels(plan: &LaunchPlan<'_>) -> Map<String, Value> {
    let mut labels = Map::new();
    labels.insert("agent".to_string(), Value::String(plan.key.clone()));
    for (key, value) in label_pairs(plan.labels.as_ref()) {
        labels.insert(key, Value::String(value));
    }
    labels
}

impl Herdr {
    /// Lanza un agente como hijo de otro y lo ubica (§8 del diseño).
    pub(super) fn launch_child(&self, plan: &mut LaunchPlan<'_>) -> Result<LaunchStep, HerdrError> {
        let Some(parent) = plan
            .parent
            .map(str::trim)
            .filter(|parent| !parent.is_empty())
            .map(str::to_string)
        else {
            return Err(HerdrError::failed("no hay padre para el subagente"));
        };
        let name = child_name(plan);
        let args = self.child_args(plan, &parent, &name);
        let data = match self.hc_run_json(&args, 40.0) {
            Ok(data) => data,
            Err(row) => return Ok(LaunchStep::Failed(row)),
        };
        let address = {
            let found = value_or_text(data.get("address"), "");
            if found.is_empty() {
                value_or_text(data.get("terminal"), "")
            } else {
                found
            }
        };
        if address.is_empty() {
            return Ok(LaunchStep::Failed(error_row(format!(
                "hc run no devolvió dirección: {}",
                python_text(&data)
            ))));
        }
        let mut placed = self.place_child(&address, &parent, plan.show);
        plan.launch
            .warnings
            .extend(std::mem::take(&mut placed.warnings));
        let terminal = self.hc_get(&address);
        if let Some(data) = &terminal {
            let warning = parent_warning(&address, &parent, data);
            if !warning.is_empty() {
                plan.launch.warnings.push(warning);
            }
        }
        let where_row = self.child_where_row(plan, &parent, &name, &placed, terminal.as_ref());
        Ok(LaunchStep::Ready(address, where_row))
    }

    /// `hc run --json --bg --parent … --name … --cwd … --label … -- <argv>`.
    fn child_args(&self, plan: &LaunchPlan<'_>, parent: &str, name: &str) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "run".to_string(),
            "--json".to_string(),
            "--bg".to_string(),
            "--parent".to_string(),
            parent.to_string(),
            "--name".to_string(),
            name.to_string(),
            "--cwd".to_string(),
            plan.folder.clone(),
        ];
        if !plan.notify {
            args.push("--no-notify".to_string());
        }
        if plan.outlive_parent {
            args.push("--outlive-parent".to_string());
        }
        args.push("--label".to_string());
        args.push(format!("agent={}", plan.key));
        for (key, value) in label_pairs(plan.labels.as_ref()) {
            args.push("--label".to_string());
            args.push(format!("{key}={value}"));
        }
        args.push("--".to_string());
        args.extend(wrapped_argv(&plan.argv, &plan.launch.env, plan.prepend));
        args
    }

    /// Lo que el resultado informa del subagente (§8, punto 3).
    fn child_where_row(
        &self,
        plan: &LaunchPlan<'_>,
        parent: &str,
        name: &str,
        placed: &Placed,
        terminal: Option<&Value>,
    ) -> Map<String, Value> {
        let real_parent = terminal
            .and_then(|data| data.get("parent"))
            .and_then(Value::as_str);
        let real_name = terminal
            .and_then(|data| data.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(name);
        let labels = terminal
            .and_then(|data| data.get("labels"))
            .filter(|value| truthy(value))
            .cloned()
            .unwrap_or_else(|| Value::Object(requested_labels(plan)));
        let mut row = Map::new();
        row.insert("visible".to_string(), Value::Bool(false));
        row.insert("background".to_string(), Value::Bool(true));
        // DECISION: si `hc get` respondió, el padre es el real (vacío si no
        // tiene); si no respondió, se queda el pedido como mejor dato.
        let parent_text = match terminal {
            Some(_) => real_parent.unwrap_or_default().to_string(),
            None => parent.to_string(),
        };
        row.insert("parent".to_string(), Value::String(parent_text));
        row.insert("name".to_string(), Value::String(real_name.to_string()));
        row.insert(
            "placed".to_string(),
            Value::String(placed.placement.as_str().to_string()),
        );
        row.insert("labels".to_string(), labels);
        for key in ["tab", "pane"] {
            if let Some(value) = placed.extra.get(key) {
                row.insert(key.to_string(), value.clone());
            }
        }
        row
    }

    /// Ubica el subagente: en el árbol, en una pestaña o en un panel.
    fn place_child(&self, terminal: &str, parent: &str, show: Placement) -> Placed {
        match show {
            Placement::Tree => Placed::tree(),
            Placement::Tab => self.keep_child(terminal, Placement::Tab),
            placement => match self.focus_pane(parent) {
                Ok(()) => self.keep_child(terminal, placement),
                Err(_) => {
                    let mut placed = self.keep_child(terminal, Placement::Tab);
                    placed.warnings.insert(0, NO_PANE_WARNING.to_string());
                    placed
                }
            },
        }
    }

    /// `terminal.keep` con reintentos (Herdr tarda en ver la terminal nueva).
    fn keep_child(&self, terminal: &str, place: Placement) -> Placed {
        match self.keep_with_retry(terminal, place) {
            Ok(response) => {
                let mut extra = Map::new();
                for key in ["tab", "pane"] {
                    if let Some(value) = response.get(key) {
                        extra.insert(key.to_string(), value.clone());
                    }
                }
                Placed {
                    placement: place,
                    warnings: Vec::new(),
                    extra,
                }
            }
            Err(error) => Placed {
                placement: Placement::Tree,
                warnings: vec![format!(
                    "Herdr no pudo ubicar el subagente ({}); quedó en el árbol bajo su agente.",
                    error.message()
                )],
                extra: Map::new(),
            },
        }
    }

    /// Repite `terminal.keep` mientras Herdr responda «no running terminal».
    fn keep_with_retry(
        &self,
        terminal: &str,
        place: Placement,
    ) -> Result<Map<String, Value>, HerdrError> {
        let start = self.runtime.monotonic();
        let mut next_at = start;
        loop {
            let error = match self.keep_once(terminal, place) {
                Ok(response) => return Ok(response),
                Err(error) => error,
            };
            if !is_missing_terminal(&error) {
                return Err(error);
            }
            let now = self.runtime.monotonic();
            if now - start >= PLACE_RETRY_LIMIT_S {
                return Err(error);
            }
            next_at += PLACE_RETRY_STEP_S;
            let wait = next_at - now;
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
        }
    }

    /// `herdr-app do terminal.keep --terminal <t> --place <lugar>`.
    fn keep_once(
        &self,
        terminal: &str,
        place: Placement,
    ) -> Result<Map<String, Value>, HerdrError> {
        let data = self.app_do(
            &[
                "do".to_string(),
                "terminal.keep".to_string(),
                "--terminal".to_string(),
                terminal.to_string(),
                "--place".to_string(),
                place.as_str().to_string(),
            ],
            15.0,
        )?;
        Ok(data.as_object().cloned().unwrap_or_default())
    }

    /// `herdr-app do pane.focus --terminal <padre>`.
    fn focus_pane(&self, parent: &str) -> Result<(), HerdrError> {
        self.app_do(
            &[
                "do".to_string(),
                "pane.focus".to_string(),
                "--terminal".to_string(),
                parent.to_string(),
            ],
            15.0,
        )?;
        Ok(())
    }

    /// `herdr-app do …`: devuelve el JSON o el motivo del fallo.
    fn app_do(&self, args: &[String], timeout: f64) -> Result<Value, HerdrError> {
        let mut argv = vec![self.herdr_app_path()];
        argv.extend_from_slice(args);
        let completed = self.run_command(&argv, timeout, false)?;
        let stdout = py_strip(&completed.stdout).to_string();
        let stderr = py_strip(&completed.stderr).to_string();
        let data = if stdout.is_empty() {
            None
        } else {
            ez_core::pyloads::loads(&stdout).ok()
        };
        if let Some(data) = &data {
            if let Some(error) = data.get("error").filter(|value| truthy(value)) {
                return Err(HerdrError::failed(error_message(error)));
            }
        }
        if completed.returncode != 0 {
            let message = if !stderr.is_empty() { stderr } else { stdout };
            return Err(HerdrError::failed(message));
        }
        Ok(data.unwrap_or_else(|| Value::Object(Map::new())))
    }

    /// `hc run --json`: distingue el error de «nombre ocupado».
    fn hc_run_json(&self, args: &[String], timeout: f64) -> Result<Value, Map<String, Value>> {
        let mut argv = vec![self.hc_path()];
        argv.extend_from_slice(args);
        let completed = match self.run_command(&argv, timeout, false) {
            Ok(completed) => completed,
            Err(error) => return Err(error_row(error.message())),
        };
        let stdout = py_strip(&completed.stdout).to_string();
        if stdout.is_empty() {
            let message = if completed.stderr.is_empty() {
                "hc run no devolvió nada".to_string()
            } else {
                completed.stderr.clone()
            };
            return Err(error_row(py_strip(&message).to_string()));
        }
        let data = match ez_core::pyloads::loads(&stdout) {
            Ok(data) => data,
            Err(_) => {
                return Err(error_row(format!(
                    "hc run devolvió algo que no es JSON: {}",
                    head_chars(&stdout, 300)
                )))
            }
        };
        if let Some(error) = data.get("error").filter(|value| truthy(value)) {
            let mut row = error_row(error_message(error));
            if error_kind(error) == Some("name_taken") {
                row.insert("name_taken".to_string(), Value::Bool(true));
            }
            return Err(row);
        }
        Ok(data)
    }

    /// `hc get <t>`: el detalle de la terminal (padre y nombre reales).
    fn hc_get(&self, terminal: &str) -> Option<Value> {
        self.json(&["get".to_string(), terminal.to_string()], 20.0)
            .ok()
    }
}

/// ¿El error es el «no running terminal» que Herdr da mientras ve la terminal?
fn is_missing_terminal(error: &HerdrError) -> bool {
    error
        .message()
        .to_lowercase()
        .contains("no running terminal")
}

/// El aviso cuando el padre real de la terminal no es el pedido.
fn parent_warning(address: &str, parent: &str, terminal: &Value) -> String {
    match terminal.get("parent").and_then(Value::as_str) {
        Some(real) if real == parent => String::new(),
        Some(real) => format!("El padre de {address} es {real}, no {parent}."),
        None => format!("El padre de {address} es ninguno, no {parent}."),
    }
}

/// El mensaje de un error de `hc`/`herdr-app`.
fn error_message(error: &Value) -> String {
    match error {
        Value::Object(spec) => spec
            .get("message")
            .map(python_str)
            .unwrap_or_else(|| python_str(error)),
        other => python_str(other),
    }
}

/// La clase del error de `hc` (`name_taken`, `invalid_params`…).
fn error_kind(error: &Value) -> Option<&str> {
    error
        .as_object()
        .and_then(|spec| spec.get("kind"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests;
