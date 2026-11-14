//! Pruebas de `paint::settings_window` (F5-12).

use super::*;
use crate::theme::card_palette;

const WORK: WorkArea = (0, 0, 1920, 1040);

fn pixel(layer: &Layer, x: usize, y: usize) -> [f32; 4] {
    let index = (y * layer.width() as usize + x) * 4;
    layer.as_raw()[index..index + 4]
        .try_into()
        .expect("4 canales")
}

#[test]
fn the_five_sections_keep_their_order_and_titles() {
    assert_eq!(
        sections(),
        [
            ("appearance", "Aspecto".to_string()),
            ("listening", "Escucha".to_string()),
            ("voice", "Voz".to_string()),
            ("models", "Modelos".to_string()),
            ("system", "Sistema".to_string()),
        ]
    );
    assert_eq!(section_title("system"), "Sistema");
    // `str.title()` también pone mayúscula tras el guion.
    assert_eq!(section_title("no-existe"), "No-Existe");
}

#[test]
fn section_groups_resolve_and_fall_back_to_appearance() {
    let appearance = section_groups("appearance");
    assert_eq!(appearance.len(), 2);
    assert_eq!(appearance[0].0, "look");
    assert_eq!(appearance[1].0, "theme");
    assert_eq!(section_groups("no-existe"), appearance);
    let system = section_groups("system");
    assert_eq!(system.len(), 7);
    assert_eq!(system[0].0, "hotkeys");
    assert_eq!(system[6].0, "files");
}

#[test]
fn content_and_window_sizes_follow_the_factory_constants() {
    assert_eq!(content_size(760.0, 620.0), (576.0, 616.0));
    assert_eq!(content_size(100.0, 4.0), (1.0, 1.0));
    assert_eq!(window_size(560.0), (760.0, 564.0));
    assert_eq!(
        (DEFAULT_W, DEFAULT_H, MIN_W, MIN_H, SIDEBAR_W),
        (760.0, 620.0, 560.0, 400.0, 184.0)
    );
}

#[test]
fn size_and_position_clamp_to_the_work_area() {
    assert_eq!(clamp_size(100.0, 100.0, WORK, 1.0), (560.0, 400.0));
    assert_eq!(clamp_size(9999.0, 9999.0, WORK, 1.0), (1904.0, 1024.0));
    assert_eq!(clamp_size(9999.0, 9999.0, WORK, 2.0), (944.0, 504.0));
    assert_eq!(clamp_size(760.0, 620.0, WORK, 1.0), (760.0, 620.0));
    assert_eq!(centered(WORK, 760, 620), (580, 210));
    assert_eq!(clamp_position(-9000, -9000, 760, 620, WORK, 1.0), (-664, 0));
    assert_eq!(clamp_position(9000, 9000, 760, 620, WORK, 1.0), (1824, 944));
    assert_eq!(clamp_position(300, 200, 760, 620, WORK, 1.0), (300, 200));
    // A escala 2 la franja visible también crece.
    assert_eq!(clamp_position(9000, 9000, 760, 620, WORK, 2.0), (1728, 848));
}

#[test]
fn resize_edges_are_the_python_zones() {
    let (w, h) = (760.0, 620.0);
    assert_eq!(resize_edges(w / 2.0, h / 2.0, w, h), None);
    assert_eq!(
        resize_edges(w - BODY_INSET - 2.0, h / 2.0, w, h),
        Some((false, false, true, false))
    );
    assert_eq!(
        resize_edges(w / 2.0, h - 2.0, w, h),
        Some((false, false, false, true))
    );
    assert_eq!(
        resize_edges(w - BODY_INSET - 4.0, h - 4.0, w, h),
        Some((false, false, true, true))
    );
    assert_eq!(
        resize_edges(BODY_INSET + 4.0, 4.0, w, h),
        Some((true, true, false, false))
    );
    assert_eq!(resize_edges(1.0, h / 2.0, w, h), None);
    assert_eq!(resize_edges(-50.0, h / 2.0, w, h), None);
}

#[test]
fn cursors_and_resizing_match_windows_identifiers() {
    assert_eq!(cursor_for(None), 0);
    assert_eq!(cursor_for(Some((false, false, true, false))), IDC_SIZEWE);
    assert_eq!(cursor_for(Some((false, false, false, true))), IDC_SIZENS);
    assert_eq!(cursor_for(Some((true, false, false, false))), IDC_SIZEWE);
    assert_eq!(cursor_for(Some((false, true, false, false))), IDC_SIZENS);
    assert_eq!(cursor_for(Some((false, false, true, true))), IDC_SIZENWSE);
    assert_eq!(cursor_for(Some((true, false, false, true))), IDC_SIZENESW);
    assert_eq!((IDC_SIZEWE, IDC_SIZENS), (32644, 32645));
    assert_eq!((IDC_SIZENWSE, IDC_SIZENESW), (32642, 32643));

    let limits = (560, 400, 1900, 1000);
    let start = (500, 300, 760, 620);
    assert_eq!(
        resized_rect((false, false, true, true), start, (100, 50), limits),
        (500, 300, 860, 670)
    );
    let (x, y, w, h) = resized_rect((true, false, false, false), start, (-80, 0), limits);
    assert_eq!((x + w, y, h), (1260, 300, 620));
    assert_eq!(w, 840);
    let (x, y, w, h) = resized_rect((true, true, false, false), start, (900, 900), limits);
    assert_eq!((w, h), (560, 400));
    assert_eq!((x + w, y + h), (1260, 920));
    let (_, _, w, h) = resized_rect((false, false, true, true), start, (9000, 9000), limits);
    assert_eq!((w, h), (1900, 1000));
}

#[test]
fn header_and_sidebar_items_are_hit_tested() {
    assert!(in_header(100.0, 0.0));
    assert!(in_header(100.0, HEADER_H));
    assert!(!in_header(100.0, HEADER_H + 0.1));
    assert!(!in_header(100.0, -0.1));
    assert_eq!(item_rect(0), (14.0, 64.0, 174.0, 104.0));
    assert_eq!(item_rect(4), (14.0, 240.0, 174.0, 280.0));
    for (index, (name, _)) in sections().iter().enumerate() {
        let (x0, y0, x1, y1) = item_rect(index as i32);
        let point = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        assert_eq!(item_at(point.0, point.1), Some(*name));
    }
    assert_eq!(item_at(14.0, 64.0), Some("appearance"));
    assert_eq!(item_at(174.0, 104.0), Some("appearance"));
    assert_eq!(item_at(175.0, 80.0), None);
    assert_eq!(item_at(100.0, 106.0), None);
}

#[test]
fn composites_and_fades_follow_the_premultiplied_arithmetic() {
    let mut dst = Layer::new(2, 2);
    for value in dst.as_raw_mut() {
        *value = 0.25;
    }
    let mut src = Layer::new(1, 1);
    src.as_raw_mut().copy_from_slice(&[0.5, 0.0, 0.0, 0.5]);
    composite_over(&mut dst, &src, 1, 1);
    let blended = pixel(&dst, 1, 1);
    assert!((blended[0] - 0.625).abs() < 1e-6);
    assert!((blended[3] - 0.625).abs() < 1e-6);
    assert!((pixel(&dst, 0, 0)[0] - 0.25).abs() < 1e-6);
    // Fuera del destino no cambia nada y no hay pánico.
    composite_over(&mut dst, &src, 5, 5);
    assert!((pixel(&dst, 0, 0)[0] - 0.25).abs() < 1e-6);

    assert_eq!(cap_top(100.0, 14.0), 95.002);
    assert!((cap_top(84.0, 11.0) - 80.073).abs() < 1e-9);
    assert_eq!(fade_factor(0.25), 0.15625);
    assert_eq!(fade_factor(0.0), 0.0);
    assert_eq!(fade_factor(1.0), 1.0);
    assert_eq!(fade_factor(1.5), 1.0);
    assert_eq!(fade_factor(-1.0), 0.0);
    assert!(fade_factor(1e-12) > 0.0);
    let mut layer = Layer::new(1, 1);
    layer.as_raw_mut().copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    fade(&mut layer, 0.5);
    assert_eq!(layer.as_raw(), &[0.5, 0.5, 0.5, 0.5]);
    fade(&mut layer, 1.0);
    assert_eq!(layer.as_raw(), &[0.5, 0.5, 0.5, 0.5]);
}

#[test]
fn the_grip_draws_three_staircase_rows_in_the_footer() {
    let mut buf = Layer::new(760, 620);
    draw_grip(&mut buf, 1.0, &card_palette("dark"));
    let right = 760.0 - BODY_INSET - 13.0;
    let cy = 620.0 - WINDOW_EXTRA_H - PAD_BOTTOM as f64 - FOOTER as f64 / 2.0;
    // El centro del círculo cae en la esquina de un píxel: la cobertura es
    // `0,5 - (√0,5 - 1,1)` y el alfa, el 55 % de la misma.
    let dot = pixel(&buf, right as usize, (cy + 5.0) as usize);
    let cover = 0.5 - (0.5f64.sqrt() - 1.1);
    assert!(
        (dot[0] - 138.0 / 255.0 * 0.55 * cover as f32).abs() < 0.01,
        "rojo: {dot:?}"
    );
    assert!((dot[3] - 0.55 * cover as f32).abs() < 0.01, "alfa: {dot:?}");
    let rows: Vec<usize> = (0..buf.height() as usize)
        .filter(|row| (0..buf.width() as usize).any(|column| pixel(&buf, column, *row)[3] > 0.0))
        .collect();
    assert!(!rows.is_empty());
    assert!(*rows.iter().max().expect("filas") <= cy as usize + 6);
}
