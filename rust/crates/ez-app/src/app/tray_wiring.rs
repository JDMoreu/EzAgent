//! Bandeja, acciones del menú, apertura de archivos y reinicio (F6-02).
//!
//! Porta `app.py` 701-907: `_create_tray`, `_update_tray_title`,
//! `_tray_action_ui`, `_menu_action_ui`, `_new_conversation`, `_open_system_md`,
//! `_app_dir`, `_open_permissions`, `_open_config_dir`, `_open_voice_json`,
//! `_resume_listening_from_mic`, `_restart_app` y `_spawn_restart`.
//!
//! Las acciones llegan desde el hilo de la bandeja como [`AppEvent::TrayAction`]
//! y las ejecuta el bucle de la interfaz; aquí nunca se toca la UI desde otro
//! hilo. Los textos son literales del Python.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_core::config::config_path;
use serde_json::Value;

use crate::app::core::{advance_turn, log, resource_root, AppEvent, VoiceApp, IDLE, LISTENING};
use crate::tray::TrayIcon;

/// `_create_tray`: crea el icono de la bandeja; si falla, la app sigue sin él.
pub fn create_tray(app: &VoiceApp) -> Option<Arc<TrayIcon>> {
    let events = Arc::clone(&app.inner.ui.events);
    let tray = Arc::new(TrayIcon::new(
        move |action: String| events.put(AppEvent::TrayAction(action)),
        Some(resource_root().join("assets").join("icon.png")),
    ));
    if tray.start() {
        log("Bandeja del sistema activa.");
        *app.inner.ui.tray.lock() = Some(Arc::clone(&tray));
        return Some(tray);
    }
    log("Bandeja del sistema no disponible.");
    None
}

/// Tooltip de la bandeja según el estado (`_update_tray_title`).
pub fn tray_title(in_call: bool, muted: bool, state: &str) -> &'static str {
    if !in_call {
        "EzAgent · en espera"
    } else if muted {
        "EzAgent · micrófono silenciado"
    } else if state == LISTENING {
        "EzAgent · escuchando"
    } else {
        "EzAgent · en conversación"
    }
}

impl VoiceApp {
    /// `_create_tray` como método (gancho `AppHooks::create_tray`).
    pub fn create_tray(&self) -> Option<Arc<TrayIcon>> {
        create_tray(self)
    }

    /// `_update_tray_title`.
    pub fn update_tray_title(&self) {
        let Some(tray) = self.tray() else {
            return;
        };
        let title = tray_title(
            self.inner.session.in_call.load(Ordering::SeqCst),
            self.inner.session.muted.load(Ordering::SeqCst),
            &self.state(),
        );
        tray.set_title(title);
    }

    /// `_tray_action_ui`: acciones del icono de la bandeja.
    pub fn tray_action_ui(&self, action: &str) {
        match action {
            "config" => {
                self.show_hud_if_hidden();
                let current = if self.inner.session.in_call.load(Ordering::SeqCst) {
                    self.state()
                } else {
                    IDLE.to_string()
                };
                self.inner.ui.hud.show(&current);
                self.inner.ui.hud.open_menu();
            }
            "shape" => self.inner.ui.hud.toggle_layout(),
            "restart" => self.restart_app(),
            "quit" => {
                log("Cierre solicitado desde la bandeja.");
                self.inner.ui.ui.quit();
            }
            _ => {}
        }
    }

    /// `_menu_action_ui`: acciones del menú de ajustes.
    pub fn menu_action_ui(&self, action: &str) {
        match action {
            "open_system_md" => self.open_system_md(),
            "open_voice_json" => self.open_voice_json(),
            "open_permissions" => self.open_permissions(),
            "open_config_dir" => self.open_config_dir(),
            // DECISION: `_handle_hotkey` es de F6-03; se encola la intención en
            // la misma cola que usan los atajos reales.
            "repeat_last" => self.inner.ui.hotkey_events.put("repeat".to_string()),
            "new_conversation" => self.new_conversation(),
            _ => log(format!("Acción de menú desconocida: {action}")),
        }
    }

    /// `_new_conversation`: olvida la conversación (el menú ya confirmó).
    pub fn new_conversation(&self) {
        advance_turn(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        self.inner.chat.contexts.lock().new_chat();
        *self.inner.session.last_summary.write() = String::new();
        log("Conversación borrada desde el menú.");
        // DECISION: `_speak` es de F6-04; el evento `say` la invoca en el
        // siguiente paso del bucle, con el mismo texto y el mismo tipo.
        self.inner
            .ui
            .events
            .put(AppEvent::Say("Listo, empezamos de cero.".to_string()));
    }

    /// `_open_system_md`: abre system.md en el editor predeterminado.
    pub fn open_system_md(&self) {
        let path = self.system_md_path();
        match ensure_system_md(&path).and_then(|()| open_with_default(&path)) {
            Ok(()) => log(format!("Abriendo {}", path.display())),
            Err(error) => log(format!("No pude abrir system.md: {error}")),
        }
    }

    /// `_app_dir`: carpeta de datos del runtime del agente o la de `voice.json`.
    pub fn app_dir(&self) -> PathBuf {
        if let Some(config) = self.agent_config() {
            return config.app_dir;
        }
        self.inner
            .config
            .config_file
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// `_open_permissions`: abre permissions.toml si ya existe.
    pub fn open_permissions(&self) {
        let path = self.agent_config().map_or_else(
            || self.app_dir().join("permissions.toml"),
            |config| config.permissions_path,
        );
        if !path.exists() {
            let folder = path.parent().map_or_else(
                || path.display().to_string(),
                |parent| parent.display().to_string(),
            );
            log(format!("permissions.toml todavía no existe en {folder}."));
            return;
        }
        match open_with_default(&path) {
            Ok(()) => log(format!("Abriendo {}", path.display())),
            Err(error) => log(format!("No pude abrir permissions.toml: {error}")),
        }
    }

    /// `_open_config_dir`: abre la carpeta de datos de la app.
    pub fn open_config_dir(&self) {
        let folder = self.app_dir();
        let opened = fs::create_dir_all(&folder)
            .map_err(|error| error.to_string())
            .and_then(|()| open_with_default(&folder));
        match opened {
            Ok(()) => log(format!("Abriendo {}", folder.display())),
            Err(error) => log(format!("No pude abrir la carpeta de datos: {error}")),
        }
    }

    /// `_open_voice_json`: abre voice.json para editar perfiles y atajos.
    pub fn open_voice_json(&self) {
        let path = self.inner.config.config_file.clone();
        match ensure_voice_json(&path).and_then(|()| open_with_default(&path)) {
            Ok(()) => log(format!("Abriendo {}", path.display())),
            Err(error) => log(format!("No pude abrir voice.json: {error}")),
        }
    }

    /// `_resume_listening_from_mic`: reactiva la escucha desde el micrófono.
    pub fn resume_listening_from_mic(&self) {
        if self.inner.session.muted.load(Ordering::SeqCst) {
            self.activate_mic();
        }
        if !self.inner.session.in_call.load(Ordering::SeqCst) {
            self.hooks().start_call();
        } else {
            self.set_listening_state();
        }
    }

    /// `_restart_app`: lanza otra instancia, apaga esta y cierra el bucle.
    pub fn restart_app(&self) {
        if self.inner.session.restarting.swap(true, Ordering::SeqCst) {
            return;
        }
        if !self.spawn_restart() {
            log("No pude reiniciar EzAgent; sigo en marcha.");
            self.inner.session.restarting.store(false, Ordering::SeqCst);
            return;
        }
        log("Reiniciando EzAgent…");
        self.shutdown();
        let ui = self.inner.ui.ui.clone();
        self.inner.ui.ui.post(move || ui.quit());
    }

    /// `_spawn_restart`: lanza otra instancia; el proceso actual se cierra después.
    pub fn spawn_restart(&self) -> bool {
        match launch_restart() {
            Ok(()) => true,
            Err(error) => {
                log(format!("No pude lanzar el reinicio: {error}"));
                false
            }
        }
    }

    /// Configuración del runtime del agente, si ya existe.
    fn agent_config(&self) -> Option<ez_core::agent_config::AgentConfig> {
        self.inner
            .agents
            .agent
            .lock()
            .as_ref()
            .map(|runtime| runtime.config())
    }

    fn system_md_path(&self) -> PathBuf {
        self.agent_config().map_or_else(
            || config_path().with_file_name("system.md"),
            |config| config.system_md_path,
        )
    }

    /// Desoculta la barra y guarda `hud_hidden` (bloque repetido en Python).
    fn show_hud_if_hidden(&self) {
        if !self.inner.config.hud_hidden.load(Ordering::SeqCst) {
            return;
        }
        self.inner.config.hud_hidden.store(false, Ordering::SeqCst);
        let mut cfg = self.inner.config.cfg.write();
        cfg.set("hud_hidden", Value::Bool(false));
        if let Err(error) = cfg.save_key(
            "hud_hidden",
            Value::Bool(false),
            Some(&self.inner.config.config_file),
        ) {
            log(format!("No pude guardar hud_hidden: {error}"));
        }
    }

    /// Reactivación del micrófono.
    // DECISION: `_set_mic_muted` es de F6-07; aquí solo la rama de reactivación
    // que necesita `_resume_listening_from_mic`.
    fn activate_mic(&self) {
        self.inner.session.muted.store(false, Ordering::SeqCst);
        self.inner.ui.hud.set_muted(false);
        self.inner.audio.reset_capture.set();
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        log("Micrófono activo (sin origen).");
        self.update_tray_title();
    }

    // DECISION: `_set_state` es de F6-04; aquí se porta solo su rama LISTENING.
    fn set_listening_state(&self) {
        self.show_hud_if_hidden();
        *self.inner.session.state.write() = LISTENING.to_string();
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
        let in_call = self.inner.session.in_call.load(Ordering::SeqCst);
        self.inner.ui.hud.set_in_call(in_call);
        self.inner.ui.hud.show(LISTENING);
        self.inner.ui.hud.draw(0.0, Some(LISTENING));
        self.update_tray_title();
    }
}

/// `os.startfile`: el Explorador abre con el programa predeterminado.
fn open_with_default(path: &Path) -> Result<(), String> {
    let opener = ez_agent::tools_fs::default_opener();
    opener(&path.display().to_string())
}

/// Parte de `_open_system_md`: crea system.md con la plantilla si falta.
fn ensure_system_md(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // `Path.write_text` traduce `\n` al final de línea del sistema.
    let body = if cfg!(windows) {
        ez_agent::system_prompt::DEFAULT_SYSTEM_MD.replace('\n', "\r\n")
    } else {
        ez_agent::system_prompt::DEFAULT_SYSTEM_MD.to_string()
    };
    fs::write(path, body).map_err(|error| error.to_string())
}

/// Parte de `_open_voice_json`: crea el archivo vacío si falta.
fn ensure_voice_json(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let body = if cfg!(windows) { "{}\r\n" } else { "{}\n" };
    fs::write(path, body).map_err(|error| error.to_string())
}

/// `sys.executable`, `sys.argv[1:]` y `cwd` de la instancia actual.
fn restart_target() -> Result<(PathBuf, Vec<OsString>, PathBuf), String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let cwd = program
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let args = std::env::args_os().skip(1).collect();
    Ok((program, args, cwd))
}

/// `subprocess.Popen` con `CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS`.
fn launch_restart() -> Result<(), String> {
    let (program, args, cwd) = restart_target()?;
    let mut command = Command::new(&program);
    command
        .args(&args)
        .current_dir(&cwd)
        .envs(std::env::vars_os());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
