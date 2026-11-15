//! Casos de `hud_bar`: curvas numéricas y cuadros contra `parity/golden/hud/`.

use ez_hud::paint::marks::{breath, mark_paths};
use ez_hud::{anim, motion};

use crate::common::json::{arr, arr3, f64s, field, num, obj, pair, same, Json};
use crate::common::FixtureCase;

use super::bar::{frame, samples};

fn curve_case(case: &FixtureCase) -> Result<Json, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "cubic_bezier" => {
            let control = f64s(field(input, "control"));
            let y = anim::cubic_bezier(control[0], control[1], control[2], control[3])
                .apply(field(input, "x").as_f64().expect("x"));
            obj(vec![("y", num(y))])
        }
        "lerp_rgb" => {
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
        "LevelFollower.step" => {
            let mut follower = anim::LevelFollower::new(
                field(input, "attack_ms").as_f64().expect("attack"),
                field(input, "release_ms").as_f64().expect("release"),
            );
            let dt = field(input, "dt_ms").as_f64().expect("dt");
            let values = field(input, "targets")
                .as_array()
                .expect("targets")
                .iter()
                .map(|target| num(follower.step(target.as_f64().expect("target"), dt)))
                .collect();
            obj(vec![("values", arr(values))])
        }
        "Tween.update" => {
            let mut tween = anim::Tween::new(field(input, "start").as_f64().expect("start"));
            let curve = field(input, "curve").as_str();
            tween.to(
                field(input, "target").as_f64().expect("target"),
                0.0,
                field(input, "duration_ms").as_f64().expect("duration"),
                match curve.unwrap_or("out") {
                    "in" => anim::EASE_IN,
                    "inout" => anim::EASE_INOUT,
                    "out" => anim::EASE_OUT,
                    "spring" => anim::EASE_SPRING,
                    other => panic!("curva desconocida: {other}"),
                },
            );
            let mut values = Vec::new();
            let invert = !field(input, "invert_ms").is_null();
            let mut seen_200 = 0;
            for ms in samples(input) {
                if invert && ms == 200 {
                    seen_200 += 1;
                    if seen_200 == 2 {
                        tween.to(0.0, 0.2, 240.0, anim::EASE_OUT);
                    }
                }
                values.push(num(tween.update(ms as f64 / 1000.0)));
            }
            obj(vec![("values", arr(values))])
        }
        "Choreo.step" => {
            let names: Vec<&str> = field(input, "buttons")
                .as_array()
                .expect("buttons")
                .iter()
                .filter_map(Json::as_str)
                .collect();
            let mut choreo = motion::Choreo::new(&names, false);
            let want: std::collections::BTreeMap<String, bool> = names
                .iter()
                .map(|name| ((*name).to_string(), true))
                .collect();
            let order: Vec<Vec<&str>> = field(input, "order")
                .as_array()
                .expect("order")
                .iter()
                .map(|group| {
                    group
                        .as_array()
                        .expect("grupo")
                        .iter()
                        .filter_map(Json::as_str)
                        .collect()
                })
                .collect();
            let order: Vec<&[&str]> = order.iter().map(Vec::as_slice).collect();
            choreo.set_targets(&want, &order, 0.0, false);
            let mut previous = 0.0;
            let mut history = Vec::new();
            let mut times = samples(input);
            if times.is_empty() {
                times = vec![0, 16, 33, 50, 100, 200, 400, 1200];
            }
            for t_ms in times {
                let now = t_ms as f64 / 1000.0;
                choreo.step(now, now - previous, false, true, None, None);
                previous = now;
                let appear = names
                    .iter()
                    .map(|name| (*name, num(choreo.appear[*name].x)))
                    .collect();
                history.push(obj(vec![
                    ("t_ms", num(t_ms as f64)),
                    ("appear", obj(appear)),
                    ("grow", num(choreo.grow.x)),
                ]));
            }
            obj(vec![("samples", arr(history))])
        }
        "emerge" => {
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
        "Spring.step" => {
            let mut spring = motion::Spring::new(
                field(input, "start").as_f64().expect("start"),
                field(input, "k").as_f64().expect("k"),
                field(input, "c").as_f64().expect("c"),
            );
            spring.t = field(input, "target").as_f64().expect("target");
            let mut previous = 0.0;
            let mut values = Vec::new();
            for ms in samples(input) {
                values.push(num(spring.step(ms as f64 / 1000.0 - previous, false)));
                previous = ms as f64 / 1000.0;
            }
            obj(vec![("values", arr(values))])
        }
        "Spring.fade" => {
            let mut spring = motion::Spring::new(
                field(input, "start").as_f64().expect("start"),
                motion::SPRING_IN.0,
                motion::SPRING_IN.1,
            );
            spring.t = field(input, "target").as_f64().expect("target");
            let values = samples(input)
                .iter()
                .map(|ms| num(spring.fade(*ms as f64 / 1000.0, motion::REDUCED_FADE)))
                .collect();
            obj(vec![("values", arr(values))])
        }
        "mark_paths" => {
            let layout = field(input, "layout").as_str().expect("layout");
            let count = field(input, "count").as_i64().expect("count") as usize;
            let width = field(input, "w").as_f64().expect("w");
            let height = field(input, "h").as_f64().expect("h");
            let paths: Vec<Json> = mark_paths(layout, count, width, height)
                .iter()
                .map(|path| {
                    arr(path
                        .iter()
                        .map(|(x, y)| arr(vec![num(*x), num(*y)]))
                        .collect())
                })
                .collect();
            obj(vec![("paths", arr(paths))])
        }
        "breath" => {
            let reduced = field(input, "reduced").as_bool().expect("reduced");
            let values = samples(input)
                .iter()
                .map(|ms| num(breath(*ms as f64 / 1000.0, reduced)))
                .collect();
            obj(vec![("values", arr(values))])
        }
        other => return Err(format!("función sin portar: {other}")),
    })
}

fn image_bytes(case: &FixtureCase) -> Result<(Vec<u8>, u32, u32), String> {
    let t_ms = case
        .case
        .rsplit_once('@')
        .and_then(|(_, tail)| tail.parse::<i64>().ok())
        .unwrap_or(0);
    frame(&case.input, case.function.as_str(), t_ms)
}

pub fn run(
    case: &FixtureCase,
    golden: &std::path::Path,
    thresholds: &ez_parity::image::Thresholds,
) -> Result<(), String> {
    if matches!(
        case.function.as_str(),
        "render_model" | "button_sprite" | "FlowField.render" | "orbit_tile" | "draw_text"
    ) {
        let (bytes, width, height) = image_bytes(case)?;
        let relative = field(&case.output, "png")
            .as_str()
            .ok_or_else(|| "el caso no nombra su PNG".to_string())?;
        let expected = ez_parity::image::decode(&golden.join(relative))?;
        if expected.dimensions() != (width, height) {
            return Err(format!(
                "{}: tamaño {width}x{height} != {:?}",
                case.case,
                expected.dimensions()
            ));
        }
        let mut actual = expected.clone();
        actual.as_mut().copy_from_slice(&bytes);
        let delta = ez_parity::image::compare(&actual, &expected);
        let limit = if case.case.contains("/text/") {
            &thresholds.pixels.text
        } else {
            &thresholds.pixels.shapes
        };
        if ez_parity::image::within(&delta, limit) {
            Ok(())
        } else {
            Err(format!(
                "{}: max={} p999={} ssim={:.6}",
                case.case, delta.max, delta.p999, delta.ssim
            ))
        }
    } else {
        let produced = curve_case(case)?;
        if same(&produced, &case.output, 1e-9) {
            Ok(())
        } else {
            Err(format!("{}: {produced:?} != {:?}", case.case, case.output))
        }
    }
}
