//! Geometry of the timeline panel: where frames and layers are, and which
//! frame or layer a point falls on.
//!
//! Coordinates are the timeline's *content* coordinates, before scrolling:
//! the layer-name column occupies `0..names_width` on x, the frame ruler
//! `0..ruler_height` on y, and frame `f` (0-based) starts at
//! `names_width + f × cell_width`. The painter subtracts the scroll offset
//! and keeps the name column and the ruler pinned.

use std::ops::Range;

/// Most frame columns drawn. Beyond this, `f32` content coordinates lose
/// sub-pixel precision; a real document never comes close.
pub const MAX_COLUMNS: u32 = 200_000;

/// Sizes of the timeline grid, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineGeom {
    /// Width of one frame cell.
    pub cell_width: f32,
    /// Height of one layer row.
    pub row_height: f32,
    /// Height of the frame-number ruler.
    pub ruler_height: f32,
    /// Width of the layer-name column.
    pub names_width: f32,
}

impl Default for TimelineGeom {
    /// The Flash 8 "normal" frame size.
    fn default() -> Self {
        Self {
            cell_width: 9.0,
            row_height: 20.0,
            ruler_height: 20.0,
            names_width: 210.0,
        }
    }
}

impl TimelineGeom {
    /// Left edge of frame `frame`.
    pub fn frame_left(&self, frame: u32) -> f32 {
        self.names_width + frame as f32 * self.cell_width
    }

    /// Centre of frame `frame`.
    pub fn frame_center(&self, frame: u32) -> f32 {
        self.frame_left(frame) + self.cell_width * 0.5
    }

    /// The frame under content x, or `None` left of the first frame (in
    /// the name column) or for a non-finite x. Very large x saturates.
    pub fn frame_at(&self, x: f32) -> Option<u32> {
        if !x.is_finite() || x < self.names_width || self.cell_width <= 0.0 {
            return None;
        }
        let f = ((x - self.names_width) / self.cell_width).floor();
        Some(if f >= u32::MAX as f32 {
            u32::MAX
        } else {
            f as u32
        })
    }

    /// Top edge of layer row `row` (0 = top layer).
    pub fn row_top(&self, row: usize) -> f32 {
        self.ruler_height + row as f32 * self.row_height
    }

    /// The layer row under content y, or `None` in the ruler, past the last
    /// of `rows` rows, or for a non-finite y.
    pub fn row_at(&self, y: f32, rows: usize) -> Option<usize> {
        if !y.is_finite() || y < self.ruler_height || self.row_height <= 0.0 {
            return None;
        }
        let r = ((y - self.ruler_height) / self.row_height).floor();
        (r < rows as f32).then_some(r as usize)
    }

    /// The frames whose cells intersect the content x range `x0..x1`
    /// (partly visible cells included), limited to `0..frames`.
    pub fn visible_frames(&self, x0: f32, x1: f32, frames: u32) -> Range<u32> {
        let first = self.frame_at(x0.max(self.names_width)).unwrap_or(0);
        let last = self
            .frame_at((x1 - 0.001).max(self.names_width))
            .map_or(0, |f| f.saturating_add(1));
        first.min(frames)..last.min(frames)
    }

    /// The rows that intersect the content y range `y0..y1`, limited to
    /// `0..rows`.
    pub fn visible_rows(&self, y0: f32, y1: f32, rows: usize) -> Range<usize> {
        let to_row = |y: f32| {
            let r = ((y - self.ruler_height) / self.row_height).floor();
            if r.is_finite() && r > 0.0 {
                r as usize
            } else {
                0
            }
        };
        let first = to_row(y0.max(self.ruler_height));
        let last = to_row((y1 - 0.001).max(self.ruler_height)).saturating_add(1);
        first.min(rows)..last.min(rows)
    }

    /// Size of the whole content for `frames` frame columns and `rows`
    /// layer rows.
    pub fn content_size(&self, frames: u32, rows: usize) -> (f32, f32) {
        (self.frame_left(frames), self.row_top(rows))
    }

    /// How many frame columns to draw: the timeline's frames plus a margin,
    /// and at least enough to fill `view_width` points, at most
    /// [`MAX_COLUMNS`].
    pub fn columns_for(&self, frame_count: u32, view_width: f32) -> u32 {
        let fill = ((view_width - self.names_width).max(0.0) / self.cell_width).ceil();
        let fill = if fill.is_finite() { fill as u32 } else { 0 };
        frame_count.saturating_add(30).max(fill).min(MAX_COLUMNS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_and_x_round_trip() {
        let g = TimelineGeom::default();
        for f in [0u32, 1, 4, 9, 59, 1000, 16_000] {
            assert_eq!(g.frame_at(g.frame_left(f)), Some(f), "left edge of {f}");
            assert_eq!(g.frame_at(g.frame_center(f)), Some(f), "centre of {f}");
            assert_eq!(
                g.frame_at(g.frame_left(f + 1) - 0.01),
                Some(f),
                "right edge of {f}"
            );
        }
        assert_eq!(g.frame_left(0), g.names_width);
        assert_eq!(g.frame_left(3) - g.frame_left(2), g.cell_width);
    }

    #[test]
    fn points_outside_the_frame_area() {
        let g = TimelineGeom::default();
        assert_eq!(g.frame_at(g.names_width - 0.5), None);
        assert_eq!(g.frame_at(-10.0), None);
        assert_eq!(g.frame_at(f32::NAN), None);
        assert_eq!(g.frame_at(f32::INFINITY), None);
        assert_eq!(g.frame_at(1e30), Some(u32::MAX));
    }

    #[test]
    fn rows() {
        let g = TimelineGeom::default();
        assert_eq!(g.row_at(g.ruler_height - 1.0, 5), None);
        assert_eq!(g.row_at(g.row_top(0), 5), Some(0));
        assert_eq!(g.row_at(g.row_top(4) + g.row_height - 0.01, 5), Some(4));
        assert_eq!(g.row_at(g.row_top(5), 5), None);
        assert_eq!(g.row_at(f32::NAN, 5), None);
    }

    #[test]
    fn visible_ranges_include_partial_cells() {
        let g = TimelineGeom::default();
        // Exactly frames 2..5 visible.
        let r = g.visible_frames(g.frame_left(2), g.frame_left(5), 100);
        assert_eq!(r, 2..5);
        // Half of frame 1 and half of frame 6 visible.
        let r = g.visible_frames(g.frame_center(1), g.frame_center(6), 100);
        assert_eq!(r, 1..7);
        // Clamped to the frame count; the name column is not a frame.
        assert_eq!(g.visible_frames(0.0, g.frame_left(50), 10), 0..10);
        assert_eq!(g.visible_frames(0.0, 10.0, 10), 0..1);
        let rows = g.visible_rows(g.row_top(1) + 3.0, g.row_top(3) + 1.0, 10);
        assert_eq!(rows, 1..4);
        assert_eq!(g.visible_rows(0.0, 1e9, 3), 0..3);
    }

    #[test]
    fn content_and_columns() {
        let g = TimelineGeom::default();
        let (w, h) = g.content_size(10, 3);
        assert_eq!(w, g.names_width + 10.0 * g.cell_width);
        assert_eq!(h, g.ruler_height + 3.0 * g.row_height);
        assert_eq!(g.columns_for(60, 0.0), 90);
        assert!(g.columns_for(1, 2000.0) as f32 * g.cell_width >= 2000.0 - g.names_width);
        assert_eq!(g.columns_for(u32::MAX, 100.0), MAX_COLUMNS);
    }
}
