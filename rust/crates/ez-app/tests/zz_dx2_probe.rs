//! DX-2 probe (temporary, research only): what the deterministic fast path
//! does with short acknowledgements. Not a product test.
use ez_app::fast::{fast_command, FastHost};

#[derive(Default)]
struct Host;

impl FastHost for Host {}

#[test]
fn dx2_probe_short_acknowledgements() {
    for text in [
        "ok",
        "okey",
        "oki",
        "oc",
        "va",
        "listo",
        "bueno",
        "dale",
        "perfecto",
        "gracias",
        "ok, gracias",
        "ok, pero cambia X",
    ] {
        let outcome = fast_command(text, &Host);
        println!(
            "PROBE text={text:?} handled={} speech={:?}",
            outcome.handled,
            outcome.speech.as_ref().map(|(k, v)| (*k, v.as_str()))
        );
    }
}
