//! Costuras de `_tick`/`_handle_event`/llamada con las tareas F6-02…F6-07.
//!
//! Cada trait agrupa los métodos que el Python llama desde estas líneas y que
//! son propiedad de otra tarea (estados y workers, chat, menú, pausa…). Los
//! valores por omisión son no-ops: esta tarea compila y se prueba sola, y la
//! tarea dueña añade su método inherente `pub fn <nombre>` en su archivo; el
//! método inherente tiene prioridad sobre el del trait, así que la costura se
//! sustituye sin tocar `events.rs` ni `call.rs`.
//!
//! // DECISION: un trait por tarea evita que dos tareas paralelas tengan que
//! // editar el mismo `impl` al integrarse.

use std::sync::Arc;

use ez_core::metrics::TurnTimings;

use super::super::core::MenuEvent;
use super::super::core::VoiceApp;

/// F6-02 (`tray_wiring.rs`): acciones de bandeja y menú, y reactivar el micro.
pub trait TrayApi {
    /// `_tray_action_ui`.
    fn tray_action_ui(&self, _action: &str) {}
    /// `_menu_action_ui`.
    fn menu_action_ui(&self, _action: &str) {}
    /// `_resume_listening_from_mic`.
    fn resume_listening_from_mic(&self) {}
}

/// F6-04 (`states.rs`/`workers.rs`): estados, voz, confirmación y STT.
pub trait TurnApi {
    /// `_set_state`.
    fn set_state(&self, _state: &str) {}
    /// `_set_idle`.
    fn set_idle(&self) {}
    /// `_speak`.
    fn speak(&self, _text: &str, _kind: &str) {}
    /// `_speak_same_turn`.
    fn speak_same_turn(&self, _text: &str, _kind: &str) {}
    /// `_show_voice_text`.
    fn show_voice_text(&self, _text: &str) {}
    /// `_show_confirmation`.
    fn show_confirmation(&self, _summary: &str, _strong: bool) {}
    /// `_record_latency`.
    fn record_latency(&self, _timing: &TurnTimings, _outcome: &str) {}
    /// `_queue_announcement`.
    fn queue_announcement(&self, _text: &str, _reopen_call: bool) {}
    /// `_pump_announcements`.
    fn pump_announcements(&self) {}
    /// `_start_stt`.
    fn start_stt(&self, _pcm: Vec<u8>, _rate: u32, _gen: i64, _timing: Arc<TurnTimings>) {}
    /// `_answer_worker`.
    fn answer_worker(&self, _text: &str, _gen: i64) {}
}

/// F6-05 (`chat.rs`/`watch.rs`): chat escrito, avisos de espera y agentes.
pub trait ChatApi {
    /// `_chat_new`.
    fn chat_new(&self) {}
    /// `_chat_switch`.
    fn chat_switch(&self, _session: i64) {}
    /// `_chat_delete`.
    fn chat_delete(&self, _session: i64) {}
    /// `_chat_stop`.
    fn chat_stop(&self) {}
    /// `_chat_send`.
    fn chat_send(&self, _text: &str) {}
    /// `_toggle_chat`.
    fn toggle_chat(&self) {}
    /// `_speak_busy`.
    fn speak_busy(&self, _text: &str, _generation: i64) {}
    /// `_handle_done_answer`.
    fn handle_done_answer(&self, _address: &str, _agent: &str, _answer: &str, _task: &str) {}
    /// `_handle_done_prompt`.
    fn handle_done_prompt(
        &self,
        _address: &str,
        _agent: &str,
        _reason: &str,
        _output: &str,
        _task: &str,
    ) {
    }
}

/// F6-06a (`settings_apply.rs`): comandos rápidos, HUD y menú incrustado.
pub trait SettingsApi {
    /// `_fast_command`: ejecuta el atajo y dice si atendió el texto.
    fn fast_command(&self, _text: &str) -> bool {
        false
    }
    /// `_hud_action_ui`.
    fn hud_action_ui(&self, _action: &str) {}
    /// `_toggle_menu`.
    fn toggle_menu(&self) {}
}

/// F6-06b (`menu_wiring.rs`): eventos del menú y nombres de modelo.
pub trait MenuApi {
    /// `_handle_menu_event`.
    fn handle_menu_event(&self, _event: MenuEvent) {}
    /// `_fix_model_name`.
    fn fix_model_name(&self, _provider: &str, _models: &[String]) {}
}

/// F6-07 (`pause.rs`/`activity.rs`): pausa, actividad y vigilancia.
pub trait PauseApi {
    /// `_news_seen`.
    fn news_seen(&self) {}
    /// `_open_activity`.
    fn open_activity(&self, _key: &str) {}
    /// `_dismiss_activity`.
    fn dismiss_activity(&self, _key: &str) {}
    /// `_news_arrived`.
    fn news_arrived(&self, _key: &str, _agent: &str, _title: &str, _reason: &str) {}
    /// `_set_mic_muted`.
    fn set_mic_muted(&self, _muted: bool, _source: &str) {}
    /// `_maybe_wake`.
    fn maybe_wake(&self, _pcm: &[u8], _rate: u32) {}
    /// `_on_wake`.
    fn on_wake(&self) {}
    /// `_watch`.
    fn watch(&self, _address: &str) {}
    /// `_close_listening`: `true` si la llamada sigue (pausa).
    fn close_listening(&self) -> bool {
        false
    }
}

impl TrayApi for VoiceApp {}
impl TurnApi for VoiceApp {}
impl ChatApi for VoiceApp {}
impl SettingsApi for VoiceApp {}
impl MenuApi for VoiceApp {}
impl PauseApi for VoiceApp {}
