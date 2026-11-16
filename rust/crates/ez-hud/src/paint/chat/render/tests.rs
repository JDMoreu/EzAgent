//! Tests for the chat message renderer.

use std::path::{Path, PathBuf};

use super::*;
use crate::paint::chat::test_support::{palette, EmptyHost};
use crate::paint::chat::{FileInfo, ListItem, Paragraph, Quote, Span, TableAlign};

struct Host {
    blocks: Vec<Block>,
}

impl ChatHost for Host {
    fn parse(&self, _: &str) -> Vec<Block> {
        self.blocks.clone()
    }

    fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(Paragraph {
        spans: vec![Span::plain(text)],
    })
}

fn list_item(text: &str) -> Block {
    Block::ListItem(ListItem {
        level: 1,
        marker: "•".to_string(),
        spans: vec![Span::plain(text)],
        checked: None,
    })
}

#[test]
fn constants_match_the_python_module() {
    assert_eq!(CODE_LINE, 17.5);
    assert_eq!(CODE_HEAD, 24.0);
    assert_eq!(CODE_PAD, 12.0);
    assert_eq!(LIST_STEP, 18.0);
    assert_eq!(LIST_MARKER, 18.0);
}

#[test]
fn layout_heights_match_the_python_blocks() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    assert_eq!(layout_block(&paragraph("Hola"), &ctx).0, 21.0);
    let heading = Block::Heading(Heading {
        level: 1,
        spans: vec![Span::plain("Hola")],
    });
    assert_eq!(layout_block(&heading, &ctx).0, 28.5);
    let h3 = Block::Heading(Heading {
        level: 3,
        spans: vec![Span::plain("Hola")],
    });
    assert_eq!(layout_block(&h3, &ctx).0, 21.75);
    assert_eq!(layout_block(&list_item("Hola"), &ctx).0, 21.0);
    let quote = Block::Quote(Quote {
        spans: vec![Span::plain("Hola")],
    });
    assert_eq!(layout_block(&quote, &ctx).0, 21.0);
    assert_eq!(layout_block(&Block::Rule, &ctx).0, 12.0);
    let code = Block::Code(Code {
        lang: String::new(),
        text: "a\nb".to_string(),
    });
    assert_eq!(layout_block(&code, &ctx).0, 71.0);
}

#[test]
fn gaps_between_blocks_match_render_py() {
    let para = paragraph("Hola");
    let item = list_item("Hola");
    let heading = Block::Heading(Heading {
        level: 2,
        spans: vec![Span::plain("Hola")],
    });
    assert_eq!(gap(None, &para), 0.0);
    assert_eq!(gap(Some(&item), &item), 3.0);
    assert_eq!(gap(Some(&para), &heading), 14.0);
    assert_eq!(gap(Some(&heading), &para), 4.0);
    assert_eq!(gap(Some(&para), &para), 8.0);
}

#[test]
fn code_lines_expand_tabs_and_split_long_rows() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    assert_eq!(code_lines(&ctx, ""), vec![String::new()]);
    assert_eq!(code_lines(&ctx, "a\tb"), vec!["a   b".to_string()]);
    let long = "x".repeat(200);
    let rows = code_lines(&ctx, &long);
    assert!(rows.len() > 1);
    assert_eq!(rows.concat(), long);
    let step = (300.0 - 2.0 * CODE_PAD) / ctx.length("M", "code").max(1.0);
    let chars = (step.trunc() as i64).max(8) as usize;
    assert!(rows.iter().all(|row| row.chars().count() <= chars));
}

#[test]
fn render_message_reports_plain_text_and_copy_hits() {
    let pal = palette();
    let host = Host {
        blocks: vec![Block::Code(Code {
            lang: "rust".to_string(),
            text: "fn main() {}".to_string(),
        })],
    };
    let rendered = render_message(&host, "x", &pal, 300.0, &RenderOptions::default());
    assert_eq!(rendered.plain, "fn main() {}");
    assert_eq!(rendered.height, CODE_HEAD + CODE_LINE + CODE_PAD);
    assert_eq!(rendered.hits.len(), 1);
    assert_eq!(
        rendered.hits[0].kind,
        crate::paint::chat::hit_kind::COPY_CODE
    );
    assert_eq!(
        rendered.hits[0].data,
        Some(HitData::Text("fn main() {}".to_string()))
    );
    assert_eq!(rendered.image.width(), 300);
    assert_eq!(rendered.image.height(), 54);
    assert!(rendered.image.pixels().any(|pixel| pixel.a > 0));
}

#[test]
fn a_single_word_longer_than_the_column_does_not_hang() {
    let pal = palette();
    let host = Host {
        blocks: vec![paragraph(&"a".repeat(400))],
    };
    let rendered = render_message(&host, "", &pal, 160.0, &RenderOptions::default());
    assert!(rendered.height > 10.0, "{}", rendered.height);
    assert!(rendered.height.is_finite());
}

#[test]
fn logical_height_does_not_depend_on_the_screen_scale() {
    let pal = palette();
    let host = Host {
        blocks: (0..12)
            .map(|_| paragraph("Un párrafo bastante largo "))
            .collect(),
    };
    let heights: Vec<f64> = [1.0, 1.5, 2.0]
        .iter()
        .map(|scale| {
            let options = RenderOptions {
                scale: *scale,
                ..RenderOptions::default()
            };
            render_message(&host, "", &pal, 420.0, &options).height
        })
        .collect();
    let maximum = heights.iter().cloned().fold(f64::MIN, f64::max);
    let minimum = heights.iter().cloned().fold(f64::MAX, f64::min);
    assert!(maximum - minimum <= 24.0, "{heights:?}");
}

#[test]
fn render_message_paints_paragraphs() {
    let pal = palette();
    let host = Host {
        blocks: vec![paragraph("Hola mundo")],
    };
    let rendered = render_message(&host, "Hola mundo", &pal, 300.0, &RenderOptions::default());
    assert_eq!(rendered.plain, "Hola mundo");
    assert!(rendered.image.pixels().any(|pixel| pixel.a > 0));
}

#[test]
fn plain_text_formats_every_block() {
    let blocks = vec![
        Block::Heading(Heading {
            level: 1,
            spans: vec![Span::plain("Título")],
        }),
        paragraph("Cuerpo"),
        Block::ListItem(ListItem {
            level: 2,
            marker: "1.".to_string(),
            spans: vec![Span::plain("Hecho")],
            checked: Some(true),
        }),
        Block::Quote(Quote {
            spans: vec![Span::plain("Cita")],
        }),
        Block::Code(Code {
            lang: "rs".to_string(),
            text: "let x = 1;".to_string(),
        }),
        Block::Table(Table {
            header: vec![vec![Span::plain("A")], vec![Span::plain("B")]],
            rows: vec![vec![vec![Span::plain("1")], vec![Span::plain("2")]]],
            aligns: vec![TableAlign::Left, TableAlign::Right],
        }),
        Block::Image(ImageBlock {
            alt: String::new(),
            src: "foto.png".to_string(),
        }),
        Block::Document(crate::paint::chat::Document {
            label: "Informe".to_string(),
            target: "C:/x.pdf".to_string(),
        }),
        Block::Rule,
    ];
    assert_eq!(
        plain_text(&blocks),
        "Título\nCuerpo\n    1. [x] Hecho\nCita\nlet x = 1;\nA | B\n1 | 2\n[imagen: foto.png]\nInforme (C:/x.pdf)"
    );
}
