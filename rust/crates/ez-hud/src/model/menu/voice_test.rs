//! Port of `src/ezagent/voice/hud/menu.py 2254-2337` (F5-09).
//!
//! Prueba de voz y guardado del menú, incluida la confirmación asíncrona de la
//! capa que persiste (`on_applied`). Es modelo puro: no escribe archivos.

use super::geometry::Menu;

/// Confirmación visible tras guardar.
pub const SAVE_CHECK_MS: f64 = 600.0;
/// Duración del destello del pie tras guardar.
pub const SAVE_FLASH_MS: f64 = 800.0;
/// Duración del mensaje de error de guardado.
pub const SAVE_ERROR_MS: f64 = 2000.0;

impl Menu {
    /// `_start_test`: confirma la edición y avisa con la instantánea.
    pub fn start_test(&mut self, now: f64) {
        self.apply_text_edit(true);
        self.set_testing(true, Some(now));
        self.notify_test(&self.settings.snapshot());
    }

    /// `_save`: confirma la edición y pide a la aplicación que persista.
    ///
    /// // DECISION: `start_test` y `save` se exponen como métodos públicos
    /// // porque `pointer` ya los necesita y Rust no tiene privados de módulo
    /// // padre para la capa de aplicación.
    pub fn save(&mut self, now: f64) {
        self.apply_text_edit(true);
        if self.editing_key.is_some() {
            return;
        }
        if self.saving_pending || !self.settings.dirty() {
            return;
        }
        self.save_backup = Some(self.settings.saved_snapshot());
        let snapshot = self.settings.snapshot();
        self.save_snapshot = Some(snapshot.clone());
        // Un `on_save` ausente equivale al `True` del Python. Un `None` real
        // significa «aceptado y pendiente» y espera a `on_applied`.
        let result = match &self.callbacks.save {
            None => Some(true),
            Some(callback) => callback(&snapshot),
        };
        match result {
            Some(false) => self.save_failed(now),
            Some(true) => self.save_succeeded(now),
            None => {
                self.saving_pending = true;
                self.check_until = 0.0;
                self.saved_flash_until = 0.0;
                self.error_until = 0.0;
            }
        }
    }

    /// `_save_succeeded`: destello de confirmación y copia guardada al día.
    fn save_succeeded(&mut self, now: f64) {
        self.saving_pending = false;
        self.settings.mark_saved(None);
        self.error_until = 0.0;
        self.check_until = now + SAVE_CHECK_MS / 1000.0;
        self.saved_flash_until = now + SAVE_FLASH_MS / 1000.0;
    }

    /// `_save_failed`: restaura la copia guardada y muestra el error.
    fn save_failed(&mut self, now: f64) {
        self.saving_pending = false;
        if let Some(backup) = self.save_backup.take() {
            self.settings.restore_saved(&backup);
        }
        self.save_snapshot = None;
        self.check_until = 0.0;
        self.saved_flash_until = 0.0;
        self.error_until = now + SAVE_ERROR_MS / 1000.0;
    }

    /// Resultado real de un guardado asíncrono (`true` = persistido).
    pub fn on_applied(&mut self, result: bool) {
        let now = self.now;
        self.saving_pending = false;
        if result {
            match self.save_snapshot.clone() {
                Some(saved) => self.settings.mark_saved(Some(&saved)),
                None => self.settings.mark_saved(None),
            }
            self.save_backup = None;
            self.save_snapshot = None;
            self.error_until = 0.0;
            self.check_until = now + SAVE_CHECK_MS / 1000.0;
            self.saved_flash_until = now + SAVE_FLASH_MS / 1000.0;
        } else {
            // El fallo es de persistencia: los valores editados siguen
            // aplicados en vivo y el menú sigue dirty (spec 10.2). Se conserva
            // la copia guardada anterior como baseline para que `dirty()`
            // siga siendo verdadero, sin revertir la interfaz.
            if let (Some(backup), Some(snapshot)) =
                (self.save_backup.clone(), self.save_snapshot.clone())
            {
                self.settings.restore_saved(&backup);
                for (key, value) in snapshot.iter() {
                    let _ = self.settings.set(key, value);
                }
            }
            self.save_backup = None;
            self.save_snapshot = None;
            self.check_until = 0.0;
            self.saved_flash_until = 0.0;
            self.error_until = now + SAVE_ERROR_MS / 1000.0;
        }
    }
}

#[cfg(test)]
mod tests;
