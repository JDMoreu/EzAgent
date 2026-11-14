//! Acciones, lista de selectores, pie y desplazamiento (F5-10b).

use crate::anim::Tween;

use super::super::PaintContext;
use super::super::{Advanced, FrameExtras};
use super::{assert_pixel, draw, layer, menu, opened, pixel, track, warm_btn};
// --------------------------------------------------------------------------
// Acciones
// --------------------------------------------------------------------------

#[test]
fn a_generic_action_is_secondary_or_amber_when_armed() {
    let menu = menu();
    let target = draw(&menu, |ctx| {
        super::super::rows::draw_generic_action_row(
            &menu,
            ctx,
            "new_conversation",
            20.0,
            1.0,
            false,
        );
    });
    assert_pixel(pixel(&target, 220, 20), track(), "botón secundario");

    let target = draw(&menu, |ctx| {
        super::super::rows::draw_generic_action_row(
            &menu,
            ctx,
            "new_conversation",
            20.0,
            1.0,
            true,
        );
    });
    assert_pixel(pixel(&target, 220, 20), warm_btn(), "botón armado");
}

#[test]
fn the_test_button_shows_the_live_wave_while_testing() {
    let mut menu = menu();
    menu.set_max_height(Some(2000.0));
    menu.set_groups(
        vec![(
            "prueba".to_string(),
            vec![("test_voice".to_string(), "action".to_string())],
        )],
        Some("Prueba"),
        Some(0.0),
    );
    menu.open(0.0);
    menu.update(1.0);
    menu.testing_t = Tween::new(1.0);
    let layout = menu.layout();
    let row = *layout.get("test_voice").expect("fila de prueba de voz");
    let center_y = (row.y0 + row.y1) / 2.0;
    let target = draw(&menu, |ctx| {
        super::super::rows::draw_action_row(&menu, ctx, "test_voice", center_y, 1.0, 1.0);
    });
    let rect = menu.test_rect(&layout, 0.0);
    assert!(rect.width() > 72.0);
    let start_x = rect.center().0 - menu.test_content_width() / 2.0;
    let green = pixel(&target, (start_x + 1.0) as u32, rect.center().1 as u32);
    assert!(
        (f64::from(green[0]) - 0.368_627_5).abs() < 1e-6
            && (f64::from(green[3]) - 1.0).abs() < 1e-6,
        "la barra de la onda se pinta en mint: {green:?}"
    );
}

// --------------------------------------------------------------------------
// Lista de selectores
// --------------------------------------------------------------------------

#[test]
fn the_selector_list_marks_hover_and_chosen_rows() {
    let mut menu = opened(1.0);
    menu.toggle_selector("theme", 1.0);
    menu.update(1.6);
    let row_y0 = 100.0;
    let target = draw(&menu, |ctx| {
        super::super::rows::draw_selector_list(&menu, ctx, "theme", row_y0, 1.6, 1.0);
    });
    assert_pixel(
        pixel(&target, 200, 158),
        [0.036_862_7, 0.088_627_5, 0.076_862_7, 0.1],
        "fila elegida",
    );
    assert_pixel(
        pixel(&target, 200, 190),
        [0.0, 0.0, 0.0, 0.0],
        "fila sin hover",
    );

    menu.hover_option = Some(1);
    let target = draw(&menu, |ctx| {
        super::super::rows::draw_selector_list(&menu, ctx, "theme", row_y0, 1.6, 1.0);
    });
    assert_pixel(
        pixel(&target, 200, 190),
        [0.109_803_9, 0.176_470_6, 0.2, 1.0],
        "fila bajo el puntero",
    );
}

// --------------------------------------------------------------------------
// Pie
// --------------------------------------------------------------------------

#[test]
fn the_footer_switches_state_and_fades_the_previous_one() {
    let mut menu = opened(1.0);
    let layout = menu.layout();
    let now = 1.0;
    let palette = menu.palette();
    let mut target = layer();
    {
        let mut ctx = PaintContext::new(&mut target, menu.scale, menu.dx, menu.theme, &palette);
        super::super::footer::draw_footer(&mut menu, &mut ctx, &layout, now);
    }
    assert_eq!(
        menu.footer_state,
        Some((
            super::super::footer::SAVED_TEXT.into(),
            "TEXT_3".into(),
            false
        ))
    );
    assert!(menu.footer_prev.is_none());

    menu.settings
        .set("speed", &crate::model::settings::SettingValue::from(1.25))
        .expect("clave válida");
    let mut target = layer();
    let mut ctx = PaintContext::new(&mut target, menu.scale, menu.dx, menu.theme, &palette);
    super::super::footer::draw_footer(&mut menu, &mut ctx, &layout, 2.0);
    assert_eq!(
        menu.footer_state,
        Some(("CAMBIOS SIN GUARDAR".into(), "WARM".into(), true))
    );
    assert_eq!(
        menu.footer_prev,
        Some((
            super::super::footer::SAVED_TEXT.into(),
            "TEXT_3".into(),
            false
        ))
    );

    let mut ctx = PaintContext::new(&mut target, menu.scale, menu.dx, menu.theme, &palette);
    super::super::footer::draw_footer(&mut menu, &mut ctx, &layout, 2.2);
    assert!(menu.footer_prev.is_none());
}

#[test]
fn the_footer_dot_is_warm_and_precedes_the_text() {
    let menu = menu();
    let target = draw(&menu, |ctx| {
        super::super::footer::draw_footer_state(
            &("CAMBIOS SIN GUARDAR".into(), "WARM".into(), true),
            ctx,
            20.0,
            1.0,
        );
    });
    assert_pixel(pixel(&target, 110, 20), warm_btn(), "punto del pie");
    assert_pixel(
        pixel(&target, 100, 20),
        [0.0, 0.0, 0.0, 0.0],
        "antes del punto",
    );
}

#[test]
fn the_save_button_is_amber_only_with_pending_changes() {
    let clean = opened(1.0);
    let layout = clean.layout();
    let palette = clean.palette();
    let mut target = layer();
    let mut mutable = clean;
    {
        let mut ctx = PaintContext::new(
            &mut target,
            mutable.scale,
            mutable.dx,
            mutable.theme,
            &palette,
        );
        super::super::footer::draw_footer(&mut mutable, &mut ctx, &layout, 1.0);
    }
    assert_pixel(pixel(&target, 30, 562), track(), "Guardar deshabilitado");

    let mut dirty = opened(1.0);
    dirty
        .settings
        .set("speed", &crate::model::settings::SettingValue::from(1.25))
        .expect("clave válida");
    let layout = dirty.layout();
    let mut target = layer();
    let mut ctx = PaintContext::new(&mut target, dirty.scale, dirty.dx, dirty.theme, &palette);
    super::super::footer::draw_footer(&mut dirty, &mut ctx, &layout, 1.0);
    assert_pixel(pixel(&target, 30, 562), warm_btn(), "Guardar ámbar");
}

#[test]
fn the_footer_prefers_the_error_and_saved_flashes() {
    let mut menu = menu();
    menu.error_until = 5.0;
    assert_eq!(
        super::super::footer::footer_state(&menu, 1.0, true),
        ("NO SE PUDO GUARDAR".into(), "ERR".into(), false)
    );
    menu.error_until = 0.0;
    menu.saved_flash_until = 5.0;
    assert_eq!(
        super::super::footer::footer_state(&menu, 1.0, true),
        (
            super::super::footer::SAVED_TEXT.into(),
            "MINT".into(),
            false
        )
    );
    menu.saved_flash_until = 0.0;
    assert_eq!(
        super::super::footer::footer_state(&menu, 1.0, true),
        ("CAMBIOS SIN GUARDAR".into(), "WARM".into(), true)
    );
    assert_eq!(
        super::super::footer::footer_state(&menu, 1.0, false),
        (
            super::super::footer::SAVED_TEXT.into(),
            "TEXT_3".into(),
            false
        )
    );
}

// --------------------------------------------------------------------------
// Desplazamiento
// --------------------------------------------------------------------------

#[test]
fn the_scroll_indicator_and_edge_gradients_are_painted() {
    let panel = opened(1.0);
    let layout = panel.layout();
    let palette = panel.palette();
    let mut target = layer();
    super::super::scroll::draw_scroll(&panel, &mut target, &palette, &layout, 1.0, 0.0);
    assert!(panel.scroll_max() > 0.5);
    let content_top = panel.content_top(&layout);
    let footer_top = panel.footer_top(&layout);
    assert_eq!(content_top, 88.0);
    assert_eq!(footer_top, 538.0);
    assert_pixel(
        pixel(&target, 305, 100),
        [0.025_976_5, 0.030_870_6, 0.030_682_4, 0.048],
        "indicador en reposo",
    );
    assert_pixel(
        pixel(&target, 200, 537),
        [0.050_980_4, 0.086_274_5, 0.101_960_8, 1.0],
        "degradado inferior",
    );
}

#[test]
fn scrolling_to_the_end_moves_the_indicator_and_flips_the_gradients() {
    let panel = opened(1.0);
    let layout = panel.layout();
    let palette = panel.palette();
    let mut target = layer();
    let max_scroll = panel.scroll_max();
    super::super::scroll::draw_scroll(&panel, &mut target, &palette, &layout, 1.0, max_scroll);
    assert_pixel(
        pixel(&target, 200, 89),
        [0.046_345_8, 0.078_431_4, 0.092_691_6, 0.909_090_9],
        "degradado superior",
    );
    let content_top = panel.content_top(&layout);
    let footer_top = panel.footer_top(&layout);
    let clip_h = footer_top - content_top;
    let indicator_h = 24.0f64
        .max(clip_h * (clip_h / (clip_h + max_scroll)))
        .min(clip_h);
    let y0 = content_top + (clip_h - indicator_h);
    assert_pixel(
        pixel(&target, 305, (y0 + indicator_h - 1.0) as u32),
        [0.025_976_5, 0.030_870_6, 0.030_682_4, 0.048],
        "indicador al final",
    );
}

#[test]
fn an_active_indicator_reaches_full_alpha() {
    let mut panel = opened(1.0);
    panel.indicator_activity(1.0);
    let layout = panel.layout();
    let palette = panel.palette();
    let mut target = layer();
    super::super::scroll::draw_scroll(&panel, &mut target, &palette, &layout, 1.2, 0.0);
    assert_pixel(
        pixel(&target, 305, 100),
        [0.216_470_6, 0.257_254_9, 0.255_686_3, 0.4],
        "indicador activo",
    );
}

// --------------------------------------------------------------------------
// Ganchos completos
// --------------------------------------------------------------------------

#[test]
fn advanced_paints_footer_and_scroll_through_the_shared_frame() {
    let mut panel = opened(1.0);
    let mut target = layer();
    let mut extras = Advanced;
    panel.draw_with(&mut target, 1.0, &mut extras);
    let footer_top = panel.footer_top(&panel.layout());
    let alpha_at = |x: u32, y: u32| pixel(&target, x, y)[3];
    assert!(
        alpha_at(30, (footer_top + 24.0) as u32) > 0.5,
        "el pie se compone sobre la ventana"
    );
    assert!(
        alpha_at(305, (panel.content_top(&panel.layout()) + 10.0) as u32) > 0.01,
        "el indicador de scroll se pinta sobre la ventana"
    );
}

#[test]
fn the_remaining_hooks_are_no_ops_only_for_the_empty_frame() {
    let menu = opened(1.0);
    let mut target = layer();
    let mut extras = Advanced;
    let palette = menu.palette();
    let mut ctx = PaintContext::new(&mut target, menu.scale, menu.dx, menu.theme, &palette);
    extras.draw_slider_row(&menu, &mut ctx, "speed", 0.0, 1.0);
    extras.draw_toggle_row(&menu, &mut ctx, "show_advanced", 20.0, 1.0);
    extras.draw_action_row(
        &menu,
        &mut ctx,
        "repeat_last",
        0.0,
        20.0,
        &menu.layout(),
        1.0,
        0.0,
        1.0,
    );
    extras.draw_hotkey_value(&menu, &mut ctx, "hotkey", 10.0, 1.0, 1.0);
    extras.draw_copy_button(&menu, &mut ctx, "api_key", 0.0, 1.0, 1.0);
    assert!(target.as_raw().iter().any(|value| *value > 0.0));
}
