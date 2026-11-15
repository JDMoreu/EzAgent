//! Pruebas del cableado de ganchos, iconos y costuras de la app (F6-07).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_core::metrics::TurnTimings;
use ez_hud::paint::peek::{IconPainter, PeekBuffer};
use ez_hud::tokens::Color;
use serde_json::{json, Value};

use crate::app::core::{AppHooks, MenuEvent, VoiceApp};
use crate::app::events::{ChatApi, MenuApi, PauseApi, SettingsApi, TrayApi, TurnApi};
use crate::app::live::{LiveHooks, LiveIcons};
use crate::app::settings_apply::SettingsEffects;
use crate::app::states::tests::support::{drain_events, Fixture};

fn setup() -> Fixture {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(true));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(false));
    cfg.set("hc_path", json!(r"C:\no-existe\fake-hc.exe"));
    cfg.set("herdr_app_path", json!(r"C:\no-existe\fake-app.exe"));
    Fixture::with_config(cfg)
}

#[test]
fn live_icons_paint_a_known_glyph() {
    let mut buffer = PeekBuffer::new(24, 24);
    LiveIcons.draw_icon(
        &mut buffer,
        "microphone",
        12.0,
        12.0,
        24.0,
        Color::Rgb([255, 255, 255]),
        1.0,
    );
    assert!(buffer.data.iter().any(|value| *value > 0.0));
}

#[test]
fn live_icons_ignore_unknown_names() {
    let mut buffer = PeekBuffer::new(24, 24);
    LiveIcons.draw_icon(
        &mut buffer,
        "no-existe",
        12.0,
        12.0,
        24.0,
        Color::Rgb([255, 255, 255]),
        1.0,
    );
    assert!(buffer.data.iter().all(|value| *value == 0.0));
}

#[test]
fn live_hooks_delegate_to_the_real_app() {
    let fixture = setup();
    if let Some(messenger) = fixture.app.inner.agents.messenger.lock().as_ref() {
        messenger.import_context(Vec::new(), "", "lo que dije");
    }
    let hooks: Arc<dyn AppHooks> = Arc::new(LiveHooks::new(&fixture.app));
    assert_eq!(hooks.last_user_text(), "lo que dije");
    hooks.remember_task("t1", "claude", "hacer");
    assert!(fixture.app.inner.agents.tasks.get("t1").is_some());
    hooks.watch("t1");
    assert!(fixture.app.inner.agents.watched.lock().contains("t1"));
    assert!(hooks.should_announce("t1"));
    hooks.request_close();
    assert!(fixture
        .app
        .inner
        .turn
        .close_after_answer
        .load(Ordering::SeqCst));
    hooks.mic_mute(true);
    assert!(drain_events(&fixture.app)
        .iter()
        .any(|event| matches!(event, crate::app::core::AppEvent::CloseRequest(_))));
    hooks.mute(4);
    assert!(*fixture.app.inner.session.mute_until.lock() > 0.0);
    hooks.deliver_answer("t1");
    assert!(fixture.app.inner.agents.deliver.lock().contains("t1"));
    assert!(!hooks.chat_media_roots().0.is_empty());
    hooks.update_tray_title();
    hooks.activity_tick();
    hooks.resume_pending_tasks();
    hooks.watcher_event("nadie", "claude", "done", "done");
    hooks.tick();
}

#[test]
fn live_hooks_queue_chains_and_prelaunch_specs() {
    let fixture = setup();
    let hooks: Arc<dyn AppHooks> = Arc::new(LiveHooks::new(&fixture.app));
    hooks.enqueue_chain(&json!({
        "source": "t1",
        "agent": "codex",
        "task": "revisar",
    }));
    assert_eq!(fixture.app.inner.agents.tasks.chains_for("t1").len(), 1);
    hooks.prewarm_chain(&json!({
        "chain_id": "abc",
        "agent": "codex",
    }));
    assert!(fixture
        .app
        .inner
        .agents
        .prelaunch
        .lock()
        .contains_key("abc"));
}

#[test]
fn every_seam_has_an_inherent_impl() {
    let _: fn(&VoiceApp, &str) = VoiceApp::tray_action_ui;
    let _: fn(&VoiceApp, &str) = VoiceApp::menu_action_ui;
    let _: fn(&VoiceApp) = VoiceApp::resume_listening_from_mic;
    let _: fn(&VoiceApp, &str) = VoiceApp::set_state;
    let _: fn(&VoiceApp) = VoiceApp::set_idle;
    let _: fn(&VoiceApp, &str, &str) = VoiceApp::speak;
    let _: fn(&VoiceApp, &str, &str) = VoiceApp::speak_same_turn;
    let _: fn(&VoiceApp, &str) = VoiceApp::show_voice_text;
    let _: fn(&VoiceApp, &str, bool) = VoiceApp::show_confirmation;
    let _: fn(&VoiceApp, &TurnTimings, &str) = VoiceApp::record_latency;
    let _: fn(&VoiceApp, &str, bool) = VoiceApp::queue_announcement;
    let _: fn(&VoiceApp) = VoiceApp::pump_announcements;
    let _: fn(&VoiceApp, Vec<u8>, u32, i64, Arc<TurnTimings>) = VoiceApp::start_stt;
    let _: fn(&VoiceApp, &str, i64) = VoiceApp::answer_worker;
    let _: fn(&VoiceApp) = VoiceApp::chat_new;
    let _: fn(&VoiceApp) = VoiceApp::chat_stop;
    let _: fn(&VoiceApp) = VoiceApp::toggle_chat;
    let _: fn(&VoiceApp, i64) = VoiceApp::chat_switch;
    let _: fn(&VoiceApp, i64) = VoiceApp::chat_delete;
    let _: fn(&VoiceApp, &str) = VoiceApp::chat_send;
    let _: fn(&VoiceApp, &str, i64) = VoiceApp::speak_busy;
    let _: fn(&VoiceApp, &str, &str, &str, &str) = VoiceApp::handle_done_answer;
    let _: fn(&VoiceApp, &str, &str, &str, &str, &str) = VoiceApp::handle_done_prompt;
    let _: fn(&VoiceApp, &str) -> bool = VoiceApp::fast_command;
    let _: fn(&VoiceApp, &str) = VoiceApp::hud_action_ui;
    let _: fn(&VoiceApp) = VoiceApp::toggle_menu;
    let _: fn(&VoiceApp, &MenuEvent) -> Result<(), String> = VoiceApp::handle_menu_event;
    let _: fn(&VoiceApp, &str, &[String]) = VoiceApp::fix_model_name;
    let _: fn(&VoiceApp) = VoiceApp::news_seen;
    let _: fn(&VoiceApp, &str) = VoiceApp::open_activity;
    let _: fn(&VoiceApp, &str) = VoiceApp::dismiss_activity;
    let _: fn(&VoiceApp, &str, &str, &str, &str) = VoiceApp::news_arrived;
    let _: fn(&VoiceApp, bool, &str) = VoiceApp::set_mic_muted;
    let _: fn(&VoiceApp, &[u8], u32) = VoiceApp::maybe_wake;
    let _: fn(&VoiceApp) = VoiceApp::on_wake;
    let _: fn(&VoiceApp, &str) = VoiceApp::watch;
    let _: fn(&VoiceApp) -> bool = VoiceApp::close_listening;
    let _: fn(&VoiceApp) = VoiceApp::request_close;
    let _: fn(&VoiceApp, bool) = VoiceApp::llm_mic_mute;
    let _: fn(&VoiceApp, i64) = VoiceApp::mute;
    let _: fn(&VoiceApp, &str, &str, &str) = VoiceApp::remember_task;
    let _: fn(&VoiceApp, &serde_json::Map<String, Value>) = VoiceApp::enqueue_chain;
    let _: fn(&VoiceApp, &serde_json::Map<String, Value>) = VoiceApp::start_prelaunch;
    let _: fn(&VoiceApp, &str) = VoiceApp::deliver_answer;
    let _: fn(&VoiceApp) -> String = VoiceApp::last_user_text;

    let _ = <VoiceApp as TurnApi>::set_state;
    let _ = <VoiceApp as ChatApi>::chat_send;
    let _ = <VoiceApp as TrayApi>::tray_action_ui;
    let _ = <VoiceApp as SettingsApi>::fast_command;
    let _ = <VoiceApp as MenuApi>::handle_menu_event;
    let _ = <VoiceApp as PauseApi>::close_listening;
    let _ = <VoiceApp as SettingsEffects>::speak;
    let _ = <VoiceApp as SettingsEffects>::set_state;
    let _ = <VoiceApp as SettingsEffects>::set_mic_muted;
    let _ = <VoiceApp as SettingsEffects>::resume_listening_from_mic;
    let _ = <VoiceApp as SettingsEffects>::toggle_chat;
    let _ = <VoiceApp as SettingsEffects>::handle_hotkey;
    let _ = <VoiceApp as SettingsEffects>::refresh_activity;

    // Todos los métodos de `AppHooks` tienen implementación explícita en
    // `LiveHooks`; si alguno faltara, el trait usaría su no-op por omisión y
    // no estaría cableado.
    let _: fn(&LiveHooks, &str, &str, &str, &str) = <LiveHooks as AppHooks>::watcher_event;
    let _: fn(&LiveHooks, &str) -> bool = <LiveHooks as AppHooks>::should_announce;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::tick;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::start_call;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::resume_pending_tasks;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::activity_tick;
    let _: fn(&LiveHooks, &str, &str, &str, &str, bool) = <LiveHooks as AppHooks>::fetch_llm_models;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::refresh_devices;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::create_tray;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::update_tray_title;
    let _: fn(&LiveHooks) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) =
        <LiveHooks as AppHooks>::chat_media_roots;
    let _: fn(&LiveHooks, &str) = <LiveHooks as AppHooks>::watch;
    let _: fn(&LiveHooks) = <LiveHooks as AppHooks>::request_close;
    let _: fn(&LiveHooks, bool) = <LiveHooks as AppHooks>::mic_mute;
    let _: fn(&LiveHooks, i64) = <LiveHooks as AppHooks>::mute;
    let _: fn(&LiveHooks) -> String = <LiveHooks as AppHooks>::last_user_text;
    let _: fn(&LiveHooks, &str, &str, &str) = <LiveHooks as AppHooks>::remember_task;
    let _: fn(&LiveHooks, &Value) = <LiveHooks as AppHooks>::enqueue_chain;
    let _: fn(&LiveHooks, &str) = <LiveHooks as AppHooks>::deliver_answer;
    let _: fn(&LiveHooks, &Value) = <LiveHooks as AppHooks>::prewarm_chain;
}
