//! Fachada de `DropUI` que anota el estado que ve el usuario (F7-01a).
//!
//! El arnés de reproducción necesita la misma traza que el `RecordingHud` de
//! Python (`parity/replay/run_py.py`): qué estado se mostró y cuándo cambió
//! `in_call`/`muted`. `DropUI` es un tipo concreto de `ez-hud`, así que la app
//! le habla por esta fachada: reenvía todo y, en las llamadas observables,
//! avisa al [`AppTrace`] instalado. Cualquier otro método llega por `Deref`.

use std::ops::Deref;
use std::sync::Arc;

use ez_hud::win::drop::{DropUI, VoidCallback};

use super::trace::{notify, TraceSlot};

/// La barra tal como la usa `VoiceApp`.
pub struct AppHud {
    inner: Arc<DropUI>,
    trace: TraceSlot,
}

impl AppHud {
    /// Envuelve la barra ya construida.
    pub fn new(inner: Arc<DropUI>, trace: TraceSlot) -> Arc<Self> {
        Arc::new(Self { inner, trace })
    }

    /// La barra sin la fachada (para el panel de chat y las pruebas).
    pub fn inner(&self) -> &Arc<DropUI> {
        &self.inner
    }

    /// `show`: la app hace visible la barra con su estado.
    pub fn show(&self, state: &str) {
        self.inner.show(state);
        notify(&self.trace, |trace| trace.hud_show(state));
    }

    /// `hide`.
    pub fn hide(&self) {
        self.inner.hide();
        notify(&self.trace, |trace| trace.hud_hide());
    }

    /// `set_in_call`.
    pub fn set_in_call(&self, value: bool) {
        self.inner.set_in_call(value);
        notify(&self.trace, |trace| trace.hud_in_call(value));
    }

    /// `set_muted`.
    pub fn set_muted(&self, value: bool) {
        self.inner.set_muted(value);
        notify(&self.trace, |trace| trace.hud_muted(value));
    }

    /// `notice`.
    pub fn notice(&self) {
        self.inner.notice();
        notify(&self.trace, |trace| trace.hud_notice());
    }

    /// `show_confirmation`: ✓ y ✕ en la barra.
    pub fn show_confirmation(
        &self,
        summary: &str,
        strong: bool,
        on_yes: Option<VoidCallback>,
        on_no: Option<VoidCallback>,
    ) {
        self.inner.show_confirmation(summary, strong, on_yes, on_no);
        notify(&self.trace, |trace| trace.hud_confirm(summary, strong));
    }

    /// `hide_confirmation`.
    pub fn hide_confirmation(&self) {
        self.inner.hide_confirmation();
        notify(&self.trace, |trace| trace.hud_confirm_done());
    }

    /// `set_testing`.
    pub fn set_testing(&self, testing: bool) {
        self.inner.set_testing(testing);
        notify(&self.trace, |trace| trace.hud_testing(testing));
    }

    /// `close`.
    pub fn close(&self) {
        self.inner.close();
        notify(&self.trace, |trace| trace.hud_close());
    }
}

impl Deref for AppHud {
    type Target = DropUI;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
