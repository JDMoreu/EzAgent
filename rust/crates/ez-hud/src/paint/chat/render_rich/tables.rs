//! Table blocks of the chat (part of `render_rich.py`, F5-13).

use ez_gfx::draw::Draw;
use ez_gfx::{Image, Mask};

use super::super::{
    line, multiply_mask, paint_lines, put_alpha, round_rect, text_height, wrap_spans, Ctx, Painter,
    Run, Span, Table, TableAlign,
};
use super::{CELL_MIN, CELL_PAD_X, CELL_PAD_Y};

// -------------------------------------------------------------------- tables
fn natural_width(ctx: &Ctx<'_>, spans: &[Span], key: &str) -> f64 {
    wrap_spans(spans, key, 1e9, ctx.scale)
        .iter()
        .map(|line| line.iter().map(|run| run.width).sum::<f64>())
        .fold(0.0, f64::max)
}

pub(super) fn column_widths(ctx: &Ctx<'_>, table: &Table) -> Vec<f64> {
    let n = table.header.len();
    if n == 0 {
        return Vec::new(); // DECISION: the parser never emits a table without columns.
    }
    let mut natural = vec![CELL_MIN; n];
    for (index, cell) in table.header.iter().enumerate() {
        natural[index] = natural[index].max(natural_width(ctx, cell, "cellbold"));
    }
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = natural.get_mut(index) {
                *width = width.max(natural_width(ctx, cell, "cell"));
            }
        }
    }
    let pad = 2.0 * CELL_PAD_X;
    let mut widths: Vec<f64> = natural.iter().map(|width| width + pad).collect();
    let mut excess = widths.iter().sum::<f64>() - ctx.width;
    // With many columns the comfortable minimum does not fit: shrink until the
    // width is shared evenly.
    let floor = (CELL_MIN + pad).min(ctx.width / n as f64);
    while excess > 0.5 {
        let room: Vec<f64> = widths
            .iter()
            .map(|width| (width - floor).max(0.0))
            .collect();
        let total: f64 = room.iter().sum();
        if total <= 0.01 {
            break;
        }
        let take = excess.min(total);
        for (width, share) in widths.iter_mut().zip(&room) {
            *width -= take * share / total;
        }
        excess -= take;
    }
    widths
}

/// Lays out a table and returns its height and painter.
pub fn layout_table(table: &Table, ctx: &Ctx<'_>) -> (f64, Painter) {
    let widths = column_widths(ctx, table);
    let mut keys: Vec<&'static str> = vec!["cellbold"];
    keys.extend(std::iter::repeat_n("cell", table.rows.len()));
    let mut source: Vec<&Vec<Vec<Span>>> = vec![&table.header];
    source.extend(table.rows.iter());
    let wrapped: Vec<Vec<Vec<Vec<Run>>>> = source
        .iter()
        .zip(&keys)
        .map(|(row, key)| {
            row.iter()
                .enumerate()
                .map(|(index, cell)| {
                    let width = widths.get(index).copied().unwrap_or(CELL_MIN) - 2.0 * CELL_PAD_X;
                    wrap_spans(cell, key, width, ctx.scale)
                })
                .collect()
        })
        .collect();
    let heights: Vec<f64> = wrapped
        .iter()
        .zip(&keys)
        .map(|(cells, key)| {
            let tallest = cells
                .iter()
                .map(|lines| text_height(lines.len(), key))
                .fold(0.0, f64::max);
            tallest + 2.0 * CELL_PAD_Y
        })
        .collect();
    let total: f64 = heights.iter().sum();
    let aligns = table.aligns.clone();
    let paint = Box::new(move |img: &mut Image, ctx: &mut Ctx<'_>, x, y| {
        let width: f64 = widths.iter().sum();
        round_rect(
            img,
            ctx,
            (x, y, x + width, y + total),
            9.0,
            Some(ctx.color("BODY", Some(0.35))),
            Some(ctx.color("LINE", None)),
        );
        let mut top = y;
        for (r, (cells, key)) in wrapped.iter().zip(&keys).enumerate() {
            if r == 0 {
                band(img, ctx, x, top, width, heights[r], "TRACK", 0.40, true);
            } else if r % 2 == 0 {
                band(img, ctx, x, top, width, heights[r], "TRACK", 0.14, false);
            }
            if r > 0 {
                line(
                    img,
                    ctx,
                    x + 1.0,
                    top,
                    x + width - 1.0,
                    top + 1.0,
                    ctx.color("LINE", None),
                );
            }
            let mut left = x;
            for (c, lines) in cells.iter().enumerate() {
                let align = aligns.get(c).copied().unwrap_or(TableAlign::Left);
                paint_cell(img, ctx, lines, key, align, left, top, widths[c], r == 0);
                left += widths[c];
            }
            top += heights[r];
        }
    });
    (total, paint)
}

/// Background band of a row (the first one with rounded top corners).
#[allow(clippy::too_many_arguments)]
fn band(
    img: &mut Image,
    ctx: &Ctx<'_>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: &str,
    alpha: f64,
    first: bool,
) {
    let width = (ctx.px(w) - 2).max(1) as u32;
    let height = ctx.px(h).max(1) as u32;
    let tone = ctx.color(color, Some(alpha));
    let mut layer = Image::new(width, height, tone);
    if first {
        let mut mask = Mask::new(width, height, 0);
        Draw::new(&mut mask)
            .rounded_rectangle(
                [
                    0.0,
                    0.0,
                    f64::from(width - 1),
                    f64::from(height) + f64::from(ctx.px(12.0)),
                ],
                f64::from(ctx.px(8.0)),
                Some(255u8),
                None,
                0,
            )
            .expect("the band box is ordered");
        put_alpha(&mut layer, &multiply_mask(&mask, tone.a));
    }
    img.alpha_composite(&layer, (ctx.px(x) + 1, ctx.px(y)));
}

#[allow(clippy::too_many_arguments)]
fn paint_cell(
    img: &mut Image,
    ctx: &Ctx<'_>,
    lines: &[Vec<Run>],
    key: &str,
    align: TableAlign,
    left: f64,
    top: f64,
    width: f64,
    header: bool,
) {
    let inner = width - 2.0 * CELL_PAD_X;
    let alpha = if header { 1.0 } else { 0.92 };
    let color = ctx.color("TEXT", Some(alpha));
    let mut shifted: Vec<Vec<Run>> = Vec::with_capacity(lines.len());
    for runs in lines {
        let used: f64 = runs.iter().map(|run| run.width).sum();
        let offset = match align {
            TableAlign::Left => 0.0,
            TableAlign::Center => (inner - used) / 2.0,
            TableAlign::Right => inner - used,
        };
        let moved = runs
            .iter()
            .map(|run| {
                let mut run = run.clone();
                run.x += offset.max(0.0);
                run
            })
            .collect();
        shifted.push(moved);
    }
    paint_lines(
        img,
        ctx,
        &shifted,
        key,
        left + CELL_PAD_X,
        top + CELL_PAD_Y,
        color,
    );
}
