//! Tema, colores personalizados, confirmación, disposición y chat
//! (`drop.py 1271-1560`).

use ez_core::metrics::monotonic;

use crate::model::drop::{default_dock, LAYOUTS};
use crate::paint::drop::render_fluid;
use crate::theme;
use crate::tokens::set_themed;

use super::traits::ConfigValue;
use super::{clean_bar_scale, form_dims, DropUI, FluidBuffer};

impl DropUI {
    /// `set_custom_colors`: colores del tema personalizado (hex).
    pub fn set_custom_colors(
        &self,
        primary: &str,
        secondary: &str,
        accent: &str,
        background: &str,
    ) {
        let palette = crate::paint::flow::custom_palette(primary, secondary, accent, background);
        {
            let mut custom = self.custom.lock();
            custom.palette = palette;
            custom.hex = Some((primary.to_string(), background.to_string()));
        }
        self.apply_themed();
    }

    /// `_apply_themed`: la paleta personalizada tiñe también chat y ajustes.
    pub(super) fn apply_themed(&self) {
        let is_custom = self.theme_setting.lock().as_str() == "custom";
        let hexes = if is_custom {
            self.custom.lock().hex.clone()
        } else {
            None
        };
        let overrides =
            hexes.and_then(|(primary, background)| theme::derive(&primary, &background));
        set_themed(overrides.as_ref());
    }

    /// `_custom_for_frame`: paleta personalizada, solo con el tema «custom».
    pub(super) fn custom_for_frame(&self) -> Option<crate::model::drop::Palette> {
        if self.theme_setting.lock().as_str() != "custom" {
            return None;
        }
        self.custom.lock().palette
    }

    /// `set_custom_base`: interfaz clara u oscura del tema personalizado.
    pub fn set_custom_base(&self, base: &str) {
        self.custom.lock().base = if base == "light" {
            String::from("light")
        } else {
            String::from("dark")
        };
        if self.theme_setting.lock().as_str() == "custom" {
            self.menu_set_theme();
        }
    }

    /// `set_theme`: `auto`, `dark`, `light` o `custom`.
    pub fn set_theme(&self, theme: &str) {
        let theme = ["auto", "dark", "light", "custom"]
            .iter()
            .find(|candidate| **candidate == theme)
            .copied()
            .unwrap_or("auto");
        *self.theme_setting.lock() = theme.to_string();
        *self.theme_cache.lock() = (String::from("dark"), f64::NEG_INFINITY);
        self.apply_themed();
        self.menu_set_theme();
    }

    /// `_theme_name`: tema efectivo (`light` o `dark`).
    pub fn theme_name(&self) -> String {
        let setting = self.theme_setting.lock().clone();
        match setting.as_str() {
            "dark" | "light" => return setting,
            "custom" => return self.custom.lock().base.clone(),
            _ => {}
        }
        let now = monotonic();
        let mut cache = self.theme_cache.lock();
        if now - cache.1 > 3.0 {
            cache.0 = if system_light() {
                String::from("light")
            } else {
                String::from("dark")
            };
            cache.1 = now;
        }
        cache.0.clone()
    }

    /// `show_confirmation`: pregunta por voz; aquí solo ✓ y ✕.
    pub fn show_confirmation(
        &self,
        _summary: &str,
        _strong: bool,
        on_yes: Option<super::traits::VoidCallback>,
        on_no: Option<super::traits::VoidCallback>,
    ) {
        *self.on_yes.lock() = on_yes;
        *self.on_no.lock() = on_no;
        self.bar.lock().asking = true;
        if self.menu_open() {
            self.close_menu();
        }
        let visible = self.bar.lock().visible;
        if !visible {
            self.show("thinking");
        }
    }

    /// `hide_confirmation`.
    pub fn hide_confirmation(&self) {
        self.bar.lock().asking = false;
        *self.on_yes.lock() = None;
        *self.on_no.lock() = None;
    }

    /// `set_layout`: píldora ↔ círculo sin mover la forma de sitio.
    pub fn set_layout(&self, layout: &str) {
        let layout = LAYOUTS
            .iter()
            .find(|candidate| **candidate == layout)
            .copied()
            .unwrap_or("pill");
        if layout == self.layout() {
            return;
        }
        let win = self.window();
        let anchor = self.form_anchor();
        {
            let mut bar = self.bar.lock();
            bar.layout = layout;
            bar.dock = default_dock(layout);
            bar.model.layout = layout;
            bar.model.dock = bar.dock;
        }
        if win.is_some() {
            if let Some(anchor) = anchor {
                self.retarget(anchor);
            }
        }
        self.save_config("ui_layout", ConfigValue::Text(layout.to_string()));
        if let Some(window) = self.window() {
            let (x, y) = window.position();
            self.save_config("drop_x", ConfigValue::Int(i64::from(x)));
            self.save_config("drop_y", ConfigValue::Int(i64::from(y)));
            self.save_config("drop_layout", ConfigValue::Text(layout.to_string()));
            let dock = self.dock();
            self.save_config("drop_dock", ConfigValue::Text(dock.to_string()));
        }
    }

    /// `toggle_layout`.
    pub fn toggle_layout(&self) {
        self.set_layout(if self.layout() == "pill" {
            "circle"
        } else {
            "pill"
        });
    }

    /// `toggle_chat`.
    pub fn toggle_chat(&self) {
        let chat = self.chat.read().clone();
        let asking = self.bar.lock().asking;
        if asking && !chat.is_open() {
            return;
        }
        chat.toggle();
    }

    /// `chat_is_open`.
    pub fn chat_is_open(&self) -> bool {
        self.chat.read().is_open()
    }

    /// `set_chat_open`: el 💬 se ilumina.
    pub fn set_chat_open(&self, active: bool) {
        self.bar.lock().chat_open = active;
    }

    /// `set_form_hidden`: el chat ocupa el lugar de la forma.
    pub fn set_form_hidden(&self, hidden: bool) {
        self.bar.lock().model.form_hidden = hidden;
    }

    /// `form_size`: tamaño lógico de la forma de color.
    pub fn form_size(&self) -> (f64, f64) {
        form_dims(self.layout())
    }

    /// `chat_pose`: pose actual de la barra (voz del orbe del chat).
    pub fn chat_pose(&self) -> String {
        self.bar.lock().model.pose.clone()
    }

    /// `remember_chat_size`: guarda el tamaño del chat tras redimensionarlo.
    pub fn remember_chat_size(&self, width: f64, height: f64) {
        *self.chat_size.lock() = Some((width, height));
        let Some(config) = &self.config else {
            return;
        };
        let width = (width * 10.0).round_ties_even() / 10.0;
        let height = (height * 10.0).round_ties_even() / 10.0;
        config.remember("chat_width", ConfigValue::Float(width));
        config.remember("chat_height", ConfigValue::Float(height));
        config.save("chat_width", ConfigValue::Float(width));
        config.save("chat_height", ConfigValue::Float(height));
    }

    /// Tamaño recordado del chat, si lo hay.
    pub fn chat_size(&self) -> Option<(f64, f64)> {
        *self.chat_size.lock()
    }

    /// `hud.chat_size = (cfg.chat_width, cfg.chat_height)` de la app.
    pub fn set_chat_size(&self, width: f64, height: f64) {
        *self.chat_size.lock() = Some((width, height));
    }

    /// `chat_body_rgb`: superficie de la tarjeta para el chat.
    pub fn chat_body_rgb(&self) -> [u8; 3] {
        theme::card_palette(&self.theme_name())
            .get("BODY")
            .map_or([0, 0, 0], |color| color.channels())
    }

    /// `chat_fluid`: el fluido Marea actual estirado a `width`×`height`.
    pub fn chat_fluid(&self, width: i32, height: i32, now: f64) -> Option<FluidBuffer> {
        let rgb = {
            let mut cache = self.fluid_cache.lock();
            if cache.as_ref().is_none_or(|(at, _)| *at != now) {
                let bar = self.bar.lock();
                *cache = Some((now, render_fluid(&bar.model, now)));
            }
            cache.as_ref().map(|(_, fluid)| fluid.clone())?
        };
        let (rows, columns, _) = rgb.dim();
        let mut pixels = Vec::with_capacity(rows * columns * 3);
        for row in 0..rows {
            for column in 0..columns {
                for channel in 0..3 {
                    pixels.push(rgb[[row, column, channel]]);
                }
            }
        }
        let image = ez_gfx::Image::from_rgb(columns as u32, rows as u32, pixels).ok()?;
        let width = width.max(2) as u32;
        let height = height.max(2) as u32;
        let image = image.resize(width, height, ez_gfx::resample::Resampling::Bicubic);
        let data = image
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| {
                [
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                ]
            })
            .collect();
        Some(FluidBuffer {
            width: width as usize,
            height: height as usize,
            data,
        })
    }

    /// `set_bar_scale`: cambia el tamaño sin mover el centro de la forma.
    pub fn set_bar_scale(&self, value: f64) {
        let new = clean_bar_scale(value);
        if (new - self.bar_scale()).abs() < 1e-6 {
            return;
        }
        let win = self.window();
        let anchor = self.form_anchor();
        self.bar.lock().bar_scale = new;
        if win.is_some() {
            if let Some(anchor) = anchor {
                self.retarget(anchor);
            }
            self.bar.lock().persist_pos_at = monotonic() + 0.6;
        }
    }

    /// `_save_bar_position`: guarda posición, disposición y dock.
    pub(super) fn save_bar_position(&self) {
        let Some(window) = self.window() else {
            return;
        };
        if self.config.is_none() {
            return;
        }
        let (x, y) = window.position();
        let layout = self.layout();
        let dock = self.dock();
        self.save_config("drop_x", ConfigValue::Int(i64::from(x)));
        self.save_config("drop_y", ConfigValue::Int(i64::from(y)));
        self.save_config("drop_layout", ConfigValue::Text(layout.to_string()));
        self.save_config("drop_dock", ConfigValue::Text(dock.to_string()));
    }

    /// `drag_end`: persiste la posición y avisa a la app.
    pub fn drag_end(&self, x: i32, y: i32) {
        if self.config.is_some() {
            let layout = self.layout();
            let dock = self.dock();
            self.save_config("drop_x", ConfigValue::Int(i64::from(x)));
            self.save_config("drop_y", ConfigValue::Int(i64::from(y)));
            self.save_config("drop_layout", ConfigValue::Text(layout.to_string()));
            self.save_config("drop_dock", ConfigValue::Text(dock.to_string()));
        }
        self.host.position_changed(x, y);
    }
}

/// `_system_light`: `AppsUseLightTheme` de Windows; `false` si falla.
pub(super) fn system_light() -> bool {
    registry::apps_use_light_theme()
}

/// Acceso al registro de Windows (sólo lectura).
mod registry {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) fn apps_use_light_theme() -> bool {
        let path = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("AppsUseLightTheme");
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: las cadenas viven hasta el final de la llamada y el búfer de
        // salida apunta a un `u32` local del tamaño declarado.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            )
        };
        status.is_ok() && value != 0
    }
}
