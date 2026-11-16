//! Tests for the rich chat blocks.

use std::path::{Path, PathBuf};

use ez_gfx::Rgba;

use super::*;
use crate::paint::chat::test_support::{palette, EmptyHost};
use crate::paint::chat::{
    hit_kind, Block, ChatHost, Ctx, Document, FileInfo, HitData, ImageBlock, Paragraph,
};
use crate::paint::chat::{MediaKind, Span, Table, TableAlign};

struct Host {
    info: Option<FileInfo>,
    image: Option<ez_gfx::Image>,
    preview: Option<(String, bool)>,
    blocks: Vec<Block>,
}

impl ChatHost for Host {
    fn parse(&self, _: &str) -> Vec<Block> {
        self.blocks.clone()
    }

    fn resolve_target(&self, target: &str, _: &[PathBuf]) -> Option<PathBuf> {
        Some(PathBuf::from(target))
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        self.info.clone()
    }

    fn load_image(&self, _: &FileInfo) -> Option<ez_gfx::Image> {
        self.image.clone()
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        self.preview.clone()
    }
}

fn host() -> Host {
    Host {
        info: None,
        image: None,
        preview: None,
        blocks: Vec::new(),
    }
}

fn file_info() -> FileInfo {
    FileInfo {
        path: PathBuf::from("C:/Users/Ana/Informe.pdf"),
        name: "Informe.pdf".to_string(),
        ext: ".pdf".to_string(),
        size: 1_468_006,
        mtime: 1_735_689_600.0,
        kind: MediaKind::Office,
        allowed: true,
        openable: true,
    }
}

fn document() -> Document {
    Document {
        label: String::new(),
        target: "C:/Users/Ana/Informe.pdf".to_string(),
    }
}

fn table() -> Table {
    Table {
        header: vec![vec![Span::plain("A")], vec![Span::plain("B")]],
        rows: vec![vec![vec![Span::plain("1")], vec![Span::plain("2")]]],
        aligns: vec![TableAlign::Left; 2],
    }
}

#[test]
fn human_size_matches_the_python_texts() {
    assert_eq!(human_size(12), "12 B");
    assert_eq!(human_size(820), "820 B");
    assert_eq!(human_size(1024), "1,0 KB");
    assert_eq!(human_size(1024 * 1024 * 3 / 2), "1,5 MB");
    assert_eq!(human_size(2 * 1024 * 1024 * 1024), "2,0 GB");
}

#[test]
fn local_targets_and_web_hosts_match_the_python_patterns() {
    for target in [
        "C:\\Users\\Ana\\x.pdf",
        "C:/Users/Ana/x.pdf",
        "\\\\servidor\\recurso",
        "file:///C:/x",
        "~/x",
        "/tmp/x",
        "./x",
        "../x",
    ] {
        assert!(is_local_target(target), "{target}");
    }
    assert!(!is_local_target("https://example.com/x.png"));
    assert!(!is_local_target("foto.png"));
    assert_eq!(
        url_netloc("https://example.com:8080/x").as_deref(),
        Some("example.com:8080")
    );
    assert_eq!(
        url_netloc("https://example.com").as_deref(),
        Some("example.com")
    );
    assert_eq!(url_netloc("file:///C:/x"), None);
    assert_eq!(url_netloc("foto.png"), None);
}

#[test]
fn column_widths_share_the_available_width() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let widths = tables::column_widths(&ctx, &table());
    assert_eq!(widths.len(), 2);
    let total: f64 = widths.iter().sum();
    assert!(total <= 300.5, "{total}");
    assert!(widths.iter().all(|width| *width >= CELL_MIN));
}

#[test]
fn a_table_header_is_never_split_by_a_rounding_error() {
    let pal = palette();
    let host = EmptyHost;
    let table = Table {
        header: vec![
            vec![Span::plain("Región")],
            vec![Span::plain("Q2")],
            vec![Span::plain("Q3")],
            vec![Span::plain("Cambio")],
        ],
        rows: vec![vec![
            vec![Span::plain("Norte")],
            vec![Span::plain("1")],
            vec![Span::plain("2")],
            vec![Span::plain("+18 %")],
        ]],
        aligns: vec![
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Center,
        ],
    };
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5] {
        let ctx = Ctx::new(scale, &pal, 408.0, &host);
        let widths = tables::column_widths(&ctx, &table);
        for (index, cell) in table.header.iter().enumerate() {
            let lines = crate::paint::chat::base::wrap_spans(
                cell,
                "cellbold",
                widths[index] - 2.0 * CELL_PAD_X,
                scale,
            );
            assert_eq!(lines.len(), 1, "escala {scale}, columna {index}");
        }
    }
}

#[test]
fn table_height_sums_its_rows() {
    let pal = palette();
    let host = EmptyHost;
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    assert_eq!(layout_table(&table(), &ctx).0, 67.0);
}

#[test]
fn document_card_offers_reveal_and_open() {
    let pal = palette();
    let mut host = host();
    host.info = Some(file_info());
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let (height, paint) = layout_document(&document(), &ctx);
    assert_eq!(height, 64.0);
    let mut image = ez_gfx::Image::new(300, 64, Rgba::TRANSPARENT);
    let mut ctx = Ctx::new(1.0, &pal, 300.0, &host);
    paint(&mut image, &mut ctx, 0.0, 0.0);
    let kinds: Vec<&str> = ctx.hits.iter().map(|hit| hit.kind).collect();
    assert_eq!(kinds, vec![hit_kind::REVEAL, hit_kind::OPEN]);
    assert_eq!(ctx.hits[0].data, Some(HitData::File(file_info())));
}

#[test]
fn document_preview_adds_the_card_height() {
    let pal = palette();
    let mut host = host();
    host.info = Some(file_info());
    host.preview = Some(("Hola".to_string(), false));
    host.blocks = vec![Block::Paragraph(Paragraph {
        spans: vec![Span::plain("Hola")],
    })];
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let (height, paint) = layout_document(&document(), &ctx);
    assert_eq!(height, 64.0 + (21.0 + 20.0) + 12.0);
    let mut image = ez_gfx::Image::new(300, height as u32, Rgba::TRANSPARENT);
    let mut ctx = Ctx::new(1.0, &pal, 300.0, &host);
    paint(&mut image, &mut ctx, 0.0, 0.0);
    assert!(ctx.hits.iter().any(|hit| hit.kind == hit_kind::OPEN));
}

#[test]
fn document_without_permission_shows_no_buttons() {
    let pal = palette();
    let mut info = file_info();
    info.allowed = false;
    let mut host = host();
    host.info = Some(info);
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let (height, paint) = layout_document(&document(), &ctx);
    assert_eq!(height, 64.0);
    let mut image = ez_gfx::Image::new(300, 64, Rgba::TRANSPARENT);
    let mut ctx = Ctx::new(1.0, &pal, 300.0, &host);
    paint(&mut image, &mut ctx, 0.0, 0.0);
    assert!(ctx.hits.is_empty());
}

#[test]
fn missing_document_is_a_54_px_card() {
    let pal = palette();
    let host = host();
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let missing = Document {
        label: String::new(),
        target: "C:/no/existe.pdf".to_string(),
    };
    assert_eq!(layout_document(&missing, &ctx).0, 54.0);
}

#[test]
fn local_image_reports_its_hit() {
    let pal = palette();
    let mut host = host();
    host.info = Some(FileInfo {
        kind: MediaKind::Image,
        ..file_info()
    });
    host.image = Some(ez_gfx::Image::new(20, 10, Rgba::opaque(10, 20, 30)));
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let block = ImageBlock {
        alt: String::new(),
        src: "C:/foto.png".to_string(),
    };
    let (height, paint) = layout_image(&block, &ctx);
    assert_eq!(height, 10.0);
    let mut image = ez_gfx::Image::new(300, 10, Rgba::TRANSPARENT);
    let mut ctx = Ctx::new(1.0, &pal, 300.0, &host);
    paint(&mut image, &mut ctx, 0.0, 0.0);
    assert_eq!(ctx.hits.len(), 1);
    assert_eq!(ctx.hits[0].kind, hit_kind::IMAGE);
    assert_eq!(
        ctx.hits[0].rect,
        crate::tokens::Rect::new(0.0, 0.0, 20.0, 10.0)
    );
}

#[test]
fn web_images_are_placeholders() {
    let pal = palette();
    let host = host();
    let ctx = Ctx::new(1.0, &pal, 300.0, &host);
    let block = ImageBlock {
        alt: String::new(),
        src: "https://example.com/foto.png".to_string(),
    };
    assert_eq!(layout_image(&block, &ctx).0, 54.0);
}
