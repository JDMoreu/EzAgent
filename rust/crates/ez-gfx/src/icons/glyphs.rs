//! Glyph geometry of `icons.py`: strokes and fills recorded on the 24×24 grid.
//!
//! Nothing is rasterized here; `icons::raster` turns a [`Pen`] into a mask.

/// A point on the 24-unit grid.
pub(crate) type Point = (f64, f64);

/// Geometry recorder on the 24-unit grid; nothing is rasterized here.
#[derive(Default)]
pub(crate) struct Pen {
    pub(crate) segs: Vec<[f64; 4]>,
    pub(crate) fills: Vec<[f64; 5]>,
}

impl Pen {
    fn line(&mut self, pts: &[Point]) {
        for pair in pts.windows(2) {
            self.segs.push([pair[0].0, pair[0].1, pair[1].0, pair[1].1]);
        }
        if pts.len() == 1 {
            let (x, y) = pts[0];
            self.segs.push([x, y, x, y]);
        }
    }

    fn close_loop(&mut self, pts: &[Point]) {
        let mut closed = pts.to_vec();
        closed.push(pts[0]);
        self.line(&closed);
    }

    /// Elliptic arc; angles in degrees, clockwise from 3 o'clock (y down).
    fn arc(&mut self, box_: [f64; 4], start: f64, end: f64) {
        let [x0, y0, x1, y1] = box_;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let steps = (((end - start).abs() / 6.0) as usize).max(6);
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let angle = (start + (end - start) * i as f64 / steps as f64).to_radians();
            pts.push((cx + rx * angle.cos(), cy + ry * angle.sin()));
        }
        self.line(&pts);
    }

    fn ellipse(&mut self, box_: [f64; 4]) {
        self.arc(box_, 0.0, 360.0);
    }

    fn rrect(&mut self, box_: [f64; 4], r: f64) {
        let [x0, y0, x1, y1] = box_;
        let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
        let mut pts = Vec::with_capacity(28);
        for (cx, cy, a0) in [
            (x1 - r, y0 + r, -90.0),
            (x1 - r, y1 - r, 0.0),
            (x0 + r, y1 - r, 90.0),
            (x0 + r, y0 + r, 180.0),
        ] {
            for i in 0..7 {
                let angle = (a0 + 90.0 * i as f64 / 6.0).to_radians();
                pts.push((cx + r * angle.cos(), cy + r * angle.sin()));
            }
        }
        self.close_loop(&pts);
    }

    fn fill_rrect(&mut self, box_: [f64; 4], r: f64) {
        self.fills.push([box_[0], box_[1], box_[2], box_[3], r]);
    }
}

/// `_PHONE`: the handset outline before rotation.
const PHONE: [Point; 14] = [
    (7.0, 3.8),
    (9.6, 4.3),
    (10.0, 6.8),
    (8.2, 8.0),
    (15.0, 15.2),
    (16.5, 13.5),
    (19.4, 14.0),
    (20.2, 16.8),
    (18.2, 20.2),
    (15.4, 19.8),
    (3.8, 8.0),
    (3.8, 5.2),
    (5.2, 3.8),
    (7.0, 3.8),
];

fn phone(pen: &mut Pen, rotation: f64) {
    let a = rotation.to_radians();
    let (ca, sa) = (a.cos(), a.sin());
    let rotated: Vec<Point> = PHONE
        .iter()
        .map(|&(x, y)| {
            (
                12.0 + (x - 12.0) * ca - (y - 12.0) * sa,
                12.0 + (x - 12.0) * sa + (y - 12.0) * ca,
            )
        })
        .collect();
    pen.line(&rotated);
}

fn microphone(pen: &mut Pen) {
    pen.rrect([8.8, 2.7, 15.2, 13.4], 3.2);
    pen.arc([4.7, 5.6, 19.3, 18.3], 0.0, 180.0);
    pen.line(&[(12.0, 18.2), (12.0, 21.0)]);
    pen.line(&[(8.8, 21.0), (15.2, 21.0)]);
}

fn speaker(pen: &mut Pen) {
    pen.close_loop(&[
        (4.0, 9.3),
        (8.2, 9.3),
        (12.9, 4.7),
        (12.9, 19.3),
        (8.2, 14.7),
        (4.0, 14.7),
    ]);
    pen.arc([12.3, 7.6, 18.2, 16.4], -52.0, 52.0);
    pen.arc([12.3, 4.8, 21.3, 19.2], -52.0, 52.0);
}

fn chip(pen: &mut Pen) {
    pen.rrect([6.4, 6.4, 17.6, 17.6], 2.4);
    for t in [9.6, 14.4] {
        pen.line(&[(t, 3.2), (t, 6.4)]);
        pen.line(&[(t, 17.6), (t, 20.8)]);
        pen.line(&[(3.2, t), (6.4, t)]);
        pen.line(&[(17.6, t), (20.8, t)]);
    }
}

fn sliders(pen: &mut Pen) {
    for (y, knob) in [(7.0, 15.0), (12.0, 9.0), (17.0, 13.5)] {
        pen.line(&[(4.5, y), (knob - 2.2, y)]);
        pen.line(&[(knob + 2.2, y), (19.5, y)]);
        pen.ellipse([knob - 2.1, y - 2.1, knob + 2.1, y + 2.1]);
    }
}

fn trash(pen: &mut Pen) {
    pen.line(&[(4.8, 7.0), (19.2, 7.0)]);
    pen.line(&[(9.4, 7.0), (9.4, 4.8), (14.6, 4.8), (14.6, 7.0)]);
    pen.line(&[
        (6.6, 7.0),
        (7.5, 18.4),
        (8.6, 19.4),
        (15.4, 19.4),
        (16.5, 18.4),
        (17.4, 7.0),
    ]);
    pen.line(&[(10.2, 10.6), (10.2, 16.0)]);
    pen.line(&[(13.8, 10.6), (13.8, 16.0)]);
}

fn copy(pen: &mut Pen) {
    pen.line(&[
        (5.4, 15.4),
        (5.4, 6.4),
        (6.8, 5.0),
        (14.6, 5.0),
        (15.8, 6.2),
        (15.8, 8.2),
    ]);
    pen.rrect([8.4, 8.4, 19.0, 19.2], 2.4);
}

fn compose(pen: &mut Pen) {
    pen.line(&[(11.6, 5.0), (7.8, 5.0)]);
    pen.arc([4.8, 5.0, 10.8, 11.0], 270.0, 180.0);
    pen.line(&[(4.8, 8.0), (4.8, 16.2)]);
    pen.arc([4.8, 13.2, 10.8, 19.2], 180.0, 90.0);
    pen.line(&[(7.8, 19.2), (16.2, 19.2)]);
    pen.arc([13.2, 13.2, 19.2, 19.2], 90.0, 0.0);
    pen.line(&[(19.2, 16.2), (19.2, 12.4)]);
    pen.close_loop(&[
        (10.2, 13.8),
        (10.8, 11.0),
        (17.4, 4.4),
        (19.6, 6.6),
        (13.0, 13.2),
    ]);
}

fn chat(pen: &mut Pen) {
    pen.rrect([3.0, 3.8, 21.0, 17.2], 5.0);
    pen.line(&[(8.0, 17.2), (6.4, 21.2), (12.6, 17.2)]);
    pen.line(&[(7.6, 8.6), (16.4, 8.6)]);
    pen.line(&[(7.6, 12.4), (13.4, 12.4)]);
}

/// `_glyph(name)`.
pub(crate) fn glyph(name: &str) -> Pen {
    let mut pen = Pen::default();
    match name {
        "chevron-down" => pen.line(&[(6.8, 9.2), (12.0, 14.4), (17.2, 9.2)]),
        "chevron-up" => pen.line(&[(6.8, 14.4), (12.0, 9.2), (17.2, 14.4)]),
        "chevron-left" => pen.line(&[(14.8, 6.6), (9.4, 12.0), (14.8, 17.4)]),
        "minus" => pen.line(&[(6.4, 12.0), (17.6, 12.0)]),
        "check" => pen.line(&[(5.2, 12.4), (9.8, 17.0), (18.8, 6.8)]),
        "close" => {
            pen.line(&[(7.0, 7.0), (17.0, 17.0)]);
            pen.line(&[(17.0, 7.0), (7.0, 17.0)]);
        }
        "plus" => {
            pen.line(&[(12.0, 5.2), (12.0, 18.8)]);
            pen.line(&[(5.2, 12.0), (18.8, 12.0)]);
        }
        "send" => {
            pen.line(&[(12.0, 19.0), (12.0, 6.0)]);
            pen.line(&[(6.2, 11.6), (12.0, 5.8), (17.8, 11.6)]);
        }
        "menu" => {
            for y in [6.5, 12.0, 17.5] {
                pen.line(&[(4.8, y), (19.2, y)]);
            }
        }
        "voice-line" => {
            for (x, h) in [
                (5.0, 2.6),
                (9.0, 6.0),
                (13.0, 8.6),
                (17.0, 5.0),
                (21.0, 2.2),
            ] {
                pen.line(&[(x - 1.0, 12.0 - h), (x - 1.0, 12.0 + h)]);
            }
        }
        "waves" => {
            for (x, h) in [(5.8, 2.8), (9.9, 7.0), (14.1, 4.9), (18.2, 2.0)] {
                pen.line(&[(x, 12.0 - h), (x, 12.0 + h)]);
            }
        }
        "microphone" | "microphone-slash" => microphone(&mut pen),
        "speaker" | "speaker-slash" => speaker(&mut pen),
        "phone" => phone(&mut pen, 0.0),
        "hangup" => phone(&mut pen, 135.0),
        "sparkle" => {
            pen.close_loop(&[
                (12.0, 3.4),
                (14.1, 9.9),
                (20.6, 12.0),
                (14.1, 14.1),
                (12.0, 20.6),
                (9.9, 14.1),
                (3.4, 12.0),
                (9.9, 9.9),
            ]);
        }
        "mic-line" => {
            pen.rrect([9.0, 3.4, 15.0, 13.4], 3.0);
            pen.arc([5.4, 6.0, 18.6, 18.0], 0.0, 180.0);
            pen.line(&[(12.0, 18.0), (12.0, 20.6)]);
        }
        "clock" => {
            pen.ellipse([4.0, 4.0, 20.0, 20.0]);
            pen.line(&[(12.0, 7.6), (12.0, 12.0), (15.4, 14.0)]);
        }
        "alert" => {
            pen.ellipse([3.6, 3.6, 20.4, 20.4]);
            pen.line(&[(12.0, 7.6), (12.0, 12.8)]);
            pen.line(&[(12.0, 16.3)]);
        }
        "stop" => pen.fill_rrect([7.0, 7.0, 17.0, 17.0], 2.2),
        "chip" => chip(&mut pen),
        "sliders" => sliders(&mut pen),
        "trash" => trash(&mut pen),
        "copy" => copy(&mut pen),
        "chat" => chat(&mut pen),
        "compose" => compose(&mut pen),
        _ => {}
    }
    pen
}
