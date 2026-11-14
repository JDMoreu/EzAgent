//! Arranque, atajos, precalentamiento y apagado de `VoiceApp` (F6-01).
//!
//! Porta `app.py` 908-1125: `_warm_tts`, `_warm_stt`, los cuatro ayudantes de
//! atajos, `run`, `_start_listening_on_launch` y `shutdown`. Los pasos que
//! pertenecen a F6-02…F6-07 se piden a [`super::core::LifecycleHooks`].

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use ez_audio::capture::pcm_to_wav;
use ez_stt::client::{close_client as close_stt_client, transcribe};
use ez_stt::server::whisper_ready;
use serde_json::{Map, Value};

use crate::hotkeys::GlobalHotkey;

use super::core::{
    advance_turn, cfg_text, hotkey_key, VoiceApp, ACTIVITY_TICK_MS, HOTKEY_KEYS, IDLE,
};

/// `_warm_tts`: sintetiza un saludo para dejar Kokoro cargado.
pub fn warm_tts(app: &VoiceApp) {
    match app.inner.speech.speaker.synthesize("Hola.") {
        Ok(_) => super::core::log("Kokoro precalentado."),
        Err(error) => super::core::log(format!("No pude precalentar Kokoro: {error}")),
    }
}

/// `_warm_stt`: carga el modelo de whisper antes del primer turno real.
pub fn warm_stt(app: &VoiceApp) {
    let cfg = app.config();
    if !whisper_ready(&cfg_text(&cfg, "whisper_url", "")) {
        // Ya se avisó al arrancar: no repetir el error en cada intento.
        super::core::log("whisper no disponible; salto el precalentamiento.");
        return;
    }
    let pcm = vec![0u8; 6400];
    match transcribe(&cfg, &pcm_to_wav(&pcm, 16_000), None) {
        Ok(_) => super::core::log("whisper precalentado."),
        Err(error) => super::core::log(format!("No pude precalentar whisper: {error}")),
    }
}

/// `_check_hotkey_duplicates`: dos claves no pueden registrar el mismo atajo.
pub fn check_hotkey_duplicates(values: &Map<String, Value>) -> Result<(), String> {
    let mut seen: Vec<(String, String)> = Vec::new();
    for key in HOTKEY_KEYS {
        let normalized = hotkey_key(values.get(key).and_then(Value::as_str).unwrap_or_default());
        if normalized.is_empty() {
            continue;
        }
        if let Some((_, first)) = seen.iter().find(|(value, _)| *value == normalized) {
            return Err(format!("Los atajos {first} y {key} son el mismo."));
        }
        seen.push((normalized, key.to_string()));
    }
    Ok(())
}

/// `_build_hotkeys`: atajos activos según `voice.json`, con sus alias.
pub fn build_hotkeys(app: &VoiceApp) -> Vec<GlobalHotkey> {
    let cfg = app.config();
    let primary = cfg_text(&cfg, "hotkey", "");
    let cancel_key = cfg_text(&cfg, "cancel_hotkey", "");
    let repeat_key = cfg_text(&cfg, "repeat_hotkey", "");
    let mute_key = cfg_text(&cfg, "mute_hotkey", "");
    let menu_key = cfg_text(&cfg, "menu_hotkey", "");
    let chat_key = cfg_text(&cfg, "chat_hotkey", "");
    let cancel_used = used_keys(&[&primary, &repeat_key, &mute_key, &menu_key]);
    let repeat_used = used_keys(&[&primary, &cancel_key, &mute_key, &menu_key]);
    let cancel_specs = alias_specs(&cancel_key, &["ctrl+alt+e", "ctrl+alt+c"], &cancel_used);
    let repeat_specs = alias_specs(&repeat_key, &["ctrl+alt+y"], &repeat_used);
    let sender = app.inner.ui.hotkey_events.sender();
    let mut hotkeys = vec![GlobalHotkey::new(primary, "toggle", sender.clone(), 47_201)];
    for (index, spec) in cancel_specs.iter().take(4).enumerate() {
        hotkeys.push(GlobalHotkey::new(
            spec.clone(),
            "cancel",
            sender.clone(),
            47_202 + index as i32,
        ));
    }
    for (index, spec) in repeat_specs.iter().take(4).enumerate() {
        hotkeys.push(GlobalHotkey::new(
            spec.clone(),
            "repeat",
            sender.clone(),
            47_210 + index as i32,
        ));
    }
    hotkeys.push(GlobalHotkey::new(mute_key, "mute", sender.clone(), 47_220));
    hotkeys.push(GlobalHotkey::new(menu_key, "menu", sender.clone(), 47_221));
    hotkeys.push(GlobalHotkey::new(chat_key, "chat", sender, 47_222));
    hotkeys
}

fn used_keys(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| hotkey_key(value)).collect()
}

fn alias_specs(configured: &str, aliases: &[&str], used: &HashSet<String>) -> Vec<String> {
    let mut specs: Vec<String> = Vec::new();
    for spec in std::iter::once(configured).chain(aliases.iter().copied()) {
        if spec.is_empty() || specs.iter().any(|value| value == spec) {
            continue;
        }
        if spec == configured || !used.contains(&hotkey_key(spec)) {
            specs.push(spec.to_string());
        }
    }
    specs
}

impl VoiceApp {
    /// `_warm_tts` en un hilo propio.
    pub fn warm_tts(&self) {
        warm_tts(self);
    }

    /// `_warm_stt` en un hilo propio.
    pub fn warm_stt(&self) {
        warm_stt(self);
    }

    /// `_build_hotkeys` como método.
    pub fn build_hotkeys(&self) -> Vec<GlobalHotkey> {
        build_hotkeys(self)
    }

    /// `_suspend_hotkeys`: suelta los atajos mientras Ajustes graba uno.
    pub fn suspend_hotkeys(&self, active: bool) {
        if !active {
            self.reload_hotkeys();
            return;
        }
        let old = std::mem::take(&mut *self.inner.ui.hotkeys.lock());
        for hotkey in &old {
            hotkey.stop(true);
        }
    }

    /// `_reload_hotkeys`: re-registra los atajos tras cambiarlos en Ajustes.
    pub fn reload_hotkeys(&self) {
        let old = std::mem::take(&mut *self.inner.ui.hotkeys.lock());
        for hotkey in &old {
            hotkey.stop(true);
        }
        let hotkeys = self.build_hotkeys();
        for hotkey in &hotkeys {
            hotkey.start();
        }
        *self.inner.ui.hotkeys.lock() = hotkeys;
        let app = self.clone();
        self.inner
            .ui
            .ui
            .schedule(Duration::from_millis(150), move || app.report_hotkeys());
    }

    /// `_report_hotkeys`: errores de registro y resumen de atajos.
    pub fn report_hotkeys(&self) {
        for hotkey in self.inner.ui.hotkeys.lock().iter() {
            if let Some(error) = hotkey.error() {
                super::core::log(format!("Hotkey {}: {error}", hotkey.spec));
            }
        }
        let cfg = self.config();
        super::core::log(format!(
            "Listo. {}=llamada, ctrl+alt+e/ctrl+alt+c=colgar, {}=mute, \
             ctrl+alt+r/ctrl+alt+y=repetir, {}=menú. Ctrl+C para salir.",
            cfg_text(&cfg, "hotkey", ""),
            cfg_text(&cfg, "mute_hotkey", ""),
            cfg_text(&cfg, "menu_hotkey", ""),
        ));
    }

    /// `run`: arranca los hilos, ejecuta el bucle de la interfaz y apaga.
    pub fn run(&self) {
        self.start();
        if let Err(error) = self.inner.ui.ui.run() {
            super::core::log(format!("No pude abrir el bucle de la interfaz: {error}"));
        }
        self.shutdown();
    }

    /// Lo que `run` hace antes de `mainloop` (y los dobles pueden probar solo).
    pub fn start(&self) {
        if self.inner.runtime.background {
            self.inner.agents.watcher.start();
            if let Some(swarm) = self.inner.agents.swarm.lock().clone() {
                self.start_worker("ezagent-swarm-start", move || {
                    if let Err(error) = swarm.start() {
                        super::core::log(format!("No pude arrancar el enjambre: {error}"));
                    }
                });
            }
        }
        let hooks = self.hooks();
        if self.inner.runtime.background {
            self.start_worker("ezagent-resume-tasks", move || hooks.resume_pending_tasks());
        } else {
            hooks.resume_pending_tasks();
        }
        // DECISION: `__init__` pedía modelos, dispositivos y bandeja; aquí se
        // difieren al arranque real para que los dobles no abran nada.
        let cfg = self.config();
        let hooks = self.hooks();
        hooks.fetch_llm_models(
            &cfg_text(&cfg, "llm_provider", ""),
            &cfg_text(&cfg, "llm_base_url", ""),
            &cfg_text(&cfg, "api_key", ""),
            &cfg_text(&cfg, "llm_api_style", ""),
            false,
        );
        let hooks = self.hooks();
        hooks.create_tray();
        let hooks = self.hooks();
        hooks.update_tray_title();
        if self.inner.runtime.background {
            let app = self.clone();
            self.start_worker("ezagent-devices", move || app.hooks().refresh_devices());
        } else {
            self.hooks().refresh_devices();
        }
        let app = self.clone();
        self.inner
            .ui
            .ui
            .schedule(Duration::from_millis(600), move || {
                app.start_listening_on_launch();
            });
        schedule_activity_tick(self);
    }

    /// `_start_listening_on_launch`: la app nace escuchando.
    pub fn start_listening_on_launch(&self) {
        if self
            .inner
            .session
            .in_call
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        if self
            .inner
            .config
            .hud_hidden
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            // Si la barra estaba oculta, este arranque la devuelve.
            self.inner
                .config
                .hud_hidden
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let mut cfg = self.inner.config.cfg.write();
            cfg.set("hud_hidden", Value::Bool(false));
            if let Err(error) = cfg.save_key(
                "hud_hidden",
                Value::Bool(false),
                Some(&self.inner.config.config_file),
            ) {
                super::core::log(format!("No pude guardar hud_hidden: {error}"));
            }
            drop(cfg);
            self.inner.ui.hud.show(IDLE);
        }
        self.hooks().start_call();
        super::core::log("Escuchando al iniciar.");
    }

    /// `_signal_tts_stop`: corta el TTS del turno y el aviso de espera.
    pub fn signal_tts_stop(&self) {
        if let Some(stop) = self.inner.speech.tts_stop.lock().as_ref() {
            stop.set();
        }
        if let Some(stop) = self.inner.speech.busy_stop.lock().as_ref() {
            stop.set();
        }
    }

    /// `_drain_workers`: espera a los hilos propios hasta `timeout`.
    // DECISION: `JoinHandle` no tiene `join(timeout)`; se sondea `is_finished`
    // y solo se une lo terminado, igual que el `join(deadline)` de Python.
    pub fn drain_workers(&self, timeout: f64) {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
        let workers = std::mem::take(&mut *self.inner.runtime.workers.lock());
        let mut pending = Vec::new();
        for handle in workers {
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                pending.push(handle);
            }
        }
        self.inner.runtime.workers.lock().extend(pending);
    }

    /// `shutdown`: mismo orden que el Python y una sola vez.
    pub fn shutdown(&self) {
        if self
            .inner
            .session
            .shutdown_done
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        self.shutdown_turn();
        self.shutdown_workers();
        self.shutdown_services();
        self.inner.runtime.observer.step("terminado");
    }

    /// Turno, vigilante, enjambre y atajos.
    fn shutdown_turn(&self) {
        let observer = &self.inner.runtime.observer;
        observer.step("agente-cancelar");
        if let Some(agent) = self.inner.agents.agent.lock().as_ref() {
            agent.cancel_confirmation();
        }
        self.inner
            .session
            .in_call
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.inner
            .session
            .user_speaking
            .store(false, std::sync::atomic::Ordering::SeqCst);
        advance_turn(self);
        observer.step("llamada-parar");
        if let Some(stop) = self.inner.turn.call_stop.lock().as_ref() {
            stop.set();
        }
        self.signal_tts_stop();
        observer.step("vigilante");
        self.inner.agents.watcher.stop();
        observer.step("enjambre");
        if let Some(swarm) = self.inner.agents.swarm.lock().clone() {
            swarm.stop();
        }
        observer.step("atajos");
        let hotkeys = std::mem::take(&mut *self.inner.ui.hotkeys.lock());
        for hotkey in &hotkeys {
            hotkey.stop(false);
        }
    }

    /// Hilos propios y el de la llamada.
    fn shutdown_workers(&self) {
        let observer = &self.inner.runtime.observer;
        observer.step("hilos");
        self.drain_workers(3.0);
        observer.step("hilo-llamada");
        self.join_call_thread();
    }

    /// Mensajero, whisper, ventanas, audio y bandeja.
    fn shutdown_services(&self) {
        let observer = &self.inner.runtime.observer;
        observer.step("mensajero");
        if let Some(messenger) = self.inner.agents.messenger.lock().take() {
            messenger.close();
        }
        observer.step("stt");
        close_stt_client();
        observer.step("whisper");
        self.kill_whisper();
        observer.step("hud");
        self.inner.ui.hud.close();
        observer.step("agente");
        if let Some(agent) = self.inner.agents.agent.lock().take() {
            agent.close();
        }
        observer.step("altavoz");
        self.inner.speech.speaker.close();
        observer.step("microfono");
        self.inner.audio.mic.lock().close();
        observer.step("detector");
        *self.inner.audio.turn_detector.lock() = None;
        self.inner.audio.mic.lock().turn_detector = None;
        observer.step("bandeja");
        if let Some(tray) = self.inner.ui.tray.lock().take() {
            tray.stop();
        }
    }

    fn join_call_thread(&self) {
        let Some(handle) = self.inner.turn.call_thread.lock().take() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            *self.inner.turn.call_thread.lock() = Some(handle);
        }
    }

    fn kill_whisper(&self) {
        let mut slot = self.inner.runtime.whisper_proc.lock();
        let Some(child) = slot.as_mut() else {
            return;
        };
        let running = child.try_wait().ok().flatten().is_none();
        if running {
            kill_process_tree(child);
        }
        slot.take();
    }
}

fn schedule_activity_tick(app: &VoiceApp) {
    let next = app.clone();
    app.inner
        .ui
        .ui
        .schedule(Duration::from_millis(ACTIVITY_TICK_MS), move || {
            if next
                .inner
                .session
                .shutdown_done
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return;
            }
            next.hooks().activity_tick();
            schedule_activity_tick(&next);
        });
}

/// `taskkill /PID <pid> /T /F` con argumentos en lista y sin shell.
// DECISION: se llama al `taskkill.exe` absoluto (SystemRoot) con `env_clear`
// para no depender del PATH; si no está, se mata solo el proceso dueño.
fn kill_process_tree(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    let system_root =
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from("C:\\Windows"));
    let taskkill = PathBuf::from(system_root)
        .join("System32")
        .join("taskkill.exe");
    let mut command = Command::new(&taskkill);
    command
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .env_clear()
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let ok = command.status().is_ok();
    if !ok {
        let _ = child.kill();
    }
}
