//! Glyph outlines as quadratic paths in ems, y down.

use jazari_xfl::model::{PathCmd, Pt};
use skrifa::outline::OutlinePen;

/// An [`OutlinePen`] that collects a glyph as [`PathCmd`]s.
///
/// Input is in font units, y up (skrifa's convention). Output is in ems
/// (1.0 = one em), y down, with straight and quadratic segments only:
/// cubic segments (CFF/CFF2 fonts) are approximated with kurbo's
/// G1-continuous quadratic splines. Every contour is closed explicitly: its
/// last point equals its `MoveTo` point.
pub(crate) struct EmPen {
    /// Ems per font unit (1 / units_per_em).
    scale: f64,
    /// Cubic approximation tolerance, in font units.
    tolerance: f64,
    cmds: Vec<PathCmd>,
    /// Start of the open contour, in font units.
    start: Option<(f64, f64)>,
    /// Current point, in font units.
    current: (f64, f64),
}

impl EmPen {
    /// A pen for a face with the given units per em.
    pub fn new(units_per_em: u16) -> Self {
        let upem = f64::from(units_per_em.max(1));
        Self {
            scale: 1.0 / upem,
            // About 1/4096 em: 0.25 unit at 1000 upem, 5 units in the
            // 20480-unit DefineFont3 em.
            tolerance: upem / 4096.0,
            cmds: Vec::new(),
            start: None,
            current: (0.0, 0.0),
        }
    }

    fn pt(&self, (x, y): (f64, f64)) -> Pt {
        Pt {
            x: x * self.scale,
            y: -y * self.scale,
        }
    }

    fn quad(&mut self, c: (f64, f64), p: (f64, f64)) {
        if c == self.current || c == p {
            self.line(p);
        } else {
            self.cmds.push(PathCmd::QuadTo(self.pt(c), self.pt(p)));
            self.current = p;
        }
    }

    fn line(&mut self, p: (f64, f64)) {
        if p != self.current {
            self.cmds.push(PathCmd::LineTo(self.pt(p)));
            self.current = p;
        }
    }

    /// Closes the last contour and returns the path.
    pub fn finish(mut self) -> Vec<PathCmd> {
        self.close();
        if matches!(self.cmds.last(), Some(PathCmd::MoveTo(_))) {
            self.cmds.pop();
        }
        self.cmds
    }
}

impl OutlinePen for EmPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = (f64::from(x), f64::from(y));
        // A contour that never drew anything is dropped.
        if matches!(self.cmds.last(), Some(PathCmd::MoveTo(_))) {
            self.cmds.pop();
        }
        self.cmds.push(PathCmd::MoveTo(self.pt(p)));
        self.start = Some(p);
        self.current = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.quad(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p0 = self.current;
        let p1 = (f64::from(cx0), f64::from(cy0));
        let p2 = (f64::from(cx1), f64::from(cy1));
        let p3 = (f64::from(x), f64::from(y));
        let cubic = kurbo::CubicBez::new(p0, p1, p2, p3);
        let quads: Vec<kurbo::QuadBez> = match cubic.approx_spline(self.tolerance) {
            Some(spline) => spline.to_quads().collect(),
            None => cubic.to_quads(self.tolerance).map(|(_, _, q)| q).collect(),
        };
        for q in quads {
            self.quad((q.p1.x, q.p1.y), (q.p2.x, q.p2.y));
        }
        // Land exactly on the end point despite rounding in the spline.
        if self.current != p3 {
            self.line(p3);
        }
    }

    fn close(&mut self) {
        if let Some(start) = self.start.take() {
            self.line(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_closed(cmds: &[PathCmd]) -> bool {
        let mut start = None;
        let mut last = None;
        for cmd in cmds {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    if start.is_some() && start != last {
                        return false;
                    }
                    start = Some(p);
                    last = Some(p);
                }
                PathCmd::LineTo(p) | PathCmd::QuadTo(_, p) => last = Some(p),
            }
        }
        start == last
    }

    #[test]
    fn cubics_become_closed_quadratics() {
        let mut pen = EmPen::new(1000);
        pen.move_to(0.0, 0.0);
        pen.curve_to(0.0, 500.0, 500.0, 1000.0, 1000.0, 1000.0);
        pen.line_to(1000.0, 0.0);
        pen.close();
        let cmds = pen.finish();
        assert!(matches!(cmds[0], PathCmd::MoveTo(Pt { x: 0.0, y: 0.0 })));
        assert!(cmds.iter().any(|c| matches!(c, PathCmd::QuadTo(..))));
        assert!(is_closed(&cmds));
        // y is flipped and scaled to ems: (1000, 1000) units -> (1, -1) em.
        assert!(
            cmds.iter()
                .any(|c| matches!(c, PathCmd::QuadTo(_, Pt { x: 1.0, y: -1.0 })))
        );
    }

    #[test]
    fn unclosed_contours_are_closed_and_empty_ones_dropped() {
        let mut pen = EmPen::new(2048);
        pen.move_to(10.0, 10.0);
        pen.move_to(0.0, 0.0);
        pen.line_to(2048.0, 0.0);
        pen.quad_to(2048.0, 2048.0, 0.0, 2048.0);
        pen.move_to(5.0, 5.0);
        let cmds = pen.finish();
        assert_eq!(cmds.len(), 4);
        assert!(is_closed(&cmds));
        assert_eq!(cmds[3], PathCmd::LineTo(Pt { x: 0.0, y: 0.0 }));
    }
}
