use super::base::subtract_masks;
use super::test_support::{palette, EmptyHost};
use super::*;
use ez_gfx::Mask;

#[test]
fn context_rounds_and_colours_like_python() {
    let pal = palette();
    let host = EmptyHost;
    let scaled = Ctx::new(1.5, &pal, 300.0, &host);
    assert_eq!(scaled.px(10.0), 15);
    assert_eq!(scaled.px(1.0), 2); // 1.5 rounds to even
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    assert_eq!(ctx.size("body"), 14.0);
    assert_eq!(ctx.length("Hola", "body"), 30.0);
    assert_eq!(ctx.color("TEXT_3", None), Rgba::new(138, 164, 163, 255));
    assert_eq!(ctx.color("MINT", Some(0.5)), Rgba::new(94, 226, 196, 128));
    assert_eq!(ctx.color("LINE", None), Rgba::new(228, 242, 240, 24));
    let child = ctx.child(120.0);
    assert_eq!(child.width, 120.0);
    assert!(child.hits.is_empty());
    assert_eq!(child.pal["BODY"], pal["BODY"]);
}

#[test]
fn wrap_spans_wraps_like_the_measurement_port() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let spans = [Span::plain("Hola mundo")];
    let lines = wrap_spans(&spans, "body", 60.0, ctx.scale);
    let view: Vec<Vec<&str>> = lines
        .iter()
        .map(|line| line.iter().map(|run| run.text.as_str()).collect())
        .collect();
    assert_eq!(view, vec![vec!["Hola"], vec!["mundo"]]);
}

#[test]
fn round_rect_fills_the_centre_and_clears_the_corners() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mut img = Image::new(120, 50, Rgba::TRANSPARENT);
    let red = Rgba::opaque(255, 0, 0);
    round_rect(
        &mut img,
        &ctx,
        (0.0, 0.0, 100.0, 40.0),
        10.0,
        Some(red),
        None,
    );
    assert_eq!(img.pixel(50, 20), red);
    assert_eq!(img.pixel(0, 0).a, 0);
    assert_eq!(img.pixel(105, 20).a, 0);
}

#[test]
fn line_paints_one_physical_pixel() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mut img = Image::new(40, 20, Rgba::TRANSPARENT);
    let mint = Rgba::opaque(94, 226, 196);
    line(&mut img, &ctx, 5.0, 5.0, 25.0, 6.0, mint);
    assert_eq!(img.pixel(10, 5), mint);
    assert_eq!(img.pixel(10, 4).a, 0);
    assert_eq!(img.pixel(10, 6).a, 0);
}

#[test]
fn draw_text_at_puts_ink_above_the_baseline() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mut img = Image::new(200, 60, Rgba::TRANSPARENT);
    draw_text_at(
        &mut img,
        &ctx,
        2.0,
        40.0,
        "Hola",
        "body",
        Rgba::opaque(255, 255, 255),
        false,
    );
    let inked: Vec<(u32, u32)> = (0..60)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .filter(|&(x, y)| img.pixel(x, y).a > 0)
        .collect();
    assert!(!inked.is_empty());
    assert!(inked.iter().all(|&(_, y)| y < 45));
    assert!(inked.iter().any(|&(_, y)| y >= 25));
}

#[test]
fn italic_text_is_slanted_and_still_inked() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mut img = Image::new(200, 60, Rgba::TRANSPARENT);
    draw_text_at(
        &mut img,
        &ctx,
        2.0,
        40.0,
        "Hola",
        "body",
        Rgba::opaque(255, 255, 255),
        true,
    );
    assert!(img.pixels().any(|pixel| pixel.a > 0));
}

#[test]
fn draw_text_keeps_leading_spaces() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mut plain = Image::new(200, 60, Rgba::TRANSPARENT);
    draw_text_at(
        &mut plain,
        &ctx,
        2.0,
        40.0,
        "Hola",
        "body",
        Rgba::opaque(255, 255, 255),
        false,
    );
    let mut indented = Image::new(200, 60, Rgba::TRANSPARENT);
    draw_text_at(
        &mut indented,
        &ctx,
        2.0,
        40.0,
        "    Hola",
        "body",
        Rgba::opaque(255, 255, 255),
        false,
    );
    let left = |img: &Image| (0..200).find(|&x| (0..60).any(|y| img.pixel(x, y).a > 0));
    let plain_left = left(&plain).expect("ink");
    let indented_left = left(&indented).expect("ink");
    assert_eq!(
        indented_left as f64 - plain_left as f64,
        ctx.length("    ", "body")
    );
}

#[test]
fn paint_lines_draws_links_in_mint_with_an_underline() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let spans = [Span {
        link: Some("https://example.com".to_string()),
        ..Span::plain("enlace")
    }];
    let lines = wrap_spans(&spans, "body", 300.0, ctx.scale);
    let mut img = Image::new(120, 40, Rgba::TRANSPARENT);
    paint_lines(
        &mut img,
        &ctx,
        &lines,
        "body",
        4.0,
        4.0,
        ctx.color("TEXT", None),
    );
    let mint = Rgba::opaque(94, 226, 196);
    assert!(img.pixels().any(|pixel| pixel == mint));
}

#[test]
fn mask_math_matches_pillow() {
    let left = Mask::from_raw(2, 1, vec![255, 100]).expect("mask");
    let right = Mask::from_raw(2, 1, vec![51, 200]).expect("mask");
    assert_eq!(multiply_masks(&left, &right).as_raw(), &[51, 78]);
    assert_eq!(subtract_masks(&left, &right).as_raw(), &[204, 0]);
    assert_eq!(multiply_mask(&left, 128).as_raw(), &[128, 50]);
    let image = Image::from_rgba(2, 1, vec![1, 2, 3, 0, 4, 5, 6, 0]).expect("image");
    assert_eq!(crop(&image, 1, 0, 2, 1).as_raw(), &[4, 5, 6, 0]);
}

#[test]
fn text_offset_matches_pillow_pen_and_baseline() {
    let body = style("body");
    let code = style("code");
    // Pillow pastes the mask at the pen: the ink starts at the glyph's bearing.
    assert_eq!(super::base::ink_bearing("ñandúes,", body, "body", 1.0), 1);
    assert_eq!(super::base::ink_bearing("y", body, "body", 1.0), 0);
    assert_eq!(super::base::ink_bearing("código", code, "code", 1.0), 0);
    // Rows: the outline box of `*` sits one row above its raster.
    assert_eq!(super::base::ink_top("*literal*.", body, 1.0), -11);
    assert_eq!(super::base::ink_top("cursiva", body, 1.0), -10);
    assert_eq!(super::base::ink_top("También", body, 1.0), -11);
    assert_eq!(super::base::ink_top("y", body, 1.0), -8);
}

#[test]
fn the_italic_shear_matches_pillow_affine_bicubic() {
    let mut data = vec![0u8; 30];
    for (x, y, value) in [
        (2, 0, 255),
        (3, 1, 255),
        (4, 2, 255),
        (2, 1, 200),
        (6, 0, 120),
    ] {
        data[y * 10 + x] = value;
    }
    let mask = Mask::from_raw(10, 3, data).expect("mask");
    // Pillow: `mask.transform(size, AFFINE, (1, 0.2, -0.4, 0, 1, 0), BICUBIC)`
    // with the baseline on row 2; the outer columns stay empty so the taps are
    // never clipped at the mask border.
    let expected = [
        [0, 0, 215, 92, 0, 0],
        [0, 0, 175, 255, 25, 0],
        [0, 0, 0, 27, 250, 0],
    ];
    let sheared = super::base::shear_rows(&mask, 2.0, 2);
    assert_eq!((sheared.width(), sheared.height()), (14, 3));
    for (y, row) in expected.iter().enumerate() {
        let produced = &sheared.row(y as u32)[2..8];
        assert_eq!(produced, row, "row {y}");
    }
    // The kernel is Pillow's unnormalised cubic: a half pixel keeps 1.25.
    assert_eq!(super::base::cubic_kernel(0.0), 1.0);
    assert_eq!(super::base::cubic_kernel(0.5), 0.625);
    assert_eq!(super::base::cubic_kernel(1.0), 0.0);
}

#[test]
fn text_over_a_transparent_pixel_copies_the_ink() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let mint = Rgba::opaque(94, 226, 196);
    let mut img = Image::new(40, 40, Rgba::TRANSPARENT);
    draw_text_at(&mut img, &ctx, 2.0, 20.0, "H", "body", mint, false);
    let ink = img
        .pixels()
        .find(|pixel| pixel.a > 0)
        .expect("the letter has ink");
    assert_eq!([ink.r, ink.g, ink.b], [94, 226, 196]);
    // A half-covered pixel over the chat body blends with Pillow's text ink.
    let mut chip = Image::new(40, 40, Rgba::opaque(40, 70, 80));
    super::base::blend_mask(
        &mut chip,
        &Mask::from_raw(1, 1, vec![128]).expect("mask"),
        (0, 0),
        mint,
    );
    assert_eq!(chip.pixel(0, 0), Rgba::new(67, 148, 138, 255));
}
