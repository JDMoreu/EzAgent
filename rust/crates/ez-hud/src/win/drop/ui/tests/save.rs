//! `PEND-MENORES`: la barra pide el guardado de su posición sin escribirlo en
//! el hilo de pintura (el adaptador lo hace en su hilo escritor).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::support::*;
use crate::win::drop::{ConfigValue, DropConfig, DropUI, DropUiOptions};

/// Guardado que tarda 2 s: si el hilo de pintura lo llamara, el cuadro tardaría.
#[derive(Default)]
struct SlowSave {
    values: Mutex<Vec<(String, ConfigValue)>>,
    soon: AtomicUsize,
}

impl DropConfig for SlowSave {
    fn value(&self, _key: &str) -> Option<ConfigValue> {
        None
    }

    fn save(&self, _key: &str, _value: ConfigValue) {
        std::thread::sleep(Duration::from_secs(2));
    }

    fn save_soon(&self, key: &str, value: ConfigValue) {
        self.soon.fetch_add(1, Ordering::SeqCst);
        self.values.lock().push((key.to_string(), value));
    }
}

#[test]
fn a_slow_save_does_not_delay_the_paint_thread() {
    let _palette = crate::tokens::test_guard();
    let windows = Arc::new(FakeWindows::default());
    let config = Arc::new(SlowSave::default());
    let ui = DropUI::new(DropUiOptions {
        config: Some(config.clone()),
        factory: Some(windows.clone()),
        scale: 1.0,
        ..DropUiOptions::default()
    });
    ui.set_theme("dark");
    ui.show("idle");
    // El cuadro siguiente guarda la posición (`persist_pos_at` vencido).
    ui.bar.lock().persist_pos_at = ez_core::metrics::monotonic() - 1.0;

    let started = Instant::now();
    let frame = ui.render_frame();
    let elapsed = started.elapsed();

    assert!(!frame.data.is_empty(), "el cuadro salió vacío");
    assert!(
        elapsed < Duration::from_secs(1),
        "el cuadro tardó {elapsed:?}: guardó en el hilo de pintura"
    );
    let saved = config.values.lock().clone();
    for key in ["drop_x", "drop_y", "drop_layout", "drop_dock"] {
        assert!(
            saved.iter().any(|(name, _)| name == key),
            "no se pidió guardar {key}: {saved:?}"
        );
    }
    assert_eq!(config.soon.load(Ordering::SeqCst), 4);
}
