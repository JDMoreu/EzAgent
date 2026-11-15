//! `_reload_mic` y los valores del VAD que el menú cambia en vivo.

use std::sync::atomic::Ordering;

use ez_audio::capture::{MicCapture, VadSettings};
use serde_json::Value;

use super::super::{schedule_start_call, MenuEffects};
use crate::app::core::{cfg_float, log, VoiceApp};

impl VoiceApp {
    /// `_reload_mic(index, restart=True)`.
    pub fn reload_mic(&self, index: Option<usize>, restart: bool) {
        self.reload_mic_with(index, restart, self);
    }

    /// Igual que [`Self::reload_mic`] con efectos intercambiables.
    pub fn reload_mic_with(&self, index: Option<usize>, restart: bool, effects: &dyn MenuEffects) {
        let was_in_call = self.inner.session.in_call.load(Ordering::SeqCst);
        if was_in_call {
            effects.hang_up();
        }
        let cfg = self.config();
        let settings = VadSettings {
            min_rms: cfg_float(&cfg, "min_rms", 0.0006),
            silence_secs: cfg_float(&cfg, "silence_secs", 2.5),
            pre_roll_secs: cfg_float(&cfg, "pre_roll_secs", 0.6),
            min_speech_secs: cfg_float(&cfg, "min_speech_secs", 0.2),
            max_utterance_secs: cfg_float(&cfg, "max_utterance_secs", 30.0),
            noise_multiplier: cfg_float(&cfg, "noise_multiplier", 1.3),
            barge_in_rms: cfg_float(&cfg, "barge_in_rms", 0.0),
            barge_in_secs: cfg_float(&cfg, "barge_in_secs", 0.25),
            ..VadSettings::default()
        };
        let new = MicCapture::new(index, settings);
        let device_index = new.device_index();
        let device_name = new.device_name().to_string();
        {
            // El cerrojo solo cubre el cambio de dispositivo: la captura vieja
            // conserva su clon y suelta el hardware al ver el `stop`.
            let mut mic = self.inner.audio.mic.lock();
            mic.close();
            *mic = new;
        }
        self.inner.config.cfg.write().set(
            "input_device_index",
            device_index.map_or(Value::Null, Value::from),
        );
        log(format!(
            "Micrófono: {device_name} (índice {})",
            device_index.map_or_else(|| "None".to_string(), |index| index.to_string())
        ));
        if was_in_call && restart {
            schedule_start_call(self);
        }
    }
}

/// `self.mic.min_rms = self.cfg.min_rms` y los demás valores del VAD.
pub(super) fn update_mic_settings(app: &VoiceApp) {
    let cfg = app.config();
    let mic = app.inner.audio.mic();
    mic.set_min_rms(cfg_float(&cfg, "min_rms", mic.min_rms()));
    mic.set_silence_secs(cfg_float(&cfg, "silence_secs", mic.silence_secs()));
    mic.set_noise_multiplier(cfg_float(&cfg, "noise_multiplier", mic.noise_multiplier()));
    mic.set_pre_roll_secs(cfg_float(&cfg, "pre_roll_secs", mic.pre_roll_secs()));
    mic.set_min_speech_secs(cfg_float(&cfg, "min_speech_secs", mic.min_speech_secs()));
    mic.set_max_utterance_secs(cfg_float(
        &cfg,
        "max_utterance_secs",
        mic.max_utterance_secs(),
    ));
    mic.set_barge_in_rms(cfg_float(&cfg, "barge_in_rms", mic.barge_in_rms()));
    mic.set_barge_in_secs(cfg_float(&cfg, "barge_in_secs", mic.barge_in_secs()));
}

/// `int(value)` de Python convertido a un índice de dispositivo.
pub(super) fn device_option(index: i64) -> Option<usize> {
    usize::try_from(index).ok()
}
