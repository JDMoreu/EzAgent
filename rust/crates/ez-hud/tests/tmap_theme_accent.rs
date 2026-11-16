//! Tema personalizado: presets legibles y paleta que tiñe órbita y botones
//! (TMAP-4: `test_theme_accent.py`).

use std::sync::Mutex;

use ez_hud::paint::drop::{button_sprite, SpriteOptions};
use ez_hud::paint::flow::PRESETS;
use ez_hud::paint::marks;
use ez_hud::theme::{contrast, derive, MIN_CONTRAST};
use ez_hud::tokens::{self, set_themed};

/// La paleta personalizada es global en el proceso: estas pruebas no corren a la vez.
static PALETTE_LOCK: Mutex<()> = Mutex::new(());

fn lock_palette() -> std::sync::MutexGuard<'static, ()> {
    PALETTE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn every_preset_keeps_readable_accents() {
    for (name, colors) in PRESETS {
        let derived = derive(colors[1], colors[0]).unwrap_or_else(|| panic!("preset {name}"));
        for theme in ["light", "dark"] {
            let pal = &derived[theme];
            assert!(
                contrast(pal["MINT"], pal["BODY"]) >= MIN_CONTRAST,
                "{name}/{theme}: acento ilegible"
            );
            assert!(
                contrast(pal["MINT_INK"], pal["MINT_BTN"]) >= MIN_CONTRAST,
                "{name}/{theme}: tinta del botón ilegible"
            );
            assert!(
                contrast(pal["TEXT"], pal["BODY"]) >= 7.0,
                "{name}/{theme}: texto principal con poco contraste"
            );
        }
    }
}

#[test]
fn the_custom_palette_reaches_the_orbit_and_the_bar_buttons() {
    let _guard = lock_palette();
    set_themed(None);
    let plain = button_sprite("call", "dark", 1.0, &SpriteOptions::default()).expect("glifo");
    let centre = (plain.width() / 2, plain.height() / 2);
    let before = plain.pixel(centre.0, centre.1).to_array();

    set_themed(derive("#7A6FF0", "#2E2A6B").as_ref());
    let tinted = button_sprite("call", "dark", 1.0, &SpriteOptions::default()).expect("glifo");
    let after = tinted.pixel(centre.0, centre.1).to_array();
    assert!(
        after[2] > after[1],
        "el botón debe quedar lavanda: {after:?}"
    );
    assert_ne!(before, after);

    let inks = marks::ink("dark");
    assert_eq!(
        inks.mint,
        tokens::palette("dark").expect("oscuro")["MINT"].channels()
    );

    // Otro tema vuelve a la paleta de fábrica.
    set_themed(None);
    assert_eq!(marks::ink("dark").mint, marks::INK_DARK.mint);
}
