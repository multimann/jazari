//! Golden tests: every `tests/fixtures/<name>` that has `xfl/` and
//! `flash8.swf` is published and compared with what Flash 8 published.
//!
//! For each fixture:
//! 1. open `xfl/` and publish it (release build, system fonts, as the CLI);
//! 2. render the frames listed in [`TABLE`] of our SWF and of `flash8.swf`
//!    with one shared software [`OffscreenRenderer`];
//! 3. per frame, count the pixels whose largest channel difference exceeds
//!    the row's threshold, and the largest difference;
//! 4. run both movies headless for the same number of frames and compare
//!    their `trace()` lines. The verdict is ours == Flash 8's SWF in Ruffle,
//!    line for line. Each is also compared with `flash8-trace.txt` (Flash 8's
//!    own Test Movie output) for the report; there, extra lines after the end
//!    of the captured log are a timing-only difference (the capture stopped
//!    after some seconds, Ruffle ran a fixed number of frames) and are
//!    ignored. See `jazari_cli::traces`.
//!
//! Reports, composites (ours | reference | diff) and our SWFs are written to
//! `target/golden/<name>/` (or `$JAZARI_GOLDEN_OUT/<name>/`), with a summary
//! in `summary.txt`. Only rows marked [`Expect::Pass`] fail the test; the
//! others are reported. Fixture folders not in the table are picked up with
//! [`DEFAULT_ROW`] (frame 1, report only). `JAZARI_GOLDEN=a,b` limits the
//! run to some fixtures.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Instant;

use jazari_cli::args::FrameSpec;
use jazari_cli::commands::{diagnostics_section, report_lines, software_renderer};
use jazari_cli::compare::{self, CompareSettings, Comparison, percent};
use jazari_cli::imagediff::Region;
use jazari_cli::{publishing, traces};
use jazari_preview::OffscreenRenderer;
use jazari_publish::BuildMode;
use jazari_text::FontLibrary;

/// Whether a check must pass or is only reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A failure fails the test.
    Pass,
    /// Reported only: the feature is still being built.
    Report,
}
use Expect::{Pass, Report};

/// One row of the golden table.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// Fixture folder name (relative to `tests/fixtures`).
    name: &'static str,
    /// 1-based frames to render and compare (frame N = after N frames).
    frames: &'static [u32],
    /// A pixel differs when a channel is more than this apart (0..=255).
    threshold: u8,
    /// A frame passes when at most this share (0..=1) of its pixels differ.
    max_share: f64,
    /// Frames to run headless for the traces (`None`: 3 x the frame count,
    /// at least 30).
    trace_frames: Option<u32>,
    /// Stage regions (x, y, width, height in px) left out of the pixel
    /// counts because our output is right there and Flash 8's is not. They
    /// are drawn blue in the diff panel and reviewed by eye.
    ignore: &'static [(u32, u32, u32, u32)],
    /// Expectation for the pixel comparison.
    pixels: Expect,
    /// Expectation for the trace comparison.
    traces: Expect,
    /// Why a check is only reported, what is left out, or what to watch for.
    note: &'static str,
}

/// Defaults for fixture folders that are not in [`TABLE`].
const DEFAULT_ROW: Row = Row {
    name: "",
    frames: &[1],
    threshold: 16,
    max_share: 0.001,
    trace_frames: None,
    ignore: &[],
    pixels: Report,
    traces: Report,
    note: "not in the golden table yet: frame 1, report only",
};

/// A row whose output matches Flash 8's in Ruffle: no pixel may differ by
/// more than the threshold, and the traces must be equal.
const EXACT: Row = Row {
    max_share: 0.0,
    pixels: Pass,
    traces: Pass,
    note: "",
    ..DEFAULT_ROW
};

/// Thresholds, frame lists and expectations, one row per fixture. Every
/// fixture whose output is right is `Pass`; a tolerance above zero is the
/// measured difference rounded up, with its cause in the note.
const TABLE: &[Row] = &[
    Row {
        name: "shapes-basic",
        ..EXACT
    },
    Row {
        name: "gradients",
        ..EXACT
    },
    Row {
        name: "symbols",
        ..EXACT
    },
    Row {
        name: "smoke",
        frames: &[1, 10, 20],
        note: "no flash8-trace.txt: traces are checked against flash8.swf in Ruffle only",
        ..EXACT
    },
    Row {
        name: "tweens",
        frames: &[1, 6, 12, 18, 24, 30, 36, 42, 48, 54, 60],
        max_share: 0.0002,
        note: "frame 36 has 18 anti-aliasing pixels because Flash 8 itself moves a still clip \
               by 1 twip there (tween_oracle.rs); the other frames are identical",
        ..EXACT
    },
    Row {
        name: "as2",
        frames: &[1, 4, 5, 8, 12, 16],
        ..EXACT
    },
    Row {
        name: "text",
        frames: &[1],
        ignore: &[(16, 196, 200, 48)],
        note: "the Arabic line at (20,200) is left out (blue in the diff panel): we shape it, \
               joined and right to left, while Flash 8 draws isolated letters left to right, \
               so ours is the correct one there (reviewed by eye; tests/text.rs checks the \
               glyph sequence). The Latin text and the dynamic field are identical",
        ..EXACT
    },
    Row {
        name: "bitmaps",
        frames: &[1],
        max_share: 0.0001,
        note: "6 anti-aliasing pixels at one rotated corner: XFL stores translations in whole \
               twips while Flash keeps sub-twip positions",
        ..EXACT
    },
    Row {
        name: "groups",
        frames: &[1],
        max_share: 0.0005,
        note: "54 edge anti-aliasing pixels: grouped matrices in XFL have less precision than \
               Flash keeps internally (edge bounds agree within 1 twip)",
        ..EXACT
    },
    Row {
        name: "masks",
        frames: &[1, 5, 10, 15, 20],
        max_share: 0.0002,
        note: "25 anti-aliasing pixels on mask A's outline: Flash 8 reuses another symbol's \
               oval, scaled, where we define the oval itself. As in Flash 8, only the backmost \
               element of a mask layer is published (the others are reported as warnings)",
        ..EXACT
    },
    Row {
        name: "scenes",
        frames: &[1, 3, 5, 8, 13, 20, 30, 40],
        trace_frames: Some(60),
        ..EXACT
    },
    Row {
        name: "tween-probes/matrices",
        frames: &[1, 15, 30, 45, 60, 75, 90, 105, 121],
        max_share: 0.001,
        note: "anti-aliasing pixels from 1-3 twip residuals: the transformation point lies far \
               from the registration point, and Flash 8's decomposition of non-axis-aligned \
               keyframe angles is not reproduced exactly (see the tween open issues)",
        ..EXACT
    },
    Row {
        name: "tween-probes/tpoint",
        frames: &[1, 8, 16, 24, 31],
        max_share: 0.0005,
        note: "anti-aliasing pixels from 1-2 twip translation residuals with an off-centre \
               transformation point",
        ..EXACT
    },
    Row {
        name: "tween-probes/ease",
        frames: &[1, 4, 8, 12, 16, 21],
        ..EXACT
    },
    Row {
        name: "tween-probes/color",
        frames: &[1, 4, 8, 12, 16, 21],
        note: "Adjust Color follows Flash's own algorithm (crate::filter::adjust_color_matrix)",
        ..EXACT
    },
    Row {
        name: "tween-probes/graphics",
        frames: &[1, 6, 12, 18, 24, 30, 36, 41],
        ..EXACT
    },
];

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest)
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

fn output_root() -> PathBuf {
    std::env::var_os("JAZARI_GOLDEN_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target").join("golden"))
}

/// Whether `dir` is a fixture: it has both `xfl/` and `flash8.swf`.
fn is_fixture(dir: &Path) -> bool {
    dir.join("xfl").is_dir() && dir.join("flash8.swf").is_file()
}

/// The sub-folders of `dir` with their names.
fn sub_dirs(dir: &Path) -> Vec<(String, PathBuf)> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
                .collect()
        })
        .unwrap_or_default()
}

/// Fixture folders, sorted by name: `tests/fixtures/<name>`, and one level
/// deeper for groups of probes (`tween-probes/<name>`).
fn discover() -> Vec<String> {
    let mut names = Vec::new();
    for (name, path) in sub_dirs(&fixtures_dir()) {
        if is_fixture(&path) {
            names.push(name);
        } else {
            for (sub, sub_path) in sub_dirs(&path) {
                if is_fixture(&sub_path) {
                    names.push(format!("{name}/{sub}"));
                }
            }
        }
    }
    names.sort();
    if let Ok(only) = std::env::var("JAZARI_GOLDEN") {
        let wanted: Vec<&str> = only.split(',').map(str::trim).collect();
        names.retain(|n| wanted.contains(&n.as_str()));
    }
    names
}

/// What happened to one fixture.
struct Outcome {
    name: String,
    row: Row,
    /// `None` when the fixture could not be compared at all.
    comparison: Option<Comparison>,
    /// Why it could not be compared.
    error: Option<String>,
    seconds: f64,
}

impl Outcome {
    fn pixels_ok(&self) -> bool {
        self.comparison
            .as_ref()
            .is_some_and(Comparison::pixels_pass)
    }

    fn traces_ok(&self) -> bool {
        self.comparison
            .as_ref()
            .is_some_and(Comparison::traces_pass)
    }

    /// Checks expected to pass that did not.
    fn failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.row.pixels == Pass && !self.pixels_ok() {
            out.push(format!("{}: pixels", self.name));
        }
        if self.row.traces == Pass && !self.traces_ok() {
            out.push(format!("{}: traces", self.name));
        }
        out
    }

    fn summary_line(&self) -> String {
        let verdict = |ok: bool, expect: Expect| match (ok, expect) {
            (true, _) => "pass",
            (false, Pass) => "FAIL",
            (false, Report) => "differs",
        };
        let worst = self
            .comparison
            .as_ref()
            .and_then(Comparison::worst_frame)
            .map(|w| format!("(worst f{} {})", w.frame, percent(w.diff.share())))
            .unwrap_or_else(|| "-".to_owned());
        let mut line = format!(
            "{:<22} pixels {:<7} {:<22} traces {:<7} expected {:<13} {:>5.1} s",
            self.name,
            verdict(self.pixels_ok(), self.row.pixels),
            worst,
            verdict(self.traces_ok(), self.row.traces),
            format!(
                "{}/{}",
                expect_word(self.row.pixels),
                expect_word(self.row.traces)
            ),
            self.seconds
        );
        if let Some(e) = &self.error {
            line.push_str(&format!("\n{:<22} error: {e}", ""));
        }
        line
    }
}

fn expect_word(e: Expect) -> &'static str {
    match e {
        Pass => "pass",
        Report => "report",
    }
}

/// Publishes and compares one fixture, writing its report.
fn run_fixture(
    renderer: &OffscreenRenderer,
    fonts: &mut FontLibrary,
    name: &str,
    row: &Row,
    out_dir: &Path,
) -> Result<Comparison, String> {
    let dir = fixtures_dir().join(name);
    std::fs::create_dir_all(out_dir).map_err(|e| format!("creating {}: {e}", out_dir.display()))?;
    compare::remove_old_frames(out_dir).map_err(|e| e.to_string())?;
    let (doc, report) = publishing::open(&dir.join("xfl")).map_err(|e| e.to_string())?;
    let out = publishing::publish_doc(&doc, fonts, BuildMode::Release, true);
    let mut text = format!(
        "fixture: {name}\nnote: {}\n",
        if row.note.is_empty() { "-" } else { row.note }
    );
    text.push_str(&diagnostics_section(
        &report_lines(&report),
        &out.diagnostics,
    ));
    let Some(ours) = out.swf else {
        write_report(out_dir, &text);
        return Err("publishing failed (see report.txt)".to_owned());
    };
    let ours_path = out_dir.join("ours.swf");
    std::fs::write(&ours_path, &ours)
        .map_err(|e| format!("writing {}: {e}", ours_path.display()))?;
    let reference =
        std::fs::read(dir.join("flash8.swf")).map_err(|e| format!("flash8.swf: {e}"))?;
    let expected =
        traces::read_expected(&dir.join("flash8-trace.txt")).map_err(|e| e.to_string())?;
    let settings = CompareSettings {
        frames: FrameSpec::List(row.frames.to_vec()),
        threshold: row.threshold,
        max_share: row.max_share,
        trace_frames: row.trace_frames,
        scale: 1.0,
        ignore: row
            .ignore
            .iter()
            .map(|&(x, y, width, height)| Region {
                x,
                y,
                width,
                height,
            })
            .collect(),
    };
    let comparison =
        compare::compare_swfs(renderer, &ours, &reference, expected, &settings, out_dir)
            .map_err(|e| e.to_string());
    match &comparison {
        Ok(c) => {
            text.push_str(&format!(
                "expected: pixels {}, traces {}\n\n",
                expect_word(row.pixels),
                expect_word(row.traces)
            ));
            text.push_str(&c.report());
        }
        Err(e) => text.push_str(&format!("\ncomparison failed: {e}\n")),
    }
    write_report(out_dir, &text);
    comparison
}

fn write_report(out_dir: &Path, text: &str) {
    let path = out_dir.join("report.txt");
    if let Err(e) = std::fs::write(&path, text) {
        eprintln!("cannot write {}: {e}", path.display());
    }
}

#[test]
fn fixtures_match_flash_8() {
    let names = discover();
    assert!(
        !names.is_empty() || std::env::var_os("JAZARI_GOLDEN").is_some(),
        "no fixtures found in {}",
        fixtures_dir().display()
    );
    let renderer = software_renderer().expect("a software renderer (WARP, lavapipe or llvmpipe)");
    let mut fonts = publishing::system_fonts();
    let root = output_root();
    let started = Instant::now();
    let mut outcomes = Vec::new();
    for name in &names {
        let row = TABLE
            .iter()
            .find(|r| r.name == name)
            .copied()
            .unwrap_or(DEFAULT_ROW);
        let out_dir = root.join(name);
        let t = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| {
            run_fixture(&renderer, &mut fonts, name, &row, &out_dir)
        }))
        .unwrap_or_else(|_| Err("panicked (see the test output)".to_owned()));
        let (comparison, error) = match result {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        };
        let outcome = Outcome {
            name: name.clone(),
            row,
            comparison,
            error,
            seconds: t.elapsed().as_secs_f64(),
        };
        println!("{}", outcome.summary_line());
        outcomes.push(outcome);
    }
    for row in TABLE {
        if !names.iter().any(|n| n == row.name) && std::env::var_os("JAZARI_GOLDEN").is_none() {
            println!("{:<22} (in the table, but no fixture folder)", row.name);
        }
    }

    let failures: Vec<String> = outcomes.iter().flat_map(Outcome::failures).collect();
    let totals = format!(
        "{} fixture{} in {:.1} s; {} expected check{} failed\nreports: {}\n",
        outcomes.len(),
        if outcomes.len() == 1 { "" } else { "s" },
        started.elapsed().as_secs_f64(),
        failures.len(),
        if failures.len() == 1 { "" } else { "s" },
        root.display()
    );
    let mut summary = String::new();
    for o in &outcomes {
        summary.push_str(&o.summary_line());
        summary.push('\n');
    }
    summary.push('\n');
    summary.push_str(&totals);
    if std::fs::create_dir_all(&root).is_ok() {
        let _ = std::fs::write(root.join("summary.txt"), &summary);
    }
    println!("\n{totals}");
    assert!(
        failures.is_empty(),
        "golden checks expected to pass failed: {}\nsee {}",
        failures.join(", "),
        root.display()
    );
}

#[test]
fn table_rows_are_sane() {
    for (i, row) in TABLE.iter().enumerate() {
        assert!(!row.name.is_empty(), "row {i} has no name");
        assert!(
            TABLE[..i].iter().all(|r| r.name != row.name),
            "{} is listed twice",
            row.name
        );
        assert!(!row.frames.is_empty(), "{}: no frames", row.name);
        assert!(row.frames.iter().all(|&f| f >= 1), "{}: frame 0", row.name);
        assert!(
            (0.0..=1.0).contains(&row.max_share),
            "{}: max_share is a fraction",
            row.name
        );
    }
}
