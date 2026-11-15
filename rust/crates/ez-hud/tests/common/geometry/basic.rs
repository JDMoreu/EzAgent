//! Casos de `hud_geometry` sin geometría del chat ni layout del menú.

use ez_hud::{a11y, anim, motion, theme, tokens};

use crate::common::json::{
    arr, arr3, arr4, color, colors, f64s, field, num, obj, pair, s, text, Json,
};
use crate::common::FixtureCase;

fn overrides(value: &Json) -> tokens::ThemeOverrides {
    let mut out = tokens::ThemeOverrides::new();
    if let Some(themes) = value.as_object() {
        for (name, entries) in themes {
            let mut map = tokens::ColorMap::new();
            for (key, rgb) in entries.as_object().expect("colores del tema") {
                let color = if f64s(rgb).len() == 4 {
                    tokens::Color::Rgba(arr4(rgb))
                } else {
                    tokens::Color::Rgb(arr3(rgb))
                };
                let key: &'static str = match key.as_str() {
                    "MINT" => "MINT",
                    "LINE" => "LINE",
                    other => panic!("clave de tema sin portar: {other}"),
                };
                map.insert(key, color);
            }
            out.insert(
                match name.as_str() {
                    "light" => "light",
                    "dark" => "dark",
                    other => panic!("tema sin portar: {other}"),
                },
                map,
            );
        }
    }
    out
}

fn check_json(check: &a11y::ContrastCheck) -> Json {
    obj(vec![
        ("theme", s(check.theme)),
        ("foreground", s(check.foreground)),
        ("background", s(check.background)),
        ("kind", s(check.kind)),
        ("minimum", num(check.minimum)),
        ("ratio", num(check.ratio)),
        ("advisory", Json::Bool(check.advisory)),
        ("passed", Json::Bool(check.passed())),
    ])
}

fn theme_arg(value: &Json) -> &'static str {
    match value.as_str().expect("tema") {
        "dark" => "dark",
        "light" => "light",
        other => panic!("tema de a11y: {other}"),
    }
}

fn curve(name: &str) -> anim::Easing {
    match name {
        "in" => anim::EASE_IN,
        "inout" => anim::EASE_INOUT,
        "out" => anim::EASE_OUT,
        "spring" => anim::EASE_SPRING,
        other => panic!("curva desconocida: {other}"),
    }
}

fn sequence_at(input: &Json) -> f64 {
    // El oráculo no guarda el instante inicial de las secuencias sin invertir;
    // se reconoce por los tiempos que fijó `parity/gen/hud_geometry.py`.
    let times = f64s(field(input, "times_s"));
    if times.as_slice() == [0.0, 0.1] {
        0.0
    } else if times.first() == Some(&-1.0) {
        0.1
    } else {
        1.5
    }
}

fn tween_case(input: &Json) -> Json {
    if let Some(samples) = field(input, "samples_s").as_array() {
        let mut tween = anim::Tween::new(field(input, "start").as_f64().expect("start"));
        tween.to(
            field(input, "target").as_f64().expect("target"),
            0.0,
            field(input, "duration_ms").as_f64().expect("duration"),
            curve(field(input, "curve").as_str().expect("curve")),
        );
        let values: Vec<Json> = samples
            .iter()
            .map(|sample| num(tween.update(sample.as_f64().expect("muestra"))))
            .collect();
        return obj(vec![
            ("values", arr(values)),
            ("active_after", Json::Bool(tween.active())),
            ("value_after", num(tween.value)),
        ]);
    }
    let invert = !field(input, "invert_at").is_null();
    let at = if invert { 0.2 } else { sequence_at(input) };
    let mut tween = anim::Tween::new(0.0);
    tween.to(1.0, at, 240.0, anim::EASE_OUT);
    let mut values: Vec<Json> = field(input, "times_s")
        .as_array()
        .expect("times")
        .iter()
        .map(|t| num(tween.update(t.as_f64().expect("t"))))
        .collect();
    if invert {
        tween.to(0.0, 0.2, 240.0, anim::EASE_OUT);
        for t in [0.2, 0.216, 0.25, 0.32, 0.44] {
            values.push(num(tween.update(t)));
        }
    }
    obj(vec![
        ("values", arr(values)),
        ("active", Json::Bool(tween.active())),
    ])
}

fn spring_step_case(input: &Json) -> Json {
    if field(input, "instant").as_bool().unwrap_or(false) {
        let mut spring = motion::Spring::new(
            field(input, "start").as_f64().expect("start"),
            motion::SPRING_IN.0,
            motion::SPRING_IN.1,
        );
        spring.t = field(input, "target").as_f64().expect("target");
        let value = spring.step(0.5, true);
        return obj(vec![
            ("value", num(value)),
            ("x", num(spring.x)),
            ("v", num(spring.v)),
        ]);
    }
    let mut spring = motion::Spring::new(
        field(input, "start").as_f64().expect("start"),
        field(input, "k").as_f64().expect("k"),
        field(input, "c").as_f64().expect("c"),
    );
    spring.t = field(input, "target").as_f64().expect("target");
    let times = f64s(field(input, "dts"));
    let values: Vec<Json> = times
        .iter()
        .map(|dt| num(spring.step(*dt, false)))
        .collect();
    // El oráculo de los resortes negativos solo guarda la traza de valores.
    if times.first() == Some(&-0.1) {
        return obj(vec![("values", arr(values))]);
    }
    obj(vec![
        ("values", arr(values)),
        ("x", num(spring.x)),
        ("v", num(spring.v)),
        ("settled", Json::Bool(spring.settled())),
    ])
}

fn choreo_case(input: &Json) -> Json {
    let names: Vec<&str> = field(input, "buttons")
        .as_array()
        .expect("buttons")
        .iter()
        .filter_map(Json::as_str)
        .collect();
    let mut choreo = motion::Choreo::new(&names, field(input, "shown").as_bool().expect("shown"));
    let want: std::collections::BTreeMap<String, bool> = field(input, "want")
        .as_object()
        .expect("want")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bool().unwrap_or(false)))
        .collect();
    let order: Vec<Vec<&str>> = field(input, "order")
        .as_array()
        .expect("order")
        .iter()
        .map(|g| {
            g.as_array()
                .expect("grupo")
                .iter()
                .filter_map(Json::as_str)
                .collect()
        })
        .collect();
    let order: Vec<&[&str]> = order.iter().map(Vec::as_slice).collect();
    let reduced = field(input, "reduced").as_bool().expect("reduced");
    choreo.set_targets(&want, &order, 0.0, reduced);
    let mut samples = Vec::new();
    let mut previous = 0.0;
    for t_ms in [0, 16, 33, 50, 80, 120, 200, 400, 800] {
        let now = t_ms as f64 / 1000.0;
        choreo.step(now, now - previous, reduced, true, None, None);
        previous = now;
        let appear: Vec<(&str, Json)> = names
            .iter()
            .map(|name| (*name, num(choreo.appear[*name].x)))
            .collect();
        let pointer: Vec<(&str, Json)> = names
            .iter()
            .map(|name| (*name, num(choreo.pointer[*name].x)))
            .collect();
        samples.push(obj(vec![
            ("t_ms", num(t_ms as f64)),
            ("appear", obj(appear)),
            ("pointer", obj(pointer)),
            ("grow", num(choreo.grow.x)),
            ("pending", Json::Bool(choreo.pending(now))),
            ("animating", Json::Bool(choreo.animating(now))),
        ]));
    }
    obj(vec![("samples", arr(samples))])
}

pub fn run(case: &FixtureCase) -> Result<Json, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "a11y.constants" => obj(vec![
            ("AA_TEXT", num(a11y::AA_TEXT)),
            ("AA_GRAPHIC", num(a11y::AA_GRAPHIC)),
        ]),
        "a11y.contrast" => {
            let a: Vec<u8> = f64s(field(input, "a")).iter().map(|v| *v as u8).collect();
            let b: Vec<u8> = f64s(field(input, "b")).iter().map(|v| *v as u8).collect();
            obj(vec![("ratio", num(a11y::contrast(&a, &b)))])
        }
        "a11y.luminance" => {
            let rgb: Vec<u8> = f64s(field(input, "rgb")).iter().map(|v| *v as u8).collect();
            obj(vec![("value", num(a11y::luminance(&rgb)))])
        }
        "a11y.failures" => arr(a11y::failures(
            theme_arg(field(input, "theme")),
            field(input, "include_advisory").as_bool().unwrap_or(false),
        )
        .iter()
        .map(check_json)
        .collect()),
        "a11y.contrast_report" => arr(a11y::contrast_report(
            theme_arg(field(input, "theme")),
            field(input, "include_advisory").as_bool().unwrap_or(false),
        )
        .iter()
        .map(check_json)
        .collect()),
        "a11y.validate_palette" => obj(vec![(
            "valid",
            Json::Bool(a11y::validate_palette(theme_arg(field(input, "theme")))),
        )]),
        "a11y.validate_all_palettes" => {
            let result = a11y::validate_all_palettes();
            obj(vec![
                ("dark", Json::Bool(result[0].1)),
                ("light", Json::Bool(result[1].1)),
            ])
        }
        "anim.constants" => {
            let bezier = |control: (f64, f64, f64, f64)| {
                arr(vec![
                    num(control.0),
                    num(control.1),
                    num(control.2),
                    num(control.3),
                ])
            };
            obj(vec![(
                "EASE_CONTROLS",
                obj(vec![
                    ("out", bezier(tokens::BEZIER_OUT)),
                    ("in", bezier(tokens::BEZIER_IN)),
                    ("inout", bezier(tokens::BEZIER_INOUT)),
                    ("spring", bezier(tokens::BEZIER_SPRING)),
                ]),
            )])
        }
        "anim.cubic_bezier" => {
            let control = f64s(field(input, "control"));
            let y = anim::cubic_bezier(control[0], control[1], control[2], control[3])
                .apply(field(input, "x").as_f64().expect("x"));
            obj(vec![("y", num(y))])
        }
        "anim.lerp_rgb" => {
            let raw = anim::lerp_rgb_raw(
                arr3(field(input, "a")),
                arr3(field(input, "b")),
                field(input, "k").as_f64().expect("k"),
            );
            obj(vec![(
                "rgb",
                arr(raw.iter().map(|v| num(*v as f64)).collect()),
            )])
        }
        "anim.LevelFollower.step" => {
            let mut follower = anim::LevelFollower::new(
                field(input, "attack_ms").as_f64().expect("attack"),
                field(input, "release_ms").as_f64().expect("release"),
            );
            let values: Vec<Json> = if let Some(targets) = field(input, "targets").as_array() {
                let dt = field(input, "dt_ms").as_f64().expect("dt");
                targets
                    .iter()
                    .map(|t| num(follower.step(t.as_f64().expect("target"), dt)))
                    .collect()
            } else {
                f64s(field(input, "dt_ms"))
                    .iter()
                    .map(|dt| num(follower.step(0.6, *dt)))
                    .collect()
            };
            obj(vec![("values", arr(values))])
        }
        "anim.Tween.update" => tween_case(input),
        "bar_motion.constants" => obj(vec![
            ("STAGGER_IN", num(motion::STAGGER_IN)),
            ("STAGGER_OUT", num(motion::STAGGER_OUT)),
            (
                "SPRING_IN",
                arr(vec![num(motion::SPRING_IN.0), num(motion::SPRING_IN.1)]),
            ),
            (
                "SPRING_OUT",
                arr(vec![num(motion::SPRING_OUT.0), num(motion::SPRING_OUT.1)]),
            ),
            (
                "SPRING_POINTER",
                arr(vec![
                    num(motion::SPRING_POINTER.0),
                    num(motion::SPRING_POINTER.1),
                ]),
            ),
            (
                "SPRING_GROW",
                arr(vec![num(motion::SPRING_GROW.0), num(motion::SPRING_GROW.1)]),
            ),
            ("FORM_GROW", num(motion::FORM_GROW)),
            ("HOVER_SCALE", num(motion::HOVER_SCALE)),
            ("PRESS_SCALE", num(motion::PRESS_SCALE)),
            ("BORN_SCALE", num(motion::BORN_SCALE)),
            ("REDUCED_FADE", num(motion::REDUCED_FADE)),
            ("SETTLE_EPS", num(motion::SETTLE_EPS)),
        ]),
        "bar_motion.emerge" => {
            let value = motion::emerge(
                pair(&f64s(field(input, "slot"))),
                pair(&f64s(field(input, "origin"))),
                field(input, "a").as_f64().expect("a"),
                field(input, "reduced").as_bool().expect("reduced"),
            );
            obj(vec![(
                "value",
                arr(vec![num(value.0), num(value.1), num(value.2), num(value.3)]),
            )])
        }
        "bar_motion.Spring.step" => spring_step_case(input),
        "bar_motion.Spring.fade" => {
            let mut spring = motion::Spring::new(
                field(input, "x").as_f64().expect("x"),
                motion::SPRING_IN.0,
                motion::SPRING_IN.1,
            );
            spring.t = field(input, "target").as_f64().expect("target");
            let seconds = field(input, "seconds").as_f64().expect("seconds");
            let values: Vec<Json> = f64s(field(input, "dts"))
                .iter()
                .map(|dt| num(spring.fade(*dt, seconds)))
                .collect();
            obj(vec![
                ("values", arr(values)),
                ("x", num(spring.x)),
                ("v", num(spring.v)),
                ("settled", Json::Bool(spring.settled())),
            ])
        }
        "bar_motion.Choreo.step" => choreo_case(input),
        "bar_theme.card_palette" => {
            let name =
                crate::common::json::theme_name(field(input, "theme").as_str().unwrap_or(""));
            obj(vec![
                ("overrides", colors(&theme::card_overrides(name))),
                ("palette", colors(&theme::card_palette(name))),
                ("body_top", color(theme::body_top(name))),
            ])
        }
        "bar_theme.contrast" => obj(vec![(
            "ratio",
            num(theme::contrast(
                tokens::Color::Rgb(arr3(field(input, "a"))),
                tokens::Color::Rgb(arr3(field(input, "b"))),
            )),
        )]),
        "theme_accent.parse_hex" => obj(vec![(
            "rgb",
            match theme::parse_hex(&text(field(input, "value"))) {
                None => Json::Null,
                Some(rgb) => arr(rgb.iter().map(|v| num(*v as f64)).collect()),
            },
        )]),
        "theme_accent.derive" => {
            match theme::derive(
                &text(field(input, "primary")),
                &text(field(input, "background")),
            ) {
                None => Json::Null,
                Some(map) => obj(map
                    .iter()
                    .map(|(name, entries)| (*name, colors(entries)))
                    .collect()),
            }
        }
        "theme_accent.constants" => obj(vec![
            (
                "SURFACE_KEYS",
                arr(theme::SURFACE_KEYS.iter().map(|v| s(v)).collect()),
            ),
            (
                "LINE_KEYS",
                arr(theme::LINE_KEYS.iter().map(|v| s(v)).collect()),
            ),
            (
                "ACCENT_KEYS",
                arr(theme::ACCENT_KEYS.iter().map(|v| s(v)).collect()),
            ),
            ("MIN_CONTRAST", num(theme::MIN_CONTRAST)),
            ("DARK_ACCENT_CONTRAST", num(theme::DARK_ACCENT_CONTRAST)),
            ("ACCENT_START", num(theme::ACCENT_START)),
            (
                "BODY_TOP",
                obj(vec![
                    ("light", color(theme::BODY_TOP["light"])),
                    ("dark", color(theme::BODY_TOP["dark"])),
                ]),
            ),
        ]),
        "tokens.constants" => super::tokens::constants(),
        "tokens.FontStyle" => {
            let style =
                tokens::font_style(field(input, "token").as_str().expect("token")).expect("estilo");
            obj(vec![
                ("token", s(style.token)),
                ("family", s(style.family)),
                ("size", num(style.size)),
                ("weight", num(style.weight.into())),
                ("tracking_em", num(style.tracking_em)),
                ("uppercase", Json::Bool(style.uppercase)),
                ("mono", Json::Bool(style.mono())),
            ])
        }
        "tokens.glow_max" => num(tokens::glow_max(
            field(input, "theme").as_str().expect("tema"),
        )),
        "tokens.palette" => colors(&tokens::palette(
            field(input, "theme").as_str().expect("tema"),
        )?),
        "tokens.Rect" => {
            let v = f64s(field(input, "rect"));
            let rect = tokens::Rect::new(v[0], v[1], v[2], v[3]);
            obj(vec![
                ("width", num(rect.width())),
                ("height", num(rect.height())),
                (
                    "center",
                    arr(vec![num(rect.center().0), num(rect.center().1)]),
                ),
            ])
        }
        "tokens.Rect.overlaps" => {
            let a = f64s(field(input, "a"));
            let b = f64s(field(input, "b"));
            obj(vec![(
                "overlaps",
                Json::Bool(
                    tokens::Rect::new(a[0], a[1], a[2], a[3])
                        .overlaps(&tokens::Rect::new(b[0], b[1], b[2], b[3])),
                ),
            )])
        }
        "tokens.set_themed" => {
            let before = tokens::palette_version();
            let themed = overrides(field(input, "overrides"));
            tokens::set_themed(Some(&themed));
            let after = tokens::palette_version();
            let light = tokens::palette("light")?;
            let dark = tokens::palette("dark")?;
            tokens::set_themed(Some(&themed));
            let same = tokens::palette_version();
            tokens::set_themed(None);
            obj(vec![
                ("version_before", num(before as f64)),
                ("version_after", num(after as f64)),
                ("version_same", num(same as f64)),
                ("version_reset", num(tokens::palette_version() as f64)),
                (
                    "light",
                    obj(vec![
                        ("MINT", color(light["MINT"])),
                        ("LINE", color(light["LINE"])),
                    ]),
                ),
                (
                    "dark",
                    obj(vec![
                        ("MINT", color(dark["MINT"])),
                        ("LINE", color(dark["LINE"])),
                    ]),
                ),
            ])
        }
        other => return Err(format!("función sin portar: {other}")),
    })
}
