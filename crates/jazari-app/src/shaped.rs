//! Right-to-left and Arabic text in the UI.
//!
//! egui shapes Arabic letters but does not reorder bidirectional text, so a
//! name like «طبقة 2» or a script line mixing Arabic and code comes out in
//! the wrong order (`docs/research/app-shell.md`, F19). Content strings that
//! need it are therefore laid out by `jazari-text` (harfrust shaping,
//! unicode-bidi reordering, the embedded Noto fonts), rasterised with
//! [`crate::raster`] and shown as a cached egui texture tinted with the text
//! colour. Strings without right-to-left characters keep egui's own text.

use std::collections::HashMap;

use eframe::egui::{
    self, Color32, ColorImage, Context, Pos2, Rect, Response, Sense, TextureHandle, TextureOptions,
    Ui, Vec2,
};
use jazari_text::{FontLibrary, TextBlock};
use jazari_xfl::model::{LineType, PathCmd, TextAttrs, TextRun};

use crate::raster::Canvas;

/// The face asked for; jazari-text falls back to Noto Sans Arabic for the
/// Arabic letters and keeps Noto Sans for Latin.
const FACE: &str = "Noto Sans";
/// Characters shaped per line; longer lines are cut.
const MAX_CHARS: usize = 2000;
/// Largest bitmap side, in pixels.
const MAX_SIDE: usize = 8192;
/// Empty border around the ink, in pixels.
const PAD: f32 = 1.0;
/// Where the baseline sits below the vertical centre of a text row, as a
/// fraction of the font size (Noto Sans: ascender 1.069, descender 0.293).
pub const BASELINE_BELOW_CENTER: f32 = 0.388;
/// Entries kept in the cache before unused ones are dropped.
const CACHE_SOFT_LIMIT: usize = 512;
/// Frames an entry may stay unused once the soft limit is passed.
const CACHE_IDLE_FRAMES: u64 = 120;

/// Whether `text` holds right-to-left characters (Hebrew, Arabic and the
/// scripts between them, their presentation forms, bidi controls), which
/// egui would show in the wrong order.
pub fn needs_shaping(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x0590..=0x08FF
                | 0xFB1D..=0xFDFF
                | 0xFE70..=0xFEFF
                | 0x200E..=0x200F
                | 0x202A..=0x202E
                | 0x2066..=0x2069
                | 0x10800..=0x10FFF
                | 0x1E800..=0x1EFFF
        )
    })
}

/// A rasterised line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphBitmap {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Coverage, row by row, 0 to 255.
    pub coverage: Vec<u8>,
    /// Bitmap x of the line's start (the pen origin), in pixels.
    pub origin_x: f32,
    /// Bitmap y of the baseline, in pixels.
    pub baseline: f32,
    /// The line's advance width in pixels.
    pub advance: f32,
}

/// Lays out `text` as one line at `size_px` pixels per em and rasterises
/// it. Line breaks and tabs become spaces. `None` for empty or blank text
/// and unusable sizes.
pub fn rasterize_line(lib: &FontLibrary, text: &str, size_px: f32) -> Option<GlyphBitmap> {
    if !(size_px.is_finite() && (1.0..=512.0).contains(&size_px)) {
        return None;
    }
    let line = single_line(text);
    if line.trim().is_empty() {
        return None;
    }
    let block = TextBlock {
        runs: vec![TextRun {
            text: line,
            attrs: TextAttrs {
                face: FACE.to_owned(),
                size: f64::from(size_px),
                ..TextAttrs::default()
            },
        }],
        width: 0.0,
        auto_expand: true,
        line_type: LineType::SingleLine,
    };
    let layout = jazari_text::layout(lib, &block);
    let baseline_twips = layout.lines.first().map_or(0.0, |l| l.baseline);

    // Outlines in pixels, relative to the layout's origin.
    let mut edges = Edges::default();
    for g in layout.glyphs() {
        let scale = (g.size / 20.0) as f32;
        let (gx, gy) = ((g.x / 20.0) as f32, (g.y / 20.0) as f32);
        let to_px = |x: f64, y: f64| (gx + x as f32 * scale, gy + y as f32 * scale);
        for cmd in lib.glyph_outline(g.font, g.glyph) {
            match cmd {
                PathCmd::MoveTo(p) => edges.move_to(to_px(p.x, p.y)),
                PathCmd::LineTo(p) => edges.line_to(to_px(p.x, p.y)),
                PathCmd::QuadTo(c, p) => edges.quad_to(to_px(c.x, c.y), to_px(p.x, p.y)),
            }
        }
        edges.close();
    }
    let (min, max) = edges.bounds()?;
    // Whole-pixel offsets keep the layout's pixel snapping.
    let off_x = PAD - min.0.min(0.0).floor();
    let off_y = PAD - min.1.floor();
    let width = ((max.0.max(0.0) + off_x + PAD).ceil() as usize).clamp(1, MAX_SIDE);
    let height = ((max.1 + off_y + PAD).ceil() as usize).clamp(1, MAX_SIDE);
    let mut canvas = Canvas::new(width, height);
    edges.draw(&mut canvas, (off_x, off_y));
    Some(GlyphBitmap {
        width: canvas.width(),
        height: canvas.height(),
        coverage: canvas.coverage(),
        origin_x: off_x,
        baseline: (baseline_twips / 20.0) as f32 + off_y,
        advance: (layout.width / 20.0) as f32,
    })
}

/// The text with line breaks and tabs turned into spaces, cut to
/// [`MAX_CHARS`] characters.
fn single_line(text: &str) -> String {
    text.chars()
        .take(MAX_CHARS)
        .flat_map(|c| {
            let (c, n) = match c {
                '\t' => (' ', 4),
                '\r' | '\n' | '\u{2028}' | '\u{2029}' => (' ', 1),
                c => (c, 1),
            };
            std::iter::repeat_n(c, n)
        })
        .collect()
}

/// Outline edges in pixels, with subpaths closed.
#[derive(Default)]
struct Edges {
    segments: Vec<Segment>,
    start: Option<(f32, f32)>,
    current: Option<(f32, f32)>,
}

enum Segment {
    Line((f32, f32), (f32, f32)),
    Quad((f32, f32), (f32, f32), (f32, f32)),
}

impl Edges {
    fn move_to(&mut self, p: (f32, f32)) {
        self.close();
        self.start = Some(p);
        self.current = Some(p);
    }

    fn line_to(&mut self, p: (f32, f32)) {
        if let Some(cur) = self.current {
            self.segments.push(Segment::Line(cur, p));
        } else {
            self.start = Some(p);
        }
        self.current = Some(p);
    }

    fn quad_to(&mut self, c: (f32, f32), p: (f32, f32)) {
        if let Some(cur) = self.current {
            self.segments.push(Segment::Quad(cur, c, p));
        } else {
            self.start = Some(p);
        }
        self.current = Some(p);
    }

    fn close(&mut self) {
        if let (Some(start), Some(cur)) = (self.start, self.current)
            && start != cur
        {
            self.segments.push(Segment::Line(cur, start));
        }
        self.start = None;
        self.current = None;
    }

    /// Bounds of every point (control points included), ignoring
    /// non-finite ones; `None` when there is nothing to draw.
    fn bounds(&self) -> Option<((f32, f32), (f32, f32))> {
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut grow = |p: (f32, f32)| {
            if p.0.is_finite() && p.1.is_finite() {
                min = (min.0.min(p.0), min.1.min(p.1));
                max = (max.0.max(p.0), max.1.max(p.1));
            }
        };
        for s in &self.segments {
            match *s {
                Segment::Line(a, b) => {
                    grow(a);
                    grow(b);
                }
                Segment::Quad(a, c, b) => {
                    grow(a);
                    grow(c);
                    grow(b);
                }
            }
        }
        (min.0 <= max.0 && min.1 <= max.1).then_some((min, max))
    }

    fn draw(&self, canvas: &mut Canvas, (dx, dy): (f32, f32)) {
        let t = |p: (f32, f32)| (p.0 + dx, p.1 + dy);
        for s in &self.segments {
            match *s {
                Segment::Line(a, b) => canvas.line(t(a), t(b)),
                Segment::Quad(a, c, b) => canvas.quad(t(a), t(c), t(b)),
            }
        }
    }
}

/// Text alpha from coverage, slightly boosted as egui does for its own
/// text (`2c − c²`), so shaped and egui text look alike.
fn alpha(coverage: u8) -> u8 {
    let c = f32::from(coverage) / 255.0;
    ((2.0 * c - c * c) * 255.0 + 0.5) as u8
}

/// A shaped line ready to paint: a white texture to tint with the colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedImage {
    /// The texture.
    pub texture: egui::TextureId,
    /// Size in points.
    pub size: Vec2,
    /// Distance from the image's left edge to the pen origin, in points.
    pub origin_x: f32,
    /// Distance from the image's top to the baseline, in points.
    pub baseline: f32,
    /// The line's advance width, in points.
    pub advance: f32,
}

struct Entry {
    image: Option<ShapedImage>,
    _texture: Option<TextureHandle>,
    last_used: u64,
}

/// The cache of shaped lines, one per `(text, pixel size)`.
#[derive(Default)]
pub struct ShapedText {
    lib: Option<FontLibrary>,
    entries: HashMap<(String, u32), Entry>,
    frame: u64,
}

impl ShapedText {
    /// The shaped image of `text` at `size_points`, or `None` when nothing
    /// would be drawn.
    pub fn image(&mut self, ctx: &Context, text: &str, size_points: f32) -> Option<ShapedImage> {
        let ppp = ctx.pixels_per_point();
        let size_px = size_points * ppp;
        let key = (text.to_owned(), (size_px * 64.0).round().max(0.0) as u32);
        let frame = self.frame;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = frame;
            return entry.image;
        }
        let lib = self.lib.get_or_insert_with(FontLibrary::embedded_only);
        let (image, texture) = match rasterize_line(lib, text, size_px) {
            Some(b) => {
                let pixels = b
                    .coverage
                    .iter()
                    .map(|&c| {
                        let a = alpha(c);
                        Color32::from_rgba_premultiplied(a, a, a, a)
                    })
                    .collect();
                let handle = ctx.load_texture(
                    "jazari-shaped-text",
                    ColorImage::new([b.width, b.height], pixels),
                    TextureOptions::LINEAR,
                );
                let image = ShapedImage {
                    texture: handle.id(),
                    size: Vec2::new(b.width as f32, b.height as f32) / ppp,
                    origin_x: b.origin_x / ppp,
                    baseline: b.baseline / ppp,
                    advance: b.advance / ppp,
                };
                (Some(image), Some(handle))
            }
            None => (None, None),
        };
        self.entries.insert(
            key,
            Entry {
                image,
                _texture: texture,
                last_used: frame,
            },
        );
        image
    }

    /// Paints `text` with its pen origin at `x` and its baseline at
    /// `baseline_y`, clipped to `clip`. Falls back to egui text when no
    /// shaping is needed. Returns the width used, in points.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        ctx: &Context,
        painter: &egui::Painter,
        x: f32,
        baseline_y: f32,
        text: &str,
        size_points: f32,
        color: Color32,
    ) -> f32 {
        if needs_shaping(text) {
            match self.image(ctx, text, size_points) {
                Some(img) => {
                    let min = Pos2::new(x - img.origin_x, baseline_y - img.baseline);
                    painter.image(
                        img.texture,
                        Rect::from_min_size(min, img.size),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        color,
                    );
                    img.advance.max(img.size.x - img.origin_x)
                }
                None => 0.0,
            }
        } else {
            let font = egui::FontId::proportional(size_points);
            let center_y = baseline_y - size_points * BASELINE_BELOW_CENTER;
            painter
                .text(
                    Pos2::new(x, center_y),
                    egui::Align2::LEFT_CENTER,
                    text,
                    font,
                    color,
                )
                .width()
        }
    }

    /// A label: egui's own for plain text, the shaped image otherwise.
    pub fn label(&mut self, ui: &mut Ui, text: &str, color: Option<Color32>) -> Response {
        if !needs_shaping(text) {
            let rich = egui::RichText::new(text);
            let rich = match color {
                Some(c) => rich.color(c),
                None => rich,
            };
            return ui.add(egui::Label::new(rich).selectable(false));
        }
        let size = ui
            .style()
            .text_styles
            .get(&egui::TextStyle::Body)
            .map_or(14.0, |f| f.size);
        let color = color.unwrap_or_else(|| ui.visuals().text_color());
        self.label_sized(ui, text, size, color)
    }

    /// A button whose text is shaped when it needs to be.
    pub fn button(&mut self, ui: &mut Ui, text: &str, enabled: bool) -> Response {
        if needs_shaping(text) {
            let size = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Body)
                .map_or(14.0, |f| f.size);
            let ctx = ui.ctx().clone();
            if let Some(img) = self.image(&ctx, text, size) {
                let color = ui.visuals().text_color();
                let image =
                    egui::Image::from_texture(egui::load::SizedTexture::new(img.texture, img.size))
                        .tint(color);
                return ui.add_enabled(enabled, egui::Button::image(image));
            }
        }
        ui.add_enabled(enabled, egui::Button::new(text))
    }

    /// A shaped label in a row of the current text height.
    pub fn label_sized(&mut self, ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
        let row = ui.text_style_height(&egui::TextStyle::Body).max(size * 1.3);
        let ctx = ui.ctx().clone();
        let width = self
            .image(&ctx, text, size)
            .map_or(0.0, |img| img.advance.max(img.size.x - img.origin_x));
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row), Sense::hover());
        if ui.is_rect_visible(rect) {
            let baseline = rect.center().y + size * BASELINE_BELOW_CENTER;
            let painter = ui.painter_at(rect.expand(row));
            self.paint(&ctx, &painter, rect.left(), baseline, text, size, color);
        }
        response
    }

    /// Call once per frame: drops long-unused entries when the cache is
    /// large.
    pub fn end_frame(&mut self) {
        self.frame += 1;
        if self.entries.len() > CACHE_SOFT_LIMIT {
            let frame = self.frame;
            self.entries
                .retain(|_, e| frame.saturating_sub(e.last_used) < CACHE_IDLE_FRAMES);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> FontLibrary {
        FontLibrary::embedded_only()
    }

    /// Columns that hold any ink.
    fn inked_columns(b: &GlyphBitmap) -> Vec<bool> {
        (0..b.width)
            .map(|x| (0..b.height).any(|y| b.coverage[y * b.width + x] > 40))
            .collect()
    }

    #[test]
    fn right_to_left_detection() {
        assert!(needs_shaping("مرحبا"));
        assert!(needs_shaping("layer طبقة 2"));
        assert!(needs_shaping("שלום"));
        assert!(needs_shaping("a\u{200F}b"));
        assert!(!needs_shaping("Scene 1"));
        assert!(!needs_shaping("café ñ 你好"));
        assert!(!needs_shaping(""));
    }

    #[test]
    fn latin_text_rasterises_with_ink_and_a_baseline() {
        let b = rasterize_line(&lib(), "Hello", 20.0).expect("bitmap");
        assert_eq!(b.coverage.len(), b.width * b.height);
        assert!(b.coverage.contains(&255), "solid stems");
        assert!(b.width > b.height);
        assert!(b.baseline > 10.0 && b.baseline < b.height as f32);
        // Nothing below the baseline for these letters.
        let base = b.baseline.ceil() as usize + 1;
        let below: u32 = (base..b.height)
            .flat_map(|y| (0..b.width).map(move |x| (x, y)))
            .map(|(x, y)| u32::from(b.coverage[y * b.width + x]))
            .sum();
        assert_eq!(below, 0);
    }

    #[test]
    fn arabic_letters_are_joined() {
        // Five behs joined by their connecting strokes: the ink is one
        // continuous run of columns. Isolated forms (separated by ZWNJ)
        // are wider than the joined word.
        let joined = rasterize_line(&lib(), "ببببب", 24.0).expect("joined");
        let cols = inked_columns(&joined);
        let first = cols.iter().position(|&c| c).expect("ink");
        let last = cols.iter().rposition(|&c| c).expect("ink");
        assert!(cols[first..=last].iter().all(|&c| c), "{cols:?}");
        let isolated = rasterize_line(&lib(), "ب\u{200C}ب\u{200C}ب\u{200C}ب\u{200C}ب", 24.0)
            .expect("isolated");
        assert!(isolated.advance > joined.advance * 1.2);
    }

    #[test]
    fn right_to_left_order_puts_the_first_letter_on_the_right() {
        // "ا" + "ـــ" style test: alef alone is a tall narrow stroke; in
        // "اب" read right to left the alef is the rightmost glyph, so the
        // tallest ink is in the right half.
        let b = rasterize_line(&lib(), "اب", 32.0).expect("bitmap");
        let top_ink_x: Vec<usize> = (0..b.width)
            .filter(|&x| {
                (0..(b.baseline as usize).saturating_sub(14))
                    .any(|y| b.coverage[y * b.width + x] > 128)
            })
            .collect();
        assert!(!top_ink_x.is_empty());
        let mean = top_ink_x.iter().sum::<usize>() as f32 / top_ink_x.len() as f32;
        assert!(
            mean > b.width as f32 / 2.0,
            "alef at x {mean}, width {}",
            b.width
        );
    }

    #[test]
    fn a_leading_lrm_makes_the_line_left_to_right_without_ink() {
        let lib = lib();
        // The mark itself draws nothing and takes no room.
        let plain = rasterize_line(&lib, "ab", 24.0).expect("plain");
        let marked = rasterize_line(&lib, "\u{200E}ab", 24.0).expect("marked");
        assert_eq!(plain.coverage, marked.coverage);
        assert!((plain.advance - marked.advance).abs() < 0.01);
        // "// ببب" is right-to-left by its first strong letter, so the
        // slashes end up on the right; after the mark they stay on the left,
        // where a code line starts. Slashes are taller than the behs, so
        // the ink high above the baseline locates them.
        let tall_mean = |b: &GlyphBitmap| {
            let top = (b.baseline as usize).saturating_sub(12);
            let xs: Vec<usize> = (0..b.width)
                .filter(|&x| (0..top).any(|y| b.coverage[y * b.width + x] > 128))
                .collect();
            xs.iter().sum::<usize>() as f32 / xs.len().max(1) as f32 / b.width as f32
        };
        let natural = rasterize_line(&lib, "// ببب", 24.0).expect("natural");
        let code = rasterize_line(&lib, "\u{200E}// ببب", 24.0).expect("code");
        assert!(
            tall_mean(&natural) > 0.5,
            "slashes on the right: {}",
            tall_mean(&natural)
        );
        assert!(
            tall_mean(&code) < 0.5,
            "slashes on the left: {}",
            tall_mean(&code)
        );
    }

    #[test]
    fn blank_and_bad_input() {
        assert_eq!(rasterize_line(&lib(), "", 14.0), None);
        assert_eq!(rasterize_line(&lib(), " \t\n", 14.0), None);
        assert_eq!(rasterize_line(&lib(), "abc", f32::NAN), None);
        assert_eq!(rasterize_line(&lib(), "abc", 0.0), None);
        assert_eq!(rasterize_line(&lib(), "abc", 1e9), None);
        let long = "مرحبا ".repeat(2000);
        let b = rasterize_line(&lib(), &long, 14.0).expect("long line");
        assert!(b.width <= MAX_SIDE);
        assert_eq!(single_line("a\tb\nc"), "a    b c");
    }

    #[test]
    fn alpha_curve_is_monotonic_and_keeps_the_ends() {
        assert_eq!(alpha(0), 0);
        assert_eq!(alpha(255), 255);
        let mut last = 0;
        for c in 0..=255u8 {
            let a = alpha(c);
            assert!(a >= last && a >= c);
            last = a;
        }
    }
}
