//! `trace()` sequences: collecting them from Ruffle, reading Flash 8's
//! captured Output panel, comparing and diffing them.
//!
//! **Timing-only differences.** `flash8-trace.txt` is Flash 8's Test Movie
//! output, captured after the movie had played for some seconds. Ruffle is
//! run for a fixed number of frames instead. When our movie (in Ruffle)
//! prints every line of the Flash 8 log in order and then more lines, the
//! extra lines are a timing difference (a looping movie that played longer
//! than the capture) and are ignored. Nothing else is: the comparison with
//! Flash 8's own SWF played in Ruffle for the same number of frames must
//! match line for line.

use std::path::Path;

use jazari_preview::{LogKind, LogLine};

use crate::error::{CliError, Result};

/// The `trace()` output in `log`, one entry per line: a trace of a string
/// with line breaks becomes several lines, as in Flash's Output panel.
pub fn trace_lines(log: &[LogLine]) -> Vec<String> {
    log.iter()
        .filter(|l| l.kind == LogKind::Trace)
        .flat_map(|l| split_lines(&l.text))
        .collect()
}

/// The other log lines (AVM warnings, Ruffle warnings, preview notices),
/// prefixed with their kind.
pub fn other_log_lines(log: &[LogLine]) -> Vec<String> {
    log.iter()
        .filter(|l| l.kind != LogKind::Trace)
        .map(|l| format!("{:?}: {}", l.kind, l.text))
        .collect()
}

/// Splits on CR LF, CR or LF.
fn split_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// Reads a captured Output panel (`flash8-trace.txt`): UTF-8 with or
/// without a byte order mark, any line ending, trailing blank lines
/// dropped. `Ok(None)` when the file does not exist.
pub fn read_expected(path: &Path) -> Result<Option<Vec<String>>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CliError::io(path, "reading", e)),
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = split_lines(text);
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    Ok(Some(lines))
}

/// How a trace sequence compares with Flash 8's captured log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogMatch {
    /// Line for line.
    Exact,
    /// Every expected line, in order, then `extra` more lines: the movie
    /// played longer than the capture (a timing-only difference).
    Prefix {
        /// Lines after the end of the captured log.
        extra: usize,
    },
    /// The sequences differ at the 0-based line `at`.
    Differs {
        /// First differing line.
        at: usize,
    },
}

impl LogMatch {
    /// Compares `actual` with the `expected` log.
    pub fn of(actual: &[String], expected: &[String]) -> Self {
        let common = actual
            .iter()
            .zip(expected)
            .take_while(|(a, e)| a == e)
            .count();
        if common == expected.len() {
            if actual.len() == expected.len() {
                LogMatch::Exact
            } else {
                LogMatch::Prefix {
                    extra: actual.len() - expected.len(),
                }
            }
        } else {
            LogMatch::Differs { at: common }
        }
    }

    /// Whether this counts as a match (exact, or only a timing difference).
    pub fn is_match(&self) -> bool {
        !matches!(self, LogMatch::Differs { .. })
    }

    /// A short description.
    pub fn describe(&self) -> String {
        match self {
            LogMatch::Exact => "matches line for line".to_owned(),
            LogMatch::Prefix { extra } => format!(
                "matches; {extra} more line{} after the captured log (longer run, ignored)",
                if *extra == 1 { "" } else { "s" }
            ),
            LogMatch::Differs { at } => format!("differs from line {}", at + 1),
        }
    }
}

/// Trace sequences of one comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceComparison {
    /// Frames each movie was run for.
    pub frames: u32,
    /// Our SWF in Ruffle.
    pub ours: Vec<String>,
    /// The reference SWF (Flash 8's) in Ruffle, run for the same frames.
    pub reference: Vec<String>,
    /// Flash 8's captured Output panel, if there is one.
    pub expected: Option<Vec<String>>,
}

impl TraceComparison {
    /// Whether ours equals the reference in Ruffle line for line. This is
    /// the verdict: it also implies agreement with Flash 8's log wherever
    /// Ruffle reproduces that log with Flash 8's own SWF.
    pub fn matches(&self) -> bool {
        self.ours == self.reference
    }

    /// Ours against Flash 8's log, if there is one.
    pub fn ours_vs_expected(&self) -> Option<LogMatch> {
        self.expected.as_ref().map(|e| LogMatch::of(&self.ours, e))
    }

    /// Flash 8's SWF in Ruffle against Flash 8's log: shows where Ruffle
    /// itself differs from Flash Player 8.
    pub fn reference_vs_expected(&self) -> Option<LogMatch> {
        self.expected
            .as_ref()
            .map(|e| LogMatch::of(&self.reference, e))
    }

    /// The report section: verdicts and line diffs.
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "traces over {} frame{}: ours {} line{}, reference in Ruffle {}",
            self.frames,
            if self.frames == 1 { "" } else { "s" },
            self.ours.len(),
            if self.ours.len() == 1 { "" } else { "s" },
            self.reference.len()
        ));
        if let Some(e) = &self.expected {
            out.push_str(&format!(", Flash 8 log {}", e.len()));
        }
        out.push('\n');
        out.push_str(&format!(
            "  ours vs reference in Ruffle: {}\n",
            if self.matches() {
                "same".to_owned()
            } else {
                LogMatch::of(&self.ours, &self.reference).describe()
            }
        ));
        match (self.ours_vs_expected(), self.reference_vs_expected()) {
            (Some(ours), Some(reference)) => {
                out.push_str(&format!("  ours vs Flash 8 log: {}\n", ours.describe()));
                out.push_str(&format!(
                    "  reference in Ruffle vs Flash 8 log: {}{}\n",
                    reference.describe(),
                    if reference.is_match() {
                        ""
                    } else {
                        " (a Ruffle difference, not ours)"
                    }
                ));
            }
            _ => out.push_str("  no Flash 8 log (flash8-trace.txt) to compare with\n"),
        }
        if !self.matches() {
            out.push_str("  diff (- reference in Ruffle, + ours):\n");
            out.push_str(&diff(&self.reference, &self.ours, "    "));
        }
        if let (Some(expected), Some(m)) = (&self.expected, self.ours_vs_expected())
            && !m.is_match()
        {
            out.push_str("  diff (- Flash 8 log, + ours):\n");
            out.push_str(&diff(expected, &self.ours, "    "));
        }
        out
    }
}

/// The most lines a diff is computed for; longer inputs are cut (the
/// table is quadratic).
const MAX_DIFF_LINES: usize = 2000;

/// A line diff of `old` and `new`: unchanged lines start with two spaces,
/// removed ones with `- `, added ones with `+ `. Each line gets `indent`.
pub fn diff(old: &[String], new: &[String], indent: &str) -> String {
    let old = &old[..old.len().min(MAX_DIFF_LINES)];
    let new = &new[..new.len().min(MAX_DIFF_LINES)];
    let (n, m) = (old.len(), new.len());
    // lcs[i][j]: longest common subsequence of old[i..] and new[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            out.push_str(&format!("{indent}  {}\n", old[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push_str(&format!("{indent}- {}\n", old[i]));
            i += 1;
        } else {
            out.push_str(&format!("{indent}+ {}\n", new[j]));
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn traces_split_into_lines() {
        let log = vec![
            LogLine::new(LogKind::Trace, "a\rb"),
            LogLine::new(LogKind::AvmWarning, "w"),
            LogLine::new(LogKind::Trace, "c"),
        ];
        assert_eq!(trace_lines(&log), lines(&["a", "b", "c"]));
        assert_eq!(other_log_lines(&log), lines(&["AvmWarning: w"]));
    }

    #[test]
    fn log_match_kinds() {
        let e = lines(&["a", "b"]);
        assert_eq!(LogMatch::of(&lines(&["a", "b"]), &e), LogMatch::Exact);
        assert_eq!(
            LogMatch::of(&lines(&["a", "b", "a"]), &e),
            LogMatch::Prefix { extra: 1 }
        );
        assert_eq!(
            LogMatch::of(&lines(&["a"]), &e),
            LogMatch::Differs { at: 1 }
        );
        assert_eq!(
            LogMatch::of(&lines(&["x", "b"]), &e),
            LogMatch::Differs { at: 0 }
        );
    }

    #[test]
    fn diff_marks_changes() {
        let d = diff(&lines(&["a", "b", "c"]), &lines(&["a", "x", "c"]), "");
        assert_eq!(d, "  a\n- b\n+ x\n  c\n");
    }

    #[test]
    fn expected_file_parsing() {
        let dir = std::env::temp_dir().join(format!("jazari-cli-trace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.txt");
        std::fs::write(&path, "\u{feff}one\r\ntwo\r\n\r\n").unwrap();
        assert_eq!(read_expected(&path).unwrap(), Some(lines(&["one", "two"])));
        assert_eq!(read_expected(&dir.join("missing.txt")).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
