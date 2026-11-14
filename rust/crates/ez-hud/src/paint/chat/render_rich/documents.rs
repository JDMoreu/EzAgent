//! Document cards of the chat (part of `render_rich.py`, F5-13).

use ez_core::pytime;
use ez_core::pyvalue::py_rstrip;
use ez_gfx::composite::linear_gradient;
use ez_gfx::resample::Resampling;
use ez_gfx::{Image, Rgba};

use super::super::{
    crop, draw_text_at, hit_kind, multiply_masks, put_alpha, round_rect, Ctx, Document, FileInfo,
    Hit, HitData, Painter,
};
use super::common::{family_color, human_size};
use super::{CARD_H, PREVIEW_MAX};

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

// --------------------------------------------------------------- documents
/// Cuts `text` with «…» so it fits in `width`.
pub(super) fn clip(ctx: &Ctx<'_>, text: &str, width: f64, key: &str) -> String {
    if ctx.length(text, key) <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    loop {
        if chars.is_empty() {
            break;
        }
        let mut candidate: String = chars.iter().collect();
        candidate.push('…');
        if ctx.length(&candidate, key) <= width {
            break;
        }
        chars.pop();
    }
    let trimmed: String = chars.iter().collect();
    format!("{}…", py_rstrip(&trimmed))
}

/// Square pill with the file extension.
pub(super) fn tile(
    img: &mut Image,
    ctx: &Ctx<'_>,
    x: f64,
    y: f64,
    size: f64,
    label: &str,
    rgb: [u8; 3],
) {
    round_rect(
        img,
        ctx,
        (x, y, x + size, y + size),
        9.0,
        Some(Rgba::new(rgb[0], rgb[1], rgb[2], 46)),
        Some(Rgba::new(rgb[0], rgb[1], rgb[2], 150)),
    );
    let tw = ctx.length(label, "micro");
    draw_text_at(
        img,
        ctx,
        x + (size - tw) / 2.0,
        y + size / 2.0 + 3.6,
        label,
        "micro",
        Rgba::new(rgb[0], rgb[1], rgb[2], 255),
        false,
    );
}

fn date_of(mtime: f64) -> String {
    // `time.localtime` drops the fractional part before breaking the date down.
    let iso = pytime::date_local(mtime.trunc()).expect("the modification time is a valid instant");
    let mut parts = iso.split('-');
    let year = parts.next().unwrap_or("1970");
    let month: usize = parts.next().and_then(|m| m.parse().ok()).unwrap_or(1);
    let day: u32 = parts.next().and_then(|d| d.parse().ok()).unwrap_or(1);
    format!("{day} {} {year}", MONTHS[month.clamp(1, 12) - 1])
}

fn button(
    img: &mut Image,
    ctx: &Ctx<'_>,
    x: f64,
    y: f64,
    label: &str,
    primary: bool,
    w: f64,
) -> crate::tokens::Rect {
    let box_ = (x, y, x + w, y + 26.0);
    let color = if primary {
        round_rect(img, ctx, box_, 8.0, Some(ctx.color("MINT_BTN", None)), None);
        ctx.color("MINT_INK", None)
    } else {
        round_rect(
            img,
            ctx,
            box_,
            8.0,
            None,
            Some(ctx.color("TEXT_3", Some(0.6))),
        );
        ctx.color("TEXT_2", None)
    };
    let tw = ctx.length(label, "smallbold");
    draw_text_at(
        img,
        ctx,
        x + (w - tw) / 2.0,
        y + 17.5,
        label,
        "smallbold",
        color,
        false,
    );
    crate::tokens::Rect::new(box_.0, box_.1, box_.2, box_.3)
}

/// Lays out a document card.
pub fn layout_document(block: &Document, ctx: &Ctx<'_>) -> (f64, Painter) {
    let path = ctx.host.resolve_target(&block.target, &ctx.base_dirs);
    let info = ctx.host.file_info(path.as_deref(), ctx.roots.as_deref());
    let Some(info) = info else {
        return missing_card(ctx, block);
    };
    let preview = ctx.host.text_preview(&info);
    let (sub, sub_h) = match preview {
        Some((text, more)) => {
            let (image, height) = preview_image(ctx, &text, more);
            (Some(image), height)
        }
        None => (None, 0.0),
    };
    let height = CARD_H + if sub.is_some() { sub_h + 12.0 } else { 0.0 };
    let label = tile_label(&info.ext);
    let label_for_sub = label.clone();
    let title = if block.label.is_empty() {
        info.name.clone()
    } else {
        block.label.clone()
    };
    let width = ctx.width;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        round_rect(
            img,
            ctx,
            (x, y, x + width, y + height),
            11.0,
            Some(ctx.color("BODY", Some(0.35))),
            Some(ctx.color("LINE", None)),
        );
        tile(
            img,
            ctx,
            x + 12.0,
            y + 12.0,
            40.0,
            &label,
            family_color(&info.ext),
        );
        let right = card_buttons(img, ctx, &info, x, y);
        draw_text_at(
            img,
            ctx,
            x + 64.0,
            y + 28.0,
            &clip(ctx, &title, right - x - 76.0, "bold"),
            "bold",
            ctx.color("TEXT", None),
            false,
        );
        let mut sub_text = format!(
            "{label_for_sub} · {} · {}",
            human_size(info.size),
            date_of(info.mtime)
        );
        if !info.allowed {
            sub_text = "Fuera de las carpetas permitidas".to_string();
        }
        draw_text_at(
            img,
            ctx,
            x + 64.0,
            y + 46.0,
            &clip(ctx, &sub_text, right - x - 76.0, "small"),
            "small",
            ctx.color("TEXT_3", None),
            false,
        );
        if let Some(sub) = &sub {
            img.alpha_composite(sub, (ctx.px(x + 12.0), ctx.px(y + CARD_H)));
        }
    });
    (height, paint)
}

fn tile_label(ext: &str) -> String {
    let label: String = ext
        .trim_start_matches('.')
        .to_uppercase()
        .chars()
        .take(4)
        .collect();
    if label.is_empty() {
        "ARCH".to_string()
    } else {
        label
    }
}

/// Right-side buttons; returns the `x` where they start (to clip the title).
fn card_buttons(img: &mut Image, ctx: &mut Ctx<'_>, info: &FileInfo, x: f64, y: f64) -> f64 {
    let right = x + ctx.width - 12.0;
    if !info.allowed {
        return right;
    }
    let (show_w, open_w) = (64.0, 56.0);
    let mut bx = right - show_w;
    ctx.hits.push(Hit {
        kind: hit_kind::REVEAL,
        rect: button(img, ctx, bx, y + 19.0, "Mostrar", false, show_w),
        data: Some(HitData::File(info.clone())),
    });
    if info.openable {
        bx -= open_w + 8.0;
        ctx.hits.push(Hit {
            kind: hit_kind::OPEN,
            rect: button(img, ctx, bx, y + 19.0, "Abrir", true, open_w),
            data: Some(HitData::File(info.clone())),
        });
    }
    bx - 8.0
}

fn missing_card(ctx: &Ctx<'_>, block: &Document) -> (f64, Painter) {
    let width = ctx.width;
    let target = block.target.clone();
    let max_width = ctx.width - 72.0;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        round_rect(
            img,
            ctx,
            (x, y, x + width, y + 54.0),
            11.0,
            Some(ctx.color("BODY", Some(0.25))),
            Some(ctx.color("LINE", None)),
        );
        tile(img, ctx, x + 12.0, y + 11.0, 32.0, "?", [150, 130, 110]);
        draw_text_at(
            img,
            ctx,
            x + 56.0,
            y + 23.0,
            "No encuentro este archivo",
            "smallbold",
            ctx.color("TEXT_2", None),
            false,
        );
        let clipped = clip(ctx, &target, max_width, "small");
        draw_text_at(
            img,
            ctx,
            x + 56.0,
            y + 40.0,
            &clipped,
            "small",
            ctx.color("TEXT_3", None),
            false,
        );
    });
    (54.0, paint)
}

/// Preview of the content: markdown drawn in a height-limited box.
fn preview_image(ctx: &Ctx<'_>, text: &str, more: bool) -> (Image, f64) {
    let mut inner = ctx.child(ctx.width - 24.0 - 20.0);
    let source = if more {
        format!("{text}\n\n…")
    } else {
        text.to_string()
    };
    let blocks = ctx.host.parse(&source);
    let (body, height) = super::super::render::render_blocks(&blocks, &mut inner);
    let shown = height.min(PREVIEW_MAX);
    let width = ctx.px(ctx.width - 24.0).max(1) as u32;
    let box_h = ctx.px(shown + 20.0).max(1) as u32;
    let mut out = Image::new(width, box_h, Rgba::TRANSPARENT);
    let box_ctx = ctx.child(ctx.width - 24.0);
    round_rect(
        &mut out,
        &box_ctx,
        (0.0, 0.0, ctx.width - 24.0, shown + 20.0),
        8.0,
        Some(ctx.color("BODY", Some(0.6))),
        None,
    );
    let cropped = crop(&body, 0, 0, body.width(), ctx.px(shown).max(1) as u32);
    out.alpha_composite(&cropped, (ctx.px(10.0), ctx.px(10.0)));
    if height > PREVIEW_MAX {
        // Fade at the end: «there is more».
        let fade_h = ctx.px(34.0).max(1) as u32;
        let fade = linear_gradient().resize(width, fade_h, Resampling::Bicubic);
        let mut veil = Image::new(width, fade_h, ctx.color("BODY", Some(0.9)));
        let alpha = veil.split()[3].clone();
        put_alpha(&mut veil, &multiply_masks(&fade, &alpha));
        out.alpha_composite(&veil, (0, box_h as i32 - fade_h as i32));
    }
    (out, shown + 20.0)
}
