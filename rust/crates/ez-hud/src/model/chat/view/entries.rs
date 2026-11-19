//! Laid-out messages of the chat, from `chat_view.py` (F5-14).
//!
//! Each message is drawn once into a premultiplied tile and cached by id; the
//! view only crops and composes it every frame.

use std::collections::BTreeMap;
use std::rc::Rc;

use ez_gfx::sdf::Layer;
use ez_gfx::{Image, Rgba};

use crate::paint::chat::render::{render_message, RenderOptions, LIST_MARKER, LIST_STEP};
use crate::paint::chat::{
    paint_lines, round_rect, wrap_spans, Block, Hit as RenderHit, HitData as RenderData,
};
use crate::tokens::{ColorMap, Rect};

use super::{
    canvas, ChatMessage, ChatView, Role, Zone, ZoneData, COPY_ROW, COPY_SLOT, COPY_W, GFX_ICONS,
    PAD, STREAM_REFRESH,
};

/// One message already drawn and placed.
pub(crate) struct Entry {
    /// Logical height.
    pub height: f64,
    /// Premultiplied RGBA tile, at the physical size.
    pub image: Layer,
    /// Logical x inside the window.
    pub x: f64,
    /// Clickable zones, relative to the tile.
    pub hits: Vec<Zone>,
    /// Zone of the copy icon, relative to the tile.
    pub caption_hit: Option<Rect>,
    /// The plain text, for copying it.
    pub plain: String,
}

/// Cache key of a rich message.
///
/// DECISION: the key does not carry the text (DX-1, paso 3). Copying the
/// whole history into the keys copied every message of the conversation once
/// per frame, just to compare them. The length and a 64-bit hash of the text
/// are enough to notice a change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CacheKey {
    text_len: usize,
    text_hash: u64,
    streaming: bool,
    error: bool,
    theme_hash: u64,
    width: i64,
}

/// A cached message plus when it was laid out and when it was last used.
pub(crate) struct Cached {
    pub(crate) key: CacheKey,
    pub(crate) entry: Rc<Entry>,
    /// When it was laid out (the throttle of the streaming answers).
    pub(crate) at: f64,
    /// When it was last used (the order of the eviction).
    pub(crate) used: f64,
    /// Bytes of the tile.
    pub(crate) bytes: usize,
}

/// Height of a message, cached apart from its tile.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CachedHeight {
    pub(crate) key: CacheKey,
    pub(crate) height: f64,
}

/// Budget of the layout cache, in bytes of tile.
pub(crate) const CACHE_BYTES: usize = 32 * 1024 * 1024;

/// How many heights are remembered (they are tiny: 40 bytes each).
const HEIGHT_LIMIT: usize = 20_000;

/// Laid-out messages of the chat, by id.
///
/// DECISION: the cache has a budget in **bytes** and evicts the tile that has
/// not been used for the longest time and is not on screen (DX-1, paso 3).
/// The old cache had a cap of 400 entries and dropped the smallest id, which
/// is the oldest message: with more messages than the cap, every frame laid
/// out the whole history again (measured: 8,9 ms with 401 messages and
/// 53 227 ms with 1 000).
#[derive(Default)]
pub(crate) struct LayoutCache {
    entries: BTreeMap<i64, Cached>,
    heights: BTreeMap<i64, CachedHeight>,
    bytes: usize,
    layouts: u64,
}

impl LayoutCache {
    /// Number of tiles held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// How many messages have been laid out since the last clear.
    pub(crate) fn layouts(&self) -> u64 {
        self.layouts
    }

    /// The cached message, if any.
    pub(crate) fn get_mut(&mut self, id: i64) -> Option<&mut Cached> {
        self.entries.get_mut(&id)
    }

    /// The height of a message, if it is known.
    pub(crate) fn height(&self, id: i64) -> Option<CachedHeight> {
        self.heights.get(&id).copied()
    }

    /// Remembers the tile of a message.
    pub(crate) fn insert(&mut self, id: i64, key: CacheKey, entry: Rc<Entry>, now: f64) {
        let bytes = tile_bytes(&entry);
        if let Some(previous) = self.entries.insert(
            id,
            Cached {
                key,
                entry,
                at: now,
                used: now,
                bytes,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        self.bytes += bytes;
        self.layouts += 1;
    }

    /// Remembers the height of a message.
    pub(crate) fn insert_height(&mut self, id: i64, key: CacheKey, height: f64) {
        if self.heights.len() >= HEIGHT_LIMIT {
            // The oldest id is the oldest message: its height is the cheapest
            // one to lose (it is the first a long conversation forgets).
            if let Some(first) = self.heights.keys().next().copied() {
                self.heights.remove(&first);
            }
        }
        self.heights.insert(id, CachedHeight { key, height });
    }

    /// Forgets everything (the theme or the size changed).
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.heights.clear();
        self.bytes = 0;
        self.layouts = 0;
    }

    /// Drops tiles until the cache fits in its budget.
    ///
    /// The victim is the tile that has not been used for the longest time and
    /// is not on screen: the messages on screen are the ones the next frame
    /// needs.
    pub(crate) fn enforce_budget(&mut self, visible: &[i64]) {
        while self.bytes > CACHE_BYTES {
            let victim = self
                .entries
                .iter()
                .filter(|(id, _)| !visible.contains(id))
                .min_by(|(id_a, a), (id_b, b)| {
                    a.used.total_cmp(&b.used).then_with(|| id_a.cmp(id_b))
                })
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                break;
            };
            if let Some(removed) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            }
        }
    }
}

/// Bytes of a premultiplied `f32` tile.
fn tile_bytes(entry: &Entry) -> usize {
    entry.image.width() as usize * entry.image.height() as usize * 16
}

/// `fnv1a`: a cheap 64-bit hash of the bytes.
pub(crate) fn hash_of(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Size, scale, theme and palette of the last `configure`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) scale: f64,
    pub(crate) theme: String,
    pub(crate) theme_hash: u64,
    pub(crate) pal: ColorMap,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 440.0,
            height: 600.0,
            scale: 1.0,
            theme: "dark".to_string(),
            theme_hash: hash_of(b"dark"),
            pal: ColorMap::new(),
        }
    }
}

impl ChatView {
    /// Fingerprint of what the tile of a message depends on.
    pub(crate) fn cache_key(&self, msg: &ChatMessage) -> CacheKey {
        CacheKey {
            text_len: msg.text.len(),
            text_hash: hash_of(msg.text.as_bytes()),
            streaming: msg.streaming,
            error: msg.error,
            theme_hash: self.g.theme_hash,
            width: canvas::px(self.g.width),
        }
    }

    /// Key of a message in the layout cache.
    pub(crate) fn entry(&mut self, msg: &ChatMessage, now: f64) -> Rc<Entry> {
        let key = self.cache_key(msg);
        if let Some(cached) = self.cache.get_mut(msg.id) {
            cached.used = now;
            // DECISION: the first text and final answer bypass batching;
            // subsequent chunks are laid out at most every 50 ms to bound the
            // rich-text work.
            if cached.key == key
                || (msg.streaming
                    && !cached.entry.plain.is_empty()
                    && now - cached.at < STREAM_REFRESH)
            {
                return Rc::clone(&cached.entry);
            }
        }
        let entry = Rc::new(self.build_entry(msg));
        self.cache.insert(msg.id, key, Rc::clone(&entry), now);
        entry
    }

    /// Height of a message, without laying it out again when it is known.
    ///
    /// DECISION: the height is cached apart from the tile (DX-1, paso 3). The
    /// total height of the conversation needs the height of every message, but
    /// the tile is only needed for the ones on screen; without this the view
    /// laid out the whole history in every frame.
    pub(crate) fn height_of(&mut self, msg: &ChatMessage, now: f64) -> f64 {
        let key = self.cache_key(msg);
        if let Some(cached) = self.cache.height(msg.id) {
            if cached.key == key {
                return cached.height;
            }
        }
        let entry = self.entry(msg, now);
        let height = entry.height;
        self.cache.insert_height(msg.id, key, height);
        height
    }

    fn build_entry(&self, msg: &ChatMessage) -> Entry {
        match msg.role {
            Role::User => self.user_entry(msg),
            Role::System => self.note_entry(msg),
            Role::Assistant => self.assistant_entry(msg),
        }
    }

    /// A note from the app («conversación compactada»): one faint, centred line.
    fn note_entry(&self, msg: &ChatMessage) -> Entry {
        let width = self.content_w() * 0.92;
        let ctx = self.ctx(width);
        let mut lines = wrap_spans(
            &self.host.parse_inline(&msg.text),
            "small",
            ctx.width,
            self.g.scale,
        );
        for runs in &mut lines {
            let used = runs.iter().map(|run| run.x + run.width).fold(0.0, f64::max);
            for run in runs {
                run.x += (ctx.width - used) / 2.0;
            }
        }
        let height = lines.len() as f64 * ctx.size("small") * 1.5 + 8.0;
        let scale = self.g.scale;
        let mut image = Image::new(
            canvas::size_px(ctx.width * scale),
            canvas::size_px(height * scale),
            Rgba::TRANSPARENT,
        );
        paint_lines(
            &mut image,
            &ctx,
            &lines,
            "small",
            0.0,
            4.0,
            ctx.color("TEXT_3", None),
        );
        Entry {
            height,
            image: premult(&image),
            x: PAD + self.content_w() * 0.04,
            hits: Vec::new(),
            caption_hit: None,
            plain: msg.text.clone(),
        }
    }

    /// Where the last line of the text reaches (`None` when the block is not
    /// text).
    fn tail_right(&self, blocks: &[Block], width: f64) -> Option<f64> {
        let block = blocks.last()?;
        let (indent, room, spans) = match block {
            Block::Paragraph(paragraph) => (0.0, width, &paragraph.spans),
            Block::ListItem(item) => {
                let indent = 4.0 + item.level as f64 * LIST_STEP + LIST_MARKER;
                (indent, width - indent, &item.spans)
            }
            Block::Quote(quote) => (14.0, width - 16.0, &quote.spans),
            _ => return None,
        };
        let lines = wrap_spans(spans, "body", room, self.g.scale);
        let last = lines.last()?;
        Some(indent + last.iter().map(|run| run.x + run.width).fold(0.0, f64::max))
    }

    /// Height of one body text line.
    fn copy_row_h(&self) -> f64 {
        self.ctx(1.0).size("body") * ez_gfx::text::chat::LINE_K
    }

    fn user_entry(&self, msg: &ChatMessage) -> Entry {
        let blocks = self.host.parse(&msg.text);
        let rich = blocks.iter().any(|block| {
            matches!(
                block,
                Block::Document(_) | Block::Image(_) | Block::Table(_) | Block::Code(_)
            )
        });
        let limit = self.content_w() * if rich { 0.94 } else { 0.82 };
        let (inner, inline) = self.user_inner(&blocks, limit, rich);
        let options = RenderOptions {
            scale: self.g.scale,
            roots: self.roots.clone(),
            base_dirs: self.base_dirs.clone(),
            icons: Some(&GFX_ICONS),
        };
        let rendered = render_message(self.host.as_ref(), &msg.text, &self.g.pal, inner, &options);
        let bw = inner + 24.0;
        let body_h = rendered.height + if inline { 18.0 } else { 18.0 + COPY_ROW };
        let bh = body_h + 18.0;
        let caption = if inline {
            // The icon is centred on the last line, inside the same box.
            let cy = 9.0 + rendered.height - self.copy_row_h() / 2.0;
            Rect::new(bw - COPY_W - 6.0, cy + 8.0, bw - 6.0, cy + 28.0)
        } else {
            Rect::new(bw - COPY_W - 6.0, bh - 28.0, bw - 6.0, bh - 4.0)
        };
        let scale = self.g.scale;
        let mut image = Image::new(
            canvas::size_px(bw * scale),
            canvas::size_px(bh * scale),
            Rgba::TRANSPARENT,
        );
        let ctx = self.ctx(bw);
        round_rect(
            &mut image,
            &ctx,
            (0.0, 18.0, bw, bh),
            16.0,
            Some(ctx.color("MINT", Some(0.15))),
            Some(ctx.color("MINT", Some(0.34))),
        );
        image.alpha_composite(
            &rendered.image,
            (
                canvas::px(12.0 * scale) as i32,
                canvas::px(27.0 * scale) as i32,
            ),
        );
        let hits = rendered
            .hits
            .iter()
            .map(|hit| zone(hit, 12.0, 27.0))
            .collect();
        Entry {
            height: bh,
            image: premult(&image),
            x: self.g.width - PAD - bw,
            hits,
            caption_hit: Some(caption),
            // What was written, as is (without interpreting markdown).
            plain: msg.text.clone(),
        }
    }

    /// Bubble width that fits the text and the copy icon, plus whether the
    /// icon fits on the last line.
    fn user_inner(&self, blocks: &[Block], limit: f64, rich: bool) -> (f64, bool) {
        let mut natural: f64 = 0.0;
        for block in blocks {
            let spans = match block {
                Block::Paragraph(paragraph) => Some(&paragraph.spans),
                Block::Heading(heading) => Some(&heading.spans),
                Block::ListItem(item) => Some(&item.spans),
                Block::Quote(quote) => Some(&quote.spans),
                _ => None,
            };
            if let Some(spans) = spans {
                for runs in wrap_spans(spans, "body", limit - 24.0, self.g.scale) {
                    natural =
                        natural.max(runs.iter().map(|run| run.x + run.width).fold(0.0, f64::max));
                }
            }
        }
        let mut inner = if rich {
            limit - 24.0
        } else {
            (limit - 24.0).min((natural + 2.0).max(30.0))
        };
        let mut tail = self.tail_right(blocks, inner);
        if let Some(reach) = tail {
            if reach + COPY_SLOT > inner && inner < limit - 24.0 {
                // The bubble widens just enough for the icon to fit on the
                // last line.
                let wider = (limit - 24.0).min(reach + COPY_SLOT);
                if let Some(wider_tail) = self.tail_right(blocks, wider) {
                    if wider_tail + COPY_SLOT <= wider {
                        inner = wider;
                        tail = Some(wider_tail);
                    }
                }
            }
        }
        let inline = tail.is_some_and(|reach| reach + COPY_SLOT <= inner);
        (inner, inline)
    }

    fn assistant_entry(&self, msg: &ChatMessage) -> Entry {
        let pal = self.assistant_palette(msg.error);
        let has_copy = !msg.text.is_empty() && !msg.streaming && !msg.error;
        // DECISION: reserve a narrow icon gutter on every answer line so the
        // copy control never adds a bottom row to the card.
        let text_width = self.content_w() - 24.0 - if has_copy { COPY_SLOT } else { 0.0 };
        let options = RenderOptions {
            scale: self.g.scale,
            roots: self.roots.clone(),
            base_dirs: self.base_dirs.clone(),
            icons: Some(&GFX_ICONS),
        };
        let rendered = render_message(self.host.as_ref(), &msg.text, &pal, text_width, &options);
        let scale = self.g.scale;
        let caption = 20.0;
        // DECISION: equal vertical inset keeps the answer text centred in its
        // card.
        let height = caption
            + 12.0
            + if msg.text.is_empty() {
                16.0
            } else {
                rendered.height
            }
            + 12.0;
        let mut image = Image::new(
            canvas::size_px(self.content_w() * scale),
            canvas::size_px(height * scale),
            Rgba::TRANSPARENT,
        );
        let ctx = self.ctx(self.content_w());
        round_rect(
            &mut image,
            &ctx,
            (0.0, caption, self.content_w(), height),
            16.0,
            Some(ctx.color("GROUP", None)),
            Some(ctx.color("LINE", None)),
        );
        if !msg.text.is_empty() {
            image.alpha_composite(
                &rendered.image,
                (
                    canvas::px(12.0 * scale) as i32,
                    canvas::px((caption + 12.0) * scale) as i32,
                ),
            );
        }
        let hits = rendered
            .hits
            .iter()
            .map(|hit| zone(hit, 12.0, caption + 12.0))
            .collect();
        let copy_rect = self.assistant_copy_rect(has_copy, caption, rendered.height);
        Entry {
            height,
            image: premult(&image),
            x: PAD,
            hits,
            caption_hit: copy_rect,
            plain: rendered.plain,
        }
    }

    /// Palette of an answer: errors paint the text in the error tone.
    fn assistant_palette(&self, error: bool) -> ColorMap {
        let mut pal = self.g.pal.clone();
        if error {
            let error = pal["ERR"];
            pal.insert("TEXT", error);
        }
        pal
    }

    /// Zone of the copy icon at the bottom-right corner of an answer.
    fn assistant_copy_rect(
        &self,
        has_copy: bool,
        caption: f64,
        rendered_height: f64,
    ) -> Option<Rect> {
        has_copy.then(|| {
            let cy = caption + 12.0 + rendered_height - self.copy_row_h() / 2.0;
            Rect::new(
                self.content_w() - COPY_W - 6.0,
                cy - 10.0,
                self.content_w() - 6.0,
                cy + 10.0,
            )
        })
    }
}

/// `_premult`: straight RGBA bytes to premultiplied `float32`.
fn premult(image: &Image) -> Layer {
    Layer::from_raw(image.width(), image.height(), image.to_f32_premultiplied())
        .expect("the premultiplied tile has its buffer")
}

/// `_shift`: moves a message hit to the tile corner.
fn zone(hit: &RenderHit, dx: f64, dy: f64) -> Zone {
    Zone {
        kind: hit.kind,
        rect: Rect::new(
            hit.rect.x0 + dx,
            hit.rect.y0 + dy,
            hit.rect.x1 + dx,
            hit.rect.y1 + dy,
        ),
        data: hit.data.clone().map(|data| match data {
            RenderData::Text(text) => ZoneData::Text(text),
            RenderData::File(file) => ZoneData::File(file),
        }),
    }
}
