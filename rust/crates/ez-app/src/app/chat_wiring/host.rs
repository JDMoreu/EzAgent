//! Markdown, media and file lookups for the chat view (F6-08).
//!
//! Bridges `ez_chat::{markdown, media}` to the `paint::chat` seams without
//! making `ez-hud` depend on `ez-chat`.

use std::path::{Path, PathBuf};

use ez_chat::markdown;
use ez_chat::media;
use ez_gfx::Image;
use ez_hud::model::chat::view::ViewHost;
use ez_hud::paint::chat::{
    Block, ChatHost, Code, Document, FileInfo, Heading, ImageBlock, ListItem, MediaKind, Paragraph,
    Quote, Span, Table, TableAlign,
};

/// `ChatView.host`: parser and file access (`chat_view.py`).
pub(super) struct HudHost;

/// `markdown.Span` → `paint::chat::Span`.
fn span(span: markdown::Span) -> Span {
    Span {
        text: span.text,
        bold: span.bold,
        italic: span.italic,
        code: span.code,
        link: span.link,
    }
}

fn spans(spans: Vec<markdown::Span>) -> Vec<Span> {
    spans.into_iter().map(span).collect()
}

fn align(align: markdown::Align) -> TableAlign {
    match align {
        markdown::Align::Left => TableAlign::Left,
        markdown::Align::Center => TableAlign::Center,
        markdown::Align::Right => TableAlign::Right,
    }
}

fn block(block: markdown::Block) -> Block {
    match block {
        markdown::Block::Heading(heading) => Block::Heading(Heading {
            level: i32::try_from(heading.level).unwrap_or(i32::MAX),
            spans: spans(heading.spans),
        }),
        markdown::Block::Paragraph(paragraph) => Block::Paragraph(Paragraph {
            spans: spans(paragraph.spans),
        }),
        markdown::Block::ListItem(item) => Block::ListItem(ListItem {
            level: item.level,
            marker: item.marker,
            spans: spans(item.spans),
            checked: item.checked,
        }),
        markdown::Block::Quote(quote) => Block::Quote(Quote {
            spans: spans(quote.spans),
        }),
        markdown::Block::Code(code) => Block::Code(Code {
            lang: code.lang,
            text: code.text,
        }),
        markdown::Block::Table(table) => Block::Table(Table {
            header: table
                .header
                .into_iter()
                .map(spans)
                .collect::<Vec<Vec<Span>>>(),
            rows: table
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(spans).collect())
                .collect(),
            aligns: table.aligns.into_iter().map(align).collect(),
        }),
        markdown::Block::Rule(_) => Block::Rule,
        markdown::Block::Image(image) => Block::Image(ImageBlock {
            alt: image.alt,
            src: image.src,
        }),
        markdown::Block::Document(document) => Block::Document(Document {
            label: document.label,
            target: document.target,
        }),
    }
}

/// `media.Kind` → `paint::chat::MediaKind`.
fn media_kind(kind: media::Kind) -> MediaKind {
    match kind {
        media::Kind::Image => MediaKind::Image,
        media::Kind::Text => MediaKind::Text,
        media::Kind::Office => MediaKind::Office,
        media::Kind::Other => MediaKind::Other,
    }
}

fn chat_kind(kind: MediaKind) -> media::Kind {
    match kind {
        MediaKind::Image => media::Kind::Image,
        MediaKind::Text => media::Kind::Text,
        MediaKind::Office => media::Kind::Office,
        MediaKind::Other => media::Kind::Other,
    }
}

/// `media.FileInfo` → the value painted by the view.
fn view_file_info(info: media::FileInfo) -> FileInfo {
    FileInfo {
        path: info.path,
        name: info.name,
        ext: info.ext,
        size: u64::try_from(info.size.max(0)).unwrap_or(0),
        mtime: info.mtime,
        kind: media_kind(info.kind),
        allowed: info.allowed,
        openable: info.openable,
    }
}

/// The view's file card → `media.FileInfo` for opening and revealing.
pub(super) fn chat_file_info(info: &FileInfo) -> media::FileInfo {
    media::FileInfo {
        path: info.path.clone(),
        name: info.name.clone(),
        ext: info.ext.clone(),
        size: i64::try_from(info.size).unwrap_or(i64::MAX),
        mtime: info.mtime,
        kind: chat_kind(info.kind),
        allowed: info.allowed,
        openable: info.openable,
    }
}

/// `media.open_file` for the view's card.
pub(super) fn open_file(info: &FileInfo) -> bool {
    media::open_file(Some(&chat_file_info(info)))
}

/// `media.reveal_file` for the view's card.
pub(super) fn reveal_file(info: &FileInfo) -> bool {
    media::reveal_file(Some(&chat_file_info(info)))
}

impl ChatHost for HudHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        markdown::parse(text).into_iter().map(block).collect()
    }

    fn resolve_target(&self, target: &str, base_dirs: &[PathBuf]) -> Option<PathBuf> {
        media::resolve_target(target, base_dirs)
    }

    fn file_info(&self, path: Option<&Path>, roots: Option<&[PathBuf]>) -> Option<FileInfo> {
        media::file_info(path, roots).map(view_file_info)
    }

    fn load_image(&self, info: &FileInfo) -> Option<Image> {
        let loaded = media::load_image(Some(&chat_file_info(info)))?;
        // DECISION: `ez-app` solo declara `png`; GIF/JPEG/BMP/WebP se pintan
        // como «No puedo mostrar esta imagen» hasta que el HUD tenga `image`.
        if loaded.format != "png" {
            return None;
        }
        let bytes = std::fs::read(&loaded.path).ok()?;
        decode_png(&bytes)
    }

    fn text_preview(&self, info: &FileInfo) -> Option<(String, bool)> {
        media::text_preview(Some(&chat_file_info(info)))
    }
}

impl ViewHost for HudHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        spans(markdown::parse_inline(text))
    }
}

/// PNG RGBA, like `PIL.Image.convert("RGBA")` (first frame).
fn decode_png(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0_u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let data = &buffer[..info.buffer_size()];
    let pixels = (info.width as usize) * (info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => {
            let mut out = vec![255_u8; pixels * 4];
            for (source, target) in data
                .as_chunks::<3>()
                .0
                .iter()
                .zip(out.as_chunks_mut::<4>().0)
            {
                target[..3].copy_from_slice(source);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = vec![0_u8; pixels * 4];
            for (source, target) in data
                .as_chunks::<2>()
                .0
                .iter()
                .zip(out.as_chunks_mut::<4>().0)
            {
                target[0] = source[0];
                target[1] = source[0];
                target[2] = source[0];
                target[3] = source[1];
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = vec![255_u8; pixels * 4];
            for (source, target) in data
                .as_chunks::<1>()
                .0
                .iter()
                .zip(out.as_chunks_mut::<4>().0)
            {
                target[0] = source[0];
                target[1] = source[0];
                target[2] = source[0];
            }
            out
        }
        png::ColorType::Indexed => return None,
    };
    if rgba.len() != pixels * 4 {
        return None;
    }
    Image::from_rgba(info.width, info.height, rgba).ok()
}
