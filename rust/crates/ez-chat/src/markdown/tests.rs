use super::*;

fn text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn paragraph(block: &Block) -> &Paragraph {
    match block {
        Block::Paragraph(item) => item,
        other => panic!("se esperaba un párrafo, llegó {}", other.kind()),
    }
}

#[test]
fn plain_text_is_one_span() {
    assert_eq!(
        parse_inline("hola mundo"),
        vec![Span {
            text: "hola mundo".into(),
            ..Span::default()
        }]
    );
}

#[test]
fn bold_italic_and_code() {
    let spans = parse_inline("a **negrita** b *cursiva* c `código` d");
    let styles: Vec<(&str, bool, bool, bool)> = spans
        .iter()
        .filter(|span| span.bold || span.italic || span.code)
        .map(|span| (span.text.as_str(), span.bold, span.italic, span.code))
        .collect();
    assert_eq!(
        styles,
        vec![
            ("negrita", true, false, false),
            ("cursiva", false, true, false),
            ("código", false, false, true)
        ]
    );
    assert_eq!(text(&spans), "a negrita b cursiva c código d");
}

#[test]
fn nested_emphasis_and_underscores() {
    let spans = parse_inline("**fuerte y _suave_**");
    assert!(spans
        .iter()
        .any(|span| span.bold && span.italic && span.text == "suave"));
    assert_eq!(
        text(&parse_inline("usa snake_case_name aquí")),
        "usa snake_case_name aquí"
    );
    assert_eq!(text(&parse_inline("2 * 3 * 4")), "2 * 3 * 4");
}

#[test]
fn unclosed_emphasis_stays_literal() {
    assert_eq!(text(&parse_inline("esto **no cierra")), "esto **no cierra");
    assert_eq!(
        text(&parse_inline("un `código sin cerrar")),
        "un `código sin cerrar"
    );
    assert_eq!(
        text(&parse_inline("un [enlace](sin cerrar")),
        "un [enlace](sin cerrar"
    );
}

#[test]
fn links_keep_target_and_inner_style() {
    let spans = parse_inline("mira [la **guía**](https://ejemplo.com/a) ya");
    let linked: Vec<&Span> = spans.iter().filter(|span| span.link.is_some()).collect();
    assert_eq!(
        linked
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>(),
        vec!["la ", "guía"]
    );
    assert_eq!(linked[1].link.as_deref(), Some("https://ejemplo.com/a"));
    assert!(linked[1].bold);
}

#[test]
fn escapes_and_html() {
    assert_eq!(
        text(&parse_inline(r"\*no cursiva\* y \# no título")),
        "*no cursiva* y # no título"
    );
    let spans = parse_inline("<b>hola</b> <script>alert(1)</script>");
    assert_eq!(text(&spans), "<b>hola</b> <script>alert(1)</script>");
    assert!(!spans.iter().any(|span| span.bold));
}

#[test]
fn headings_and_paragraphs() {
    let blocks = parse("# Título\n\nUn párrafo\nque sigue.\n\n### Menor ###");
    assert!(matches!(&blocks[0], Block::Heading(item) if item.level == 1));
    assert_eq!(text(&paragraph(&blocks[1]).spans), "Un párrafo que sigue.");
    assert!(
        matches!(&blocks[2], Block::Heading(item) if item.level == 3 && text(&item.spans) == "Menor")
    );
}

#[test]
fn lists_ordered_nested_and_tasks() {
    let blocks =
        parse("- uno\n  - anidado\n- dos\n\n1. primero\n2) segundo\n\n- [x] hecho\n- [ ] falta");
    let items: Vec<&ListItem> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::ListItem(item) => Some(item),
            _ => None,
        })
        .collect();
    assert_eq!(
        items[..3]
            .iter()
            .map(|item| (item.level, item.marker.as_str(), text(&item.spans)))
            .collect::<Vec<_>>(),
        vec![
            (0, "•", "uno".to_string()),
            (1, "•", "anidado".into()),
            (0, "•", "dos".into())
        ]
    );
    assert_eq!(
        items[3..5]
            .iter()
            .map(|item| item.marker.as_str())
            .collect::<Vec<_>>(),
        vec!["1.", "2."]
    );
    assert_eq!(
        items[5..]
            .iter()
            .map(|item| (item.checked, text(&item.spans)))
            .collect::<Vec<_>>(),
        vec![
            (Some(true), "hecho".to_string()),
            (Some(false), "falta".into())
        ]
    );
}

#[test]
fn list_continuation_joins_the_item() {
    let blocks = parse("- una línea\n  que continúa");
    let Block::ListItem(item) = &blocks[0] else {
        panic!("se esperaba un elemento de lista");
    };
    assert_eq!(text(&item.spans), "una línea que continúa");
}

#[test]
fn code_block_language_and_unterminated_streaming() {
    let blocks = parse("```python\nprint('hola')\nx = 1\n```");
    assert_eq!(
        blocks,
        vec![Block::Code(Code {
            lang: "python".into(),
            text: "print('hola')\nx = 1".into()
        })]
    );
    let partial = parse("```sql\nSELECT 1;\nSELECT 2");
    assert_eq!(
        partial,
        vec![Block::Code(Code {
            lang: "sql".into(),
            text: "SELECT 1;\nSELECT 2".into()
        })]
    );
}

#[test]
fn quote_and_rule() {
    let blocks = parse("> una cita\n> larga\n\n---\n\ntexto");
    assert!(matches!(&blocks[0], Block::Quote(item) if text(&item.spans) == "una cita larga"));
    assert!(matches!(blocks[1], Block::Rule(_)));
    assert!(matches!(blocks[2], Block::Paragraph(_)));
}

#[test]
fn table_with_alignment_and_ragged_rows() {
    let blocks =
        parse("| Plan | Precio | Notas |\n|:--|--:|:-:|\n| Básico | 5 | **ok** |\n| Pro | 12 |");
    let Block::Table(table) = &blocks[0] else {
        panic!("se esperaba una tabla");
    };
    assert_eq!(
        table
            .header
            .iter()
            .map(|cell| text(cell))
            .collect::<Vec<_>>(),
        vec!["Plan", "Precio", "Notas"]
    );
    assert_eq!(table.aligns, vec![Align::Left, Align::Right, Align::Center]);
    assert_eq!(text(&table.rows[0][2]), "ok");
    assert!(table.rows[0][2][0].bold);
    assert_eq!(text(&table.rows[1][2]), "");
}

#[test]
fn table_cells_accept_escaped_pipes() {
    let blocks = parse("| a | b |\n|---|---|\n| x \\| y | z |");
    let Block::Table(table) = &blocks[0] else {
        panic!("se esperaba una tabla");
    };
    assert_eq!(text(&table.rows[0][0]), "x | y");
}

#[test]
fn images_and_documents() {
    let blocks = parse("Mira:\n\n![gráfico de ventas](C:\\datos\\ventas.png)\n\nlisto");
    assert_eq!(
        blocks[1],
        Block::Image(Image {
            alt: "gráfico de ventas".into(),
            src: "C:\\datos\\ventas.png".into()
        })
    );
    let path = r"C:\Users\Ana Pérez\Documentos\Informe Q3.docx";
    assert_eq!(
        parse(&format!("[Informe]({path})")),
        vec![Block::Document(Document {
            label: "Informe".into(),
            target: path.into()
        })]
    );
    assert_eq!(
        parse("- [A](C:\\a.pdf)\n- [B](C:\\b.pdf)"),
        vec![
            Block::Document(Document {
                label: "A".into(),
                target: "C:\\a.pdf".into()
            }),
            Block::Document(Document {
                label: "B".into(),
                target: "C:\\b.pdf".into()
            })
        ]
    );
}

#[test]
fn local_target_detection() {
    for target in [
        "C:\\x",
        "c:/x",
        "\\\\servidor\\x",
        "file:///C:/x",
        "~/x",
        "./x",
        "/x",
    ] {
        assert!(is_local_target(target), "{target}");
    }
    for target in ["https://x.com", "mailto:a@b.c", "ejemplo.com/x", "C:x"] {
        assert!(!is_local_target(target), "{target}");
    }
}

#[test]
fn plain_text_removes_markup() {
    let blocks = parse(
        "# T\n\nhola **mundo**\n\n- a\n- b\n\n| x | y |\n|---|---|\n| 1 | 2 |\n\n![f](C:\\f.png)",
    );
    let plain = plain_text(&blocks);
    assert!(plain.contains("T\nhola mundo\n• a\n• b\nx | y\n1 | 2"));
    assert!(plain.contains("[imagen: f]"));
    assert!(!plain.contains("**"));
}

#[test]
fn odd_input_never_fails() {
    assert!(parse("").is_empty());
    assert!(parse("\n\n   \n").is_empty());
    assert!(!parse("|||\n|---|").is_empty());
    assert!(matches!(parse(&"*".repeat(500))[0], Block::Rule(_)));
}
