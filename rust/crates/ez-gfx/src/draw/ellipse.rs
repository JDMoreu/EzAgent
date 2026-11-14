//! Pillow's quarter-ellipse rasterizer (`ellipseNew` of `Draw.c`).

use super::Surface;

/// `quarter_state`: one quarter of an ellipse on a two-pixel grid.
#[derive(Clone, Copy, Debug, Default)]
struct Quarter {
    cx: i32,
    cy: i32,
    ex: i32,
    ey: i32,
    a2: i64,
    b2: i64,
    a2b2: i64,
    finished: bool,
}

impl Quarter {
    fn new(a: i32, b: i32) -> Self {
        if a < 0 || b < 0 {
            return Self {
                finished: true,
                ..Self::default()
            };
        }
        let a2 = i64::from(a) * i64::from(a);
        let b2 = i64::from(b) * i64::from(b);
        Self {
            cx: a,
            cy: b % 2,
            ex: a % 2,
            ey: b,
            a2,
            b2,
            a2b2: a2 * b2,
            finished: false,
        }
    }

    fn delta(&self, x: i64, y: i64) -> i64 {
        (self.a2 * y * y + self.b2 * x * x - self.a2b2).abs()
    }

    fn next(&mut self) -> Option<(i32, i32)> {
        if self.finished {
            return None;
        }
        let point = (self.cx, self.cy);
        if self.cx == self.ex && self.cy == self.ey {
            self.finished = true;
        } else {
            let (mut nx, mut ny) = (self.cx, self.cy + 2);
            let mut ndelta = self.delta(i64::from(nx), i64::from(ny));
            if nx > 1 {
                let diagonal = self.delta(i64::from(self.cx - 2), i64::from(self.cy + 2));
                if ndelta > diagonal {
                    nx = self.cx - 2;
                    ny = self.cy + 2;
                    ndelta = diagonal;
                }
                let straight = self.delta(i64::from(self.cx - 2), i64::from(self.cy));
                if ndelta > straight {
                    nx = self.cx - 2;
                    ny = self.cy;
                }
            }
            self.cx = nx;
            self.cy = ny;
        }
        Some(point)
    }
}

/// `ellipse_state`: horizontal runs of a thick ellipse.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EllipseState {
    outer: Quarter,
    inner: Quarter,
    py: i32,
    pl: i32,
    pr: i32,
    cy: [i32; 4],
    cl: [i32; 4],
    cr: [i32; 4],
    count: usize,
    finished: bool,
    leftmost: i32,
}

impl EllipseState {
    pub(crate) fn new(a: i32, b: i32, width: i32) -> Self {
        let leftmost = a % 2;
        let mut outer = Quarter::new(a, b);
        let mut state = Self {
            outer,
            inner: Quarter::new(-1, -1),
            py: 0,
            pl: 0,
            pr: 0,
            cy: [0; 4],
            cl: [0; 4],
            cr: [0; 4],
            count: 0,
            finished: false,
            leftmost,
        };
        if width < 1 {
            state.finished = true;
            return state;
        }
        match outer.next() {
            None => state.finished = true,
            Some((right, y)) => {
                state.pr = right;
                state.py = y;
                state.inner = Quarter::new(a - 2 * (width - 1), b - 2 * (width - 1));
                state.pl = leftmost;
            }
        }
        state.outer = outer;
        state
    }

    fn push(&mut self, left: i32, y: i32, right: i32) {
        self.cl[self.count] = left;
        self.cy[self.count] = y;
        self.cr[self.count] = right;
        self.count += 1;
    }

    pub(crate) fn next(&mut self) -> Option<(i32, i32, i32)> {
        if self.count == 0 {
            if self.finished {
                return None;
            }
            let y = self.py;
            let mut left = self.pl;
            let right = self.pr;
            let mut next = self.outer.next();
            while let Some((_, next_y)) = next {
                if next_y > y {
                    break;
                }
                next = self.outer.next();
            }
            match next {
                None => self.finished = true,
                Some((cx, cy)) => {
                    self.pr = cx;
                    self.py = cy;
                }
            }
            let mut next_inner = self.inner.next();
            while let Some((cx, next_y)) = next_inner {
                if next_y > y {
                    break;
                }
                left = cx;
                next_inner = self.inner.next();
            }
            self.pl = match next_inner {
                None => self.leftmost,
                Some((cx, _)) => cx,
            };
            if (left > 0 || left < right) && y > 0 {
                self.push(if left == 0 { 2 } else { left }, y, right);
            }
            if y > 0 {
                self.push(-right, y, -left);
            }
            if left > 0 || left < right {
                self.push(if left == 0 { 2 } else { left }, -y, right);
            }
            self.push(-right, -y, -left);
        }
        if self.count == 0 {
            return None;
        }
        self.count -= 1;
        let index = self.count;
        Some((self.cl[index], self.cy[index], self.cr[index]))
    }
}

/// `ellipseNew`: a filled or outlined ellipse between two corners.
pub(crate) fn ellipse<S: Surface>(surface: &mut S, xy: [f64; 4], ink: u32, fill: bool, width: i32) {
    let (x0, y0) = (xy[0] as i32, xy[1] as i32);
    let (x1, y1) = (xy[2] as i32, xy[3] as i32);
    let a = x1 - x0;
    let b = y1 - y0;
    if a < 0 || b < 0 {
        return;
    }
    let w = if fill { a + b } else { width };
    let mut state = EllipseState::new(a, b, w);
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
