//! Fotogramas de la barra y de la tarjeta (`drop.py 1363-1439, 1970-2072`).

use std::sync::{Arc, Weak};

use ez_core::metrics::monotonic;
use ez_gfx::sdf::Layer;
use parking_lot::Mutex;

use crate::model::drop::{form_center, Update, FPS_BUSY, FPS_CALM, HOVER_HOLD};
use crate::paint::drop::{render_card_body, render_model, RenderOptions};
use crate::paint::marks::breath;
use crate::paint::peek::PeekItem;
use crate::paint::settings_window::{composite_over, draw_grip, fade, fade_factor, SIDEBAR_W};
use crate::tokens::{palette_version, H_ISLAND};
use crate::win::monitor;

use super::traits::{BarFrame, CardFrame, HudWindow, MenuPanel};
use super::{form_dims, frame_size, CardKey, DropUI, PeekView, ORBIT_LIMIT};

/// `POSE_OF`: estado visual (`UiState`) → pose de la barra.
fn pose_of(state: &str) -> &'static str {
    match state {
        "off" => "idle",
        "idle" => "idle",
        "listen" => "listen",
        "think" => "think",
        "speak" => "speak",
        "notice" => "speak",
        "mute" => "mute",
        "error" => "error",
        _ => "idle",
    }
}

fn state_order(state: &str) -> u8 {
    match state {
        // El modo autónomo va primero: su marca nunca debe quedar recortada por ORBIT_LIMIT.
        "autonomous" | "needs_you" => 0,
        "failed" => 1,
        "working" => 2,
        "waiting" => 3,
        "stale" => 4,
        "done" => 5,
        _ => 9,
    }
}

/// `activity.orbit`: estados marcados, en orden de prioridad.
pub(super) fn orbit_states(items: &[PeekItem]) -> Vec<String> {
    let mut marked: Vec<&PeekItem> = items.iter().filter(|item| item.marked()).collect();
    marked.sort_by(|left, right| {
        state_order(&left.state)
            .cmp(&state_order(&right.state))
            .then((!left.unseen).cmp(&!right.unseen))
            .then(right.since.total_cmp(&left.since))
    });
    marked
        .iter()
        .take(ORBIT_LIMIT)
        .map(|item| item.state.clone())
        .collect()
}

/// Vista de la barra para la vista rápida, sin reentrar en sus locks.
pub(super) struct BarPeekUi {
    pub(super) ui: Weak<DropUI>,
}

impl crate::win::peek::PeekUi for BarPeekUi {
    fn bar_scale(&self) -> Option<f64> {
        Some(self.ui.upgrade()?.peek_view.lock().bs)
    }

    fn scale(&self) -> f64 {
        self.ui
            .upgrade()
            .map_or(1.0, |ui| ui.peek_view.lock().scale)
    }

    fn theme_name(&self) -> String {
        self.ui.upgrade().map_or_else(
            || String::from("dark"),
            |ui| ui.peek_view.lock().theme.clone(),
        )
    }

    fn reduced(&self) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.peek_view.lock().reduced)
    }

    fn hub_extent(&self) -> Option<(f64, f64, f64, f64)> {
        self.ui.upgrade()?.peek_view.lock().hub
    }

    fn form_extent(&self) -> Option<(f64, f64, f64)> {
        self.ui.upgrade()?.peek_view.lock().form
    }

    fn work_area(&self) -> (i32, i32, i32, i32) {
        self.ui
            .upgrade()
            .map_or(monitor::DEFAULT_WORK_AREA, |ui| ui.peek_view.lock().work)
    }

    fn on_activity_dismiss(&self, key: &str) {
        if let Some(ui) = self.ui.upgrade() {
            ui.host.activity_dismiss(key);
        }
    }

    fn on_activity_open(&self, key: &str) {
        if let Some(ui) = self.ui.upgrade() {
            ui.host.activity_open(key);
        }
    }
}

/// `_cursor_logical`: puntero en coordenadas lógicas de la barra.
fn cursor_logical(window: Option<&Arc<dyn HudWindow>>, bs: f64) -> Option<(f64, f64)> {
    let window = window?;
    let (screen_x, screen_y) = crate::win::layered::screen_cursor();
    let (x, y) = window.position();
    Some(((screen_x - x) as f64 / bs, (screen_y - y) as f64 / bs))
}

fn target_fps(bar: &super::BarState, pose: &str, hover: bool, now: f64) -> f64 {
    let busy = !matches!(pose, "idle" | "sleep" | "mute")
        || hover
        || bar.asking
        || bar.model.animating(now);
    if busy {
        FPS_BUSY
    } else {
        FPS_CALM
    }
}

impl DropUI {
    /// `render_frame`: el fotograma de la barra y la cadencia que pide.
    #[allow(clippy::too_many_lines)]
    pub fn render_frame(&self) -> BarFrame {
        let now = monotonic();
        let window = self.window();
        let mut save_position = false;
        // `peek.tick` mueve, muestra u oculta la ventana de la vista rápida
        // (otro hilo): va después de soltar el lock de la barra.
        let mut over_form = false;
        let blocked;
        let frame = {
            let mut bar = self.bar.lock();
            let dt = (now - bar.last).clamp(0.0, 0.05);
            bar.last = now;
            if now - bar.reduced_at > 2.0 {
                bar.reduced = match bar.motion {
                    "reduced" => true,
                    "full" => false,
                    _ => !crate::anim::animations_enabled(),
                };
                bar.reduced_at = now;
            }
            let bs = bar.scale * bar.bar_scale;
            bar.hover_item = None;
            if let Some((x, y)) = cursor_logical(window.as_ref(), bs) {
                if bar.model.near(x, y) {
                    bar.hover_until = now + HOVER_HOLD;
                }
                let item = bar.model.item_at(x, y);
                over_form = item == Some("pill");
                bar.hover_item = item.filter(|name| *name != "pill").map(str::to_string);
            }
            let dragging = window
                .as_ref()
                .is_some_and(|window| window.dragging_moved());
            blocked = !bar.visible
                || bar.menu_open
                || bar.chat_open
                || bar.asking
                || bar.model.form_hidden
                || dragging;
            self.update_peek_view(&bar, window.as_deref());
            let hover = now < bar.hover_until;
            let shown = pose_of(self.ui.update(Some(now)));
            let pose = if bar.asking {
                "ask"
            } else if shown == "idle" && !bar.in_call {
                "sleep"
            } else {
                shown
            };
            let theme = self.theme_name();
            let custom = self.custom_for_frame();
            let muted = bar.muted;
            let hover_item = bar.hover_item.clone();
            let pressed = bar.pressed.clone();
            let rms = bar.rms;
            let asking = bar.asking;
            let menu_open = bar.menu_open;
            let reduced = bar.reduced;
            let in_call = bar.in_call;
            let buttons = bar.buttons.clone();
            bar.model.update(&Update {
                now,
                dt,
                pose,
                rms,
                theme: &theme,
                hover,
                asking,
                menu_open,
                reduced,
                buttons: &buttons,
                muted,
                hover_item: hover_item.as_deref(),
                pressed: pressed.as_deref(),
                in_call,
                custom,
            });
            if bar.persist_pos_at != 0.0 && now >= bar.persist_pos_at {
                bar.persist_pos_at = 0.0;
                save_position = true;
            }
            let (width, height) = frame_size(&bar);
            let orbit = orbit_states(&self.activity.read());
            let orbit_refs: Vec<&str> = orbit.iter().map(String::as_str).collect();
            let pulse = breath(now, bar.reduced);
            let options = RenderOptions {
                width: width.max(1) as u32,
                height: height.max(1) as u32,
                scale: bs,
                theme: &theme,
                muted,
                in_call: bar.in_call,
                menu_open: bar.menu_open,
                now: Some(now),
                hover_item: hover_item.as_deref(),
                chat_open: bar.chat_open,
                orbit: &orbit_refs,
                pulse,
            };
            let fps = target_fps(&bar, pose, hover, now);
            match render_model(&bar.model, &options) {
                Ok(data) => BarFrame {
                    width: options.width,
                    height: options.height,
                    data,
                    fps,
                },
                Err(error) => {
                    tracing::error!("barra: no se pudo pintar el fotograma: {error}");
                    BarFrame {
                        width: options.width,
                        height: options.height,
                        data: Vec::new(),
                        fps,
                    }
                }
            }
        };
        self.peek.tick(now, over_form, blocked);
        if save_position {
            self.save_bar_position();
        }
        let chat = self.chat.read().clone();
        chat.poll(now);
        frame
    }

    /// `target_fps`: 60 fps mientras algo se mueve; 30 en reposo.
    pub fn target_fps(&self, pose: &str, hover: bool, now: f64) -> f64 {
        let bar = self.bar.lock();
        target_fps(&bar, pose, hover, now)
    }

    /// `_peek_blocked` + la instantánea que lee la vista rápida.
    fn update_peek_view(&self, bar: &super::BarState, window: Option<&dyn HudWindow>) {
        let bs = bar.scale * bar.bar_scale;
        let (form_x, form_y) = form_center(bar.layout, Some(bar.dock));
        let (_, form_height) = form_dims(bar.layout);
        let (hub, form) = match window {
            None => (None, None),
            Some(window) => {
                let (x, y) = window.position();
                let mic = bar.model.mic_space.x.clamp(0.0, 1.0);
                let rect = bar.model.hub_rect(mic);
                (
                    Some((
                        x as f64 + rect.0 * bs,
                        y as f64 + rect.1 * bs,
                        x as f64 + rect.2 * bs,
                        y as f64 + rect.3 * bs,
                    )),
                    Some((
                        x as f64 + form_x * bs,
                        y as f64 + form_y * bs,
                        form_height / 2.0 * bs,
                    )),
                )
            }
        };
        *self.peek_view.lock() = PeekView {
            scale: bar.scale,
            bs,
            theme: self.theme_name(),
            reduced: bar.reduced,
            work: monitor::work_area(),
            hub,
            form,
        };
    }

    /// `render_card`: cuadro de la ventana de ajustes.
    pub fn render_card(&self) -> CardFrame {
        let Some(window) = self.card_window() else {
            return blank_card(1, 1);
        };
        let Some(menu) = self.menu_panel() else {
            return blank_card(1, 1);
        };
        let (width, height) = window.size();
        if width <= 0 || height <= 0 {
            return blank_card(1, 1);
        }
        let (width, height) = (width as u32, height as u32);
        let now = monotonic();
        let scale = self.scale();
        let (muted, in_call, state, mic_level) = {
            let bar = self.bar.lock();
            (bar.muted, bar.in_call, bar.state.clone(), bar.mic_level)
        };
        let pending = self.pending_save.lock().take();
        if let Some(ok) = pending {
            menu.lock().on_applied(ok);
        }
        let active = in_call && state == "listening";
        {
            let mut menu = menu.lock();
            menu.set_input_level(mic_level, muted, active);
            menu.update(now);
        }
        let (menu_height, viewport_height) = {
            let mut menu = menu.lock();
            (menu.height(now), menu.viewport_height())
        };
        if self.card_window().is_none() {
            return blank_card(width, height);
        }
        let theme = self.theme_name();
        let section = self.section.lock().clone();
        let sidebar_hover = self.sidebar_hover.lock().clone();
        let still = menu.lock().frame_key(now);
        let key = still.map(|still| CardKey {
            width,
            height,
            scale: ((scale * 10_000.0).round_ties_even() / 10_000.0).to_bits(),
            theme: theme.clone(),
            palette: palette_version(),
            section: section.clone(),
            sidebar_hover: sidebar_hover.clone(),
            still,
        });
        let base = match key.as_ref() {
            Some(key) => {
                let cached = {
                    let cache = self.card_cache.lock();
                    cache
                        .as_ref()
                        .filter(|(cached_key, _)| cached_key == key)
                        .map(|(_, data)| data.clone())
                };
                if let Some(data) = cached {
                    data
                } else {
                    let Some(data) = self.compose_card(
                        &menu,
                        width,
                        height,
                        scale,
                        &theme,
                        now,
                        menu_height,
                        viewport_height,
                    ) else {
                        return blank_card(width, height);
                    };
                    *self.card_cache.lock() = Some((key.clone(), data.clone()));
                    data
                }
            }
            None => {
                let Some(data) = self.compose_card(
                    &menu,
                    width,
                    height,
                    scale,
                    &theme,
                    now,
                    menu_height,
                    viewport_height,
                ) else {
                    return blank_card(width, height);
                };
                data
            }
        };
        let mut out = base;
        {
            let mut sidebar = self.sidebar.lock();
            let sidebar = sidebar.get_or_insert_with(crate::paint::settings_window::Sidebar::new);
            sidebar.overlay_mark(
                &mut out,
                width as usize,
                height as usize,
                scale,
                now,
                fade_factor(menu_height / viewport_height.max(1.0)),
                self.bar.lock().reduced,
            );
        }
        let fps = if menu.lock().is_animating(now) {
            60.0
        } else {
            30.0
        };
        CardFrame {
            width,
            height,
            data: out,
            fps,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compose_card(
        &self,
        menu: &Arc<Mutex<dyn MenuPanel>>,
        width: u32,
        height: u32,
        scale: f64,
        theme: &str,
        now: f64,
        menu_height: f64,
        viewport_height: f64,
    ) -> Option<Vec<u8>> {
        let body = render_card_body(width, height, scale, theme, true, palette_version()).ok()?;
        let mut frame = Layer::from_raw(width, height, body.to_vec()).ok()?;
        let palette = crate::theme::card_palette(theme);
        let section = self.section.lock().clone();
        let hover = self.sidebar_hover.lock().clone();
        let reduced = self.bar.lock().reduced;
        {
            let mut sidebar = self.sidebar.lock();
            let sidebar = sidebar.get_or_insert_with(crate::paint::settings_window::Sidebar::new);
            sidebar.draw(
                &mut frame,
                scale,
                &palette,
                theme,
                &section,
                hover.as_deref(),
                now,
                reduced,
                false,
            );
        }
        let logical_width = menu.lock().width();
        let content_width = (logical_width * scale).round_ties_even().max(1.0) as u32;
        let y0 = (f64::from(H_ISLAND) * scale).round_ties_even().max(0.0) as usize;
        let content_height =
            y0 + ((viewport_height + 4.0) * scale).round_ties_even().max(1.0) as usize;
        let mut content = Layer::new(content_width, content_height as u32);
        menu.lock().draw(&mut content, now);
        let crop_height = (height as usize).min(content_height.saturating_sub(y0));
        if crop_height > 0 {
            let crop_width = content_width.min(width);
            let mut cropped = Layer::new(crop_width, crop_height as u32);
            let source_width = content_width as usize;
            for row in 0..crop_height {
                let source_start = ((y0 + row) * source_width) * 4;
                let target_start = row * crop_width as usize * 4;
                let length = crop_width as usize * 4;
                let source = &content.as_raw()[source_start..source_start + length];
                cropped.as_raw_mut()[target_start..target_start + length].copy_from_slice(source);
            }
            let left = (SIDEBAR_W * scale).round_ties_even().max(0.0) as usize;
            composite_over(&mut frame, &cropped, left, 0);
        }
        draw_grip(&mut frame, scale, &palette);
        fade(&mut frame, menu_height / viewport_height.max(1.0));
        let mut out = Vec::with_capacity(frame.as_raw().len());
        for pixel in frame.as_raw().as_chunks::<4>().0 {
            for channel in [2, 1, 0, 3] {
                let value = pixel[channel].clamp(0.0, 1.0);
                out.push((value * 255.0 + 0.5) as u8);
            }
        }
        Some(out)
    }
}

fn blank_card(width: u32, height: u32) -> CardFrame {
    CardFrame {
        width,
        height,
        data: vec![0; width as usize * height as usize * 4],
        fps: 30.0,
    }
}
