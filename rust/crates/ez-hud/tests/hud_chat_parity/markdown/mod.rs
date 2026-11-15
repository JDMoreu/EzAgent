//! Markdown parser for the parity tests of the chat.
//!
//! `ez-hud` takes the parser through the [`ChatHost`](ez_hud::paint::chat::ChatHost)
//! seam and does not depend on `ez-chat`, so the tests bring their own copy of
//! the ported parser (`ez-chat::markdown`) over the `ez-hud` block types.

mod blocks;
mod inline;

pub use ez_hud::paint::chat::{
    Block, Code, Document, Heading, ImageBlock as Image, ListItem, Paragraph, Quote, Span, Table,
    TableAlign as Align,
};
pub use inline::parse_inline;

use blocks::{
    is_table_start, list_item, match_fence, match_quote, read_code, read_list, read_quote,
    read_table, single_line,
};
use ez_core::pyvalue::py_strip;

/// El destino sin los «<>» opcionales ni espacios sobrantes.
pub fn clean_target(target: &str) -> String {
    let mut text = py_strip(target);
    if text.starts_with('<') && text.ends_with('>') && text.len() >= 2 {
        text = py_strip(&text[1..text.len() - 1]);
    }
    text.to_string()
}

/// Convierte markdown en bloques. Nunca falla: lo que no entiende es un párrafo.
pub fn parse(text: &str) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        if py_strip(line).is_empty() {
            end_paragraph(&mut paragraph, &mut blocks);
            index += 1;
            continue;
        }
        if let Some(fence) = match_fence(line) {
            end_paragraph(&mut paragraph, &mut blocks);
            let (next, block) = read_code(&lines, index, &fence);
            blocks.push(block);
            index = next;
            continue;
        }
        if let Some(block) = single_line(line) {
            end_paragraph(&mut paragraph, &mut blocks);
            blocks.push(block);
            index += 1;
            continue;
        }
        if is_table_start(&lines, index) {
            end_paragraph(&mut paragraph, &mut blocks);
            let (next, block) = read_table(&lines, index);
            blocks.push(block);
            index = next;
            continue;
        }
        if match_quote(line).is_some() {
            end_paragraph(&mut paragraph, &mut blocks);
            let (next, block) = read_quote(&lines, index);
            blocks.push(block);
            index = next;
            continue;
        }
        if list_item(line).is_some() {
            end_paragraph(&mut paragraph, &mut blocks);
            let (next, items) = read_list(&lines, index);
            blocks.extend(items);
            index = next;
            continue;
        }
        paragraph.push(line);
        index += 1;
    }
    end_paragraph(&mut paragraph, &mut blocks);
    blocks
}

fn end_paragraph(paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if paragraph.is_empty() {
        return;
    }
    let text = paragraph
        .iter()
        .map(|part| py_strip(part))
        .collect::<Vec<_>>()
        .join(" ");
    paragraph.clear();
    blocks.push(Block::Paragraph(Paragraph {
        spans: parse_inline(&text),
    }));
}
