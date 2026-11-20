//! El modo `--real-hc`: dos hijos falsos en el `hc` real y el reintento
//! con el `ez` compilado. Vive aparte para no pasar de 600 líneas.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::SubagentWatch;
use ez_chat::session::ChatModel;
use ez_herdr::herdr::system_runtime;
use ez_herdr::subagents::{HcRuntime, Registry, TaskState};
use serde_json::{json, Value};

use super::*;

// ------------------------------------------------------------------ hc real

fn hc_json(hc: &Path, args: &[&str]) -> Result<Value, String> {
    let mut command = Command::new(hc);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| format!("hc {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "hc {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("hc {} no devolvió JSON: {error}", args.join(" ")))
}

fn hc_call(hc: &Path, args: &[&str]) -> bool {
    let mut command = Command::new(hc);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.output().is_ok_and(|output| output.status.success())
}

/// Devuelve el foco a donde estaba antes de abrir la pestaña del reintento.
fn restore_focus(herdr: &Path, pane: &str, agent: &str) {
    let args: Vec<&str> = if !pane.is_empty() {
        vec!["do", "pane.focus", "--pane", pane]
    } else if !agent.is_empty() {
        vec!["do", "pane.focus", "--terminal", agent]
    } else {
        return;
    };
    let _ = hc_call(herdr, &args);
}

/// Un `Real` tiene todo lo de la prueba real; su `Drop` cierra lo que abrió.
struct Real {
    hc: PathBuf,
    temp: PathBuf,
    id: String,
    app: VoiceApp,
    chat: Arc<ChatModel>,
    registry: Registry,
    old_name: String,
    dead_name: String,
    old_terminal: String,
    dead_terminal: String,
    logs: PathBuf,
    subagents: PathBuf,
    registros: PathBuf,
    fake_ws: PathBuf,
    fake_agent: PathBuf,
    herdr: PathBuf,
    focus_pane: String,
    focus_agent: String,
}

impl Drop for Real {
    fn drop(&mut self) {
        self.app.shutdown();
        close_test_terminals(&self.hc, &self.id);
        restore_focus(&self.herdr, &self.focus_pane, &self.focus_agent);
        let _ = std::fs::remove_dir_all(&self.temp);
    }
}

impl Real {
    /// Lanza los dos hijos falsos en `hc` y les deja sus registros.
    fn launch(
        &mut self,
        fake_ws: &Path,
        fake_agent: &Path,
        registros: &Path,
    ) -> Result<(), String> {
        let limit = vec![
            fake_ws.to_string_lossy().into_owned(),
            "--echo".into(),
            registros.to_string_lossy().into_owned(),
            "--as".into(),
            "opencode".into(),
        ];
        let dead = vec![
            fake_agent.to_string_lossy().into_owned(),
            "--agent".into(),
            "opencode".into(),
            "--scene".into(),
            "working".into(),
            "--delay-ms".into(),
            "0".into(),
        ];
        self.old_terminal = self.launch_child(&self.old_name.clone(), &limit)?;
        self.dead_terminal = self.launch_child(&self.dead_name.clone(), &dead)?;
        std::thread::sleep(Duration::from_millis(700));
        hc_json(
            &self.hc,
            &[
                "send",
                &self.old_terminal.clone(),
                "Free limit reached. Resets at 7:00 p.m.",
                "--json",
            ],
        )?;
        for item in [
            record(
                &self.old_name.clone(),
                &self.old_terminal.clone(),
                OLD_MODEL,
            ),
            record(
                &self.dead_name.clone(),
                &self.dead_terminal.clone(),
                OLD_MODEL,
            ),
        ] {
            self.registry
                .create(&item)
                .map_err(|error| error.message().to_string())?;
        }
        Ok(())
    }

    fn launch_child(&self, name: &str, argv: &[String]) -> Result<String, String> {
        let label = format!("ez.nv3c={}", self.id);
        let mut args: Vec<String> = vec![
            "run".into(),
            "--json".into(),
            "--bg".into(),
            "--no-parent".into(),
            "--name".into(),
            name.into(),
            "--cwd".into(),
            self.temp.to_string_lossy().into_owned(),
            "--no-notify".into(),
            "--label".into(),
            label,
            "--label".into(),
            "ez.role=developer".into(),
            "--label".into(),
            "ez.agent=opencode".into(),
            "--".into(),
        ];
        args.extend(argv.iter().cloned());
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let data = hc_json(&self.hc, &borrowed)?;
        let terminal = data
            .get("terminal")
            .or_else(|| data.get("address"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if terminal.is_empty() {
            return Err(format!("hc run no devolvió terminal: {data}"));
        }
        Ok(terminal)
    }

    /// Espera los anuncios y deja el estado real de los dos hijos.
    fn wait_states(&self) -> (bool, bool) {
        let blocked = wait_record(&self.registry, &self.old_name, TaskState::Blocked, 20)
            && wait_for_note(&self.app, &self.chat, "está bloqueado", 20);
        let failed = wait_record(&self.registry, &self.dead_name, TaskState::Failed, 20)
            && wait_for_note(&self.app, &self.chat, "falló", 20);
        for text in notes(&self.chat) {
            if text.contains("está bloqueado") || text.contains("falló") {
                eprintln!("ANUNCIO: {text}");
            }
        }
        (blocked, failed)
    }

    /// Acepta la oferta como el turno de voz y comprueba el hijo nuevo.
    fn retry(&self) -> Result<RetryFacts, String> {
        if self.app.pending_retry().as_deref() != Some(self.old_name.as_str()) {
            return Err("no hay oferta de reintento".to_string());
        }
        self.app.start_call();
        let generation = ez_app::app::core::advance_turn(&self.app);
        // DECISION: la oferta se acepta por el mismo camino que una
        // transcripción de voz; así se ejercita `answer_retry_offer` y el `ez`
        // real de producción, sin abrir una API nueva solo para el humo.
        self.app
            .handle_event(AppEvent::Transcribed("sí".to_string(), generation));
        let new_name = format!("{}-r1", self.old_name);
        let waited = wait_record(&self.registry, &new_name, TaskState::InProgress, 90)
            || wait_for_note(&self.app, &self.chat, "Listo:", 90);
        let record = self.registry.get("voz", &new_name).ok().flatten();
        let terminal = record
            .as_ref()
            .map(|item| item.terminal.clone())
            .unwrap_or_default();
        let parent = if terminal.is_empty() {
            String::new()
        } else {
            hc_json(&self.hc, &["get", &terminal])
                .ok()
                .and_then(|data| {
                    data.get("parent")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default()
        };
        let old_alive = !self.old_terminal.is_empty()
            && hc_json(&self.hc, &["get", &self.old_terminal])
                .ok()
                .and_then(|data| {
                    data.get("state")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .is_some_and(|state| state == "live");
        Ok(RetryFacts {
            name: new_name,
            terminal,
            parent,
            old_alive,
            retry_of: record.and_then(|item| item.retry_of),
            waited,
        })
    }
}

struct RetryFacts {
    name: String,
    terminal: String,
    parent: String,
    old_alive: bool,
    retry_of: Option<String>,
    waited: bool,
}

fn wait_record(registry: &Registry, name: &str, state: TaskState, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if registry
            .get("voz", name)
            .ok()
            .flatten()
            .is_some_and(|item| item.state == state)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn close_test_terminals(hc: &Path, id: &str) {
    let Ok(data) = hc_json(hc, &["ls", "--all", "--json"]) else {
        return;
    };
    // DECISION: solo se cierran las terminales con el id de la corrida o el
    // prefijo `nv3c`; las de la persona no se tocan.
    let Some(terminals) = data.get("terminals").and_then(Value::as_array) else {
        return;
    };
    let prefix = id.trim_end_matches(|character: char| character.is_ascii_digit());
    for terminal in terminals {
        let text = terminal.to_string();
        let name = terminal.get("name").and_then(Value::as_str).unwrap_or("");
        if !text.contains(id) && !name.starts_with(prefix) {
            continue;
        }
        if let Some(address) = terminal.get("address").and_then(Value::as_str) {
            let _ = hc_call(hc, &["kill", address]);
            let _ = hc_call(hc, &["rm", address]);
        }
    }
}

fn count_test_terminals(hc: &Path, id: &str) -> usize {
    hc_json(hc, &["ls", "--all", "--json"])
        .ok()
        .and_then(|data| data.get("terminals").and_then(Value::as_array).cloned())
        .map(|items| {
            items
                .iter()
                .filter(|item| item.to_string().contains(id))
                .count()
        })
        .unwrap_or(0)
}

pub(super) fn run() -> i32 {
    match setup() {
        Ok(mut real) => real.flow(),
        Err(code) => code,
    }
}

fn setup() -> Result<Real, i32> {
    let temp = temp_dir();
    let logs = temp.join("logs");
    let subagents = temp.join("subagentes");
    let appdata = temp.join("appdata");
    let work = temp.join("work");
    let registros = temp.join("registros");
    for folder in [&logs, &subagents, &work, &registros] {
        let _ = std::fs::create_dir_all(folder);
    }
    std::env::set_var("APPDATA", &appdata);
    std::env::set_var("LOCALAPPDATA", temp.join("localappdata"));
    std::env::set_var("EZAGENT_LOGS_DIR", &logs);
    std::env::set_var("EZ_SUBAGENTS_DIR", &subagents);
    std::env::set_var("MINIMAX_DATA_DIR", temp.join("minimax"));
    std::env::set_var("CODEX_HOME", temp.join("codex"));
    std::env::set_var("OPENCODE_CONFIG_DIR", temp.join("opencode-config"));
    for name in [
        "EZ_URL",
        "EZ_TOKEN",
        "MINIMAX_API_KEY",
        "OPENCODE_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
    ] {
        std::env::remove_var(name);
    }
    ez_core::panics::install_hook_in(&logs);
    let Some(hc) = which("hc") else {
        return missing(&logs, &temp, "No encuentro `hc` (herdr-core) en el PATH.");
    };
    let Some(herdr) = which("herdr-app") else {
        return missing(&logs, &temp, "No encuentro `herdr-app` en el PATH.");
    };
    let Some(fake_ws) = find_binary("fake-ws-agent") else {
        return missing(
            &logs,
            &temp,
            "Compila `cargo build -p ez-fakes --bin fake-ws-agent`.",
        );
    };
    let Some(fake_agent) = find_binary("fake-agent") else {
        return missing(
            &logs,
            &temp,
            "Compila `cargo build -p ez-fakes --bin fake-agent`.",
        );
    };
    if find_binary("ez").is_none() {
        return missing(&logs, &temp, "Compila `cargo build -p ez-ctl --bin ez`.");
    }
    let bin = temp.join("bin");
    let _ = std::fs::create_dir_all(&bin);
    // DECISION: todos los tipos del catálogo apuntan a copias de
    // `fake-ws-agent`, en el PATH y en `agents`; ningún CLI real puede abrir.
    for name in [
        "opencode", "claude", "codex", "qwen", "mcode", "minimax", "pi",
    ] {
        let _ = std::fs::copy(&fake_ws, bin.join(format!("{name}.exe")));
    }
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let joined =
        std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(&old_path)))
            .unwrap_or_else(|_| bin.clone().into_os_string());
    std::env::set_var("PATH", joined);
    let mut agents = serde_json::Map::new();
    for kind in [
        "opencode", "claude", "codex", "qwen", "mcode", "minimax", "pi",
    ] {
        agents.insert(
            kind.to_string(),
            json!([
                bin.join("opencode.exe").to_string_lossy(),
                "--echo",
                registros.to_string_lossy(),
                "--as",
                kind
            ]),
        );
    }
    let config = match seed_config(&appdata, &hc, &herdr, Value::Object(agents), &work) {
        Ok(config) => config,
        Err(error) => return missing(&logs, &temp, &format!("configuración: {error}")),
    };
    let modelos = format!(
        "[{OLD_MODEL}]\nagente = \"opencode\"\nnivel = \"medio\"\ncosto = \"gratis\"\nfamilia = \"zhipu\"\n\n\
         [{PLAN_MODEL}]\nagente = \"opencode\"\nnivel = \"medio\"\ncosto = \"plan\"\nfamilia = \"deepseek\"\n"
    );
    let _ = std::fs::write(appdata.join("EzAgent").join("modelos.toml"), modelos);
    if !hc_call(&herdr, &["ls"]) {
        return missing(
            &logs,
            &temp,
            "La ventana de Herdr no responde (`herdr-app ls`).",
        );
    }
    let focus = hc_json(&herdr, &["status"]).ok();
    let focus_pane = focus
        .as_ref()
        .and_then(|data| data.get("focused_pane").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let focus_agent = focus
        .as_ref()
        .and_then(|data| data.get("agent").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let chat = Arc::new(ChatModel::new(None, false));
    let watch = SubagentWatch {
        runtime: Arc::new(HcRuntime::new(
            system_runtime(),
            hc.to_string_lossy().into_owned(),
        )),
        novelty_file: Some(temp.join("novedades.toml")),
        interval_ms: 500,
        grace_ms: 1_000,
    };
    let app = match build_app(&config, &chat, Some(watch)) {
        Ok(app) => app,
        Err(error) => return missing(&logs, &temp, &format!("app: {error}")),
    };
    let id = format!("nv3c{}", std::process::id());
    let real = Real {
        hc: hc.clone(),
        temp: temp.clone(),
        id: id.clone(),
        app,
        chat,
        registry: Registry::at(&subagents),
        old_name: format!("{id}a"),
        dead_name: format!("{id}b"),
        old_terminal: String::new(),
        dead_terminal: String::new(),
        logs: logs.clone(),
        subagents: subagents.clone(),
        registros: registros.clone(),
        fake_ws: fake_ws.clone(),
        fake_agent: fake_agent.clone(),
        herdr: herdr.clone(),
        focus_pane,
        focus_agent,
    };
    Ok(real)
}

impl Real {
    fn flow(&mut self) -> i32 {
        eprintln!(
            "app viva; hc real {}; ez {}",
            self.hc.display(),
            find_binary("ez").unwrap_or_default().display()
        );
        let fake_ws = self.fake_ws.clone();
        let fake_agent = self.fake_agent.clone();
        let registros = self.registros.clone();
        if let Err(error) = self.launch(&fake_ws, &fake_agent, &registros) {
            return self.fail(&format!("lanzamiento: {error}"));
        }
        eprintln!(
            "hijos reales: {} ({}) y {} ({})",
            self.old_name, self.old_terminal, self.dead_name, self.dead_terminal
        );
        let (blocked, failed) = self.wait_states();
        let detail = |name: &str| {
            self.registry
                .get("voz", name)
                .ok()
                .flatten()
                .map(|item| {
                    format!(
                        "{} ({})",
                        item.state.as_str(),
                        item.history
                            .last()
                            .map(|change| change.by.clone())
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_else(|| "sin registro".to_string())
        };
        eprintln!(
            "estado real: {} = {}; {} = {}",
            self.old_name,
            detail(&self.old_name.clone()),
            self.dead_name,
            detail(&self.dead_name.clone())
        );
        let usage = std::fs::read_to_string(self.subagents.join("uso.json")).unwrap_or_default();
        eprintln!("uso.json con {OLD_MODEL}: {}", usage.contains(OLD_MODEL));
        eprintln!("oferta de reintento: {:?}", self.app.pending_retry());
        let facts = match self.retry() {
            Ok(facts) => facts,
            Err(error) => return self.fail(&format!("reintento: {error}")),
        };
        let retry_ok = facts.waited
            && facts.retry_of.as_deref() == Some(self.old_name.as_str())
            && facts.parent != "voz"
            && !facts.old_alive;
        eprintln!(
            "reintento: {} terminal={} padre_hc={:?} viejo_vivo={} retry_of={:?}",
            facts.name, facts.terminal, facts.parent, facts.old_alive, facts.retry_of
        );
        let alive = !self.app.state().is_empty();
        self.app.shutdown();
        close_test_terminals(&self.hc, &self.id);
        restore_focus(&self.herdr, &self.focus_pane, &self.focus_agent);
        let leftovers = count_test_terminals(&self.hc, &self.id);
        let panics = report_panics(&self.logs);
        let ok = blocked && failed && retry_ok && alive && leftovers == 0 && panics == 0;
        eprintln!(
        "RESULT modo=real-hc bloqueado={blocked} fallido={failed} reintento={retry_ok} padre_voz={} viejo_vivo={} viva={alive} restos={leftovers} panico={panics}",
        facts.parent == "voz", facts.old_alive
    );
        if ok {
            0
        } else {
            1
        }
    }
}

impl Real {
    fn fail(&self, message: &str) -> i32 {
        eprintln!("{message}");
        let _ = report_panics(&self.temp.join("logs"));
        1
    }
}

fn missing(logs: &Path, temp: &Path, message: &str) -> Result<Real, i32> {
    let _ = report_panics(logs);
    eprintln!("{message}");
    let _ = std::fs::remove_dir_all(temp);
    Err(2)
}
