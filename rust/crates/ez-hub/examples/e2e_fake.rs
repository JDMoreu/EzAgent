//! Prueba de punta a punta con agentes falsos y herdr-core real (ECO-8).
//!
//! Se ejecuta con `cargo run -p ez-hub --example e2e_fake`. Sale 0 si el ciclo
//! completo termina con dos rondas limpias y la rama se integra; 1 si falta
//! algo del producto (con los últimos eventos), 2 si no hay `hc`.

#[path = "e2e_fake/fixtures.rs"]
mod fixtures;
#[path = "e2e_fake/scenario.rs"]
mod scenario;
#[path = "e2e_fake/watch.rs"]
mod watch;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant};

use ez_hub::runtime::spaces::MainChoice;
use ez_hub::runtime::{HubRuntime, RuntimeOptions};
use ez_term::locate_hc;
use ez_term::HcClient;
use serde_json::Value;

use fixtures::{command, prepare, Layout};
use watch::DiskCheck;

const DEADLINE: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(500);

/// Mata y borra solo las terminales y la carpeta que creó este ejemplo.
struct Cleanup {
    space: String,
    root: PathBuf,
    hc: Option<HcClient>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(client) = &self.hc {
            let labels = BTreeMap::from([("ez_space".to_string(), self.space.clone())]);
            if let Ok(terminals) = client.list(&labels) {
                for terminal in &terminals {
                    let _ = client.kill(&terminal.address);
                }
                for terminal in &terminals {
                    let _ = client.rm(&terminal.address);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn emit(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

fn tail_text(tail: &[Value]) -> String {
    serde_json::to_string(tail).unwrap_or_else(|_| "sin eventos legibles".to_string())
}

fn run_flow(layout: &mut Layout) -> Result<String, String> {
    let (runtime, elephant) = start_engine(layout)?;
    layout.elephant = elephant;
    drive(&runtime, layout)?;
    verify(layout)
}

fn start_engine(layout: &Layout) -> Result<(HubRuntime, String), String> {
    let options = RuntimeOptions {
        ez_dir: Some(layout.bins.clone()),
    };
    let runtime = HubRuntime::start_with(&layout.data, options)?;
    if runtime.status().mode != "normal" {
        return Err(format!("el motor no arrancó: {}", runtime.status().reason));
    }
    let choice = MainChoice {
        agent: "fake_main".to_string(),
        model: "main-1".to_string(),
        effort: None,
    };
    let agent = runtime.main_agent_create(&layout.space, "Principal falso", &choice)?;
    emit(&format!(
        "Motor normal en {}; agente principal {}.",
        runtime.status().url,
        agent.id
    ));
    emit(&format!(
        "Agente principal {} lanzado en {}.",
        agent.id, agent.address
    ));
    Ok((runtime, agent.id))
}

fn drive(runtime: &HubRuntime, layout: &Layout) -> Result<(), String> {
    let deadline = Instant::now() + DEADLINE;
    let mut handled = std::collections::BTreeSet::new();
    let mut closed = false;
    let mut integrated = false;
    loop {
        let seen = watch::read(runtime, &layout.space, &layout.elephant)?;
        let tail = seen.tail;
        for card in seen.cards {
            if handled.insert(card.id.clone()) {
                handle_card(runtime, &card, &tail)?;
            }
        }
        if seen.kinds.contains("rejected") {
            return Err(format!(
                "el árbitro rechazó un comando; últimos eventos: {}",
                tail_text(&tail)
            ));
        }
        closed |= seen.kinds.contains("task_closed");
        integrated |= seen.kinds.contains("integrated");
        if closed && integrated {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "tope de diez minutos: task_closed={closed}, integrated={integrated}; últimos eventos: {}",
                tail_text(&tail)
            ));
        }
        sleep(POLL);
    }
}

fn handle_card(runtime: &HubRuntime, card: &watch::Card, tail: &[Value]) -> Result<(), String> {
    match card.kind.as_str() {
        "task" | "team" => {
            runtime.card_action(&card.id, "approve", None)?;
            emit(&format!("Aprobada la tarjeta {} ({}).", card.id, card.kind));
        }
        "integrate" => {
            runtime.card_action(&card.id, "merge", None)?;
            emit(&format!("Integrada la rama desde la tarjeta {}.", card.id));
        }
        "pause" => {
            return Err(format!(
                "el árbitro pausó la tarea (tarjeta {}); últimos eventos: {}",
                card.id,
                tail_text(tail)
            ));
        }
        _ => {}
    }
    Ok(())
}

fn verify(layout: &Layout) -> Result<String, String> {
    let DiskCheck {
        clean_rounds,
        leaked_token,
    } = watch::on_disk(&layout.data, &layout.token)?;
    if clean_rounds < 2 {
        return Err(format!(
            "la bitácora tiene {clean_rounds} rondas limpias; hacen falta 2"
        ));
    }
    if leaked_token {
        return Err("aparece un token en la bitácora o en estado.json".to_string());
    }
    let files = fake_files(&layout.repo)?;
    if files.is_empty() {
        return Err("la rama base no tiene archivos fake/*.txt".to_string());
    }
    let merge = git_head(&layout.repo, &["rev-list", "--merges", "-1", "HEAD"])?;
    if merge.is_empty() {
        return Err("la rama base no tiene commit de integración".to_string());
    }
    Ok(format!(
        "Éxito: task_closed, integrated, {clean_rounds} rondas limpias, {} archivos fake y merge {}.",
        files.len(),
        &merge[..merge.len().min(8)]
    ))
}

fn fake_files(repo: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    let folder = repo.join("fake");
    let entries = std::fs::read_dir(&folder)
        .map_err(|error| format!("no pude leer {}: {error}", folder.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("txt") {
            files.push(path);
        }
    }
    Ok(files)
}

fn git_head(repo: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = command("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("no pude ejecutar git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} falló: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn execute() -> i32 {
    let Some(hc) = locate_hc(None) else {
        emit("Falta herdr-core (hc). Instálalo y vuelve a ejecutar el ejemplo.");
        return 2;
    };
    let mut layout = match prepare() {
        Ok(layout) => layout,
        Err(error) => {
            emit(&format!("No pude preparar el ejemplo: {error}"));
            return 1;
        }
    };
    let _cleanup = Cleanup {
        hc: HcClient::connect(&hc).ok(),
        space: layout.space.clone(),
        root: layout.root.clone(),
    };
    match run_flow(&mut layout) {
        Ok(summary) => {
            emit(&summary);
            0
        }
        Err(reason) => {
            emit(&format!("Fallo del ejemplo: {reason}"));
            1
        }
    }
}

fn main() -> ExitCode {
    ExitCode::from(execute() as u8)
}
