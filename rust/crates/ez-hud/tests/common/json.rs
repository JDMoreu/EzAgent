//! Construcción y comparación de valores JSON sin depender de `serde_json`.

use ez_core::pyloads::loads;
use ez_hud::{geometry, tokens};

pub type Json = <ez_core::pyjson::OrderedObject as std::ops::Index<&'static str>>::Output;

pub fn parse(text: &str) -> Json {
    loads(text).expect("JSON interno válido")
}

pub fn num(value: f64) -> Json {
    parse(&value.to_string())
}

pub fn arr(items: Vec<Json>) -> Json {
    Json::Array(items)
}

pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
    let Json::Object(mut map) = parse("{}") else {
        unreachable!()
    };
    for (key, value) in pairs {
        map.insert(key.to_string(), value);
    }
    Json::Object(map)
}

pub fn s(value: &str) -> Json {
    Json::String(value.to_string())
}

pub fn field<'a>(input: &'a Json, name: &str) -> &'a Json {
    input.get(name).unwrap_or(&Json::Null)
}

pub fn f64s(value: &Json) -> Vec<f64> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(Json::as_f64).collect())
        .unwrap_or_default()
}

pub fn arr3(value: &Json) -> [u8; 3] {
    let v = f64s(value);
    [v[0] as u8, v[1] as u8, v[2] as u8]
}

pub fn arr4(value: &Json) -> [u8; 4] {
    let v = f64s(value);
    [v[0] as u8, v[1] as u8, v[2] as u8, v[3] as u8]
}

pub fn pair(v: &[f64]) -> (f64, f64) {
    (v[0], v[1])
}

pub fn rect4(v: &[f64]) -> (f64, f64, f64, f64) {
    (v[0], v[1], v[2], v[3])
}

pub fn text(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn color(value: tokens::Color) -> Json {
    match value {
        tokens::Color::Rgb(c) => arr(c.iter().map(|v| num(f64::from(*v))).collect()),
        tokens::Color::Rgba(c) => arr(c.iter().map(|v| num(f64::from(*v))).collect()),
    }
}

pub fn colors(map: &tokens::ColorMap) -> Json {
    obj(map
        .iter()
        .map(|(key, value)| (*key, color(*value)))
        .collect())
}

pub fn theme_name(theme: &str) -> &'static str {
    if theme == "light" {
        "light"
    } else {
        "dark"
    }
}

pub fn work_area(value: &Json) -> geometry::WorkArea {
    let v = f64s(value);
    (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32)
}

pub fn saved(input: &Json) -> Option<(f64, f64)> {
    field(input, "saved")
        .as_array()
        .map(|_| pair(&f64s(field(input, "saved"))))
}

pub fn edges(value: Option<geometry::Edges>) -> Json {
    match value {
        None => Json::Null,
        Some((l, t, r, b)) => arr(vec![
            Json::Bool(l),
            Json::Bool(t),
            Json::Bool(r),
            Json::Bool(b),
        ]),
    }
}

pub fn edges_arg(value: &Json) -> geometry::Edges {
    let v = value.as_array().expect("edges");
    (
        v[0].as_bool().expect("l"),
        v[1].as_bool().expect("t"),
        v[2].as_bool().expect("r"),
        v[3].as_bool().expect("b"),
    )
}

pub fn same(got: &Json, expected: &Json, tolerance: f64) -> bool {
    match (got, expected) {
        (Json::Number(a), Json::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) if x.fract() == 0.0 && y.fract() == 0.0 => x == y,
            (Some(x), Some(y)) => {
                x.is_finite()
                    && y.is_finite()
                    && (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0)
            }
            _ => false,
        },
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y, tolerance))
        }
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.get(key)
                        .is_some_and(|other| same(value, other, tolerance))
                })
        }
        _ => got == expected,
    }
}

pub fn mark(
    got: &Result<Json, String>,
    expected: &Json,
    error: Option<&String>,
    tolerance: f64,
) -> bool {
    match (got, error) {
        (Ok(value), None) => same(value, expected, tolerance),
        (Err(message), Some(expected)) => message == expected,
        _ => false,
    }
}
