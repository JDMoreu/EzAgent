//! Constantes de `tokens` para `hud_geometry`.

use ez_hud::tokens;

use crate::common::json::{arr, num, obj, s, Json};

pub fn constants() -> Json {
    let bezier = |control: (f64, f64, f64, f64)| {
        arr(vec![
            num(control.0),
            num(control.1),
            num(control.2),
            num(control.3),
        ])
    };
    obj(vec![
        (
            "COLOR_KEYS",
            arr(tokens::COLOR_KEYS.iter().map(|v| s(v)).collect()),
        ),
        ("GLOW_MAX_DARK", num(tokens::GLOW_MAX_DARK)),
        ("GLOW_MAX_LIGHT", num(tokens::GLOW_MAX_LIGHT)),
        ("SP_1", num(tokens::SP_1.into())),
        ("SP_2", num(tokens::SP_2.into())),
        ("SP_3", num(tokens::SP_3.into())),
        ("SP_4", num(tokens::SP_4.into())),
        ("SP_5", num(tokens::SP_5.into())),
        ("SP_6", num(tokens::SP_6.into())),
        ("SP_7", num(tokens::SP_7.into())),
        ("SP_8", num(tokens::SP_8.into())),
        ("R_MENU", num(tokens::R_MENU.into())),
        ("R_GROUP", num(tokens::R_GROUP.into())),
        ("FLARE", num(tokens::FLARE.into())),
        ("W", num(tokens::W.into())),
        ("H_ISLAND", num(tokens::H_ISLAND.into())),
        ("BODY_X0", num(tokens::BODY_X0)),
        ("BODY_X1", num(tokens::BODY_X1)),
        ("H_MENU", num(tokens::H_MENU.into())),
        ("AA_RAMP", num(tokens::AA_RAMP)),
        ("T_MICRO", num(tokens::T_MICRO.into())),
        ("T_FAST", num(tokens::T_FAST.into())),
        ("T_BASE", num(tokens::T_BASE.into())),
        ("T_SLOW", num(tokens::T_SLOW.into())),
        ("T_CLOSE_RETRACT", num(tokens::T_CLOSE_RETRACT.into())),
        ("VOICE_ATTACK", num(tokens::VOICE_ATTACK)),
        ("VOICE_RELEASE", num(tokens::VOICE_RELEASE)),
        ("FPS_ACTIVE", num(tokens::FPS_ACTIVE.into())),
        ("FPS_IDLE", num(tokens::FPS_IDLE.into())),
        ("BEZIER_OUT", bezier(tokens::BEZIER_OUT)),
        ("BEZIER_IN", bezier(tokens::BEZIER_IN)),
        ("BEZIER_INOUT", bezier(tokens::BEZIER_INOUT)),
        ("BEZIER_SPRING", bezier(tokens::BEZIER_SPRING)),
        ("ICON_GRID", num(tokens::ICON_GRID)),
        ("ICON_STROKE", num(tokens::ICON_STROKE)),
        ("ICON_STROKE_SMALL", num(tokens::ICON_STROKE_SMALL)),
        ("ICON_SMALL_THRESHOLD", num(tokens::ICON_SMALL_THRESHOLD)),
        ("ICON_SSAA", num(tokens::ICON_SSAA.into())),
    ])
}
