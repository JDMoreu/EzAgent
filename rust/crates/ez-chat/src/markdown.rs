//! Port of `src/ezagent/voice/chat/markdown.py` (F3-14).
//!
//! Subconjunto de markdown para las respuestas del asistente: bloques que la
//! vista del chat sabe dibujar. Tolera el texto a medias del streaming.

mod blocks;
mod inline;

pub use inline::parse_inline;

use ez_core::pyvalue::py_strip;

use blocks::{
    is_table_start, list_item, match_fence, match_quote, read_code, read_list, read_quote,
    read_table, single_line,
};

/// Un trozo de texto con su estilo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: usize,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// 0 = primer nivel.
    pub level: usize,
    /// «•» o «1.».
    pub marker: String,
    pub spans: Vec<Span>,
    /// Lista de tareas: `Some(true/false)`; `None` si no lo es.
    pub checked: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    pub lang: String,
    pub text: String,
}

/// Alineación de una columna de tabla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Vec<Vec<Span>>,
    pub rows: Vec<Vec<Vec<Span>>>,
    pub aligns: Vec<Align>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub alt: String,
    pub src: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub label: String,
    pub target: String,
}

/// Bloque dibujable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(Heading),
    Paragraph(Paragraph),
    ListItem(ListItem),
    Quote(Quote),
    Code(Code),
    Table(Table),
    Rule(Rule),
    Image(Image),
    Document(Document),
}

impl Block {
    /// Nombre que usan los fixtures (`heading`, `paragraph`…).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Heading(_) => "heading",
            Self::Paragraph(_) => "paragraph",
            Self::ListItem(_) => "list_item",
            Self::Quote(_) => "quote",
            Self::Code(_) => "code",
            Self::Table(_) => "table",
            Self::Rule(_) => "rule",
            Self::Image(_) => "image",
            Self::Document(_) => "document",
        }
    }
}

/// El destino sin los «<>» opcionales ni espacios sobrantes.
pub fn clean_target(target: &str) -> String {
    let mut text = py_strip(target);
    if text.starts_with('<') && text.ends_with('>') && text.len() >= 2 {
        text = py_strip(&text[1..text.len() - 1]);
    }
    text.to_string()
}

/// ¿Parece una ruta de archivo local (y no una dirección web)?
pub fn is_local_target(target: &str) -> bool {
    blocks::is_local_target(&clean_target(target))
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

/// El texto sin formato (para copiar un mensaje al portapapeles).
pub fn plain_text(blocks: &[Block]) -> String {
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            Block::Heading(item) => out.push(join_text(&item.spans)),
            Block::Paragraph(item) => out.push(join_text(&item.spans)),
            Block::Quote(item) => out.push(join_text(&item.spans)),
            Block::ListItem(item) => {
                let mark = match item.checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                };
                out.push(format!(
                    "{}{} {}{}",
                    "  ".repeat(item.level),
                    item.marker,
                    mark,
                    join_text(&item.spans)
                ));
            }
            Block::Code(item) => out.push(item.text.clone()),
            Block::Table(item) => {
                for row in std::iter::once(&item.header).chain(item.rows.iter()) {
                    let cells: Vec<String> = row.iter().map(|cell| join_text(cell)).collect();
                    out.push(cells.join(" | "));
                }
            }
            Block::Image(item) => {
                let name = if item.alt.is_empty() {
                    &item.src
                } else {
                    &item.alt
                };
                out.push(format!("[imagen: {name}]"));
            }
            Block::Document(item) => out.push(format!("{} ({})", item.label, item.target)),
            Block::Rule(_) => {}
        }
    }
    out.join("\n")
}

fn join_text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

#[cfg(test)]
mod tests;
