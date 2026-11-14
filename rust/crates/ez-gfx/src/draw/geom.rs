//! Bresenham lines, the polygon scan converter and rectangles from Pillow's
//! `src/libImaging/Draw.c`.

use crate::raster::Mask;

use super::Surface;

#[derive(Clone, Copy, Debug)]
struct Edge {
    x0: i32,
    y0: i32,
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    dx: f32,
}

fn add_edge(x0: i32, y0: i32, x1: i32, y1: i32) -> Edge {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let dx = if y0 == y1 {
        0.0
    } else {
        (x1 - x0) as f32 / (y1 - y0) as f32
    };
    Edge {
        x0,
        y0,
        xmin,
        ymin,
        xmax,
        ymax,
        dx,
    }
}

/// `line()` of `Draw.c`: Bresenham, endpoint excluded.
pub(crate) fn line<S: Surface>(surface: &mut S, x0: i32, y0: i32, x1: i32, y1: i32, ink: u32) {
    let (mut x0, mut y0) = (x0, y0);
    let mut dy = y1 - y0;
    let ys;
    if dy < 0 {
        dy = -dy;
        ys = -1;
    } else if dy == 0 {
        if x1 > x0 {
            surface.hline(x0, y0, x1 - 1, ink, None);
        } else if x0 > x1 {
            surface.hline(x1 + 1, y0, x0, ink, None);
        }
        return;
    } else {
        ys = 1;
    }
    let mut dx = x1 - x0;
    let xs;
    if dx < 0 {
        dx = -dx;
        xs = -1;
    } else {
        xs = 1;
    }
    if dx == 0 {
        for _ in 0..dy {
            surface.point(x0, y0, ink);
            y0 += ys;
        }
    } else if dx > dy {
        let n = dx;
        dy += dy;
        let mut e = dy - dx;
        dx += dx;
        for _ in 0..n {
            surface.point(x0, y0, ink);
            if e >= 0 {
                y0 += ys;
                e -= dx;
            }
            e += dy;
            x0 += xs;
        }
    } else {
        let n = dy;
        dx += dx;
        let mut e = dx - dy;
        dy += dy;
        for _ in 0..n {
            surface.point(x0, y0, ink);
            if e >= 0 {
                x0 += xs;
                e -= dy;
            }
            e += dx;
            y0 += ys;
        }
    }
}

/// `polygon_generic()` without the RGBA-on-RGB blending path.
fn polygon_generic<S: Surface>(surface: &mut S, edges: &[Edge], ink: u32, mask: Option<&Mask>) {
    if edges.is_empty() {
        return;
    }
    let height = surface.height() as i32;
    let mut ymin = height - 1;
    let mut ymax = 0;
    let mut active: Vec<&Edge> = Vec::with_capacity(edges.len());
    for edge in edges {
        ymin = ymin.min(edge.ymin);
        ymax = ymax.max(edge.ymax);
        if edge.ymin == edge.ymax {
            surface.hline(edge.xmin, edge.ymin, edge.xmax, ink, mask);
            continue;
        }
        active.push(edge);
    }
    ymin = ymin.max(0);
    ymax = ymax.min(height);
    let mut xx: Vec<f32> = Vec::with_capacity(active.len() * 2);
    for y in ymin..=ymax {
        xx.clear();
        for (index, current) in active.iter().enumerate() {
            if y < current.ymin || y > current.ymax {
                continue;
            }
            let point = (y - current.y0) as f32 * current.dx + current.x0 as f32;
            xx.push(point);
            if y == current.ymax && y < ymax {
                xx.push(point);
            } else if (y == current.ymin || y == current.ymax) && current.dx != 0.0 {
                join_corner(&mut xx, current, &active[..index], y);
            }
        }
        xx.sort_by(f32::total_cmp);
        let mut index = 1;
        while index < xx.len() {
            surface.hline(round_up(xx[index - 1]), y, round_down(xx[index]), ink, mask);
            index += 2;
        }
    }
}

/// The "connect discontiguous corners" block of `polygon_generic`.
fn join_corner(xx: &mut [f32], current: &Edge, previous: &[&Edge], y: i32) {
    let last = xx[xx.len() - 1];
    for other in previous {
        if (y != other.ymin && y != other.ymax) || other.dx == 0.0 {
            continue;
        }
        let other_x = (y - other.y0) as f32 * other.dx + other.x0 as f32;
        if last.round() != other_x.round() {
            continue;
        }
        let offset = if y == current.ymax { -1 } else { 1 };
        let adjacent = (y + offset - current.y0) as f32 * current.dx + current.x0 as f32;
        if y + offset < other.ymin || y + offset > other.ymax {
            continue;
        }
        let adjacent_other = (y + offset - other.y0) as f32 * other.dx + other.x0 as f32;
        let last = xx[xx.len() - 1];
        if last > adjacent + 1.0 && last > adjacent_other + 1.0 {
            xx[xx.len() - 1] = adjacent.max(adjacent_other).round() + 1.0;
        } else if last < adjacent - 1.0 && last < adjacent_other - 1.0 {
            xx[xx.len() - 1] = adjacent.min(adjacent_other).round() - 1.0;
        }
        break;
    }
}

/// `ImagingDrawPolygon(..., fill=1, ...)`.
pub(crate) fn polygon_fill<S: Surface>(
    surface: &mut S,
    points: &[(i32, i32)],
    ink: u32,
    mask: Option<&Mask>,
) {
    let mut edges: Vec<Edge> = Vec::with_capacity(points.len());
    for index in 0..points.len().saturating_sub(1) {
        let (x0, y0) = points[index];
        let (x1, y1) = points[index + 1];
        if index != 0 && y0 == y1 && y0 == points[index - 1].1 {
            let (px, _) = points[index - 1];
            let last = edges.last_mut();
            if let Some(edge) = last {
                if x1 > x0 && x0 > px {
                    edge.xmax = x1;
                    continue;
                }
                if x1 < x0 && x0 < px {
                    edge.xmin = x1;
                    continue;
                }
            }
        }
        edges.push(add_edge(x0, y0, x1, y1));
    }
    if let (Some(&(last_x, last_y)), Some(&(first_x, first_y))) = (points.last(), points.first()) {
        if last_x != first_x || last_y != first_y {
            edges.push(add_edge(last_x, last_y, first_x, first_y));
        }
    }
    polygon_generic(surface, &edges, ink, mask);
}

/// `ImagingDrawPolygon(..., fill=0, ...)`.
pub(crate) fn polygon_outline<S: Surface>(
    surface: &mut S,
    points: &[(i32, i32)],
    ink: u32,
    width: u32,
    mask: Option<&Mask>,
) {
    let Some(&(first_x, first_y)) = points.first() else {
        return;
    };
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if width == 1 {
            line(surface, x0, y0, x1, y1, ink);
        } else {
            wide_line(surface, (x0, y0), (x1, y1), ink, width as i32, mask);
        }
    }
    let (last_x, last_y) = points[points.len() - 1];
    if width == 1 {
        line(surface, last_x, last_y, first_x, first_y, ink);
    } else {
        wide_line(
            surface,
            (last_x, last_y),
            (first_x, first_y),
            ink,
            width as i32,
            mask,
        );
    }
}

/// `ImagingDrawRectangle`.
pub(crate) fn rectangle<S: Surface>(
    surface: &mut S,
    box_: [i32; 4],
    ink: u32,
    fill: bool,
    width: i32,
) {
    let [x0, mut y0, x1, mut y1] = box_;
    if y0 > y1 {
        std::mem::swap(&mut y0, &mut y1);
    }
    if fill {
        let height = surface.height() as i32;
        if y0 < 0 {
            y0 = 0;
        } else if y0 >= height {
            return;
        }
        if y1 < 0 {
            return;
        } else if y1 > height {
            y1 = height;
        }
        for y in y0..=y1 {
            surface.hline(x0, y, x1, ink, None);
        }
        return;
    }
    let width = if width == 0 { 1 } else { width };
    for i in 0..width {
        surface.hline(x0, y0 + i, x1, ink, None);
        surface.hline(x0, y1 - i, x1, ink, None);
        line(surface, x1 - i, y0 + width, x1 - i, y1 - width + 1, ink);
        line(surface, x0 + i, y0 + width, x0 + i, y1 - width + 1, ink);
    }
}

/// `ImagingDrawWideLine`.
pub(crate) fn wide_line<S: Surface>(
    surface: &mut S,
    from: (i32, i32),
    to: (i32, i32),
    ink: u32,
    width: i32,
    mask: Option<&Mask>,
) {
    let (x0, y0) = from;
    let (x1, y1) = to;
    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0 && dy == 0 {
        surface.point(x0, y0, ink);
        return;
    }
    let big_hypotenuse = f64::from(dx).hypot(f64::from(dy));
    let small_hypotenuse = f64::from(width - 1) / 2.0;
    let ratio_max = f64::from(round_up_double(small_hypotenuse)) / big_hypotenuse;
    let ratio_min = f64::from(round_down_double(small_hypotenuse)) / big_hypotenuse;
    let dxmin = round_down_double(ratio_min * f64::from(dy));
    let dxmax = round_down_double(ratio_max * f64::from(dy));
    let dymin = round_down_double(ratio_min * f64::from(dx));
    let dymax = round_down_double(ratio_max * f64::from(dx));
    let vertices = [
        (x0 - dxmin, y0 + dymax),
        (x1 - dxmin, y1 + dymax),
        (x1 + dxmax, y1 - dymin),
        (x0 + dxmax, y0 - dymin),
    ];
    let mut edges = [add_edge(0, 0, 0, 0); 4];
    for index in 0..4 {
        let (ax, ay) = vertices[index];
        let (bx, by) = vertices[(index + 1) % 4];
        edges[index] = add_edge(ax, ay, bx, by);
    }
    polygon_generic(surface, &edges, ink, mask);
}

/// `ROUND_UP` of `Draw.c` over a double.
fn round_up_double(value: f64) -> i32 {
    if value >= 0.0 {
        (value + 0.5).floor() as i32
    } else {
        -(f64::abs(value) + 0.5).floor() as i32
    }
}

/// `ROUND_DOWN` of `Draw.c` over a double.
fn round_down_double(value: f64) -> i32 {
    if value >= 0.0 {
        (value - 0.5).ceil() as i32
    } else {
        -(f64::abs(value) - 0.5).ceil() as i32
    }
}

/// `ROUND_UP` of `Draw.c` over a float.
fn round_up(value: f32) -> i32 {
    if value >= 0.0 {
        (value + 0.5).floor() as i32
    } else {
        -(f32::abs(value) + 0.5).floor() as i32
    }
}

/// `ROUND_DOWN` of `Draw.c` over a float.
fn round_down(value: f32) -> i32 {
    if value >= 0.0 {
        (value - 0.5).ceil() as i32
    } else {
        -(f32::abs(value) - 0.5).ceil() as i32
    }
}
