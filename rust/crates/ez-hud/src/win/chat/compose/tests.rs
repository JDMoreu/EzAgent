//! Tests of the panel composition (`chat_paint.py`, F5-15).

use std::sync::Arc;

use ndarray::Array3;

use crate::win::chat::test_support::{fake_ui, sample_view, FakeUi};
use crate::win::chat::{fluid_array, row_reveal, shape_progress};

use super::*;

#[test]
fn shape_progress_settles_with_a_small_overshoot() {
    assert_eq!(shape_progress(0.0, true), 0.0);
    assert_eq!(shape_progress(0.5, false), 0.5);
    assert_eq!(shape_progress(1.0, true), 1.0);
    assert!(shape_progress(0.8, true) > 0.8);
    assert!(shape_progress(0.8, true) > shape_progress(0.8, false));
    // Values from the Python: numpy float64 in, float64 out.
    assert!((shape_progress(0.8, true) - 1.0304).abs() < 1e-12);
    assert!((shape_progress(0.5, true) - 0.5290816326530612).abs() < 1e-12);
}

#[test]
fn row_reveal_goes_from_top_to_bottom() {
    let hidden = row_reveal(4, 0.0, true);
    assert_eq!(hidden, vec![0.0; 4]);
    let shown = row_reveal(4, 1.0, true);
    assert_eq!(shown, vec![1.0; 4]);
    let middle = row_reveal(4, 0.6, true);
    // The top row appears first: the reveal falls towards the bottom.
    assert!(middle.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!((middle[0] - 0.500_000_06).abs() < 1e-6);
    assert!((middle[1] - 0.143_073_08).abs() < 1e-6);
    assert_eq!(middle[2], 0.0);
    assert_eq!(middle[3], 0.0);
    let closing = row_reveal(4, 0.6, false);
    assert!(closing.iter().all(|value| *value <= 1.0 && *value >= 0.0));
}

#[test]
fn the_fluid_is_stretched_and_normalized() {
    let rgb = Array3::from_shape_vec((1, 1, 3), vec![255, 0, 0]).expect("one colour");
    let fluid = fluid_array(Some(&rgb), 4, 3).expect("fluid");
    assert_eq!(fluid.dim(), (3, 4, 3));
    for row in 0..3 {
        for column in 0..4 {
            assert!((fluid[[row, column, 0]] - 1.0).abs() < 1e-6);
            assert_eq!(fluid[[row, column, 1]], 0.0);
            assert_eq!(fluid[[row, column, 2]], 0.0);
        }
    }
    assert!(fluid_array(None, 4, 3).is_none());
    let minimum = fluid_array(Some(&rgb), 0, 0).expect("fluid");
    assert_eq!(minimum.dim(), (2, 2, 3));
}

#[test]
fn a_scene_without_a_fluid_paints_the_body_colour() {
    let ui = fake_ui();
    let mut view = sample_view();
    view.configure(
        80.0,
        80.0,
        1.0,
        "dark",
        crate::tokens::palette("dark").expect("the dark palette exists"),
    );
    let mut content = ContentCache::default();
    let mut scene = Scene {
        ui: ui.as_ref(),
        view: &mut view,
        anim: AnimState::Open,
        form: (10.0, 10.0, 30.0, 30.0),
        act: 0.0,
        now: 0.0,
    };
    let frame = compose(
        &mut scene,
        1.0,
        false,
        WindowRect {
            x: 0,
            y: 0,
            width: 120,
            height: 120,
        },
        &mut content,
    );
    assert_eq!((frame.width, frame.height), (120, 120));
    assert_eq!(frame.pixel(60, 60)[3], 255);
    let body = frame
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel == [26, 22, 13, 255])
        .count();
    assert!(body > 100, "the body colour is missing ({body} px)");
    // The margin around the panel stays transparent.
    assert_eq!(frame.pixel(2, 2)[3], 0);
}

#[test]
fn the_live_orb_and_status_line_become_layers() {
    let ui = Arc::new(FakeUi {
        fluid: true,
        ..FakeUi::default()
    });
    let mut view = sample_view();
    view.orb = Some((20.0, 20.0, 8.0));
    let mut scene = Scene {
        ui: ui.as_ref(),
        view: &mut view,
        anim: AnimState::Open,
        form: (0.0, 0.0, 0.0, 0.0),
        act: 1.0,
        now: 0.0,
    };
    let layers = live_layers(&mut scene, 100, 100, 0.0, 1.0, &[1.0], Some(16), Some(16));
    assert_eq!(layers.len(), 2);
    assert!(layers[0].2[[4, 4, 3]] > 0.0);
    assert_eq!(layers[0].0, 32);
    assert_eq!(layers[0].1, 32);
}

#[test]
fn the_stable_frame_is_cached_and_only_the_live_layer_moves() {
    let ui = fake_ui();
    let mut view = sample_view();
    let mut content = ContentCache::default();
    let mut base = None;
    let window = WindowRect {
        x: 0,
        y: 0,
        width: 120,
        height: 120,
    };
    let mut scene = Scene {
        ui: ui.as_ref(),
        view: &mut view,
        anim: AnimState::Open,
        form: (0.0, 0.0, 0.0, 0.0),
        act: 0.0,
        now: 0.0,
    };
    let first = stable(&mut scene, &mut base, &mut content, window);
    let second = stable(&mut scene, &mut base, &mut content, window);
    assert_eq!(first, second);
    assert!(base.is_some());
    assert!(content.0.is_some());
    // A different status invalidates the cached content.
    scene.view.status = String::from("Pensando…");
    let third = stable(&mut scene, &mut base, &mut content, window);
    assert_ne!(first, third);
}

#[test]
fn a_shared_ui_can_be_reused_for_several_scenes() {
    let ui = Arc::clone(&fake_ui());
    let mut view = sample_view();
    let mut scene = Scene {
        ui: ui.as_ref(),
        view: &mut view,
        anim: AnimState::Closing,
        form: (0.0, 0.0, 10.0, 10.0),
        act: 0.0,
        now: 0.0,
    };
    let frame = compose(
        &mut scene,
        0.5,
        false,
        WindowRect {
            x: 0,
            y: 0,
            width: 60,
            height: 60,
        },
        &mut ContentCache::default(),
    );
    assert_eq!((frame.width, frame.height), (60, 60));
}
