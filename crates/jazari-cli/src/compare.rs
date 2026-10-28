//! Comparing our SWF with a reference SWF: render the same frames of both
//! with one [`OffscreenRenderer`], diff them pixel by pixel, write the
//! composites (ours | reference | diff), and compare the `trace()`
//! sequences of both movies run headless for the same number of frames.
//!
//! Used by `jazari-cli compare` and by the golden tests.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jazari_preview::{OffscreenRenderer, run_headless};

use crate::args::{FrameSpec, MAX_FRAME};
use crate::error::{CliError, Result};
use crate::imagediff::{self, PixelDiff};
use crate::swfdump;
use crate::traces::{self, TraceComparison};

/// Default channel threshold: a pixel differs when one of its channels is
/// more than this far apart (out of 255).
pub const DEFAULT_THRESHOLD: u8 = 16;
/// Default tolerance: the share of differing pixels a frame may have.
pub const DEFAULT_MAX_SHARE: f64 = 0.001;

/// What to compare and how strictly.
#[derive(Debug, Clone)]
pub struct CompareSettings {
    /// Frames to render (`All` = every frame of the reference).
    pub frames: FrameSpec,
    /// A pixel differs when its largest channel difference exceeds this.
    pub threshold: u8,
    /// A frame passes when at most this share (0..=1) of its pixels differ.
    pub max_share: f64,
    /// Frames to run headless for the traces; `None` = [`default_trace_frames`].
    pub trace_frames: Option<u32>,
    /// Render scale (1 = stage size in pixels).
    pub scale: f32,
    /// Regions (stage pixels at scale 1) left out of the pixel counts, for
    /// content where our output is right and Flash 8's is not (reviewed by
    /// eye; drawn blue in the diff panel).
    pub ignore: Vec<imagediff::Region>,
}

impl Default for CompareSettings {
    fn default() -> Self {
        Self {
            frames: FrameSpec::List(vec![1]),
            threshold: DEFAULT_THRESHOLD,
            max_share: DEFAULT_MAX_SHARE,
            trace_frames: None,
            scale: 1.0,
            ignore: Vec::new(),
        }
    }
}

/// The number of frames to run for traces when none is given: three times
/// the longer movie (so loops back through the timeline are covered), at
/// least 30, and at least the last rendered frame.
pub fn default_trace_frames(num_frames: u32, last_rendered: u32) -> u32 {
    num_frames
        .saturating_mul(3)
        .max(30)
        .max(last_rendered)
        .min(MAX_FRAME)
}

/// One compared frame.
#[derive(Debug, Clone)]
pub struct FrameResult {
    /// 1-based frame number.
    pub frame: u32,
    /// The pixel comparison.
    pub diff: PixelDiff,
    /// The composite image written for it.
    pub image: PathBuf,
}

/// The outcome of [`compare_swfs`].
#[derive(Debug, Clone)]
pub struct Comparison {
    /// Per-frame results, in frame order.
    pub frames: Vec<FrameResult>,
    /// The tolerance used for the verdict.
    pub max_share: f64,
    /// The trace sequences.
    pub traces: TraceComparison,
    /// Non-trace log lines of our movie (AVM and Ruffle warnings).
    pub ours_log: Vec<String>,
    /// Non-trace log lines of the reference movie.
    pub reference_log: Vec<String>,
    /// Whether the AVM halted while rendering ours / the reference.
    pub halted: (bool, bool),
}

impl Comparison {
    /// Whether every frame is within tolerance.
    pub fn pixels_pass(&self) -> bool {
        self.frames.iter().all(|f| f.diff.share() <= self.max_share)
    }

    /// Whether the traces match (see [`TraceComparison::matches`]).
    pub fn traces_pass(&self) -> bool {
        self.traces.matches()
    }

    /// The frame with the largest share of differing pixels (the first
    /// one among equals).
    pub fn worst_frame(&self) -> Option<&FrameResult> {
        self.frames.iter().fold(None, |worst, f| match worst {
            Some(w) if w.diff.share() >= f.diff.share() => Some(w),
            _ => Some(f),
        })
    }

    /// The report: the frame table, the traces and the other log lines.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let threshold = self.frames.first().map_or(0, |f| f.diff.threshold);
        let _ = writeln!(
            out,
            "frames (a pixel differs when a channel is more than {threshold} apart; \
             tolerance {} of the pixels; images: ours | reference | diff):",
            percent(self.max_share)
        );
        if let Some(f) = self.frames.iter().find(|f| f.diff.ignored > 0) {
            let _ = writeln!(
                out,
                "  ({} pixels per frame in ignored regions are left out: blue in the diff panel)",
                f.diff.ignored
            );
        }
        let _ = writeln!(
            out,
            "  {:>6}  {:>10}  {:>9}  {:>7}  verdict",
            "frame", "differing", "share", "largest"
        );
        for f in &self.frames {
            let _ = writeln!(
                out,
                "  {:>6}  {:>10}  {:>9}  {:>7}  {}{}",
                f.frame,
                f.diff.differing,
                percent(f.diff.share()),
                f.diff.max_diff,
                if f.diff.share() <= self.max_share {
                    "ok"
                } else {
                    "DIFFERS"
                },
                if f.diff.size_mismatch {
                    " (sizes differ)"
                } else {
                    ""
                }
            );
        }
        let _ = writeln!(
            out,
            "  pixels: {}{}",
            if self.pixels_pass() { "pass" } else { "FAIL" },
            self.worst_frame()
                .map(|w| format!(" (worst: frame {}, {})", w.frame, percent(w.diff.share())))
                .unwrap_or_default()
        );
        if self.halted.0 || self.halted.1 {
            let _ = writeln!(
                out,
                "  AVM halted while rendering: ours {}, reference {}",
                self.halted.0, self.halted.1
            );
        }
        let _ = writeln!(out);
        out.push_str(&self.traces.report());
        let _ = writeln!(
            out,
            "  traces: {}",
            if self.traces_pass() { "pass" } else { "FAIL" }
        );
        for (who, lines) in [("ours", &self.ours_log), ("reference", &self.reference_log)] {
            if !lines.is_empty() {
                let _ = writeln!(out);
                let _ = writeln!(out, "other log lines ({who}, headless run):");
                for l in lines.iter().take(50) {
                    let _ = writeln!(out, "  {l}");
                }
                if lines.len() > 50 {
                    let _ = writeln!(out, "  ... {} more", lines.len() - 50);
                }
            }
        }
        out
    }
}

/// Renders and compares `ours` with `reference`, writing a composite PNG
/// per frame and the two trace sequences into `out_dir` (which must
/// exist). `expected` is Flash 8's captured log, if any.
pub fn compare_swfs(
    renderer: &OffscreenRenderer,
    ours: &[u8],
    reference: &[u8],
    expected: Option<Vec<String>>,
    settings: &CompareSettings,
    out_dir: &Path,
) -> Result<Comparison> {
    let ours_info = swfdump::info(ours, "our SWF")?;
    let ref_info = swfdump::info(reference, "the reference SWF")?;
    let frames = settings.frames.resolve(u32::from(ref_info.num_frames));
    let last = frames.iter().copied().max().unwrap_or(1);

    let ours_out = renderer
        .render(ours, &frames, settings.scale)
        .map_err(|e| CliError::preview("our SWF", &e))?;
    let ref_out = renderer
        .render(reference, &frames, settings.scale)
        .map_err(|e| CliError::preview("the reference SWF", &e))?;

    let mut results = Vec::with_capacity(frames.len());
    for ((frame, a), b) in frames.iter().zip(&ours_out.images).zip(&ref_out.images) {
        let ignore: Vec<imagediff::Region> = settings
            .ignore
            .iter()
            .map(|r| r.scaled(settings.scale))
            .collect();
        let (diff, diff_image) = imagediff::compare_except(a, b, settings.threshold, &ignore);
        let composite = imagediff::side_by_side(&[a, b, &diff_image]);
        let image = out_dir.join(frame_file(*frame));
        composite.save(&image).map_err(|source| CliError::Image {
            path: image.clone(),
            source,
        })?;
        results.push(FrameResult {
            frame: *frame,
            diff,
            image,
        });
    }

    let trace_frames = settings.trace_frames.unwrap_or_else(|| {
        default_trace_frames(
            u32::from(ours_info.num_frames.max(ref_info.num_frames)),
            last,
        )
    });
    let ours_log =
        run_headless(ours, trace_frames).map_err(|e| CliError::preview("our SWF", &e))?;
    let ref_log = run_headless(reference, trace_frames)
        .map_err(|e| CliError::preview("the reference SWF", &e))?;
    let comparison = Comparison {
        frames: results,
        max_share: settings.max_share,
        traces: TraceComparison {
            frames: trace_frames,
            ours: traces::trace_lines(&ours_log),
            reference: traces::trace_lines(&ref_log),
            expected,
        },
        ours_log: traces::other_log_lines(&ours_log),
        reference_log: traces::other_log_lines(&ref_log),
        halted: (ours_out.halted, ref_out.halted),
    };
    write_lines(&out_dir.join("traces-ours.txt"), &comparison.traces.ours)?;
    write_lines(
        &out_dir.join("traces-reference.txt"),
        &comparison.traces.reference,
    )?;
    Ok(comparison)
}

/// The composite image name of a frame.
pub fn frame_file(frame: u32) -> String {
    format!("frame-{frame:03}.png")
}

/// Removes composites (`frame-*.png`) left in `dir` by an earlier run.
pub fn remove_old_frames(dir: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(CliError::io(dir, "listing", e)),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("frame-") && name.ends_with(".png") {
            let path = entry.path();
            std::fs::remove_file(&path).map_err(|e| CliError::io(&path, "removing", e))?;
        }
    }
    Ok(())
}

/// Writes `lines`, one per line, to `path`.
pub fn write_lines(path: &Path, lines: &[String]) -> Result<()> {
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(path, text).map_err(|e| CliError::io(path, "writing", e))
}

/// A share as a percentage with three decimals.
pub fn percent(share: f64) -> String {
    format!("{:.3}%", share * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_frames_default() {
        assert_eq!(default_trace_frames(1, 1), 30);
        assert_eq!(default_trace_frames(60, 1), 180);
        assert_eq!(default_trace_frames(12, 100), 100);
        assert_eq!(default_trace_frames(u32::MAX, 1), MAX_FRAME);
    }

    #[test]
    fn frame_names_sort() {
        assert_eq!(frame_file(7), "frame-007.png");
        assert_eq!(frame_file(1234), "frame-1234.png");
    }
}
