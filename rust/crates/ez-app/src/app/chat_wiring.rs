//! Cableado del chat escrito entre `ez-chat` y el HUD (F6-08).
//!
//! `ez-hud` no depende de `ez-chat`: la vista del chat recibe su modelo, su
//! campo de texto y su parser por costuras (`ChatUi`, `ChatModel`,
//! `TextField`, `ViewHost`) y el panel emite [`ChatEvent`]. Este módulo es el
//! adaptador que `app.py` tenía dentro: `chat_model`, `chat_roots`,
//! `chat_hotkey`, `chat_size` y los callbacks `on_chat_*`.
//!
//! El panel se instala al arrancar (`install_chat`) y la barra lo abre con
//! `toggle_chat`; los callbacks entran en la cola `events` de la app, salvo
//! `on_chat_voice` (se aplica al momento, como en Python) y abrir/mostrar
//! archivos (los hace el propio panel).

mod host;
mod input;
mod model;
mod ui;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Weak};

use ez_chat::session::ChatModel;
use ez_hud::win::chat::{ChatEvent, ChatEvents, ChatPanel as ChatWindowPanel};
use ez_hud::win::drop::ChatPanel as DropChatPanel;

use super::core::state::Inner;
use super::core::{cfg_float, cfg_text, AppEvent, VoiceApp};

use ui::HudChatUi;

/// Callbacks del panel (`on_chat_send`, `on_chat_new`… de `app.py`).
pub(super) struct HudChatEvents {
    inner: Weak<Inner>,
}

impl HudChatEvents {
    pub(super) fn new(app: &VoiceApp) -> Self {
        Self {
            inner: Arc::downgrade(&app.inner),
        }
    }
}

impl ChatEvents for HudChatEvents {
    fn emit(&self, event: ChatEvent) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        let app = VoiceApp { inner };
        let events = &app.inner.ui.events;
        match event {
            ChatEvent::Send(text) => events.put(AppEvent::ChatSend(text)),
            // `on_chat_voice = self._set_chat_voice`: se aplica sin pasar por la cola.
            ChatEvent::Voice(enabled) => app.set_chat_voice(enabled),
            ChatEvent::Mic(muted) => events.put(AppEvent::ChatMute(muted)),
            ChatEvent::Settings => events.put(AppEvent::HudAction("panel".to_string())),
            ChatEvent::New => events.put(AppEvent::ChatNew),
            ChatEvent::Stop => events.put(AppEvent::ChatStop),
            ChatEvent::Switch(session) => events.put(AppEvent::ChatSwitch(session)),
            ChatEvent::Delete(session) => events.put(AppEvent::ChatDelete(session)),
            ChatEvent::Confirm(yes) => events.put(AppEvent::ConfirmClick(yes, None)),
            ChatEvent::Open(file) => {
                host::open_file(&file);
            }
            ChatEvent::Reveal(file) => {
                host::reveal_file(&file);
            }
        }
    }
}

/// `DropUI.chat`: el panel de ventana detrás del contrato de la barra.
struct PanelBridge {
    panel: Arc<ChatWindowPanel>,
    chat: Arc<ChatModel>,
}

impl DropChatPanel for PanelBridge {
    fn toggle(&self) {
        self.panel.toggle();
    }

    fn is_open(&self) -> bool {
        self.panel.is_open()
    }

    fn close_now(&self) {
        self.panel.close_now();
    }

    fn poll(&self, now: f64) {
        self.panel.poll(now);
    }

    /// `chat_model.set_mic_muted(muted)` de `drop.py`.
    fn set_mic_muted(&self, muted: bool) {
        self.chat.set_mic_muted(muted);
    }
}

impl VoiceApp {
    /// `__init__`: conecta el chat con la barra (`hud.chat_model = self.chat`).
    // DECISION: `set_chat_panel` sustituye el `NoChat` de la barra porque en
    // Rust el panel se inyecta; Python lo crea en `DropUI.__init__`.
    pub(crate) fn install_chat(&self) {
        let cfg = self.config();
        let width = cfg_float(&cfg, "chat_width", 0.0);
        let height = cfg_float(&cfg, "chat_height", 0.0);
        if width > 0.0 && height > 0.0 {
            self.inner.ui.hud.set_chat_size(width, height);
        }
        let hotkey = cfg_text(&cfg, "chat_hotkey", "ctrl+alt+t");
        let panel_ui = Arc::new(HudChatUi::new(self, hotkey));
        let events = Arc::new(HudChatEvents::new(self));
        let panel = Arc::new(ChatWindowPanel::new(panel_ui, events));
        let bridge = PanelBridge {
            panel,
            chat: Arc::clone(self.inner.chat.chat.inner()),
        };
        self.inner.ui.hud.set_chat_panel(Arc::new(bridge));
    }
}
