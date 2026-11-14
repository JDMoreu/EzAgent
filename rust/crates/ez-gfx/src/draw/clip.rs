//! `arc` and `pieslice` from Pillow's `Draw.c`, including its clipping tree.

use std::collections::VecDeque;

use super::ellipse::{self, EllipseState};
use super::Surface;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClipType {
    And,
    Or,
    Clip,
}

#[derive(Clone, Copy, Debug)]
struct ClipNode {
    kind: ClipType,
    a: f64,
    b: f64,
    c: f64,
    left: Option<usize>,
    right: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    x: i32,
    kind: i8,
}

#[derive(Default)]
struct ClipTree {
    nodes: Vec<ClipNode>,
}

impl ClipTree {
    fn node(&mut self, kind: ClipType) -> usize {
        self.nodes.push(ClipNode {
            kind,
            a: 0.0,
            b: 0.0,
            c: 0.0,
            left: None,
            right: None,
        });
        self.nodes.len() - 1
    }

    fn transpose(&mut self, index: usize) {
        if self.nodes[index].kind == ClipType::Clip {
            let node = &mut self.nodes[index];
            std::mem::swap(&mut node.a, &mut node.b);
        }
        let left = self.nodes[index].left;
        let right = self.nodes[index].right;
        if let Some(left) = left {
            self.transpose(left);
        }
        if let Some(right) = right {
            self.transpose(right);
        }
    }

    fn clip(&self, root: Option<usize>, mut x0: i32, y: i32, mut x1: i32) -> Vec<Event> {
        let Some(root) = root else {
            return vec![Event { x: x0, kind: 1 }, Event { x: x1, kind: -1 }];
        };
        let node = self.nodes[root];
        match node.kind {
            ClipType::Clip => {
                let eps = 1e-9;
                if node.a.abs() < eps {
                    if node.b * f64::from(y) + node.c < -eps {
                        x0 = 1;
                        x1 = 0;
                    }
                } else {
                    let ix = -(node.b * f64::from(y) + node.c) / node.a;
                    if node.a * f64::from(x0) + node.b * f64::from(y) + node.c < eps {
                        x0 = f64::from(x0).max(ix).round() as i32;
                    }
                    if node.a * f64::from(x1) + node.b * f64::from(y) + node.c < eps {
                        x1 = f64::from(x1).min(ix).round() as i32;
                    }
                }
                if x0 <= x1 {
                    vec![Event { x: x0, kind: 1 }, Event { x: x1, kind: -1 }]
                } else {
                    Vec::new()
                }
            }
            ClipType::And | ClipType::Or => {
                let left = self.clip(node.left, x0, y, x1);
                let right = self.clip(node.right, x0, y, x1);
                combine(node.kind, &left, &right)
            }
        }
    }
}

fn combine(kind: ClipType, left: &[Event], right: &[Event]) -> Vec<Event> {
    let mut events = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0usize, 0usize);
    let (mut k1, mut k2) = (0i32, 0i32);
    let mut tail: Option<i8> = None;
    while i < left.len() || j < right.len() {
        let take_left = j >= right.len()
            || (i < left.len()
                && (left[i].x < right[j].x
                    || (left[i].x == right[j].x && left[i].kind > right[j].kind)));
        let event = if take_left {
            let event = left[i];
            i += 1;
            k1 += i32::from(event.kind);
            event
        } else {
            let event = right[j];
            j += 1;
            k2 += i32::from(event.kind);
            event
        };
        let emit = match kind {
            ClipType::Or => {
                (event.kind == 1 && (tail.is_none() || tail == Some(-1)))
                    || (event.kind == -1 && k1 == 0 && k2 == 0)
            }
            ClipType::And => {
                (event.kind == 1 && (tail.is_none() || tail == Some(-1)) && k1 > 0 && k2 > 0)
                    || (event.kind == -1 && tail == Some(1) && (k1 == 0 || k2 == 0))
            }
            ClipType::Clip => false,
        };
        if emit {
            events.push(event);
            tail = Some(event.kind);
        }
    }
    events
}

/// The three `clip_ellipse_init` callbacks the HUD needs.
#[derive(Clone, Copy, Debug)]
enum ClipInit {
    Arc,
    Pie,
    PieSide,
}

struct ClippedEllipse {
    state: EllipseState,
    tree: ClipTree,
    root: Option<usize>,
    head: VecDeque<Event>,
    y: i32,
}

impl ClippedEllipse {
    fn new(kind: ClipInit, a: i32, b: i32, width: i32, start: f32, end: f32) -> Self {
        let mut tree = ClipTree::default();
        let mut state = EllipseState::new(-1, -1, 0);
        let root = match kind {
            ClipInit::Arc => arc_init(&mut tree, &mut state, a, b, width, start, end),
            ClipInit::Pie => pie_init(&mut tree, &mut state, a, b, width, start, end),
            ClipInit::PieSide => {
                pie_side_init(&mut tree, &mut state, a, b, width, start);
                None
            }
        };
        Self {
            state,
            tree,
            root,
            head: VecDeque::new(),
            y: 0,
        }
    }

    fn next(&mut self) -> Option<(i32, i32, i32)> {
        while self.head.is_empty() {
            let (x0, y, x1) = self.state.next()?;
            self.head = self.tree.clip(self.root, x0, y, x1).into();
            self.y = y;
        }
        let first = self.head.pop_front()?;
        let second = self.head.pop_front()?;
        Some((first.x, self.y, second.x))
    }
}

fn normalize_angles(al: &mut f32, ar: &mut f32) {
    if *ar - *al >= 360.0 {
        *al = 0.0;
        *ar = 360.0;
        return;
    }
    let start = if *al < 0.0 {
        360.0 - (-f64::from(*al)).rem_euclid(360.0)
    } else {
        f64::from(*al)
    };
    *al = start.rem_euclid(360.0) as f32;
    let mut delta = f64::from(*ar) - f64::from(*al);
    if delta < 0.0 {
        delta = 360.0 - (f64::from(*al) - f64::from(*ar)).rem_euclid(360.0);
    }
    *ar = (f64::from(*al) + delta.rem_euclid(360.0)) as f32;
}

fn arc_init(
    tree: &mut ClipTree,
    state: &mut EllipseState,
    a: i32,
    b: i32,
    width: i32,
    start: f32,
    end: f32,
) -> Option<usize> {
    if a < b {
        let root = arc_init(tree, state, b, a, width, 90.0 - end, 90.0 - start);
        *state = EllipseState::new(a, b, width);
        if let Some(root) = root {
            tree.transpose(root);
        }
        return root;
    }
    *state = EllipseState::new(a, b, width);
    let (mut al, mut ar) = (start, end);
    normalize_angles(&mut al, &mut ar);
    if ar == al + 360.0 {
        return None;
    }
    let (al64, ar64) = (f64::from(al), f64::from(ar));
    let (left, right) = arc_edges(tree, a, b, al64, ar64);
    if al64.rem_euclid(180.0) == 0.0 || ar64.rem_euclid(180.0) == 0.0 {
        return Some(arc_flat(tree, left, right, ar - al < 180.0));
    }
    let (al_half, ar_half) = ((al64 / 180.0) as i32, (ar64 / 180.0) as i32);
    if (al_half + ar_half) % 2 == 1 {
        return Some(arc_halves(tree, left, right, al_half, ar_half));
    }
    Some(arc_general(tree, left, right, ar - al < 180.0, ar))
}

/// The two half-plane clipping nodes at the `start` and `end` angles.
fn arc_edges(tree: &mut ClipTree, a: i32, b: i32, al: f64, ar: f64) -> (usize, usize) {
    let left = tree.node(ClipType::Clip);
    let right = tree.node(ClipType::Clip);
    let a2 = i64::from(a) * i64::from(a);
    let b2 = i64::from(b) * i64::from(b);
    let node = &mut tree.nodes[left];
    node.a = -f64::from(a) * (al * std::f64::consts::PI / 180.0).sin();
    node.b = f64::from(b) * (al * std::f64::consts::PI / 180.0).cos();
    node.c = (a2 - b2) as f64 * (al * std::f64::consts::PI / 90.0).sin() / 2.0;
    let node = &mut tree.nodes[right];
    node.a = f64::from(a) * (ar * std::f64::consts::PI / 180.0).sin();
    node.b = -f64::from(b) * (ar * std::f64::consts::PI / 180.0).cos();
    node.c = (b2 - a2) as f64 * (ar * std::f64::consts::PI / 90.0).sin() / 2.0;
    (left, right)
}

fn arc_flat(tree: &mut ClipTree, left: usize, right: usize, and: bool) -> usize {
    let root = tree.node(if and { ClipType::And } else { ClipType::Or });
    tree.nodes[root].left = Some(left);
    tree.nodes[root].right = Some(right);
    root
}

fn arc_halves(tree: &mut ClipTree, left: usize, right: usize, al_half: i32, ar_half: i32) -> usize {
    let root = tree.node(ClipType::Or);
    let and_left = tree.node(ClipType::And);
    let straight_left = tree.node(ClipType::Clip);
    tree.nodes[and_left].left = Some(straight_left);
    tree.nodes[and_left].right = Some(left);
    let and_right = tree.node(ClipType::And);
    let straight_right = tree.node(ClipType::Clip);
    tree.nodes[and_right].left = Some(straight_right);
    tree.nodes[and_right].right = Some(right);
    tree.nodes[root].left = Some(and_left);
    tree.nodes[root].right = Some(and_right);
    tree.nodes[straight_left].b = 1.0 - 2.0 * f64::from(al_half.rem_euclid(2));
    tree.nodes[straight_right].b = 1.0 - 2.0 * f64::from(ar_half.rem_euclid(2));
    root
}

fn arc_general(tree: &mut ClipTree, left: usize, right: usize, and: bool, ar: f32) -> usize {
    let root = tree.node(if and { ClipType::And } else { ClipType::Or });
    let pair = tree.node(tree.nodes[root].kind);
    tree.nodes[root].left = Some(pair);
    tree.nodes[pair].left = Some(left);
    tree.nodes[pair].right = Some(right);
    let side = tree.node(ClipType::Clip);
    tree.nodes[root].right = Some(side);
    tree.nodes[side].b = if ar < 180.0 || ar > 540.0 { 1.0 } else { -1.0 };
    root
}

fn pie_init(
    tree: &mut ClipTree,
    state: &mut EllipseState,
    a: i32,
    b: i32,
    width: i32,
    start: f32,
    end: f32,
) -> Option<usize> {
    *state = EllipseState::new(a, b, width);
    let (al, ar) = (f64::from(start), f64::from(end));
    let xl = f64::from(a) * (al * std::f64::consts::PI / 180.0).cos();
    let xr = f64::from(a) * (ar * std::f64::consts::PI / 180.0).cos();
    let yl = f64::from(b) * (al * std::f64::consts::PI / 180.0).sin();
    let yr = f64::from(b) * (ar * std::f64::consts::PI / 180.0).sin();
    let left = tree.node(ClipType::Clip);
    let right = tree.node(ClipType::Clip);
    tree.nodes[left].a = -yl;
    tree.nodes[left].b = xl;
    tree.nodes[right].a = yr;
    tree.nodes[right].b = -xr;
    let root = tree.node(if end - start < 180.0 {
        ClipType::And
    } else {
        ClipType::Or
    });
    tree.nodes[root].left = Some(left);
    tree.nodes[root].right = Some(right);
    if end - start < 90.0 {
        let spike = tree.node(ClipType::Clip);
        tree.nodes[spike].a = (xl + xr) / 2.0;
        tree.nodes[spike].b = (yl + yr) / 2.0;
        let outer = tree.node(ClipType::And);
        tree.nodes[outer].left = Some(root);
        tree.nodes[outer].right = Some(spike);
        return Some(outer);
    }
    Some(root)
}

fn pie_side_init(
    tree: &mut ClipTree,
    state: &mut EllipseState,
    a: i32,
    b: i32,
    width: i32,
    start: f32,
) {
    *state = EllipseState::new(a, b, a + b + 1);
    let angle = f64::from(start) * std::f64::consts::PI / 180.0;
    let xl = f64::from(a) * angle.cos();
    let yl = f64::from(b) * angle.sin();
    let (a1, b1) = (-yl, xl);
    let c1 = f64::from(width) * (a1 * a1 + b1 * b1).sqrt();
    let root = tree.node(ClipType::And);
    let pair = tree.node(ClipType::And);
    tree.nodes[root].left = Some(pair);
    let first = tree.node(ClipType::Clip);
    tree.nodes[first].a = a1;
    tree.nodes[first].b = b1;
    tree.nodes[first].c = c1;
    let second = tree.node(ClipType::Clip);
    tree.nodes[second].a = -a1;
    tree.nodes[second].b = -b1;
    tree.nodes[second].c = c1;
    tree.nodes[pair].left = Some(first);
    tree.nodes[pair].right = Some(second);
    let side = tree.node(ClipType::Clip);
    tree.nodes[side].a = b1;
    tree.nodes[side].b = -a1;
    tree.nodes[root].right = Some(side);
}

fn clipped<S: Surface>(
    surface: &mut S,
    xy: [f64; 4],
    kind: ClipInit,
    start: f32,
    end: f32,
    ink: u32,
    width: i32,
) {
    let (x0, y0) = (xy[0] as i32, xy[1] as i32);
    let (x1, y1) = (xy[2] as i32, xy[3] as i32);
    let a = x1 - x0;
    let b = y1 - y0;
    if a < 0 || b < 0 {
        return;
    }
    let mut state = ClippedEllipse::new(kind, a, b, width, start, end);
    while let Some((left, y, right)) = state.next() {
        surface.hline(
            x0 + (left + a) / 2,
            y0 + (y + b) / 2,
            x0 + (right + a) / 2,
            ink,
            None,
        );
    }
}

/// `ImagingDrawArc`.
pub(crate) fn arc<S: Surface>(
    surface: &mut S,
    xy: [f64; 4],
    start: f32,
    end: f32,
    ink: u32,
    width: i32,
) {
    let (mut start, mut end) = (start, end);
    normalize_angles(&mut start, &mut end);
    if start + 360.0 == end {
        ellipse::ellipse(surface, xy, ink, false, width);
        return;
    }
    if start == end {
        return;
    }
    clipped(surface, xy, ClipInit::Arc, start, end, ink, width);
}

/// `ImagingDrawPieslice`.
pub(crate) fn pieslice<S: Surface>(
    surface: &mut S,
    xy: [f64; 4],
    start: f32,
    end: f32,
    ink: u32,
    fill: bool,
    width: i32,
) {
    let (mut start, mut end) = (start, end);
    normalize_angles(&mut start, &mut end);
    let (x0, y0) = (xy[0] as i32, xy[1] as i32);
    let (x1, y1) = (xy[2] as i32, xy[3] as i32);
    if start + 360.0 == end {
        ellipse::ellipse(surface, xy, ink, fill, width);
        return;
    }
    if start == end {
        return;
    }
    if fill {
        clipped(
            surface,
            xy,
            ClipInit::Pie,
            start,
            end,
            ink,
            x1 + y1 - x0 - y0,
        );
        return;
    }
    clipped(surface, xy, ClipInit::PieSide, start, 0.0, ink, width);
    clipped(surface, xy, ClipInit::PieSide, end, 0.0, ink, width);
    if width > 0 {
        let cx = ((f64::from(x0) + f64::from(x1) - f64::from(width)) / 2.0).round() as i32;
        let cy = ((f64::from(y0) + f64::from(y1) - f64::from(width)) / 2.0).round() as i32;
        ellipse::ellipse(
            surface,
            [
                f64::from(cx),
                f64::from(cy),
                f64::from(cx + width - 1),
                f64::from(cy + width - 1),
            ],
            ink,
            true,
            0,
        );
    }
    clipped(surface, xy, ClipInit::Pie, start, end, ink, width);
}
