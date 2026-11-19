//! Height of the messages without painting them (DX-1c).
//!
//! The conversation needs the height of **every** message to know where each
//! one starts, but the tile (the painted image) is only needed for the
//! messages that are on screen. Before this module the view laid out and
//! painted the whole history in every frame: with 1 000 messages the first
//! frame cost about a minute.
//!
//! DECISION: the height is *measured* (the text is parsed and measured, but
//! nothing is painted) and the tile is only built for the viewport and the
//! margin around it. The two numbers must be the same one, so both go
//! through the same arithmetic as the painting path.

use crate::paint::chat::render::measure_blocks;
use crate::paint::chat::{wrap_spans, Block, Ctx};
use crate::tokens::ColorMap;

use super::{ChatMessage, ChatView, Role, COPY_ROW, COPY_SLOT, GFX_ICONS};

impl ChatView {
    /// Height of a message, without painting it.
    pub(crate) fn measure_entry(&self, msg: &ChatMessage) -> f64 {
        match msg.role {
            Role::User => self.measure_user(msg),
            Role::System => self.measure_note(msg),
            Role::Assistant => self.measure_assistant(msg),
        }
    }

    /// A note from the app («conversación compactada»): one faint line.
    fn measure_note(&self, msg: &ChatMessage) -> f64 {
        let ctx = self.ctx(self.content_w() * 0.92);
        let lines = wrap_spans(
            &self.host.parse_inline(&msg.text),
            "small",
            ctx.width,
            self.g.scale,
        );
        lines.len() as f64 * ctx.size("small") * 1.5 + 8.0
    }

    /// A message of the user: the bubble grows with the text.
    fn measure_user(&self, msg: &ChatMessage) -> f64 {
        let blocks = self.host.parse(&msg.text);
        let rich = super::entries::is_rich(&blocks);
        let limit = self.content_w() * if rich { 0.94 } else { 0.82 };
        let (inner, inline) = self.user_inner(&blocks, limit, rich);
        let rendered = self.measure_blocks_at(&blocks, inner, &self.g.pal);
        let body_h = rendered + if inline { 18.0 } else { 18.0 + COPY_ROW };
        body_h + 18.0
    }

    /// An answer of the assistant: a card with the text inside.
    fn measure_assistant(&self, msg: &ChatMessage) -> f64 {
        let pal = self.assistant_palette(msg.error);
        let has_copy = !msg.text.is_empty() && !msg.streaming && !msg.error;
        let text_width = self.content_w() - 24.0 - if has_copy { COPY_SLOT } else { 0.0 };
        let body = if msg.text.is_empty() {
            16.0
        } else {
            let blocks = self.host.parse(&msg.text);
            self.measure_blocks_at(&blocks, text_width, &pal)
        };
        20.0 + 12.0 + body + 12.0
    }

    /// Height of the blocks in a column of `width`, with the given palette.
    ///
    /// The context is the one the painting path uses (`render_message`): same
    /// scale, same width, same folders and the same icons.
    fn measure_blocks_at(&self, blocks: &[Block], width: f64, pal: &ColorMap) -> f64 {
        let mut ctx = Ctx::new(self.g.scale, pal, width, self.host.as_ref())
            .with_paths(self.roots.clone(), self.base_dirs.clone())
            .with_icons(&GFX_ICONS);
        measure_blocks(blocks, &mut ctx)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use ez_gfx::Image;

    use crate::paint::chat::{
        Block, ChatHost, Code, FileInfo, Heading, ListItem, Paragraph, Quote, Span, Table,
        TableAlign,
    };

    use super::super::{ChatMessage, Role};

    /// Host that reads a tiny markup, so the messages have blocks of every
    /// kind (the height of each one is a different arithmetic).
    struct RichHost;

    impl ChatHost for RichHost {
        fn parse(&self, text: &str) -> Vec<Block> {
            let mut out = Vec::new();
            for line in text.lines() {
                let line = line.trim_end();
                if let Some(rest) = line.strip_prefix("# ") {
                    out.push(Block::Heading(Heading {
                        level: 1,
                        spans: vec![Span::plain(rest)],
                    }));
                } else if let Some(rest) = line.strip_prefix("- ") {
                    out.push(Block::ListItem(ListItem {
                        level: 0,
                        marker: "•".to_string(),
                        spans: vec![Span::plain(rest)],
                        checked: None,
                    }));
                } else if let Some(rest) = line.strip_prefix("> ") {
                    out.push(Block::Quote(Quote {
                        spans: vec![Span::plain(rest)],
                    }));
                } else if line.starts_with("```") {
                    out.push(Block::Code(Code {
                        lang: "text".to_string(),
                        text: line.trim_start_matches('`').to_string(),
                    }));
                } else if line.starts_with('|') {
                    out.push(Block::Table(Table {
                        header: vec![vec![Span::plain("caso")], vec![Span::plain("antes")]],
                        rows: vec![vec![vec![Span::plain("10")], vec![Span::plain("3 ms")]]],
                        aligns: vec![TableAlign::Left, TableAlign::Right],
                    }));
                } else if !line.is_empty() {
                    out.push(Block::Paragraph(Paragraph {
                        spans: vec![Span::plain(line)],
                    }));
                }
            }
            out
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

    impl super::super::ViewHost for RichHost {
        fn parse_inline(&self, text: &str) -> Vec<Span> {
            vec![Span::plain(text)]
        }
    }

    /// The measured height must be the height of the painted message: the
    /// view places the messages with the measured height and paints the
    /// tile, and the two have to be the same number.
    #[test]
    fn the_measured_height_is_the_height_of_the_painted_message() {
        let text = "# Resumen\n\n\
                    El panel congela la interfaz.\n\n\
                    - ¿Repite el último error?\n\
                    - ¿Qué pruebas pasan?\n\n\
                    > una cita larga que se parte en varias líneas porque es larga\n\n\
                    ```text\n\
                    frame_ms = render(history)\n\
                    ```\n\n\
                    | Caso | Antes |\n\
                    |---|---|\n\
                    | 10 | 3 ms |";
        let messages = vec![
            ChatMessage::new(1, Role::User, text),
            ChatMessage::new(2, Role::Assistant, text),
            ChatMessage::new(3, Role::System, text),
            ChatMessage::new(4, Role::User, "hola"),
            ChatMessage::new(5, Role::Assistant, ""),
            ChatMessage::new(
                6,
                Role::Assistant,
                "una respuesta que ocupa más de una línea",
            ),
        ];
        let view = rich_view(messages.clone());
        for msg in &messages {
            let measured = view.measure_entry(msg);
            let painted = view.build_entry(msg).height;
            assert!(
                (measured - painted).abs() < 0.001,
                "the measured height ({measured}) is not the painted one ({painted})"
            );
        }
    }

    /// View with the host of the test and the messages given.
    fn rich_view(messages: Vec<ChatMessage>) -> super::super::ChatView {
        use super::super::test_support::{FakeInput, FakeModel};

        let mut view = super::super::ChatView::new(
            Box::new(FakeModel {
                messages,
                ..FakeModel::default()
            }),
            Box::new(RichHost),
            Box::new(FakeInput::default()),
        );
        view.configure(
            440.0,
            600.0,
            1.0,
            "dark",
            crate::tokens::palette("dark").expect("the dark palette exists"),
        );
        view
    }
}
