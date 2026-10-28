//! Greedy line breaking on logical advances.

use std::ops::Range;

use unicode_linebreak::{BreakOpportunity, linebreaks};

/// Width comparisons allow this much slack (twips) for rounding noise.
const EPSILON: f64 = 1e-6;

/// One line of a paragraph, as a byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineSpan {
    pub range: Range<usize>,
    /// The line ends at a mandatory break or at the end of the paragraph
    /// (not because the next word did not fit). Justified text does not
    /// stretch such lines.
    pub hard: bool,
}

/// Logical advances of a paragraph, for fitting.
pub(crate) struct Advances {
    /// `prefix[i]` = total advance of the clusters starting before byte `i`.
    prefix: Vec<f64>,
    /// Byte offsets where clusters start, ascending.
    cluster_starts: Vec<usize>,
}

impl Advances {
    /// `per_byte[i]` is the advance of the cluster starting at byte `i`
    /// (zero inside clusters); `starts[i]` marks cluster starts.
    pub fn new(per_byte: &[f64], starts: &[bool]) -> Self {
        let mut prefix = Vec::with_capacity(per_byte.len() + 1);
        let mut total = 0.0;
        prefix.push(0.0);
        for a in per_byte {
            total += a;
            prefix.push(total);
        }
        let cluster_starts = starts
            .iter()
            .enumerate()
            .filter(|(_, s)| **s)
            .map(|(i, _)| i)
            .collect();
        Self {
            prefix,
            cluster_starts,
        }
    }

    /// Total advance of the clusters starting in `start..end`.
    pub fn width(&self, start: usize, end: usize) -> f64 {
        let at = |i: usize| {
            self.prefix
                .get(i)
                .or(self.prefix.last())
                .copied()
                .unwrap_or(0.0)
        };
        at(end) - at(start)
    }
}

/// Whether a character is whitespace that hangs at the end of a line
/// (no-break spaces are part of words).
pub(crate) fn is_breaking_space(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{00A0}' | '\u{2007}' | '\u{202F}')
}

/// The end of `text[start..end]` without its trailing breaking whitespace.
pub(crate) fn trim_end(text: &str, start: usize, end: usize) -> usize {
    let Some(slice) = text.get(start..end) else {
        return end;
    };
    start + slice.trim_end_matches(is_breaking_space).len()
}

/// Breaks a paragraph into lines.
///
/// `avail(k)` is the width available to line `k` (it may differ for the
/// first line because of the indent), or `None` for no wrapping: then lines
/// end only at mandatory breaks. Lines are filled greedily with whole
/// segments between break opportunities; trailing spaces do not count
/// toward the fit. A segment wider than a whole line is broken between
/// clusters, keeping at least one cluster per line.
pub(crate) fn break_lines(
    text: &str,
    adv: &Advances,
    avail: impl Fn(usize) -> Option<f64>,
) -> Vec<LineSpan> {
    let mut lines = Vec::new();
    if text.is_empty() {
        lines.push(LineSpan {
            range: 0..0,
            hard: true,
        });
        return lines;
    }
    let mut line_start = 0;
    // Width of the current line up to `seg_start`, trailing spaces included.
    let mut line_w = 0.0;
    let mut seg_start = 0;
    for (pos, kind) in linebreaks(text) {
        let seg_trim = trim_end(text, seg_start, pos);
        let w_trim = adv.width(seg_start, seg_trim);
        if let Some(max) = avail(lines.len()) {
            if line_start < seg_start && line_w + w_trim > max + EPSILON {
                lines.push(LineSpan {
                    range: line_start..seg_start,
                    hard: false,
                });
                line_start = seg_start;
                line_w = 0.0;
            }
            if line_start == seg_start && w_trim > avail(lines.len()).unwrap_or(max) + EPSILON {
                line_start = break_inside(adv, seg_start, seg_trim, &avail, &mut lines);
                // The pushed lines took the head of the segment; its whole
                // width is added below, so subtract the head here.
                line_w = -adv.width(seg_start, line_start);
            }
        }
        line_w += adv.width(seg_start, pos);
        seg_start = pos;
        if kind == BreakOpportunity::Mandatory && pos < text.len() {
            lines.push(LineSpan {
                range: line_start..pos,
                hard: true,
            });
            line_start = pos;
            line_w = 0.0;
        }
    }
    lines.push(LineSpan {
        range: line_start..text.len(),
        hard: true,
    });
    lines
}

/// Emergency breaks inside an overlong segment `start..end`, between
/// clusters. Pushes the full lines and returns where the last, partial line
/// starts.
fn break_inside(
    adv: &Advances,
    start: usize,
    end: usize,
    avail: &impl Fn(usize) -> Option<f64>,
    lines: &mut Vec<LineSpan>,
) -> usize {
    let mut piece_start = start;
    let mut piece_w = 0.0;
    let clusters: Vec<usize> = adv
        .cluster_starts
        .iter()
        .copied()
        .filter(|&b| b >= start && b < end)
        .collect();
    for (k, &b) in clusters.iter().enumerate() {
        let next = clusters.get(k + 1).copied().unwrap_or(end);
        let w = adv.width(b, next);
        let max = avail(lines.len()).unwrap_or(f64::INFINITY);
        if piece_start < b && piece_w + w > max + EPSILON {
            lines.push(LineSpan {
                range: piece_start..b,
                hard: false,
            });
            piece_start = b;
            piece_w = 0.0;
        }
        piece_w += w;
    }
    piece_start
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit of advance per character, every character a cluster.
    fn unit_advances(text: &str) -> Advances {
        let mut per_byte = vec![0.0; text.len()];
        let mut starts = vec![false; text.len()];
        for (i, _) in text.char_indices() {
            per_byte[i] = 1.0;
            starts[i] = true;
        }
        Advances::new(&per_byte, &starts)
    }

    fn texts<'a>(text: &'a str, lines: &[LineSpan]) -> Vec<&'a str> {
        lines.iter().map(|l| &text[l.range.clone()]).collect()
    }

    #[test]
    fn greedy_fill_ignores_trailing_spaces() {
        let t = "aaa bbb ccc";
        let lines = break_lines(t, &unit_advances(t), |_| Some(7.0));
        assert_eq!(texts(t, &lines), ["aaa bbb ", "ccc"]);
        assert!(!lines[0].hard && lines[1].hard);
    }

    #[test]
    fn no_wrap_breaks_only_at_mandatory_breaks() {
        let t = "aaa bbb\u{2028}ccc";
        let lines = break_lines(t, &unit_advances(t), |_| None);
        assert_eq!(texts(t, &lines), ["aaa bbb\u{2028}", "ccc"]);
    }

    #[test]
    fn overlong_words_break_between_clusters() {
        let t = "abcdefgh ij";
        let lines = break_lines(t, &unit_advances(t), |_| Some(3.0));
        assert_eq!(texts(t, &lines), ["abc", "def", "gh ", "ij"]);
    }

    #[test]
    fn first_line_may_be_narrower() {
        let t = "aa bb cc";
        let lines = break_lines(t, &unit_advances(t), |k| {
            Some(if k == 0 { 2.0 } else { 5.0 })
        });
        assert_eq!(texts(t, &lines), ["aa ", "bb cc"]);
        assert_eq!(
            break_lines("", &unit_advances(""), |_| None),
            [LineSpan {
                range: 0..0,
                hard: true
            }]
        );
    }
}
