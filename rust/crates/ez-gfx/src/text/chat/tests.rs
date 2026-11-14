//! Tests for the chat measurement and wrapping port.

use super::*;

fn span(text: &str, bold: bool, italic: bool, code: bool, link: bool) -> SpanInput<'_> {
    SpanInput {
        text,
        bold,
        italic,
        code,
        link,
    }
}

#[test]
fn styles_match_render_base() {
    assert_eq!(STYLES.len(), 11);
    assert_eq!(style("body").expect("body").size, 14.0);
    assert_eq!(style("code").expect("code").family, "GeistMono");
    assert_eq!(style("micro").expect("micro").size, 10.5);
    assert!(style("nope").is_none());
    assert_eq!(LINE_K, 1.5);
    assert_eq!(PARA_GAP, 8.0);
}

#[test]
fn lengths_and_height_match_pillow() {
    assert_eq!(length("Hola", "body", 1.0), 30.0);
    assert_eq!(length("Hola", "code", 1.0), 32.0);
    assert_eq!(length("Hola", "h1", 1.0), 42.0);
    assert_eq!(length("Hola", "micro", 1.0), 28.0);
    assert_eq!(text_height(3, "body"), 63.0);
}

#[test]
fn pieces_split_words_spaces_and_code() {
    let spans = [
        span("Hola mundo", false, false, false, false),
        span(" code ", false, false, true, false),
        span("!", false, false, false, true),
    ];
    let pieces = pieces(&spans, "body");
    let view: Vec<(&str, &str, bool)> = pieces
        .iter()
        .map(|piece| (piece.text.as_str(), piece.key, piece.space))
        .collect();
    assert_eq!(
        view,
        vec![
            ("Hola", "body", false),
            (" ", "body", true),
            ("mundo", "body", false),
            (" code ", "code", false),
            ("!", "body", false),
        ]
    );
    assert!(pieces[4].link);
}

#[test]
fn bold_base_promotes_every_piece() {
    let spans = [span("Uno dos", false, true, false, false)];
    let pieces = pieces(&spans, "bold");
    assert!(pieces
        .iter()
        .filter(|piece| !piece.space)
        .all(|piece| piece.key == "bold" && piece.italic));
}

#[test]
fn wrap_matches_render_base() {
    let spans = [
        span("Hola mundo", false, false, false, false),
        span(" code ", false, false, true, false),
        span("!", false, false, false, true),
    ];
    let lines = wrap(&pieces(&spans, "body"), 60.0, 1.0);
    let view = view(&lines);
    assert_eq!(
        view,
        vec![
            vec![(0.0, "Hola", "body", 30.0)],
            vec![(0.0, "mundo", "body", 44.0)],
            vec![(0.0, " code ", "code", 54.0), (54.0, "!", "body", 3.0)],
        ]
    );
}

#[test]
fn long_words_split_into_fitting_chunks() {
    let spans = [span(
        "Supercalifragilisticoespialidoso",
        false,
        false,
        false,
        false,
    )];
    let lines = wrap(&pieces(&spans, "body"), 40.0, 1.0);
    assert_eq!(
        view(&lines),
        vec![
            vec![(0.0, "Super", "body", 38.0)],
            vec![(0.0, "califr", "body", 34.0)],
            vec![(0.0, "agilist", "body", 38.0)],
            vec![(0.0, "icoes", "body", 34.0)],
            vec![(0.0, "pialid", "body", 34.0)],
            vec![(0.0, "oso", "body", 23.0)],
        ]
    );
}

#[test]
fn trailing_spaces_drop_before_a_new_line() {
    let spans = [
        span("a", false, false, false, false),
        span(" ", false, false, false, false),
        span("b", false, false, false, false),
        span(" ", false, false, false, false),
        span("c", false, false, false, true),
        span(" ", false, false, false, false),
        span("d", false, false, true, false),
    ];
    let lines = wrap(&pieces(&spans, "body"), 50.0, 1.0);
    assert_eq!(
        view(&lines),
        vec![vec![
            (0.0, "a", "body", 8.0),
            (8.0, " ", "body", 4.0),
            (12.0, "b", "body", 8.0),
            (20.0, " ", "body", 4.0),
            (24.0, "c", "body", 8.0),
            (32.0, " ", "body", 4.0),
            (36.0, "d", "code", 14.0),
        ]]
    );
}

fn view(lines: &[Vec<Run>]) -> Vec<Vec<(f64, &str, &str, f64)>> {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|run| (run.x, run.text.as_str(), run.piece.key, run.width))
                .collect()
        })
        .collect()
}
