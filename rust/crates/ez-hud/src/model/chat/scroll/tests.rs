use super::*;

#[test]
fn a_new_scroll_is_glued_to_the_end() {
    let mut scroll = Scroll::new();
    assert_eq!(scroll.offset(), 0.0);
    assert_eq!(scroll.max(), 0.0);
    scroll.wheel(120);
    assert_eq!(scroll.offset(), 0.0);
}

#[test]
fn the_wheel_walks_in_notches_and_stops_at_both_ends() {
    let mut scroll = Scroll::new();
    scroll.set_max(200.0);
    scroll.wheel(120);
    assert_eq!(scroll.offset(), SCROLL_STEP);
    scroll.wheel(240);
    assert_eq!(scroll.offset(), 3.0 * SCROLL_STEP);
    scroll.wheel(1200);
    assert_eq!(scroll.offset(), 200.0);
    scroll.wheel(-1200);
    assert_eq!(scroll.offset(), 0.0);
}

#[test]
fn the_maximum_clamps_the_current_offset() {
    let mut scroll = Scroll::new();
    scroll.set_max(100.0);
    scroll.wheel(120);
    scroll.wheel(120);
    assert_eq!(scroll.offset(), 100.0);
    scroll.set_max(40.0);
    assert_eq!(scroll.max(), 40.0);
    assert_eq!(scroll.offset(), 40.0);
    scroll.set_max(-5.0);
    assert_eq!(scroll.max(), 0.0);
    assert_eq!(scroll.offset(), 0.0);
}

#[test]
fn the_thumb_is_hidden_when_everything_fits() {
    let scroll = Scroll::new();
    assert_eq!(scroll.thumb(56.0, 400.0, 400.0, 440.0), None);
}

#[test]
fn the_thumb_moves_towards_the_top_as_the_offset_grows() {
    let mut scroll = Scroll::new();
    scroll.set_max(400.0);
    let bottom = scroll.thumb(56.0, 400.0, 800.0, 440.0).expect("thumb");
    // viewport^2 / total = 200; travel = 400 - 200 - 8.
    assert_eq!(bottom.height(), 200.0);
    assert_eq!(bottom.y0, 252.0);
    assert_eq!(bottom.x0, 434.0);
    scroll.wheel(120);
    let moved = scroll.thumb(56.0, 400.0, 800.0, 440.0).expect("thumb");
    let expected = 60.0 + 192.0 * (1.0 - scroll.offset() / 400.0);
    assert!((moved.y0 - expected).abs() < 1e-9);
    assert!(moved.y0 < bottom.y0);
}

#[test]
fn reset_returns_to_the_last_message() {
    let mut scroll = Scroll::new();
    scroll.set_max(100.0);
    scroll.set_offset(75.0);
    scroll.reset();
    assert_eq!(scroll.offset(), 0.0);
}
