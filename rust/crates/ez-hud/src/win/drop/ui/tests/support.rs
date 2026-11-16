//! Dobles de prueba de `win::drop`: ventanas, configuración, host, chat y menú.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;

use crate::layout::Group;
use crate::model::settings::{MenuSettings, SettingsValues};

use super::super::super::ui::{
    ChatPanel, ConfigValue, DropConfig, DropHost, DropUI, DropUiOptions, HudWindow, MenuCallbacks,
    MenuFactory, MenuPanel, WindowFactory,
};

#[derive(Default)]
pub(super) struct FakeWindow {
    position: Mutex<(i32, i32)>,
    size: Mutex<(i32, i32)>,
    pub(super) visible: AtomicBool,
    pub(super) started: AtomicBool,
    pub(super) stopped: AtomicBool,
    dragging: AtomicBool,
    keyboard: AtomicBool,
    /// La barra dueña, para mirar sus locks como hace el hilo de la ventana.
    owner: Mutex<Option<Weak<DropUI>>>,
    /// `P-FIX-LOCKAUDIT`: `stop` llegó con el lock del menú tomado.
    pub(super) menu_taken: AtomicBool,
    /// `P-FIX-LOCKAUDIT`: `resize` llegó con el lock de la tarjeta tomado.
    pub(super) card_taken: AtomicBool,
    /// La tarjeta vigila su lock; la barra no lo usa nunca.
    pub(super) watch_card: AtomicBool,
}

impl FakeWindow {
    pub(super) fn new(x: i32, y: i32, width: i32, height: i32) -> Arc<Self> {
        Arc::new(Self {
            position: Mutex::new((x, y)),
            size: Mutex::new((width, height)),
            ..Self::default()
        })
    }

    pub(super) fn keyboard_enabled(&self) -> bool {
        self.keyboard.load(Ordering::SeqCst)
    }

    /// El dueño de la ventana (el hilo de la ventana le pide cosas).
    pub(super) fn owned_by(&self, ui: &Weak<DropUI>) {
        *self.owner.lock() = Some(ui.clone());
    }

    fn owner(&self) -> Option<Arc<DropUI>> {
        self.owner.lock().as_ref().and_then(Weak::upgrade)
    }

    /// Como el hilo de la ventana en un gancho: ¿está libre el menú ahora?
    fn menu_is_free(&self) -> bool {
        let Some(ui) = self.owner() else {
            return true;
        };
        let Some(panel) = ui.menu_panel() else {
            return true;
        };
        let free = panel
            .try_lock_for(std::time::Duration::from_millis(250))
            .is_some();
        free
    }

    /// Como `render_card`: ¿está libre el lock de la tarjeta ahora?
    fn card_is_free(&self) -> bool {
        let Some(ui) = self.owner() else {
            return true;
        };
        let free = ui
            .card
            .try_lock_for(std::time::Duration::from_millis(250))
            .is_some();
        free
    }
}

impl HudWindow for FakeWindow {
    fn start(&self) -> bool {
        self.started.store(true, Ordering::SeqCst);
        self.visible.store(true, Ordering::SeqCst);
        true
    }

    fn show(&self) {
        self.visible.store(true, Ordering::SeqCst);
    }

    fn hide(&self) {
        self.visible.store(false, Ordering::SeqCst);
    }

    fn stop(&self) {
        if !self.menu_is_free() {
            self.menu_taken.store(true, Ordering::SeqCst);
        }
        self.stopped.store(true, Ordering::SeqCst);
        self.visible.store(false, Ordering::SeqCst);
    }

    fn set_pos(&self, x: i32, y: i32) {
        *self.position.lock() = (x, y);
    }

    fn resize(&self, width: i32, height: i32) {
        if self.watch_card.load(Ordering::SeqCst) && !self.card_is_free() {
            self.card_taken.store(true, Ordering::SeqCst);
        }
        *self.size.lock() = (width, height);
    }

    fn position(&self) -> (i32, i32) {
        *self.position.lock()
    }

    fn size(&self) -> (i32, i32) {
        *self.size.lock()
    }

    fn is_visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }

    fn dragging_moved(&self) -> bool {
        self.dragging.load(Ordering::SeqCst)
    }

    fn set_keyboard_enabled(&self, enabled: bool) {
        self.keyboard.store(enabled, Ordering::SeqCst);
    }

    fn copy_text(&self, _text: &str) -> bool {
        true
    }
}

#[derive(Default)]
pub(super) struct FakeWindows {
    pub(super) bars: Mutex<Vec<Arc<FakeWindow>>>,
    pub(super) cards: Mutex<Vec<Arc<FakeWindow>>>,
}

impl FakeWindows {
    pub(super) fn bar(&self) -> Arc<FakeWindow> {
        self.bars.lock().last().cloned().expect("la barra se creó")
    }

    pub(super) fn card(&self) -> Arc<FakeWindow> {
        self.cards
            .lock()
            .last()
            .cloned()
            .expect("la tarjeta se creó")
    }
}

impl WindowFactory for FakeWindows {
    fn bar(&self, ui: Weak<DropUI>, x: i32, y: i32, width: i32, height: i32) -> Arc<dyn HudWindow> {
        let window = FakeWindow::new(x, y, width, height);
        window.owned_by(&ui);
        self.bars.lock().push(Arc::clone(&window));
        window
    }

    fn card(
        &self,
        ui: Weak<DropUI>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Arc<dyn HudWindow> {
        let window = FakeWindow::new(x, y, width, height);
        window.owned_by(&ui);
        self.cards.lock().push(Arc::clone(&window));
        window
    }
}

#[derive(Default)]
pub(super) struct RecordingConfig {
    values: Mutex<BTreeMap<String, ConfigValue>>,
    pub(super) saved: Mutex<Vec<(String, ConfigValue)>>,
    pub(super) remembered: Mutex<Vec<(String, ConfigValue)>>,
}

impl RecordingConfig {
    pub(super) fn with(values: &[(&str, ConfigValue)]) -> Arc<Self> {
        let config = Arc::new(Self::default());
        for (key, value) in values {
            config
                .values
                .lock()
                .insert((*key).to_string(), value.clone());
        }
        config
    }

    pub(super) fn saved_value(&self, key: &str) -> Option<ConfigValue> {
        self.saved
            .lock()
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }
}

impl DropConfig for RecordingConfig {
    fn value(&self, key: &str) -> Option<ConfigValue> {
        self.values.lock().get(key).cloned()
    }

    fn save(&self, key: &str, value: ConfigValue) {
        self.values.lock().insert(key.to_string(), value.clone());
        self.saved.lock().push((key.to_string(), value));
    }

    fn remember(&self, key: &str, value: ConfigValue) {
        self.values.lock().insert(key.to_string(), value.clone());
        self.remembered.lock().push((key.to_string(), value));
    }
}

#[derive(Default)]
pub(super) struct RecordingHost {
    pub(super) actions: Mutex<Vec<String>>,
    pub(super) positions: Mutex<Vec<(i32, i32)>>,
    pub(super) live: Mutex<Vec<(String, ConfigValue)>>,
    pub(super) menu_actions: Mutex<Vec<String>>,
    pub(super) hotkeys: Mutex<Vec<bool>>,
    pub(super) saves: AtomicUsize,
}

impl DropHost for RecordingHost {
    fn action(&self, action: &str) {
        self.actions.lock().push(action.to_string());
    }

    fn position_changed(&self, x: i32, y: i32) {
        self.positions.lock().push((x, y));
    }

    fn live_setting(&self, key: &str, value: ConfigValue) {
        self.live.lock().push((key.to_string(), value));
    }

    fn save_settings(&self, _values: &SettingsValues) -> Option<bool> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        Some(true)
    }

    fn menu_action(&self, action: &str) {
        self.menu_actions.lock().push(action.to_string());
    }

    fn hotkey_capture(&self, active: bool) {
        self.hotkeys.lock().push(active);
    }
}

#[derive(Default)]
pub(super) struct FakeChat {
    pub(super) open: AtomicBool,
    pub(super) toggles: AtomicUsize,
    pub(super) muted: AtomicBool,
    pub(super) closed: AtomicBool,
    pub(super) polls: AtomicUsize,
}

impl ChatPanel for FakeChat {
    fn toggle(&self) {
        self.toggles.fetch_add(1, Ordering::SeqCst);
        let open = self.open.load(Ordering::SeqCst);
        self.open.store(!open, Ordering::SeqCst);
    }

    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    fn close_now(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.open.store(false, Ordering::SeqCst);
    }

    fn poll(&self, _now: f64) {
        self.polls.fetch_add(1, Ordering::SeqCst);
    }

    fn set_mic_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct FakeMenuState {
    pub(super) open: bool,
    pub(super) close_immediately: bool,
    pub(super) hit: bool,
    pub(super) groups: Vec<String>,
    pub(super) themes: Vec<String>,
    pub(super) scale: f64,
    pub(super) settings_received: bool,
    pub(super) pointer_down: usize,
    pub(super) pointer_up: usize,
    pub(super) applied: Vec<bool>,
    pub(super) testing: bool,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) viewport: f64,
    pub(super) frame_key: Option<u64>,
    pub(super) input_level: (f64, bool, bool),
}

pub(super) struct FakeMenu {
    pub(super) state: Mutex<FakeMenuState>,
    callbacks: Option<MenuCallbacks>,
}

impl FakeMenu {
    fn new(callbacks: MenuCallbacks) -> Self {
        Self {
            state: Mutex::new(FakeMenuState {
                width: 500.0,
                height: 400.0,
                viewport: 600.0,
                ..FakeMenuState::default()
            }),
            callbacks: Some(callbacks),
        }
    }
}

impl MenuPanel for FakeMenu {
    fn set_theme(&mut self, theme: &str) {
        self.state.lock().themes.push(theme.to_string());
    }

    fn set_scale(&mut self, scale: f64) {
        self.state.lock().scale = scale;
    }

    fn set_groups(&mut self, groups: &[Group], _title: &str, _now: f64) {
        self.state.lock().groups = groups.iter().map(|(id, _)| id.to_string()).collect();
    }

    fn open(&mut self, _now: f64) {
        self.state.lock().open = true;
    }

    fn close(&mut self, _now: f64) {
        self.state.lock().open = false;
        let immediate = self.state.lock().close_immediately;
        if immediate {
            if let Some(callbacks) = &self.callbacks {
                (callbacks.close)();
            }
        }
    }

    fn set_testing(&mut self, testing: bool) {
        self.state.lock().testing = testing;
    }

    fn set_settings(&mut self, _settings: MenuSettings) {
        self.state.lock().settings_received = true;
    }

    fn on_applied(&mut self, ok: bool) {
        self.state.lock().applied.push(ok);
    }

    fn set_input_level(&mut self, level: f64, muted: bool, active: bool) {
        self.state.lock().input_level = (level, muted, active);
    }

    fn height(&mut self, _now: f64) -> f64 {
        self.state.lock().height
    }

    fn viewport_height(&mut self) -> f64 {
        self.state.lock().viewport
    }

    fn width(&mut self) -> f64 {
        self.state.lock().width
    }

    fn frame_key(&mut self, _now: f64) -> Option<u64> {
        self.state.lock().frame_key
    }

    fn hit(&self, _x: f64, _y: f64, _now: f64) -> bool {
        self.state.lock().hit
    }

    fn pointer_down(&mut self, _x: f64, _y: f64, _now: f64) {
        self.state.lock().pointer_down += 1;
    }

    fn pointer_up(&mut self, _x: f64, _y: f64, _now: f64) {
        self.state.lock().pointer_up += 1;
    }

    fn wheel(&mut self, _x: f64, _y: f64, _delta: i32, _now: f64) -> bool {
        true
    }

    fn text_key(&mut self, _vk: u32, _ctrl: bool) -> bool {
        true
    }

    fn text_input(&mut self, _character: char) -> bool {
        true
    }

    fn paste_text(&mut self, _text: &str) -> bool {
        true
    }
}

#[derive(Default)]
pub(super) struct FakeMenuFactory {
    pub(super) menus: Mutex<Vec<Arc<Mutex<FakeMenu>>>>,
}

impl FakeMenuFactory {
    pub(super) fn menu(&self) -> Arc<Mutex<FakeMenu>> {
        self.menus.lock().last().cloned().expect("el menú se creó")
    }
}

impl MenuFactory for FakeMenuFactory {
    fn create(
        &self,
        _settings: MenuSettings,
        _scale: f64,
        _theme: &str,
        _screen_h: f64,
        callbacks: MenuCallbacks,
    ) -> Option<Arc<Mutex<dyn MenuPanel>>> {
        let menu = Arc::new(Mutex::new(FakeMenu::new(callbacks)));
        self.menus.lock().push(Arc::clone(&menu));
        let dynamic: Arc<Mutex<dyn MenuPanel>> = menu;
        Some(dynamic)
    }
}

pub(super) struct Harness {
    pub(super) ui: Arc<DropUI>,
    pub(super) windows: Arc<FakeWindows>,
    pub(super) config: Arc<RecordingConfig>,
    pub(super) host: Arc<RecordingHost>,
    pub(super) menus: Arc<FakeMenuFactory>,
    pub(super) chat: Arc<FakeChat>,
    /// La barra publica la paleta global (`set_theme`): mientras viva el
    /// arnés, ninguna otra prueba la cambia (`theme::tests` corre en paralelo).
    _palette: std::sync::MutexGuard<'static, ()>,
}

impl Harness {
    pub(super) fn new() -> Self {
        let palette = crate::tokens::test_guard();
        let windows = Arc::new(FakeWindows::default());
        let config = Arc::new(RecordingConfig::default());
        let host = Arc::new(RecordingHost::default());
        let menus = Arc::new(FakeMenuFactory::default());
        let chat = Arc::new(FakeChat::default());
        let ui = DropUI::new(DropUiOptions {
            config: Some(config.clone()),
            host: Some(host.clone()),
            factory: Some(windows.clone()),
            menu_factory: Some(menus.clone()),
            scale: 1.0,
            ..DropUiOptions::default()
        });
        ui.set_menu_settings_provider(Some(Arc::new(|| Some(MenuSettings::default()))));
        ui.set_chat_panel(chat.clone());
        ui.set_theme("dark");
        Self {
            ui,
            windows,
            config,
            host,
            menus,
            chat,
            _palette: palette,
        }
    }

    pub(super) fn open_card(&self) {
        self.ui.show("idle");
        self.ui.toggle_menu();
    }
}

/// Espera a que un doble marque su bandera, con plazo: la suite no se cuelga.
pub(super) fn wait_for(flag: &AtomicBool, what: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !flag.load(Ordering::SeqCst) {
        assert!(std::time::Instant::now() < deadline, "no llegó: {what}");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

pub(super) fn drop_window_size(layout: &str, dock: &str) -> (i32, i32) {
    let (width, height) = crate::model::drop::layout_size(layout, Some(dock));
    (
        width.round_ties_even() as i32,
        height.round_ties_even() as i32,
    )
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.ui.close();
    }
}
