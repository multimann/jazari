//! A small anti-aliased scanline rasteriser for glyph outlines.
//!
//! Every edge adds its signed coverage to an accumulation buffer: within a
//! pixel row, each piece of an edge that lies inside one pixel column adds
//! `dy × (1 − x̄)` to that pixel and `dy × x̄` to the next one, where `dy` is
//! the piece's signed height and `x̄` its mean horizontal position inside the
//! pixel. A running sum along the row then gives the signed area covered to
//! the right of every edge, which is the pixel's winding-weighted coverage.
//! Coverage is `min(|sum|, 1)`, so overlapping contours of the same
//! direction stay opaque and holes (opposite direction) stay empty, as
//! TrueType outlines expect.
//!
//! Coordinates are pixels, x to the right and y down. Points outside the
//! canvas are clamped; non-finite points are ignored. Nothing here panics
//! on any input.

/// Most segments one quadratic curve is split into.
const MAX_CURVE_STEPS: usize = 64;
/// Largest distance (pixels) between a curve and its flattening.
const FLATNESS: f32 = 0.1;

/// A coverage canvas.
#[derive(Debug, Clone)]
pub struct Canvas {
    width: usize,
    height: usize,
    /// `(width + 2) × height` signed area deltas; the two spare columns take
    /// the spill of edges on or past the right border.
    cells: Vec<f32>,
}

impl Canvas {
    /// A transparent canvas. Sizes are clamped to at least one pixel.
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            cells: vec![0.0; (width + 2) * height],
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Adds a straight edge.
    pub fn line(&mut self, p0: (f32, f32), p1: (f32, f32)) {
        if !(p0.0.is_finite() && p0.1.is_finite() && p1.0.is_finite() && p1.1.is_finite()) {
            return;
        }
        let clamp = |(x, y): (f32, f32)| {
            (
                x.clamp(0.0, self.width as f32),
                y.clamp(0.0, self.height as f32),
            )
        };
        let (a, b) = (clamp(p0), clamp(p1));
        if a.1 == b.1 {
            return;
        }
        let (dir, top, bottom) = if a.1 < b.1 { (1.0, a, b) } else { (-1.0, b, a) };
        let dxdy = (bottom.0 - top.0) / (bottom.1 - top.1);
        let first_row = top.1.floor() as usize;
        let last_row = (bottom.1.ceil() as usize).min(self.height);
        for row in first_row..last_row {
            let y0 = top.1.max(row as f32);
            let y1 = bottom.1.min(row as f32 + 1.0);
            if y1 <= y0 {
                continue;
            }
            let x0 = top.0 + (y0 - top.1) * dxdy;
            let x1 = top.0 + (y1 - top.1) * dxdy;
            self.row_piece(row, x0, x1, (y1 - y0) * dir);
        }
    }

    /// Adds a quadratic Bézier edge (start, control, end), flattened into
    /// straight segments.
    pub fn quad(&mut self, p0: (f32, f32), c: (f32, f32), p1: (f32, f32)) {
        let ddx = p0.0 - 2.0 * c.0 + p1.0;
        let ddy = p0.1 - 2.0 * c.1 + p1.1;
        let deviation = (ddx * ddx + ddy * ddy).sqrt() / 4.0;
        let steps = if deviation.is_finite() {
            ((deviation / FLATNESS).sqrt().ceil() as usize).clamp(1, MAX_CURVE_STEPS)
        } else {
            1
        };
        let mut prev = p0;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let mt = 1.0 - t;
            let next = (
                mt * mt * p0.0 + 2.0 * mt * t * c.0 + t * t * p1.0,
                mt * mt * p0.1 + 2.0 * mt * t * c.1 + t * t * p1.1,
            );
            self.line(prev, next);
            prev = next;
        }
    }

    /// Distributes one edge piece that stays inside pixel row `row`, going
    /// from `x0` to `x1` with signed height `d`.
    fn row_piece(&mut self, row: usize, x0: f32, x1: f32, d: f32) {
        let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let span = hi - lo;
        let first = lo.floor() as usize;
        let last = (hi.floor() as usize).max(first);
        for col in first..=last {
            let left = lo.max(col as f32);
            let right = hi.min(col as f32 + 1.0);
            if right < left {
                continue;
            }
            // The piece inside this column carries its share of the height.
            let share = if span > 0.0 {
                (right - left) / span
            } else {
                1.0
            };
            if share <= 0.0 && span > 0.0 {
                continue;
            }
            let part = d * share;
            let mean = (left + right) * 0.5 - col as f32;
            self.add(row, col, part * (1.0 - mean));
            self.add(row, col + 1, part * mean);
        }
    }

    fn add(&mut self, row: usize, col: usize, value: f32) {
        let stride = self.width + 2;
        if let Some(cell) = self.cells.get_mut(row * stride + col.min(stride - 1)) {
            *cell += value;
        }
    }

    /// The coverage of every pixel, row by row from the top, 0 to 255.
    pub fn coverage(&self) -> Vec<u8> {
        let stride = self.width + 2;
        let mut out = Vec::with_capacity(self.width * self.height);
        for row in self.cells.chunks_exact(stride) {
            let mut sum = 0.0f32;
            for &delta in &row[..self.width] {
                sum += delta;
                let c = sum.abs().min(1.0);
                out.push((c * 255.0 + 0.5) as u8);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed polygon.
    fn polygon(canvas: &mut Canvas, points: &[(f32, f32)]) {
        for (i, &p) in points.iter().enumerate() {
            canvas.line(p, points[(i + 1) % points.len()]);
        }
    }

    fn total(cov: &[u8]) -> f32 {
        cov.iter().map(|&c| f32::from(c) / 255.0).sum()
    }

    #[test]
    fn pixel_aligned_square_is_exact() {
        let mut c = Canvas::new(8, 8);
        polygon(&mut c, &[(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)]);
        let cov = c.coverage();
        for y in 0..8 {
            for x in 0..8 {
                let inside = (2..6).contains(&x) && (2..6).contains(&y);
                assert_eq!(cov[y * 8 + x], if inside { 255 } else { 0 }, "({x},{y})");
            }
        }
    }

    #[test]
    fn half_pixel_edges_give_half_coverage() {
        let mut c = Canvas::new(8, 8);
        polygon(&mut c, &[(2.5, 2.0), (5.5, 2.0), (5.5, 4.0), (2.5, 4.0)]);
        let cov = c.coverage();
        assert_eq!(cov[2 * 8 + 2], 128);
        assert_eq!(cov[2 * 8 + 3], 255);
        assert_eq!(cov[2 * 8 + 5], 128);
        assert_eq!(cov[2 * 8 + 6], 0);
        assert!((total(&cov) - 6.0).abs() < 0.02);
    }

    #[test]
    fn winding_direction_does_not_matter_and_holes_stay_empty() {
        let mut cw = Canvas::new(10, 10);
        polygon(&mut cw, &[(1.0, 1.0), (9.0, 1.0), (9.0, 9.0), (1.0, 9.0)]);
        let mut ccw = Canvas::new(10, 10);
        polygon(&mut ccw, &[(1.0, 1.0), (1.0, 9.0), (9.0, 9.0), (9.0, 1.0)]);
        assert_eq!(cw.coverage(), ccw.coverage());

        // Outer square one way, inner square the other way: a hole.
        polygon(&mut cw, &[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)]);
        let cov = cw.coverage();
        assert_eq!(cov[5 * 10 + 5], 0, "hole");
        assert_eq!(cov[2 * 10 + 2], 255, "ring");
        assert!((total(&cov) - (64.0 - 16.0)).abs() < 0.02);
    }

    #[test]
    fn triangle_area_is_preserved() {
        let mut c = Canvas::new(20, 20);
        polygon(&mut c, &[(1.3, 1.7), (18.6, 4.2), (6.1, 17.9)]);
        // Shoelace formula.
        let area = 0.5 * ((18.6f32 - 1.3) * (17.9 - 1.7) - (6.1 - 1.3) * (4.2 - 1.7)).abs();
        assert!((total(&c.coverage()) - area).abs() < 0.1, "{}", area);
    }

    #[test]
    fn curved_disc_area_is_close_to_pi_r_squared() {
        // A circle of radius 8 from four quadratic arcs is a little larger
        // than the true circle (about 0.5 %); compare with a tolerance.
        let mut c = Canvas::new(20, 20);
        let (cx, cy, r) = (10.0f32, 10.0f32, 8.0f32);
        let k = r * std::f32::consts::SQRT_2; // control point distance for 90° arcs
        let pts = [(cx + r, cy), (cx, cy + r), (cx - r, cy), (cx, cy - r)];
        let ctl = [
            (
                cx + k / std::f32::consts::SQRT_2,
                cy + k / std::f32::consts::SQRT_2,
            ),
            (
                cx - k / std::f32::consts::SQRT_2,
                cy + k / std::f32::consts::SQRT_2,
            ),
            (
                cx - k / std::f32::consts::SQRT_2,
                cy - k / std::f32::consts::SQRT_2,
            ),
            (
                cx + k / std::f32::consts::SQRT_2,
                cy - k / std::f32::consts::SQRT_2,
            ),
        ];
        for i in 0..4 {
            c.quad(pts[i], ctl[i], pts[(i + 1) % 4]);
        }
        let area = std::f32::consts::PI * r * r;
        let got = total(&c.coverage());
        assert!((got - area).abs() / area < 0.06, "got {got}, circle {area}");
    }

    #[test]
    fn out_of_range_and_broken_input_never_panics() {
        let mut broken = Canvas::new(4, 4);
        broken.line((f32::NAN, 0.0), (1.0, 1.0));
        broken.line((0.0, f32::INFINITY), (1.0, 1.0));
        broken.quad((0.0, 0.0), (f32::MAX, f32::MAX), (1.0, 1.0));
        broken.line((-1e30, -1e30), (1e30, 1e30));
        assert_eq!(broken.coverage().len(), 16);

        // A shape far larger than the canvas is clamped to it.
        let mut big = Canvas::new(4, 4);
        polygon(
            &mut big,
            &[
                (-100.0, -100.0),
                (100.0, -100.0),
                (100.0, 100.0),
                (-100.0, 100.0),
            ],
        );
        let cov = big.coverage();
        assert!(cov.iter().all(|&v| v == 255), "{cov:?}");
        let empty = Canvas::new(0, 0);
        assert_eq!(empty.coverage(), vec![0]);
    }
}
