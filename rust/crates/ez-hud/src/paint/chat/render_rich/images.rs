//! Image blocks of the chat (part of `render_rich.py`, F5-13).

use ez_core::pyvalue::py_strip;
use ez_gfx::draw::Draw;
use ez_gfx::resample::Resampling;
use ez_gfx::{Image, Mask};

use super::super::{
    draw_text_at, hit_kind, multiply_masks, put_alpha, round_rect, Ctx, FileInfo, Hit, HitData,
    ImageBlock, Painter,
};
use super::common::{is_local_target, url_netloc, DEFAULT_TILE};
use super::documents::{clip, tile};
use super::IMAGE_MAX_H;

// ------------------------------------------------------------------ images
fn placeholder(ctx: &Ctx<'_>, title: &str, detail: &str) -> (f64, Painter) {
    let width = ctx.width;
    let max_width = ctx.width - 72.0;
    let title = title.to_string();
    let detail = detail.to_string();
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        round_rect(
            img,
            ctx,
            (x, y, x + width, y + 54.0),
            9.0,
            Some(ctx.color("BODY", Some(0.35))),
            Some(ctx.color("LINE", None)),
        );
        tile(img, ctx, x + 12.0, y + 11.0, 32.0, "IMG", DEFAULT_TILE);
        draw_text_at(
            img,
            ctx,
            x + 56.0,
            y + 23.0,
            &title,
            "smallbold",
            ctx.color("TEXT_2", None),
            false,
        );
        let clipped = clip(ctx, &detail, max_width, "small");
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

/// Lays out an image block.
pub fn layout_image(block: &ImageBlock, ctx: &Ctx<'_>) -> (f64, Painter) {
    if !is_local_target(&block.src) {
        let host = url_netloc(&block.src).unwrap_or_else(|| block.src.clone());
        return placeholder(
            ctx,
            "Imagen de la web",
            &format!("No se descarga sola · {host}"),
        );
    }
    let path = ctx.host.resolve_target(&block.src, &ctx.base_dirs);
    let info = ctx.host.file_info(path.as_deref(), ctx.roots.as_deref());
    let Some(info) = info else {
        return placeholder(ctx, "No encuentro la imagen", &block.src);
    };
    if !info.allowed {
        return placeholder(ctx, "Imagen fuera de las carpetas permitidas", &info.name);
    }
    let Some(image) = ctx.host.load_image(&info) else {
        return placeholder(ctx, "No puedo mostrar esta imagen", &info.name);
    };
    image_block(ctx, block, info, image)
}

fn image_block(ctx: &Ctx<'_>, block: &ImageBlock, info: FileInfo, image: Image) -> (f64, Painter) {
    let natural_w = f64::from(image.width()) / ctx.scale;
    let natural_h = f64::from(image.height()) / ctx.scale;
    let k = 1.0_f64
        .min(ctx.width / natural_w)
        .min(IMAGE_MAX_H / natural_h);
    let w = (natural_w * k).max(8.0);
    let h = (natural_h * k).max(8.0);
    let caption = py_strip(&block.alt).to_string();
    let extra = if caption.is_empty() { 0.0 } else { 20.0 };
    let max_width = ctx.width;
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        let pw = ctx.px(w).max(1) as u32;
        let ph = ctx.px(h).max(1) as u32;
        let mut shown = image.resize(pw, ph, Resampling::Lanczos);
        let mut mask = Mask::new(pw * 4, ph * 4, 0);
        Draw::new(&mut mask)
            .rounded_rectangle(
                [0.0, 0.0, f64::from(pw * 4 - 1), f64::from(ph * 4 - 1)],
                f64::from(ctx.px(10.0).max(0)) * 4.0,
                Some(255u8),
                None,
                0,
            )
            .expect("the image box is ordered");
        let mask = mask.resize(pw, ph, Resampling::Lanczos);
        let channel = shown.split()[3].clone();
        put_alpha(&mut shown, &multiply_masks(&mask, &channel));
        img.alpha_composite(&shown, (ctx.px(x), ctx.px(y)));
        round_rect(
            img,
            ctx,
            (x, y, x + w, y + h),
            10.0,
            None,
            Some(ctx.color("LINE", None)),
        );
        if !caption.is_empty() {
            let text = clip(ctx, &caption, max_width, "small");
            draw_text_at(
                img,
                ctx,
                x,
                y + h + 15.0,
                &text,
                "small",
                ctx.color("TEXT_3", None),
                false,
            );
        }
        ctx.hits.push(Hit {
            kind: hit_kind::IMAGE,
            rect: crate::tokens::Rect::new(x, y, x + w, y + h),
            data: Some(HitData::File(info.clone())),
        });
    });
    (h + extra, paint)
}
