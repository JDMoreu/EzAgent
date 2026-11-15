//! Animation and live-state helpers of the chat panel (`chat_ui.py`).

use crate::geometry::{ease, WorkArea, FADE_SECS, MORPH_SECS};
use crate::model::chat::view::ChatView;

use super::{status_text, Anim, AnimState, ChatUi, ACTIVE_POSES, CLOSE_SECS};

/// `_duration`: the animation length for the current motion setting.
pub fn duration(reduced: bool, closing: bool) -> f64 {
    if reduced {
        FADE_SECS
    } else if closing {
        CLOSE_SECS
    } else {
        MORPH_SECS
    }
}

/// `_elapsed_for`: time that corresponds to an eased progress `p`.
pub fn elapsed_for(reduced: bool, p: f64, duration: f64) -> f64 {
    let duration = if reduced { FADE_SECS } else { duration };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if ease(middle) < p {
            low = middle;
        } else {
            high = middle;
        }
    }
    low * duration
}

/// `_step`: advances the animation and returns the current progress.
pub fn step(anim: &mut Anim, now: f64, reduced: bool, finished: &mut bool) -> f64 {
    if !matches!(anim.state, AnimState::Opening | AnimState::Closing) {
        return if anim.state == AnimState::Open {
            1.0
        } else {
            0.0
        };
    }
    let closing = anim.state == AnimState::Closing;
    let raw = (now - anim.t0) / duration(reduced, closing);
    if !closing {
        anim.p = ease(raw);
        if raw >= 1.0 {
            anim.state = AnimState::Open;
            anim.p = 1.0;
        }
    } else {
        anim.p = 1.0 - ease(raw);
        if raw >= 1.0 {
            anim.p = 0.0;
            *finished = true;
        }
    }
    anim.p
}

/// `_update_live`: status label and smoothing of the orb activity.
pub(super) fn update_live(
    ui: &dyn ChatUi,
    view: &mut ChatView,
    act: &mut f64,
    last: &mut f64,
    now: f64,
) {
    let mut pose = ui.chat_pose();
    let confirm = view.model.confirm();
    if (view.model.busy() || confirm.is_some())
        && matches!(pose.as_str(), "idle" | "sleep" | "mute")
    {
        pose = if confirm.is_some() {
            String::from("ask")
        } else {
            String::from("think")
        };
    }
    view.in_call = ui.in_call();
    view.status = status_text(&pose).to_string();
    let dt = (now - *last).clamp(0.0, 0.1);
    *last = now;
    let target = if ACTIVE_POSES.contains(&pose.as_str()) {
        1.0
    } else {
        0.0
    };
    *act += (target - *act) * (dt * 7.0).min(1.0);
    if (target - *act).abs() < 0.004 {
        *act = target;
    }
}

/// Pure `clamp` for the tests and for [`ChatPanel::clamp`].
pub(super) fn clamp_to_work(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    work: WorkArea,
    margin: i32,
) -> (i32, i32) {
    let high_x = work.2 - width + margin;
    let low_x = work.0 - margin;
    let high_y = work.3 - height + margin;
    let low_y = work.1 - margin;
    (x.min(high_x).max(low_x), y.min(high_y).max(low_y))
}
