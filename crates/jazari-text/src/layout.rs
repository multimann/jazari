//! Laying out a text block: paragraphs, bidi, shaping, wrapping, alignment.
//!
//! The coordinate system and the layout rules are documented on the crate
//! (`lib.rs`), where they are public.

use std::collections::HashMap;
use std::ops::Range;

use jazari_xfl::model::{Align, LineType, TextAttrs, TextElement, TextKind, TextRun};
use unicode_bidi::{Level, ParagraphBidiInfo};

use crate::itemize::{self, Item, RunLookup, Views};
use crate::library::{FaceChain, FontId, FontLibrary};
use crate::shaping::{self, FaceView, Segment, ShapeStyle};
use crate::wrap::{self, Advances};

/// Twips per pixel (and per point).
pub const TWIPS_PER_PX: f64 = 20.0;

/// A block of text to lay out: the runs of one text element and its box.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextBlock {
    /// Runs in logical order. A CR (`\r`, also `\n`, `\r\n` or U+2029) ends
    /// a paragraph; U+2028 breaks a line inside one.
    pub runs: Vec<TextRun>,
    /// Box width in pixels: the wrapping width for wrapping text, the
    /// alignment width otherwise.
    pub width: f64,
    /// The box grows to fit the text instead of wrapping (a "label").
    pub auto_expand: bool,
    /// Line type of the text field. Only [`LineType::Multiline`] wraps; use
    /// [`TextBlock::from_element`] to get the right value for static text.
    pub line_type: LineType,
}

impl TextBlock {
    /// The block of a text element. Static text has no `lineType`: it wraps
    /// when its width is fixed (`autoExpand="false"`) and is a single-line
    /// label otherwise.
    pub fn from_element(el: &TextElement) -> Self {
        let line_type = match el.kind {
            TextKind::Static if el.auto_expand => LineType::SingleLine,
            TextKind::Static => LineType::Multiline,
            TextKind::Dynamic | TextKind::Input => el.line_type,
        };
        Self {
            runs: el.runs.clone(),
            width: el.width,
            auto_expand: el.auto_expand,
            line_type,
        }
    }

    /// Whether lines wrap at the box width.
    pub fn wraps(&self) -> bool {
        !self.auto_expand && self.width > 0.0 && self.line_type == LineType::Multiline
    }

    /// The text of all runs, concatenated: the string that
    /// [`PositionedGlyph::cluster`] and [`LayoutLine::text`] index.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// The result of [`layout`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLayout {
    /// Lines from top to bottom. An empty paragraph is an empty line.
    pub lines: Vec<LayoutLine>,
    /// Width of the box the lines were aligned in, in twips: the block's
    /// width, or more for an auto-expanding block whose text is wider.
    pub width: f64,
    /// Height of the text in twips: the last baseline plus its descent.
    pub height: f64,
}

impl TextLayout {
    /// Every glyph, line by line, in visual order.
    pub fn glyphs(&self) -> impl Iterator<Item = &PositionedGlyph> {
        self.lines.iter().flat_map(|l| l.glyphs.iter())
    }
}

/// One laid-out line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutLine {
    /// Glyphs in visual order (left to right on screen).
    pub glyphs: Vec<PositionedGlyph>,
    /// Baseline y in twips from the top of the box, on a whole pixel.
    pub baseline: f64,
    /// Left edge of the line's content in twips, on a whole pixel. Trailing
    /// spaces of a right-to-left line sit to the left of it.
    pub x: f64,
    /// Advance width of the content in twips, without trailing spaces.
    pub width: f64,
    /// Ascent of the line in twips (positive).
    pub ascent: f64,
    /// Descent of the line in twips (positive).
    pub descent: f64,
    /// The paragraph is right-to-left.
    pub rtl: bool,
    /// Bytes of [`TextBlock::text`] on this line (separators excluded).
    pub text: Range<usize>,
}

/// A glyph placed in the text box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// The face.
    pub font: FontId,
    /// Glyph id in the face.
    pub glyph: u16,
    /// Drawing x in twips from the box's left edge: the pen position plus
    /// the shaper's x offset.
    pub x: f64,
    /// Drawing y in twips from the box's top: the line's baseline minus the
    /// shaper's y offset (raised marks have a smaller y).
    pub y: f64,
    /// Pen advance in twips, letter spacing and justification included.
    /// The next glyph's pen position is this one's plus `advance`.
    pub advance: f64,
    /// Em size in twips (`size × 20`): the DefineText record height.
    pub size: f64,
    /// Character code for the DefineFont3 code table: the cmap code, else the
    /// Arabic Presentation Form, else a Private Use code. Unique per font.
    pub code: u16,
    /// Index into [`TextBlock::runs`] of the run this glyph belongs to (for
    /// its colour and link).
    pub run: usize,
    /// Byte offset in [`TextBlock::text`] of the first character of the
    /// glyph's cluster (a ligature or a base with its marks is one cluster).
    pub cluster: usize,
}

/// Lays out a text block: shaping, bidi reordering, wrapping, alignment,
/// indents, margins, line and letter spacing. The crate documentation
/// describes the coordinate system and the rules. Never fails: missing faces
/// and glyphs fall back, and unusable sizes become 12 pt.
pub fn layout(lib: &FontLibrary, block: &TextBlock) -> TextLayout {
    let table = RunTable::new(lib, block);
    if table.runs.is_empty() {
        return TextLayout::default();
    }
    let text = block.text();
    let views = prepare_views(lib, &table);
    let default_level = match unicode_bidi::get_base_direction_full(text.as_str()) {
        unicode_bidi::Direction::Rtl => Level::rtl(),
        _ => Level::ltr(),
    };
    let box_width = (block.width * TWIPS_PER_PX).max(0.0);
    let wrap_width = block.wraps().then_some(box_width);
    let mut lines = Vec::new();
    for para in split_paragraphs(&text) {
        let ctx = ParaCtx {
            text: &text,
            range: para,
            table: &table,
            views: &views,
        };
        ctx.prepare(default_level, wrap_width, &mut lines);
    }
    let width = if wrap_width.is_some() || (!block.auto_expand && box_width > 0.0) {
        box_width
    } else {
        lines
            .iter()
            .map(PreparedLine::needed_width)
            .fold(box_width, f64::max)
    };
    position(lib, lines, width)
}

/// Rounds twips to a whole pixel.
fn snap_px(twips: f64) -> f64 {
    (twips / TWIPS_PER_PX).round() * TWIPS_PER_PX
}

/// A finite value or a default.
fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_finite() { v } else { default }
}

/// Paragraph separators (bidi class B).
fn is_paragraph_separator(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{1C}'..='\u{1E}' | '\u{85}' | '\u{2029}')
}

/// Paragraph byte ranges, separators excluded. `\r\n` is one separator;
/// a trailing separator leaves an empty last paragraph.
fn split_paragraphs(text: &str) -> Vec<Range<usize>> {
    let mut paras = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if is_paragraph_separator(c) {
            paras.push(start..i);
            start = i + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
                chars.next();
                start += 1;
            }
        }
    }
    paras.push(start..text.len());
    paras
}

/// What layout needs from each run, with sizes in twips.
struct RunInfo<'a> {
    start: usize,
    attrs: &'a TextAttrs,
    chain: FaceChain,
    /// The resolved face (with the style fallback), for line metrics.
    metrics_font: FontId,
    size: f64,
    letter_spacing: f64,
    style: ShapeStyle,
}

struct RunTable<'a> {
    runs: Vec<RunInfo<'a>>,
}

impl<'a> RunTable<'a> {
    fn new(lib: &FontLibrary, block: &'a TextBlock) -> Self {
        let mut chains: HashMap<&str, (FaceChain, FontId)> = HashMap::new();
        let mut start = 0;
        let runs = block
            .runs
            .iter()
            .map(|run| {
                let face = run.attrs.face.as_str();
                let (chain, metrics_font) = *chains
                    .entry(face)
                    .or_insert_with(|| (lib.chain(face), lib.resolve(face)));
                let size = finite_or(run.attrs.size, 12.0);
                let size = if size > 0.0 { size } else { 12.0 } * TWIPS_PER_PX;
                let letter_spacing = finite_or(run.attrs.letter_spacing, 0.0) * TWIPS_PER_PX;
                let info = RunInfo {
                    start,
                    attrs: &run.attrs,
                    chain,
                    metrics_font,
                    size,
                    letter_spacing,
                    style: ShapeStyle {
                        kern: run.attrs.auto_kern,
                        spaced: letter_spacing != 0.0,
                    },
                };
                start += run.text.len();
                info
            })
            .collect();
        Self { runs }
    }

    fn info(&self, run: usize) -> &RunInfo<'a> {
        // `run` always comes from `run_at`, which clamps; stay total anyway.
        &self.runs[run.min(self.runs.len().saturating_sub(1))]
    }
}

impl RunLookup for RunTable<'_> {
    fn run_at(&self, abs: usize) -> usize {
        self.runs
            .partition_point(|r| r.start <= abs)
            .saturating_sub(1)
    }

    fn chain(&self, run: usize) -> &FaceChain {
        &self.info(run).chain
    }

    fn style(&self, run: usize) -> ShapeStyle {
        self.info(run).style
    }
}

/// Prepares every face a run may use.
fn prepare_views<'l>(lib: &'l FontLibrary, table: &RunTable<'_>) -> Views<'l> {
    let mut views = Views::new();
    for run in &table.runs {
        let c = run.chain;
        let ids = c
            .primary
            .into_iter()
            .chain(c.arabic)
            .chain([c.latin, run.metrics_font]);
        for id in ids {
            if !views.contains_key(&id)
                && let Some(view) = FaceView::new(lib, id)
            {
                views.insert(id, view);
            }
        }
    }
    views
}

/// Paragraph attributes in twips.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ParaAttrs {
    align: Align,
    indent: f64,
    left_margin: f64,
    right_margin: f64,
    line_spacing: f64,
}

impl ParaAttrs {
    fn new(a: &TextAttrs) -> Self {
        Self {
            align: a.align,
            indent: finite_or(a.indent, 0.0) * TWIPS_PER_PX,
            left_margin: finite_or(a.left_margin, 0.0) * TWIPS_PER_PX,
            right_margin: finite_or(a.right_margin, 0.0) * TWIPS_PER_PX,
            line_spacing: finite_or(a.line_spacing, 0.0) * TWIPS_PER_PX,
        }
    }
}

/// A shaped glyph of a line, before positioning (twips, y up offsets).
#[derive(Debug, Clone, Copy)]
struct LineGlyph {
    font: FontId,
    glyph: u16,
    /// Paragraph byte offset of the cluster.
    cluster: usize,
    run: usize,
    size: f64,
    advance: f64,
    x_offset: f64,
    y_offset: f64,
    /// The cluster is a breaking space (stretched by justification).
    space: bool,
    /// The cluster is in the line's trailing whitespace.
    trailing: bool,
}

/// A line measured but not yet positioned.
struct PreparedLine {
    glyphs: Vec<LineGlyph>,
    width: f64,
    ascent: f64,
    descent: f64,
    rtl: bool,
    first: bool,
    hard: bool,
    attrs: ParaAttrs,
    /// Byte offset of the paragraph in the block text.
    para_start: usize,
    text: Range<usize>,
}

impl PreparedLine {
    /// The box width this line needs, margins and indent included.
    fn needed_width(&self) -> f64 {
        let indent = if self.first { self.attrs.indent } else { 0.0 };
        self.width + self.attrs.left_margin + self.attrs.right_margin + indent
    }
}

/// One paragraph being laid out.
struct ParaCtx<'a, 'l> {
    /// The block text.
    text: &'a str,
    /// The paragraph's bytes in `text`.
    range: Range<usize>,
    table: &'a RunTable<'a>,
    views: &'a Views<'l>,
}

impl ParaCtx<'_, '_> {
    fn para(&self) -> &str {
        self.text.get(self.range.clone()).unwrap_or("")
    }

    /// Itemizes, measures, wraps and shapes the paragraph's lines.
    fn prepare(&self, default_level: Level, wrap_width: Option<f64>, out: &mut Vec<PreparedLine>) {
        let para = self.para();
        let run = self.table.run_at(self.range.start);
        let attrs = ParaAttrs::new(self.table.info(run).attrs);
        let level = match unicode_bidi::get_base_direction(para) {
            unicode_bidi::Direction::Ltr => Level::ltr(),
            unicode_bidi::Direction::Rtl => Level::rtl(),
            unicode_bidi::Direction::Mixed => default_level,
        };
        let bidi = ParagraphBidiInfo::new(para, Some(level));
        let rtl = bidi.paragraph_level.is_rtl();
        let items = itemize::itemize(para, self.range.start, &bidi.levels, self.table, self.views);

        // Pass 1: logical advances per cluster, for fitting.
        let mut per_byte = vec![0.0; para.len()];
        let mut starts = vec![false; para.len()];
        for it in &items {
            for g in self.shape(it, it.range.clone(), it.level.is_rtl()) {
                if let Some(a) = per_byte.get_mut(g.cluster) {
                    *a += g.advance;
                    starts[g.cluster] = true;
                }
            }
        }
        let adv = Advances::new(&per_byte, &starts);
        let avail = |k: usize| {
            wrap_width.map(|w| {
                let indent = if k == 0 { attrs.indent } else { 0.0 };
                (w - attrs.left_margin - attrs.right_margin - indent).max(0.0)
            })
        };

        // Pass 2: reshape each line in visual order.
        for (k, span) in wrap::break_lines(para, &adv, avail).into_iter().enumerate() {
            let mut glyphs = self.visual_line(&bidi, &span.range, &items);
            let trim = wrap::trim_end(para, span.range.start, span.range.end);
            for g in &mut glyphs {
                g.trailing = g.cluster >= trim;
            }
            let width = glyphs
                .iter()
                .filter(|g| !g.trailing)
                .map(|g| g.advance)
                .sum();
            let (ascent, descent) = self.line_metrics(&glyphs, self.range.start + span.range.start);
            out.push(PreparedLine {
                glyphs,
                width,
                ascent,
                descent,
                rtl,
                first: k == 0,
                hard: span.hard,
                attrs,
                para_start: self.range.start,
                text: self.range.start + span.range.start..self.range.start + span.range.end,
            });
        }
    }

    /// Shapes `range` of an item in one direction; applies letter spacing.
    fn shape(&self, item: &Item, range: Range<usize>, rtl: bool) -> Vec<LineGlyph> {
        let Some(view) = self.views.get(&item.font) else {
            return Vec::new();
        };
        let para = self.para();
        let seg = Segment {
            para,
            range,
            rtl,
            script: item.script,
            style: item.style,
        };
        let mut out: Vec<LineGlyph> = shaping::shape(view, &seg)
            .into_iter()
            .map(|g| {
                let run = self.table.run_at(self.range.start + g.cluster);
                let size = self.table.info(run).size;
                let k = size / view.upem;
                let space = para
                    .get(g.cluster..)
                    .and_then(|s| s.chars().next())
                    .is_some_and(wrap::is_breaking_space);
                LineGlyph {
                    font: item.font,
                    glyph: g.glyph,
                    cluster: g.cluster,
                    run,
                    size,
                    // Whole twips, as Flash writes them: line widths, and
                    // so alignment, then match Flash's (`text` fixture).
                    advance: (g.x_advance * k).round(),
                    x_offset: g.x_offset * k,
                    y_offset: g.y_offset * k,
                    space,
                    trailing: false,
                }
            })
            .collect();
        if !item.cursive {
            // Glyphs of one cluster are adjacent; space after the last one.
            for i in 0..out.len() {
                if out.get(i + 1).is_none_or(|n| n.cluster != out[i].cluster) {
                    out[i].advance += self.table.info(out[i].run).letter_spacing;
                }
            }
        }
        out
    }

    /// Shapes one line in visual order: bidi level runs left to right, and
    /// inside a right-to-left run, its items from the logical end.
    fn visual_line(
        &self,
        bidi: &ParagraphBidiInfo<'_>,
        line: &Range<usize>,
        items: &[Item],
    ) -> Vec<LineGlyph> {
        if line.is_empty() {
            return Vec::new();
        }
        // Levels after rule L1 (trailing whitespace back to the paragraph
        // level); items were cut on the levels before it, so cut them again.
        let (levels, runs) = bidi.visual_runs(line.clone());
        let mut out = Vec::new();
        for vr in runs {
            let rtl = levels.get(vr.start).is_some_and(|l| l.is_rtl());
            let mut parts: Vec<(Range<usize>, &Item)> = items
                .iter()
                .filter_map(|it| {
                    let start = it.range.start.max(vr.start);
                    let end = it.range.end.min(vr.end);
                    (start < end).then_some((start..end, it))
                })
                .collect();
            if rtl {
                parts.reverse();
            }
            for (range, it) in parts {
                out.extend(self.shape(it, range, rtl));
            }
        }
        out
    }

    /// Largest ascent and descent of the runs on a line (twips). An empty
    /// line uses the run at its start.
    fn line_metrics(&self, glyphs: &[LineGlyph], abs_start: usize) -> (f64, f64) {
        let mut runs: Vec<usize> = glyphs.iter().map(|g| g.run).collect();
        runs.sort_unstable();
        runs.dedup();
        if runs.is_empty() {
            runs.push(self.table.run_at(abs_start));
        }
        runs.iter().fold((0.0, 0.0), |(asc, desc), &r| {
            let info = self.table.info(r);
            let m = self
                .views
                .get(&info.metrics_font)
                .map(|v| v.face.metrics())
                .unwrap_or_default();
            (
                f64::max(asc, m.ascent * info.size),
                f64::max(desc, m.descent * info.size),
            )
        })
    }
}

/// Aligns the lines in a box of `width` twips and stacks them.
fn position(lib: &FontLibrary, lines: Vec<PreparedLine>, width: f64) -> TextLayout {
    let mut out = Vec::with_capacity(lines.len());
    // (baseline, descent, line spacing) of the previous line.
    let mut prev: Option<(f64, f64, f64)> = None;
    for mut line in lines {
        let a = line.attrs;
        let indent_left = if line.first && !line.rtl {
            a.indent
        } else {
            0.0
        };
        let indent_right = if line.first && line.rtl {
            a.indent
        } else {
            0.0
        };
        let start = a.left_margin + indent_left;
        let end = width - a.right_margin - indent_right;
        let area = end - start;

        let mut content = line.width;
        let mut stretched = false;
        if a.align == Align::Justify && !line.hard {
            let spaces = line
                .glyphs
                .iter()
                .filter(|g| g.space && !g.trailing)
                .count();
            if spaces > 0 && area > content {
                let extra = (area - content) / spaces as f64;
                for g in line.glyphs.iter_mut().filter(|g| g.space && !g.trailing) {
                    g.advance += extra;
                }
                content = area;
                stretched = true;
            }
        }
        let x0 = match a.align {
            Align::Left => start,
            Align::Right => end - content,
            Align::Center => start + (area - content) / 2.0,
            Align::Justify if line.rtl && !stretched => end - content,
            Align::Justify => start,
        };
        let x0 = snap_px(x0);
        let baseline = snap_px(match prev {
            None => line.ascent,
            Some((b, d, spacing)) => b + d + spacing + line.ascent,
        });
        // Trailing spaces that come first visually (right-to-left lines) go
        // to the left of the content.
        let ws_before: f64 = if line.glyphs.iter().any(|g| !g.trailing) {
            line.glyphs
                .iter()
                .take_while(|g| g.trailing)
                .map(|g| g.advance)
                .sum()
        } else {
            0.0
        };
        let mut pen = x0 - ws_before;
        let glyphs = line
            .glyphs
            .iter()
            .map(|g| {
                let p = PositionedGlyph {
                    font: g.font,
                    glyph: g.glyph,
                    x: pen + g.x_offset,
                    y: baseline - g.y_offset,
                    advance: g.advance,
                    size: g.size,
                    code: lib.glyph_code(g.font, g.glyph),
                    run: g.run,
                    cluster: line.para_start + g.cluster,
                };
                pen += g.advance;
                p
            })
            .collect();
        prev = Some((baseline, line.descent, a.line_spacing));
        out.push(LayoutLine {
            glyphs,
            baseline,
            x: x0,
            width: content,
            ascent: line.ascent,
            descent: line.descent,
            rtl: line.rtl,
            text: line.text,
        });
    }
    let height = prev.map_or(0.0, |(b, d, _)| b + d);
    TextLayout {
        lines: out,
        width,
        height,
    }
}
