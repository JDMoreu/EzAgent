//! NV-3: la app de voz como padre de sus subagentes, sin micrófono.
//!
//! Dos modos, siempre con `APPDATA`, `EZAGENT_LOGS_DIR` y `EZ_SUBAGENTS_DIR`
//! temporales:
//!
//! - por omisión, `fake-hc`: el `fake-hc` de `ez-fakes` sirve pantallas y
//!   terminales; rápido y sin tocar Herdr.
//! - `--real-hc`: el `hc` real y su `herdr-app`; dos hijos falsos de `ez-fakes`
//!   viven en el Herdr de la persona con nombre único y etiqueta `ez.nv3c`; uno
//!   queda parado con un límite de uso y otro sale sin informe. La app los
//!   anuncia y acepta el reintento con el `ez` compilado; al final se cierran
//!   todas sus terminales.
//!
//! Ningún agente real se ejecuta: el `PATH` apunta a copias de `fake-ws-agent`.
//! No abre micrófono, no habla de verdad y no toca la configuración, el
//! historial, los registros ni las terminales de la persona.
//!
//! ```text
//! cargo build -p ez-fakes --bin fake-hc --bin fake-agent --bin fake-ws-agent
//! cargo build -p ez-ctl --bin ez
//! cargo build -p ez-app --example diag_subagents_smoke
//! ./target/debug/examples/diag_subagents_smoke.exe             # fake-hc
//! ./target/debug/examples/diag_subagents_smoke.exe --real-hc    # hc real
//! ```
//!
//! DX-7b: pone `EZAGENT_TEST_MONITOR=secondary` y comprueba antes de arrancar
//! que la pantalla secundaria exista; si no, falla sin lanzar la app. No hay
//! ruta de prueba hacia la principal. Este humo no abre el HUD (`hud_hidden`),
//! así que no tiene ventanas propias que comprobar.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::{AppEvent, AppParts, SpeakerApi, SubagentWatch, VoiceApp};
use ez_app::app::live::LiveHooks;
use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::system_runtime;
use ez_herdr::subagents::{epoch_ms, Change, HcRuntime, Record, Registry, TaskState};
use serde_json::{json, Value};

// DECISION: el modo real vive en un submódulo para respetar el tope de 600
// líneas por archivo; el ejemplo y su CLI siguen siendo los mismos.
#[path = "diag_subagents_smoke/real.rs"]
mod real;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// El modelo agotado del hijo bloqueado y el que debe elegir `--auto`.
const OLD_MODEL: &str = "glm-5.3";
const PLAN_MODEL: &str = "deepseek-v4.1-flash";

struct SilentSpeaker;

impl SpeakerApi for SilentSpeaker {
    fn synthesize(&self, _: &str) -> Result<(Vec<f32>, u32), String> {
        Ok((Vec::new(), 16_000))
    }
    fn play(
        &self,
        _: &[f32],
        _: u32,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }
    fn speak(
        &self,
        _: &str,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }
}

fn temp_dir() -> PathBuf {
    let unique = format!(
        "ezagent-nv3-humo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|folder| folder.join(format!("{name}.exe")))
        .find(|candidate| candidate.is_file())
}

fn find_binary(name: &str) -> Option<PathBuf> {
    let folder = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        folder.join(format!("{name}.exe")),
        folder.join("../").join(format!("{name}.exe")),
        folder.join("../../").join(format!("{name}.exe")),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
    .or_else(|| which(name))
}

fn seed_config(
    appdata: &Path,
    hc: &Path,
    herdr: &Path,
    agents: Value,
    cwd: &Path,
) -> std::io::Result<PathBuf> {
    let folder = appdata.join("EzAgent");
    std::fs::create_dir_all(&folder)?;
    let config = json!({
        // DECISION: sin barra visible; la prueba mira el chat y la voz, no el HUD.
        "hud_hidden": true, "smart_turn": false, "chat_voice": false,
        "hc_path": hc.to_string_lossy(), "herdr_app_path": herdr.to_string_lossy(),
        "default_cwd": cwd.to_string_lossy(), "agents": agents,
        "agent_profiles_version": 8
    });
    let path = folder.join("voice.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    )?;
    Ok(path)
}

fn record(name: &str, terminal: &str, model: &str) -> Record {
    let now = epoch_ms();
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: "voz".to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some(model.to_string()),
        effort: Some("low".to_string()),
        role: "voz".to_string(),
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        brief_head: "Revisa la rama work/NV-3 y deja un informe corto.".to_string(),
        brief_path: None,
        retry_of: None,
        state: TaskState::InProgress,
        since_ms: now,
        history: vec![Change {
            state: TaskState::InProgress,
            at_ms: now,
            by: "voz".to_string(),
            detail: "encargo entregado".to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

fn notes(chat: &ChatModel) -> Vec<String> {
    chat.snapshot()
        .1
        .into_iter()
        .map(|message| message.text)
        .collect()
}

fn wait_for_note(app: &VoiceApp, chat: &ChatModel, needle: &str, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        app.activity_tick();
        app.tick();
        if notes(chat).iter().any(|text| text.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    notes(chat).iter().any(|text| text.contains(needle))
}

fn panic_lines(logs: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(logs).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("ezagent.log"))
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        found.extend(
            text.lines()
                .filter(|line| line.contains("PÁNICO"))
                .map(str::to_string),
        );
    }
    found
}

fn report_panics(logs: &Path) -> usize {
    let panics = panic_lines(logs);
    eprintln!("PÁNICO en el registro temporal: {}", panics.len());
    for panic in &panics {
        eprintln!("  {panic}");
    }
    panics.len()
}

fn build_app(
    config: &Path,
    chat: &Arc<ChatModel>,
    watch: Option<SubagentWatch>,
) -> Result<VoiceApp, String> {
    let cfg = VoiceConfig::load(Some(config)).map_err(|error| error.to_string())?;
    let mut parts = AppParts::new(
        Arc::new(SilentSpeaker),
        Arc::new(parking_lot::Mutex::new(MicCapture::silent(
            VadSettings::default(),
        ))),
    );
    // DECISION: sin hilos de fondo, atajos ni agente: vigilante, anuncios y chat.
    parts.background = false;
    parts.build_swarm = false;
    parts.build_agent = false;
    parts.mic_capture = false;
    parts.whisper_ready = true;
    parts.chat = Some(Arc::clone(chat));
    parts.config_file = Some(config.to_path_buf());
    parts.subagent_watch = watch;
    let app = VoiceApp::with_services(cfg, None, parts);
    app.set_hooks(Arc::new(LiveHooks::new(&app)));
    Ok(app)
}

// --------------------------------------------------------------------- fake

fn fake_mode() -> i32 {
    let temp = temp_dir();
    let logs = temp.join("logs");
    let subagents = temp.join("subagentes");
    let _ = std::fs::create_dir_all(&logs);
    let _ = std::fs::create_dir_all(&subagents);
    std::env::set_var("APPDATA", &temp);
    std::env::set_var("EZAGENT_LOGS_DIR", &logs);
    std::env::set_var("EZ_SUBAGENTS_DIR", &subagents);
    ez_core::panics::install_hook_in(&logs);
    let Some(fake_hc) = find_binary("fake-hc") else {
        eprintln!("Compila `cargo build -p ez-fakes --bin fake-hc`.");
        return 1;
    };
    if let Err(error) = seed_fake_state(&temp) {
        return fail(&logs, &temp, &format!("estado del fake-hc: {error}"));
    }
    let config = match seed_config(&temp, &fake_hc, &fake_hc, json!({}), &temp) {
        Ok(config) => config,
        Err(error) => return fail(&logs, &temp, &format!("configuración: {error}")),
    };
    let chat = Arc::new(ChatModel::new(None, false));
    let watch = SubagentWatch {
        runtime: Arc::new(HcRuntime::new(
            system_runtime(),
            fake_hc.to_string_lossy().into_owned(),
        )),
        novelty_file: Some(temp.join("novedades.toml")),
        interval_ms: 100,
        grace_ms: 0,
    };
    let app = match build_app(&config, &chat, Some(watch)) {
        Ok(app) => app,
        Err(error) => return fail(&logs, &temp, &format!("app: {error}")),
    };
    eprintln!("app viva; fake-hc {}", fake_hc.display());
    let registry = Registry::at(&subagents);
    for item in [
        record("ir-1", "t1", OLD_MODEL),
        record("ir-2", "t2", OLD_MODEL),
    ] {
        if let Err(error) = registry.create(&item) {
            return fail(&logs, &temp, &format!("registro: {}", error.message()));
        }
    }
    fake_flow(&app, &chat, &registry, &logs, &temp)
}

fn seed_fake_state(temp: &Path) -> std::io::Result<()> {
    let state = json!({
        "next": 2, "spaces": ["main"],
        "terminals": [
            {"address":"t1","tab":"tab1","space":"main","name":"ir-1","title":"ir-1",
             "agent":"opencode","state":"live","cwd":"C:/humo","where":"main / ir-1",
             "screen":"Free limit reached. Resets at 7:00 p.m.
    "},
            {"address":"t2","tab":"tab2","space":"main","name":"ir-2","title":"ir-2",
             "agent":"opencode","state":"finished","cwd":"C:/humo","where":"main / ir-2",
             "screen":"","exit_code":1}
        ]
    });
    let path = temp.join("fake-hc.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&state).unwrap_or_default())?;
    std::env::set_var("FAKE_HC_STATE", path);
    Ok(())
}

fn fake_flow(
    app: &VoiceApp,
    chat: &ChatModel,
    registry: &Registry,
    logs: &Path,
    temp: &Path,
) -> i32 {
    let blocked = wait_for_note(app, chat, "está bloqueado", 5);
    let failed = wait_for_note(app, chat, "falló", 5);
    for text in notes(chat) {
        if text.contains("está bloqueado") || text.contains("falló") {
            eprintln!("ANUNCIO: {text}");
        }
    }
    let state = |name: &str| {
        registry
            .get("voz", name)
            .ok()
            .flatten()
            .map(|item| item.state.as_str().to_string())
            .unwrap_or_else(|| "sin registro".to_string())
    };
    eprintln!(
        "estado real: ir-1 = {}; ir-2 = {}",
        state("ir-1"),
        state("ir-2")
    );
    let offer = app.pending_retry();
    eprintln!("oferta de reintento: {offer:?}");
    let status = app.fast_command("¿cómo va ir-1?");
    eprintln!("atajo «¿cómo va ir-1?»: handled={status}");
    let alive = !app.state().is_empty();
    let ok = blocked && failed && offer.as_deref() == Some("ir-1") && status && alive;
    app.shutdown();
    let panics = report_panics(logs);
    eprintln!(
        "RESULT modo=fake-hc bloqueado={blocked} fallido={failed} oferta={} atajo={status} viva={alive} panico={panics}",
        offer.as_deref().unwrap_or("ninguna")
    );
    let _ = std::fs::remove_dir_all(temp);
    if ok && panics == 0 {
        0
    } else {
        1
    }
}

fn fail(logs: &Path, temp: &Path, message: &str) -> i32 {
    eprintln!("{message}");
    let _ = report_panics(logs);
    let _ = std::fs::remove_dir_all(temp);
    1
}

fn main() {
    use std::io::Write as _;

    // DX-7b: sin secundaria verificada no se arranca ningún proceso de la app.
    std::env::set_var(ez_hud::win::monitor::TEST_MONITOR_ENV, "secondary");
    let monitor = match ez_hud::win::monitor::require_secondary_monitor() {
        Ok(rect) => rect,
        Err(reason) => {
            let _ = writeln!(std::io::stderr(), "no se arranca el humo: {reason}");
            std::process::exit(2);
        }
    };
    let _ = writeln!(
        std::io::stderr(),
        "monitor verificado: secundario · rect {monitor:?}"
    );
    let mode = std::env::args().nth(1).unwrap_or_default();
    let code = if mode == "--real-hc" {
        real::run()
    } else {
        fake_mode()
    };
    std::process::exit(code);
}
