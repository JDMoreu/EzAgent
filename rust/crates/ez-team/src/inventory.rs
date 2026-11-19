//! Inventory of available agents (design §6): is the binary in `PATH` (or an absolute
//! path), does a subscription name it, and which models does it offer.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::family::family;
use crate::profiles::AgentProfile;
use crate::subscriptions::Subscription;

/// Upper bound for `modelos_cmd` (design §6).
pub const MODELS_TIMEOUT: Duration = Duration::from_secs(20);

/// The local machine, behind a trait so tests can fake it.
pub trait System: Send + Sync {
    /// Environment variable (`PATH`, `PATHEXT`).
    fn var(&self, name: &str) -> Option<String>;
    /// True when `path` is an existing file.
    fn is_file(&self, path: &Path) -> bool;
    /// Runs `argv` without a shell and returns its stdout; `None` on failure or timeout.
    fn run(&self, argv: &[String], timeout: Duration) -> Option<String>;
}

/// Real machine: process environment, filesystem and `std::process` (no shell, no console).
pub struct RealSystem;

impl System for RealSystem {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn run(&self, argv: &[String], timeout: Duration) -> Option<String> {
        let (program, arguments) = argv.split_first()?;
        let mut command = Command::new(program);
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let reader = std::thread::Builder::new()
            .name("ez-team-models".to_string())
            .spawn(move || {
                let mut bytes = Vec::new();
                let mut stdout = stdout;
                let _ = stdout.read_to_end(&mut bytes);
                String::from_utf8_lossy(&bytes).into_owned()
            })
            .ok();
        let reader = match reader {
            Some(handle) => handle,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        };
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return None;
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return None;
                }
            }
        };
        let text = reader.join().ok()?;
        status.success().then_some(text)
    }
}

/// An agent that is installed and named by a declared subscription (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableAgent {
    pub profile: AgentProfile,
    /// Path of the executable found for `argv[0]`.
    pub path: PathBuf,
    /// Models offered by the agent (from `modelos_cmd` or the `modelos` field).
    pub models: Vec<String>,
    /// Distinct families of `models`, in order of appearance.
    pub families: Vec<String>,
}

/// Result of [`detect`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    pub agents: Vec<AvailableAgent>,
}

impl Inventory {
    /// The available agent with this id, if any.
    pub fn find(&self, agent_id: &str) -> Option<&AvailableAgent> {
        self.agents
            .iter()
            .find(|agent| agent.profile.id == agent_id)
    }

    /// Text printed by `ez inventory`: one section per agent, models grouped by family.
    pub fn text(&self) -> String {
        if self.agents.is_empty() {
            return "No encontré agentes disponibles. Revisa agentes.toml y suscripciones.toml."
                .to_string();
        }
        let sections: Vec<String> = self
            .agents
            .iter()
            .map(|agent| {
                let mut lines = vec![format!(
                    "{} ({}) · {}",
                    agent.profile.name,
                    agent.profile.id,
                    agent.path.display()
                )];
                if agent.models.is_empty() {
                    lines.push("  (sin modelos)".to_string());
                }
                for family_name in &agent.families {
                    let models: Vec<&str> = agent
                        .models
                        .iter()
                        .filter(|model| family(&agent.profile, model) == *family_name)
                        .map(String::as_str)
                        .collect();
                    lines.push(format!("  {family_name}: {}", models.join(", ")));
                }
                lines.join("\n")
            })
            .collect();
        sections.join("\n\n")
    }

    /// Same data as [`Inventory::text`], for `ez inventory --json`.
    pub fn json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.agents
                .iter()
                .map(|agent| {
                    serde_json::json!({
                        "id": agent.profile.id,
                        "nombre": agent.profile.name,
                        "ruta": agent.path.display().to_string(),
                        "modelos": agent.models,
                        "familias": agent.families,
                    })
                })
                .collect(),
        )
    }
}

/// Process-wide cache of `modelos_cmd` output, keyed by the command line.
fn models_cache() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Detects the available agents: executable present and named by a subscription.
///
/// Without declared subscriptions every detected agent counts. A profile without
/// `argv` is ignored because there is nothing to run.
pub fn detect(profiles: &[AgentProfile], subs: &[Subscription], system: &dyn System) -> Inventory {
    let search_path = system.var("PATH").unwrap_or_default();
    let extensions = system
        .var("PATHEXT")
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
    let declared: Vec<&str> = subs
        .iter()
        .flat_map(|subscription| subscription.agents.iter().map(String::as_str))
        .collect();
    let agents = profiles
        .iter()
        .filter(|profile| declared.is_empty() || declared.contains(&profile.id.as_str()))
        .filter_map(|profile| {
            let program = profile.argv.first()?;
            let path = find_program(program, &search_path, &extensions, system)?;
            let models = models_for(profile, system);
            let families = families_of(profile, &models);
            Some(AvailableAgent {
                profile: profile.clone(),
                path,
                models,
                families,
            })
        })
        .collect();
    Inventory { agents }
}

fn find_program(
    name: &str,
    search_path: &str,
    extensions: &str,
    system: &dyn System,
) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() || candidate.components().count() > 1 {
        return system.is_file(candidate).then(|| candidate.to_path_buf());
    }
    for directory in split_list(search_path) {
        let directory = Path::new(directory);
        let direct = directory.join(name);
        if system.is_file(&direct) {
            return Some(direct);
        }
        if candidate.extension().is_some() {
            continue;
        }
        for extension in split_list(extensions) {
            let extension = extension.trim_start_matches('.');
            for spelling in [extension.to_string(), extension.to_lowercase()] {
                let with_extension = directory.join(format!("{name}.{spelling}"));
                if system.is_file(&with_extension) {
                    return Some(with_extension);
                }
            }
        }
    }
    None
}

fn split_list(value: &str) -> impl Iterator<Item = &str> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    value.split(separator).filter(|part| !part.is_empty())
}

fn models_for(profile: &AgentProfile, system: &dyn System) -> Vec<String> {
    if let Some(command) = profile.models_cmd.as_ref().filter(|argv| !argv.is_empty()) {
        let key = command.join("\u{1f}");
        // DECISION: only successful outputs are cached; a failure falls back to
        // `modelos` and is retried on the next detection.
        let cached = models_cache()
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned());
        if let Some(models) = cached {
            return models;
        }
        if let Some(stdout) = system.run(command, MODELS_TIMEOUT) {
            let models: Vec<String> = stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            if let Ok(mut cache) = models_cache().lock() {
                cache.insert(key, models.clone());
            }
            return models;
        }
    }
    profile.models.clone()
}

fn families_of(profile: &AgentProfile, models: &[String]) -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    for model in models {
        let name = family(profile, model);
        if !name.is_empty() && !families.contains(&name) {
            families.push(name);
        }
    }
    families
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::{detect, Inventory, System};
    use crate::profiles::AgentProfile;
    use crate::subscriptions::Subscription;

    #[derive(Default)]
    struct FakeSystem {
        vars: HashMap<String, String>,
        files: Vec<PathBuf>,
        outputs: HashMap<String, Option<String>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeSystem {
        fn with_path(path: &str) -> Self {
            let mut system = Self::default();
            system.vars.insert("PATH".to_string(), path.to_string());
            system
                .vars
                .insert("PATHEXT".to_string(), ".COM;.EXE;.BAT;.CMD".to_string());
            system
        }

        fn add_file(mut self, path: &str) -> Self {
            self.files.push(PathBuf::from(path));
            self
        }

        fn add_output(mut self, command: &[&str], stdout: Option<&str>) -> Self {
            self.outputs
                .insert(command.join(" "), stdout.map(str::to_string));
            self
        }
    }

    impl System for FakeSystem {
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }

        fn is_file(&self, path: &Path) -> bool {
            self.files.iter().any(|file| file == path)
        }

        fn run(&self, argv: &[String], _timeout: Duration) -> Option<String> {
            self.calls.lock().expect("calls").push(argv.join(" "));
            self.outputs.get(&argv.join(" ")).cloned().flatten()
        }
    }

    fn profile(id: &str, argv: &[&str]) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            name: id.to_string(),
            argv: argv.iter().map(|arg| arg.to_string()).collect(),
            models_cmd: None,
            models: Vec::new(),
            family: None,
            effort_args: Vec::new(),
        }
    }

    #[test]
    fn finds_agent_in_path_with_pathext() {
        let system =
            FakeSystem::with_path(r"C:\Windows;C:\tools").add_file(r"C:\tools\opencode.exe");
        let profiles = [profile("opencode", &["opencode", "run"])];
        let inventory = detect(&profiles, &[], &system);
        assert_eq!(inventory.agents.len(), 1);
        assert_eq!(
            inventory.agents[0].path,
            PathBuf::from(r"C:\tools\opencode.exe")
        );
    }

    #[test]
    fn finds_agent_by_absolute_path() {
        let system = FakeSystem::with_path(r"C:\tools").add_file(r"C:\bin\claude.exe");
        let profiles = [profile("claude", &[r"C:\bin\claude.exe"])];
        let inventory = detect(&profiles, &[], &system);
        assert_eq!(inventory.agents.len(), 1);
        assert_eq!(
            inventory.agents[0].path,
            PathBuf::from(r"C:\bin\claude.exe")
        );
    }

    #[test]
    fn missing_binary_is_not_available() {
        let system = FakeSystem::with_path(r"C:\tools");
        let profiles = [profile("opencode", &["opencode"])];
        assert!(detect(&profiles, &[], &system).agents.is_empty());
    }

    #[test]
    fn subscription_filters_agents() {
        let system = FakeSystem::with_path(r"C:\tools")
            .add_file(r"C:\tools\opencode.exe")
            .add_file(r"C:\tools\codex.exe");
        let profiles = [
            profile("opencode", &["opencode"]),
            profile("codex", &["codex"]),
        ];
        let subs = [Subscription {
            name: "OpenCode Go".to_string(),
            agents: vec!["opencode".to_string()],
        }];
        let inventory = detect(&profiles, &subs, &system);
        assert_eq!(inventory.agents.len(), 1);
        assert_eq!(inventory.agents[0].profile.id, "opencode");
    }

    #[test]
    fn without_subscriptions_every_agent_counts() {
        let system = FakeSystem::with_path(r"C:\tools")
            .add_file(r"C:\tools\opencode.exe")
            .add_file(r"C:\tools\codex.exe");
        let profiles = [
            profile("opencode", &["opencode"]),
            profile("codex", &["codex"]),
        ];
        assert_eq!(detect(&profiles, &[], &system).agents.len(), 2);
    }

    #[test]
    fn models_cmd_output_is_parsed_and_cached() {
        let system = FakeSystem::with_path(r"C:\tools")
            .add_file(r"C:\tools\opencode.exe")
            .add_output(&["models-test-cached"], Some("a/one\n\n a/two \n"));
        let mut opencode = profile("opencode", &["opencode"]);
        opencode.models_cmd = Some(vec!["models-test-cached".to_string()]);
        let profiles = [opencode];
        let first = detect(&profiles, &[], &system);
        assert_eq!(first.agents[0].models, ["a/one", "a/two"]);
        let second = detect(&profiles, &[], &system);
        assert_eq!(second.agents[0].models, ["a/one", "a/two"]);
        assert_eq!(system.calls.lock().expect("calls").len(), 1);
    }

    #[test]
    fn failing_models_cmd_falls_back_to_the_list() {
        let system = FakeSystem::with_path(r"C:\tools")
            .add_file(r"C:\tools\broken.exe")
            .add_output(&["models-test-broken"], None);
        let mut broken = profile("broken", &["broken"]);
        broken.models_cmd = Some(vec!["models-test-broken".to_string()]);
        broken.models = vec!["fallback-model".to_string()];
        let inventory = detect(&[broken], &[], &system);
        assert_eq!(inventory.agents[0].models, ["fallback-model"]);
    }

    #[test]
    fn real_system_sees_the_environment_and_files() {
        let system = super::RealSystem;
        let executable = std::env::current_exe().expect("current executable");
        assert!(system.is_file(&executable));
        assert!(!system.is_file(Path::new(r"C:\ez-team\no-existe.exe")));
        assert!(system.var("PATH").is_some());
    }

    #[test]
    fn text_and_json_describe_the_inventory() {
        let empty = Inventory::default();
        assert_eq!(
            empty.text(),
            "No encontré agentes disponibles. Revisa agentes.toml y suscripciones.toml."
        );
        assert_eq!(empty.json(), serde_json::json!([]));

        let system = FakeSystem::with_path(r"C:\tools").add_file(r"C:\tools\opencode.exe");
        let mut opencode = profile("opencode", &["opencode"]);
        opencode.name = "OpenCode".to_string();
        opencode.models = vec![
            "go/deepseek-v4.1-flash".to_string(),
            "go/qwen4-max".to_string(),
        ];
        let inventory = detect(&[opencode], &[], &system);
        let text = inventory.text();
        assert!(text.contains("OpenCode (opencode)"), "{text}");
        assert!(text.contains("deepseek: go/deepseek-v4.1-flash"), "{text}");
        assert!(text.contains("qwen: go/qwen4-max"), "{text}");
        let json = inventory.json();
        assert_eq!(json[0]["id"], "opencode");
        assert_eq!(json[0]["modelos"][1], "go/qwen4-max");
        assert_eq!(json[0]["familias"][1], "qwen");
    }
}
