//! Acciones de Herdr: teclas, terminales nuevas y lanzamiento de agentes.

use std::sync::Arc;

use ez_core::pyjson::dumps;
use ez_core::pyvalue::{head_chars, py_strip, python_str, truthy};
use serde_json::{json, Map, Value};

use super::child::{hc_slug, Placement};
use super::text::{command_line, valid_keys};
use super::{
    error_row, python_text, value_or_text, AgentLaunch, Herdr, HerdrError, LaunchSupport,
    DEFAULT_SHELL, GUI_INSTRUCTION,
};

/// Resultado de abrir la terminal: lista para el `finish_launch` o el error
/// que Python devuelve tal cual.
pub(super) enum LaunchStep {
    Ready(String, Map<String, Value>),
    Failed(Map<String, Value>),
}

/// Lo que `run_agent` ya resolvió antes de abrir la terminal.
pub(super) struct LaunchPlan<'a> {
    pub(super) argv: Vec<String>,
    pub(super) launch: AgentLaunch,
    pub(super) key: String,
    pub(super) folder: String,
    pub(super) label: String,
    pub(super) task_text: String,
    pub(super) background: bool,
    pub(super) space: Option<&'a str>,
    pub(super) prepend: Option<&'a [String]>,
    /// Terminal padre: con ella el subagente cuelga de su agente (§8).
    pub(super) parent: Option<&'a str>,
    /// Nombre que `hc` acepta para el subagente.
    pub(super) hc_name: Option<&'a str>,
    /// Etiquetas extra del subagente.
    pub(super) labels: Option<Map<String, Value>>,
    /// Dónde se ve el subagente en Herdr.
    pub(super) show: Placement,
    /// Si `hc` avisa al padre de lo que hace el subagente.
    pub(super) notify: bool,
    /// Si el subagente sobrevive al padre.
    pub(super) outlive_parent: bool,
}

/// Callback del lanzamiento asíncrono (`on_ready` del Python).
pub type ReadyCallback = Box<dyn FnOnce(Map<String, Value>) + Send + 'static>;

/// Parámetros de `run_agent` (Python no tiene argumentos por defecto en un
/// método de Rust; este struct los reproduce con `Default`).
#[derive(Default)]
pub struct RunAgent<'a> {
    pub agent: &'a str,
    pub task: &'a str,
    pub cwd: Option<&'a str>,
    pub name: Option<&'a str>,
    /// Sin uso, como en el Python (la confirmación la decide la capa de tools).
    pub confirmed: bool,
    pub background: bool,
    pub gui: bool,
    pub space: Option<&'a str>,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub on_ready: Option<ReadyCallback>,
    pub env: Option<Map<String, Value>>,
    pub path_prepend: Option<Vec<String>>,
    pub extra_args: Option<Vec<String>>,
    /// Terminal padre del subagente (§8): con ella el lanzamiento va por
    /// `hc run --parent` y el subagente queda bajo su agente.
    pub parent: Option<&'a str>,
    /// Nombre del subagente para `hc` (el que se ve en Herdr).
    pub hc_name: Option<&'a str>,
    /// Etiquetas extra del subagente (los valores van como texto).
    pub labels: Option<Map<String, Value>>,
    /// Dónde se ve el subagente: árbol, pestaña, panel a la derecha o debajo.
    pub show: Placement,
    /// Si `hc` avisa al padre de lo que hace el subagente (por omisión, no).
    pub notify: bool,
    /// Si el subagente sobrevive al cierre del padre.
    pub outlive_parent: bool,
}

impl Herdr {
    /// Pulsa teclas nombradas (enter, esc, up, ctrl+t...) en un terminal.
    pub fn keys_agent(
        &self,
        address: &str,
        keys: &[String],
    ) -> Result<Map<String, Value>, HerdrError> {
        let Some(cleaned) = valid_keys(keys) else {
            return Ok(error_row("Teclas no válidas."));
        };
        let mut args = vec!["keys".to_string(), address.to_string()];
        args.extend(cleaned.iter().cloned());
        let result = self.run(&args, 15.0)?;
        if result.returncode != 0 {
            let message = if !result.stderr.is_empty() {
                &result.stderr
            } else {
                &result.stdout
            };
            return Ok(error_row(head_chars(py_strip(message), 300).to_string()));
        }
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert("address".to_string(), Value::String(address.to_string()));
        row.insert(
            "keys".to_string(),
            Value::Array(cleaned.into_iter().map(Value::String).collect()),
        );
        Ok(row)
    }

    /// Abre una terminal visible en una pestaña nueva de Herdr.
    pub fn open_terminal(
        &self,
        name: &str,
        cwd: Option<&str>,
        shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, HerdrError> {
        let folder = cwd
            .filter(|folder| !folder.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_cwd());
        let mut args = vec![
            "do".to_string(),
            "tab.new".to_string(),
            "--name".to_string(),
            name.to_string(),
            "--folder".to_string(),
            folder,
            "--argv".to_string(),
            dumps(&json!([shell])),
        ];
        if let Some(space) = space.filter(|space| !space.is_empty()) {
            args.push("--space".to_string());
            args.push(space.to_string());
        }
        let data = self.json_app(&args, 30.0)?;
        let terminal = match value_or_text(data.get("terminal"), "") {
            terminal if !terminal.is_empty() => terminal,
            _ => value_or_text(data.get("address"), ""),
        };
        if terminal.is_empty() {
            return Ok(error_row(format!(
                "tab.new no devolvió terminal: {}",
                python_text(&data)
            )));
        }
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert("terminal".to_string(), Value::String(terminal.clone()));
        row.insert("address".to_string(), Value::String(terminal));
        row.insert(
            "tab".to_string(),
            data.get("tab").cloned().unwrap_or(Value::Null),
        );
        row.insert(
            "space".to_string(),
            data.get("space").cloned().unwrap_or(Value::Null),
        );
        row.insert("visible".to_string(), Value::Bool(true));
        Ok(row)
    }

    /// Abre un agente como lo haría una persona y le pega la tarea.
    ///
    /// Visible (por defecto): pestaña nueva con una shell donde se escribe el
    /// comando del agente. Cuando el programa ha arrancado se fijan modelo y
    /// esfuerzo con sus comandos internos y, al final, se pega la tarea.
    pub fn run_agent<L: LaunchSupport + Clone + Send + 'static>(
        &self,
        launcher: L,
        params: RunAgent<'_>,
    ) -> Result<Map<String, Value>, HerdrError> {
        let mut launch = launcher.agent_launch(params.agent, params.model, params.effort);
        let Some((key, mut argv)) = self.launch_inputs(params.agent, &launch) else {
            return Ok(self.unavailable(params.agent, &launch.error));
        };
        let (folder, label, task_text) = self.launch_texts(&params, &key);
        if let Some(extra) = &params.extra_args {
            argv.extend(extra.iter().cloned());
        }
        if let Some(env) = params.env {
            launch.env.extend(env);
        }
        let mut prepared = LaunchPlan {
            argv,
            launch,
            key,
            folder,
            label,
            task_text,
            background: params.background,
            space: params.space,
            prepend: params.path_prepend.as_deref(),
            parent: params.parent.map(str::trim).filter(|p| !p.is_empty()),
            hc_name: params.hc_name.map(str::trim).filter(|n| !n.is_empty()),
            labels: params.labels,
            show: params.show,
            notify: params.notify,
            outlive_parent: params.outlive_parent,
        };
        let (address, where_row) = match self.open_launch(&launcher, &mut prepared)? {
            LaunchStep::Ready(address, where_row) => (address, where_row),
            LaunchStep::Failed(row) => return Ok(row),
        };
        let base = launch_base(
            &address,
            &prepared.key,
            params.model,
            params.effort,
            &prepared.folder,
            &prepared.argv,
            where_row,
        );
        self.finish_run(launcher, params.on_ready, &prepared, address, base)
    }

    /// Carpeta, etiqueta y tarea ya resueltas antes de abrir la terminal.
    fn launch_texts(&self, params: &RunAgent<'_>, key: &str) -> (String, String, String) {
        let folder = params
            .cwd
            .filter(|folder| !folder.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_cwd());
        let label = params
            .name
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{key} · voz"));
        let task = py_strip(params.task).to_string();
        // DECISION: sin tarea se abre el agente listo y a la espera.
        let task_text = if task.is_empty() {
            String::new()
        } else if params.gui {
            format!("{GUI_INSTRUCTION}{task}")
        } else {
            task
        };
        (folder, label, task_text)
    }

    /// `(key, argv)` si el agente existe y el perfil devolvió un comando.
    fn launch_inputs(&self, agent: &str, launch: &AgentLaunch) -> Option<(String, Vec<String>)> {
        Some((self.resolve_agent(agent)?, launch.argv.clone()?))
    }

    fn open_launch<L: LaunchSupport>(
        &self,
        launcher: &L,
        plan: &mut LaunchPlan<'_>,
    ) -> Result<LaunchStep, HerdrError> {
        // Con padre, el subagente va siempre por `hc run --parent` (§8): es la
        // única ruta que puede dejarlo colgando de su agente.
        if plan.parent.is_some() {
            return self.launch_child(plan);
        }
        if plan.background {
            self.launch_background(
                &plan.argv,
                &plan.launch,
                plan.prepend,
                &plan.folder,
                &plan.label,
                &plan.key,
            )
        } else {
            self.launch_visible(
                launcher,
                &plan.argv,
                &plan.launch,
                plan.prepend,
                &plan.folder,
                &plan.label,
                plan.space,
            )
        }
    }

    fn finish_run<L: LaunchSupport + Clone + Send + 'static>(
        &self,
        launcher: L,
        on_ready: Option<ReadyCallback>,
        plan: &LaunchPlan<'_>,
        address: String,
        base: Map<String, Value>,
    ) -> Result<Map<String, Value>, HerdrError> {
        let Some(on_ready) = on_ready else {
            return Ok(launch_outcome(
                &launcher,
                &address,
                &plan.key,
                &plan.launch,
                &plan.task_text,
                &base,
            ));
        };
        // Asíncrono: la terminal ya está abierta y con el comando escrito, así
        // que se responde al instante y el arranque del programa (hasta ~14 s
        // en Codex/OpenCode), modelo, esfuerzo y tarea se resuelven aparte.
        let warnings = plan.launch.warnings.clone();
        let key = plan.key.clone();
        let launch = plan.launch.clone();
        let task_text = plan.task_text.clone();
        let worker = launcher;
        let thread_base = base.clone();
        let runtime = Arc::clone(&self.runtime);
        let task = Box::new(move || {
            let result = launch_outcome(&worker, &address, &key, &launch, &task_text, &thread_base);
            on_ready(result);
        });
        if !runtime.spawn("ez-launch", task) {
            return Err(HerdrError::failed("no pude iniciar el hilo de lanzamiento"));
        }
        let mut result = Map::new();
        result.insert("ok".to_string(), Value::Bool(true));
        result.insert("launching".to_string(), Value::Bool(true));
        result.extend(base);
        result.insert("task_sent".to_string(), Value::Bool(false));
        result.insert(
            "warnings".to_string(),
            Value::Array(warnings.into_iter().map(Value::String).collect()),
        );
        Ok(result)
    }

    fn unavailable(&self, agent: &str, error: &str) -> Map<String, Value> {
        let message = if error.is_empty() {
            format!("Agente desconocido: {}.", ez_core::pyvalue::repr_str(agent))
        } else {
            error.to_string()
        };
        let mut row = error_row(message);
        let mut available: Vec<String> = self
            .config()
            .get("agents")
            .and_then(Value::as_object)
            .map(|agents| agents.keys().cloned().collect())
            .unwrap_or_default();
        available.sort();
        row.insert(
            "available".to_string(),
            Value::Array(available.into_iter().map(Value::String).collect()),
        );
        row
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_background(
        &self,
        argv: &[String],
        launch: &AgentLaunch,
        prepend: Option<&[String]>,
        folder: &str,
        label: &str,
        key: &str,
    ) -> Result<LaunchStep, HerdrError> {
        let run_argv = wrapped_argv(argv, &launch.env, prepend);
        let mut args: Vec<String> = vec![
            "run".to_string(),
            "--json".to_string(),
            "--bg".to_string(),
            "--no-parent".to_string(),
            "--cwd".to_string(),
            folder.to_string(),
            "--name".to_string(),
            // DECISION: `hc` solo acepta nombres de 1-32 caracteres de
            // `a-z 0-9 - _ .`; la etiqueta legible («claude · voz») se
            // convierte con `hc_slug`.
            hc_slug(label),
            "--label".to_string(),
        ];
        args.push(format!("agent={key}"));
        args.push("--".to_string());
        args.extend(run_argv);
        let data = self.json(&args, 40.0)?;
        let address = value_or_text(data.get("address"), "");
        if address.is_empty() {
            return Ok(LaunchStep::Failed(error_row(format!(
                "hc run no devolvió dirección: {}",
                python_text(&data)
            ))));
        }
        let mut where_row = Map::new();
        where_row.insert("visible".to_string(), Value::Bool(false));
        where_row.insert("background".to_string(), Value::Bool(true));
        Ok(LaunchStep::Ready(address, where_row))
    }

    #[allow(clippy::too_many_arguments)]
    fn launch_visible<L: LaunchSupport>(
        &self,
        launcher: &L,
        argv: &[String],
        launch: &AgentLaunch,
        prepend: Option<&[String]>,
        folder: &str,
        label: &str,
        space: Option<&str>,
    ) -> Result<LaunchStep, HerdrError> {
        let mut args = vec![
            "do".to_string(),
            "tab.new".to_string(),
            "--name".to_string(),
            label.to_string(),
            "--folder".to_string(),
            folder.to_string(),
            "--argv".to_string(),
            dumps(&json!([DEFAULT_SHELL])),
        ];
        if let Some(space) = space.filter(|space| !space.is_empty()) {
            args.push("--space".to_string());
            args.push(space.to_string());
        }
        let data = self.json_app(&args, 45.0)?;
        let terminal = value_or_text(data.get("terminal"), "");
        let address = if terminal.is_empty() {
            value_or_text(data.get("address"), "")
        } else {
            terminal
        };
        if address.is_empty() {
            return Ok(LaunchStep::Failed(error_row(format!(
                "tab.new no devolvió terminal: {}",
                python_text(&data)
            ))));
        }
        launcher.wait_shell_prompt(&address);
        let typed = self.send_agent(&address, &command_line(argv, &launch.env, prepend), true)?;
        if !typed.get("ok").is_some_and(truthy) {
            let message = python_str(typed.get("error").unwrap_or(&Value::Null));
            return Ok(LaunchStep::Failed(error_row(format!(
                "No pude escribir el comando en {address}: {message}"
            ))));
        }
        let mut where_row = Map::new();
        where_row.insert("visible".to_string(), Value::Bool(true));
        where_row.insert("background".to_string(), Value::Bool(false));
        where_row.insert(
            "tab".to_string(),
            data.get("tab").cloned().unwrap_or(Value::Null),
        );
        where_row.insert(
            "space".to_string(),
            data.get("space").cloned().unwrap_or(Value::Null),
        );
        Ok(LaunchStep::Ready(address, where_row))
    }

    /// `hc`: enfoca la terminal de un agente.
    pub fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, HerdrError> {
        self.json_app(
            &[
                "do".to_string(),
                "agent.focus".to_string(),
                "--terminal".to_string(),
                address.to_string(),
            ],
            15.0,
        )?;
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert("address".to_string(), Value::String(address.to_string()));
        row.insert("focused".to_string(), Value::Bool(true));
        Ok(row)
    }

    /// `hc kill`: termina la terminal de un agente.
    pub fn kill_agent(
        &self,
        address: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        self.json(&["kill".to_string(), address.to_string()], 20.0)?;
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert("address".to_string(), Value::String(address.to_string()));
        row.insert(
            "message".to_string(),
            Value::String("Terminal terminado.".to_string()),
        );
        Ok(row)
    }
}

/// El comando del agente con su entorno: si hay variables o carpetas en el
/// PATH, va por PowerShell (igual que el lanzamiento en segundo plano).
pub(super) fn wrapped_argv(
    argv: &[String],
    env: &Map<String, Value>,
    prepend: Option<&[String]>,
) -> Vec<String> {
    if !env.is_empty() || prepend.is_some_and(|list| !list.is_empty()) {
        super::powershell_command(&command_line(argv, env, prepend))
    } else {
        argv.to_vec()
    }
}

pub(super) fn launch_base(
    address: &str,
    key: &str,
    model: Option<&str>,
    effort: Option<&str>,
    folder: &str,
    argv: &[String],
    where_row: Map<String, Value>,
) -> Map<String, Value> {
    let mut base = Map::new();
    base.insert("address".to_string(), Value::String(address.to_string()));
    base.insert("terminal".to_string(), Value::String(address.to_string()));
    base.insert("agent".to_string(), Value::String(key.to_string()));
    base.insert(
        "model".to_string(),
        Value::String(model.unwrap_or("").to_string()),
    );
    base.insert(
        "effort".to_string(),
        Value::String(effort.unwrap_or("").to_string()),
    );
    base.insert("cwd".to_string(), Value::String(folder.to_string()));
    base.insert(
        "command".to_string(),
        Value::Array(argv.iter().cloned().map(Value::String).collect()),
    );
    base.extend(where_row);
    base
}

pub(super) fn launch_outcome<L: LaunchSupport>(
    launcher: &L,
    address: &str,
    key: &str,
    launch: &AgentLaunch,
    task_text: &str,
    base: &Map<String, Value>,
) -> Map<String, Value> {
    let mut outcome = launcher.finish_launch(address, key, launch, task_text);
    let ok = outcome.remove("ok").is_some_and(|value| truthy(&value));
    if !ok {
        outcome.insert("ok".to_string(), Value::Bool(false));
        outcome.insert("address".to_string(), Value::String(address.to_string()));
        outcome.insert("agent".to_string(), Value::String(key.to_string()));
        return outcome;
    }
    let mut result = Map::new();
    result.insert("ok".to_string(), Value::Bool(true));
    result.extend(base.clone());
    result.extend(outcome);
    result
}
