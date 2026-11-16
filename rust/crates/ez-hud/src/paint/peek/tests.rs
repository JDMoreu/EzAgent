//! Pruebas de `paint::peek` (F5-06): valores calculados con el Python del proyecto.

use std::sync::Arc;

use super::*;
use parking_lot::Mutex;

use super::body::body;

type IconCall = (String, [u8; 4], f64, f64, f64);

fn item(key: &str, title: &str, agent: &str, state: &str, since: f64) -> PeekItem {
    PeekItem {
        key: key.to_string(),
        title: title.to_string(),
        agent: agent.to_string(),
        state: state.to_string(),
        since,
        unseen: false,
        live: false,
    }
}

fn items() -> Vec<PeekItem> {
    vec![
        PeekItem {
            live: true,
            ..item("k", "Tarea", "opencode", "working", 100.0)
        },
        PeekItem {
            unseen: true,
            ..item("k2", "Otra", "Claude", "done", 100.0)
        },
        item("k3", "Tres", "", "failed", 50.0),
        item("k4", "Cuatro", "  enjambre  ", "stale", 10.0),
    ]
}

#[test]
fn age_counts_minutes_and_hours() {
    assert_eq!(age(-5.0), "ahora");
    assert_eq!(age(0.0), "ahora");
    assert_eq!(age(59.9), "ahora");
    assert_eq!(age(60.0), "1 min");
    assert_eq!(age(119.0), "1 min");
    assert_eq!(age(120.0), "2 min");
    assert_eq!(age(3599.0), "59 min");
    assert_eq!(age(3600.0), "1 h");
    assert_eq!(age(7200.0), "2 h");
    assert_eq!(age(60.0 * 60.0 * 25.0 + 30.0), "25 h");
}

#[test]
fn meta_line_joins_label_agent_and_time() {
    let rows = items();
    assert_eq!(meta_line(&rows[0], 130.0), "En curso · OpenCode · ahora");
    assert_eq!(meta_line(&rows[1], 130.0), "Completada · Claude · ahora");
    assert_eq!(meta_line(&rows[2], 130.0), "Falló · hace 1 min");
    assert_eq!(
        meta_line(&rows[3], 130.0),
        "Sin actualización reciente · Enjambre · hace 2 min"
    );
    // Un agente desconocido conserva su nombre con la inicial en mayúscula.
    let unknown = item("x", "X", "hermes", "waiting", 100.0);
    assert_eq!(meta_line(&unknown, 130.0), "En espera · Hermes · ahora");
    // Los mismos tres tiempos que pide `test_activity.py`.
    let live = PeekItem {
        live: true,
        ..item("a", "Tarea a", "codex", "working", 0.0)
    };
    let done = item("b", "Tarea b", "codex", "done", 0.0);
    assert_eq!(meta_line(&live, 12.0 * 60.0), "En curso · Codex · 12 min");
    assert_eq!(
        meta_line(&done, 3.0 * 3600.0),
        "Completada · Codex · hace 3 h"
    );
    assert_eq!(meta_line(&done, 20.0), "Completada · Codex · ahora");
}

#[test]
fn a_one_row_notice_is_a_capsule_sized_like_the_pill() {
    use crate::model::drop::{BTN, GAP, PILL_H, PILL_W};
    fn measures() -> (f64, f64, f64) {
        (PILL_W, PEEK_W, BTN + GAP)
    }
    assert_eq!(card_height(1, false), PILL_H);
    assert_eq!(RADIUS, PILL_H / 2.0);
    let (pill_w, peek_w, step) = measures();
    assert!(pill_w < peek_w && peek_w < pill_w + 4.0 * step);
}

#[test]
fn the_two_lines_sit_centred_in_the_row() {
    for scale in [1.0, 1.275, 2.0] {
        let (first, second) = baselines(scale);
        let above = first - super::text::text_cap_height(FS_TASK, scale);
        let below = ROW_H - second;
        assert!(
            (above - below).abs() <= 0.01,
            "scale {scale}: {above} != {below}"
        );
    }
}

#[test]
fn fit_elides_with_the_ellipsis() {
    assert_eq!(
        fit("Tarea muy larga que no cabe", FS_TASK, 100.0, 1.0),
        "Tarea muy larg…"
    );
    assert_eq!(
        fit("Tarea muy larga que no cabe", FS_TASK, 200.0, 1.5),
        "Tarea muy larga que no cabe"
    );
    assert_eq!(fit("Corto", FS_META, 50.0, 1.0), "Corto");
    assert_eq!(fit("  espacio  ", FS_TASK, 10.0, 1.0), "…");
}

#[test]
fn card_height_adds_the_more_row() {
    assert_eq!(card_height(1, false), 38.0);
    assert_eq!(card_height(1, true), 60.0);
    assert_eq!(card_height(2, true), 98.0);
    assert_eq!(card_height(4, false), 152.0);
    assert_eq!(card_height(0, false), 0.0);
}

#[test]
fn baselines_center_the_text_block() {
    assert_eq!(baselines(1.0), (17.0, 30.0));
    assert_eq!(baselines(2.0), (17.0, 30.0));
    let (first, second) = baselines(1.25);
    assert!((first - 17.3).abs() < 1e-9);
    assert!((second - 30.3).abs() < 1e-9);
}

#[test]
fn ease_out_clamps_and_curves() {
    assert_eq!(ease_out(-1.0), 0.0);
    assert_eq!(ease_out(0.25), 0.578_125);
    assert_eq!(ease_out(0.5), 0.875);
    assert_eq!(ease_out(0.75), 0.984_375);
    assert_eq!(ease_out(1.0), 1.0);
    assert_eq!(ease_out(2.0), 1.0);
}

#[test]
fn dismissible_keeps_a_running_swarm() {
    let mut running = item("swarm:1", "E", "enjambre", "working", 0.0);
    running.live = true;
    assert!(!dismissible(&running));
    let mut done = item("swarm:1", "E", "enjambre", "done", 0.0);
    done.live = false;
    assert!(dismissible(&done));
    assert!(dismissible(&items()[0]));
}

#[test]
fn the_autonomous_row_is_not_dismissible_and_has_no_age() {
    let mut row = item(
        "autonomous",
        "Autónomo · ronda 2",
        "verificando",
        "autonomous",
        100.0,
    );
    row.live = true;
    assert!(
        !dismissible(&row),
        "la fila solo se quita deteniendo la tarea"
    );
    assert_eq!(meta_line(&row, 500.0), "verificando");
    let paused = item(
        "autonomous",
        "Autónomo · ronda 2",
        "en pausa: el mismo problema vuelve",
        "autonomous",
        100.0,
    );
    assert_eq!(
        meta_line(&paused, 500.0),
        "en pausa: el mismo problema vuelve"
    );
}

#[test]
fn toast_secs_uses_the_table_and_the_default() {
    assert_eq!(toast_secs("done"), 3.2);
    assert_eq!(toast_secs("needs_you"), 6.0);
    assert_eq!(toast_secs("failed"), 6.0);
    assert_eq!(toast_secs("stale"), 4.5);
    assert_eq!(toast_secs("working"), 3.5);
}

#[test]
fn the_body_matches_the_pillow_oracle() {
    let dark = body(260, 200, 1.0, "dark");
    assert_eq!(dark.len(), 260 * 200 * 4);
    assert_eq!(&dark[0..4], &[0.0, 0.0, 0.0, 0.0]);
    let center = (100 * 260 + 130) * 4;
    assert!((dark[center] - 13.0 / 255.0).abs() < 1e-9);
    assert!((dark[center + 1] - 22.0 / 255.0).abs() < 1e-9);
    assert!((dark[center + 2] - 26.0 / 255.0).abs() < 1e-9);
    assert_eq!(dark[center + 3], 1.0);
    assert_eq!(fnv1a_f32(&dark), 0xCA18_719C_BAF6_BBB5);

    let light = body(261, 201, 1.0, "light");
    assert_eq!(fnv1a_f32(&light), 0x17CA_4BB1_7F7E_BB77);

    let scaled = body(390, 300, 1.5, "dark");
    assert_eq!(fnv1a_f32(&scaled), 0x24F1_E42B_44C1_7B9D);
}

#[test]
fn the_spinner_matches_the_numpy_oracle() {
    let sprite = spinner(1.0, Color::Rgb([94, 226, 196]), 0, 14.0);
    assert_eq!((sprite.width, sprite.height), (14, 14));
    assert_eq!(fnv1a_f32(&sprite.data), 0xD92A_51A2_7930_6B6C);

    let sprite = spinner(1.0, Color::Rgb([94, 226, 196]), 7, 14.0);
    assert_eq!(fnv1a_f32(&sprite.data), 0x25FD_E173_E287_79DA);

    let sprite = spinner(1.5, Color::Rgba([255, 192, 82, 128]), 13, 16.0);
    assert_eq!((sprite.width, sprite.height), (24, 24));
    assert_eq!(fnv1a_f32(&sprite.data), 0x1114_1D3F_FDE9_B379);

    let sprite = spinner(2.0, Color::Rgb([12, 112, 117]), 23, 10.0);
    assert_eq!((sprite.width, sprite.height), (20, 20));
    assert_eq!(fnv1a_f32(&sprite.data), 0x766C_9EDF_B833_25E1);
}

#[test]
fn paint_rows_draws_the_hover_fill_the_rule_and_the_unseen_dot() {
    let rows = items();
    let plain = paint_rows(&rows[..3], 1, 260, 200, 1.0, "dark", None, 130.0, &NoIcons);
    // Fondo del cuerpo oscuro (13, 22, 26).
    let body_color = [13.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0, 1.0];
    assert_eq!(plain.pixel(43, 20), body_color);
    // Separador de la fila 0 con el alfa de LINE.
    let rule = plain.pixel(100, 51);
    assert!((rule[0] - 0.090_657_44).abs() < 1e-6);
    assert!((rule[1] - 0.126_874_28).abs() < 1e-6);
    assert!((rule[2] - 0.141_453_3).abs() < 1e-6);
    assert_eq!(rule[3], 1.0);
    // El punto de «sin ver» de la fila 1, en menta oscuro.
    let dot = plain.pixel(233, 71);
    assert!((dot[0] - 94.0 / 255.0).abs() < 1e-6);
    assert!((dot[1] - 226.0 / 255.0).abs() < 1e-6);
    assert!((dot[2] - 196.0 / 255.0).abs() < 1e-6);
    assert_eq!(dot[3], 1.0);

    let hovered = paint_rows(
        &rows[..3],
        1,
        260,
        200,
        1.0,
        "dark",
        Some(0),
        130.0,
        &NoIcons,
    );
    let hover_fill = [28.0 / 255.0, 45.0 / 255.0, 51.0 / 255.0, 1.0];
    for (x, y) in [(43, 20), (24, 20), (60, 20)] {
        let pixel = hovered.pixel(x, y);
        for channel in 0..4 {
            assert!((pixel[channel] - hover_fill[channel]).abs() < 1e-6);
        }
    }
    // Sin separador en la fila bajo el puntero.
    assert_eq!(hovered.pixel(100, 51), body_color);
}

#[test]
fn the_body_cache_returns_the_same_layer() {
    let first = body(100, 80, 1.0, "dark");
    let second = body(100, 80, 1.0, "dark");
    assert!(Arc::ptr_eq(&first, &second));
    let other = body(100, 80, 1.0, "light");
    assert!(!Arc::ptr_eq(&first, &other));
}

#[test]
fn paint_rows_draws_text_over_the_card() {
    let rows = vec![item("k", "Tarea", "opencode", "working", 100.0)];
    let buf = paint_rows(&rows, 0, 260, 200, 1.0, "dark", None, 130.0, &NoIcons);
    let body = [13.0 / 255.0, 22.0 / 255.0, 26.0 / 255.0, 1.0];
    let mut ink = 0;
    for y in 14..31 {
        for x in 48..120 {
            if buf.pixel(x, y) != body {
                ink += 1;
            }
        }
    }
    assert!(ink > 10, "el título no se dibujó ({ink} px)");
}

#[test]
fn paint_marks_uses_the_state_icon() {
    let recorder = Recorder::default();
    let rows = vec![
        item("a", "Tarea", "opencode", "needs_you", 100.0),
        item("b", "Tarea", "opencode", "failed", 100.0),
        item("c", "Tarea", "opencode", "waiting", 100.0),
        item("d", "Tarea", "opencode", "stale", 100.0),
        item("e", "Tarea", "opencode", "done", 100.0),
        item("f", "Tarea", "", "autonomous", 100.0),
    ];
    let mut buf = PeekBuffer::new(12, 6 * 48);
    paint_marks(&mut buf, &rows, 1.0, "dark", 0.0, true, &recorder);
    let calls = recorder.calls.lock();
    assert_eq!(calls.len(), 6);
    assert_eq!(calls[0].0, "alert");
    assert_eq!(calls[0].1, [255, 192, 82, 255]);
    assert_eq!(calls[1].0, "close");
    assert_eq!(calls[1].1, [255, 93, 82, 255]);
    assert_eq!(calls[2].0, "clock");
    assert_eq!(calls[2].1, [138, 164, 163, 255]);
    assert_eq!(calls[3].0, "minus");
    assert_eq!(calls[4].0, "check");
    assert_eq!(calls[4].1, [94, 226, 196, 255]);
    assert_eq!(calls[5].0, "sparkle");
    assert_eq!(calls[5].1, [94, 226, 196, 255]);
    assert_eq!(calls[0].2, 32.0);
    assert_eq!(calls[0].3, 33.0);
    assert_eq!(calls[0].4, 15.0);
}

#[test]
fn paint_marks_matches_the_numpy_oracle() {
    let working = vec![item("w", "Tarea", "opencode", "working", 100.0)];
    for (scale, now, reduced, expected) in [
        (1.0, 0.0, false, 0xa42f_e12d_6437_932c),
        (1.0, 0.4, false, 0x49f9_8bdf_23f3_c8b9),
        (1.5, 0.4, false, 0x6919_ffa2_8098_5071),
        (2.0, 1.234, true, 0x292d_ba0e_a3d3_d190),
    ] {
        let mut buf = PeekBuffer::new(260, 200);
        paint_marks(&mut buf, &working, scale, "dark", now, reduced, &NoIcons);
        assert_eq!(
            fnv1a_f32(&buf.data),
            expected,
            "escala {scale}, ahora {now}"
        );
    }
}

#[test]
fn the_working_mark_is_the_spinner() {
    let recorder = Recorder::default();
    let mut buf = PeekBuffer::new(64, 60);
    let working = vec![item("w", "Tarea", "opencode", "working", 100.0)];
    paint_marks(&mut buf, &working, 1.0, "dark", 0.0, true, &recorder);
    assert!(recorder.calls.lock().is_empty());
    let lit = (24..41)
        .flat_map(|y| (24..41).map(move |x| (x, y)))
        .filter(|&(x, y)| buf.pixel(x, y)[3] > 0.0)
        .count();
    assert!(lit > 10, "el indicador en curso no se dibujó ({lit} px)");
}

#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<IconCall>>,
}

impl IconPainter for Recorder {
    fn draw_icon(
        &self,
        _buf: &mut PeekBuffer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Color,
        _scale: f64,
    ) {
        let rgba = match color {
            Color::Rgb(rgb) => [rgb[0], rgb[1], rgb[2], 255],
            Color::Rgba(rgba) => rgba,
        };
        self.calls
            .lock()
            .push((name.to_string(), rgba, cx, cy, size));
    }
}

fn fnv1a_f32(values: &[f32]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for value in values {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    hash
}
