//! The commands: argument handling, output and exit codes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use jazari_preview::{OffscreenRenderer, adapter_summary, is_software_adapter};
use jazari_publish::{BuildMode, Diagnostic};

use crate::args::{Args, FrameSpec};
use crate::compare::{self, CompareSettings, percent};
use crate::docdump::{self, DumpOptions};
use crate::error::{CliError, Result};
use crate::publishing;
use crate::swfdump::{self, SwfDumpOptions};
use crate::traces;

/// The usage text printed by `help` and on usage errors.
pub const USAGE: &str = "\
Al-Jazari command line

usage:
  jazari-cli dump <doc> [--scripts]
      Summary of a document (stage, scenes, layers, library) and its import
      report. --scripts prints frame and instance scripts in full.
  jazari-cli dump --swf <file.swf> [--full]
      The tags of a SWF, with actions disassembled. --full adds shape
      records, glyph entries and font code tables.
  jazari-cli publish <doc> <out.swf> [--stage] [--no-compress] [--strict]
      Publishes a SWF 8. Diagnostics go to stderr with their locations.
      Exit code 1 when no SWF could be written (with --strict also when a
      script or element failed).
  jazari-cli render <file.swf> <outdir> [--frames 1,5,10|1-12|all] [--scale 1]
      Renders frames to PNG on the software adapter and prints the log.
  jazari-cli compare <doc> <reference.swf> <outdir> [--frames ...] [--scale 1]
                     [--threshold 16] [--tolerance 0.1] [--trace-frames N]
                     [--trace <flash8-trace.txt>]
      Publishes <doc>, renders the same frames of both movies, writes
      frame-NNN.png (ours | reference | diff) and report.txt, and compares
      the trace() output of both, run for the same number of frames.
      A pixel differs when a channel is more than --threshold apart; a frame
      passes when at most --tolerance percent of its pixels differ. A
      flash8-trace.txt next to the reference is used as Flash 8's log unless
      --trace names another. Exit code 1 when the movies differ.

<doc> is an XFL folder, its .xfl file or a CS5+ .fla. Frames are 1-based;
rendering frame N runs the movie for N frames. Rendering never uses a GPU.
Exit codes: 0 success, 1 failure, 2 bad arguments.";

/// Runs the command line `args` (without the program name).
pub fn run(args: &[OsString]) -> Result<()> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::Usage("no command given".to_owned()));
    };
    match command.to_str().unwrap_or("") {
        "dump" => dump(rest),
        "publish" => publish(rest),
        "render" => render(rest),
        "compare" => compare(rest),
        "help" | "--help" | "-h" => {
            out!("{USAGE}");
            Ok(())
        }
        other => Err(CliError::Usage(format!("unknown command {other:?}"))),
    }
}

fn dump(raw: &[OsString]) -> Result<()> {
    let args = Args::parse(raw, &["swf", "scripts", "full"], &[])?;
    let text = if args.has("swf") {
        let [path] = args.paths(&["file.swf"])?.try_into().map_err(|_| usage())?;
        let bytes = read(&path)?;
        swfdump::dump(
            &bytes,
            &path.display().to_string(),
            SwfDumpOptions {
                full: args.has("full"),
            },
        )?
    } else {
        let [path] = args.paths(&["doc"])?.try_into().map_err(|_| usage())?;
        let (doc, report) = publishing::open(&path)?;
        docdump::summary(
            &doc,
            &report,
            DumpOptions {
                scripts: args.has("scripts"),
            },
        )
    };
    print_text(&text);
    Ok(())
}

fn publish(raw: &[OsString]) -> Result<()> {
    let args = Args::parse(raw, &["stage", "no-compress", "strict"], &[])?;
    let [doc_path, out_path] = args
        .paths(&["doc", "out.swf"])?
        .try_into()
        .map_err(|_| usage())?;
    let (doc, report) = publishing::open(&doc_path)?;
    for w in &report {
        err!("import: warning: {w}");
    }
    let mode = if args.has("stage") {
        BuildMode::Stage
    } else {
        BuildMode::Release
    };
    let mut fonts = publishing::system_fonts();
    let started = Instant::now();
    let out = publishing::publish_doc(&doc, &mut fonts, mode, !args.has("no-compress"));
    let elapsed = started.elapsed();
    print_diagnostics(&out.diagnostics);
    let Some(swf) = out.swf else {
        return Err(CliError::PublishFailed);
    };
    std::fs::write(&out_path, &swf).map_err(|e| CliError::io(&out_path, "writing", e))?;
    let (errors, warnings, infos) = publishing::severity_counts(&out.diagnostics);
    out!(
        "wrote {} ({} bytes, {} frame{}; {errors} error{}, {warnings} warning{}, {infos} note{}) in {:.2} s",
        out_path.display(),
        swf.len(),
        out.frame_map.len(),
        plural(out.frame_map.len()),
        plural(errors),
        plural(warnings),
        plural(infos),
        elapsed.as_secs_f64()
    );
    if args.has("strict") && errors > 0 {
        err!(
            "--strict: {errors} error{} while publishing",
            plural(errors)
        );
        return Err(CliError::PublishFailed);
    }
    Ok(())
}

fn render(raw: &[OsString]) -> Result<()> {
    let args = Args::parse(raw, &[], &["frames", "scale"])?;
    let [swf_path, out_dir] = args
        .paths(&["file.swf", "outdir"])?
        .try_into()
        .map_err(|_| usage())?;
    let frames = FrameSpec::parse(args.value("frames").unwrap_or("1"))?;
    let scale = scale(&args)?;
    let bytes = read(&swf_path)?;
    let info = swfdump::info(&bytes, &swf_path.display().to_string())?;
    let frames = frames.resolve(u32::from(info.num_frames));
    create_dir(&out_dir)?;
    let renderer = software_renderer()?;
    let stem = swf_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "movie".to_owned());
    let started = Instant::now();
    let output = renderer
        .render(&bytes, &frames, scale)
        .map_err(|e| CliError::preview(swf_path.display().to_string(), &e))?;
    out!(
        "rendered {} frame{} in {:.2} s{}",
        frames.len(),
        plural(frames.len()),
        started.elapsed().as_secs_f64(),
        if output.halted {
            " (the AVM halted)"
        } else {
            ""
        }
    );
    for (frame, image) in frames.iter().zip(&output.images) {
        let path = out_dir.join(format!("{stem}-f{frame:03}.png"));
        image.save(&path).map_err(|source| CliError::Image {
            path: path.clone(),
            source,
        })?;
        out!(
            "  {} ({}x{})",
            path.display(),
            image.width(),
            image.height()
        );
    }
    for line in &output.log {
        out!("log {:?}: {}", line.kind, line.text);
    }
    Ok(())
}

fn compare(raw: &[OsString]) -> Result<()> {
    let args = Args::parse(
        raw,
        &[],
        &[
            "frames",
            "scale",
            "threshold",
            "tolerance",
            "trace-frames",
            "trace",
        ],
    )?;
    let [doc_path, ref_path, out_dir] = args
        .paths(&["doc", "reference.swf", "outdir"])?
        .try_into()
        .map_err(|_| usage())?;
    let tolerance: f64 = args.number("tolerance", compare::DEFAULT_MAX_SHARE * 100.0)?;
    if !(0.0..=100.0).contains(&tolerance) {
        return Err(CliError::Usage(
            "--tolerance is a percentage, 0 to 100".to_owned(),
        ));
    }
    let trace_frames = match args.value("trace-frames") {
        None => None,
        Some(_) => {
            let n: u32 = args.number("trace-frames", 0)?;
            if n == 0 || n > crate::args::MAX_FRAME {
                return Err(CliError::Usage(format!(
                    "--trace-frames must be 1 to {}",
                    crate::args::MAX_FRAME
                )));
            }
            Some(n)
        }
    };
    let settings = CompareSettings {
        frames: FrameSpec::parse(args.value("frames").unwrap_or("1"))?,
        threshold: args.number("threshold", compare::DEFAULT_THRESHOLD)?,
        max_share: tolerance / 100.0,
        trace_frames,
        scale: scale(&args)?,
        ignore: Vec::new(),
    };
    let trace_path = match args.value("trace") {
        Some(p) => Some(PathBuf::from(p)),
        None => ref_path
            .parent()
            .map(|dir| dir.join("flash8-trace.txt"))
            .filter(|p| p.is_file()),
    };
    let expected = match &trace_path {
        Some(p) => traces::read_expected(p)?,
        None => None,
    };
    if trace_path.is_some() && expected.is_none() {
        return Err(CliError::Usage(format!(
            "--trace: {} does not exist",
            trace_path.as_deref().unwrap_or(Path::new("")).display()
        )));
    }

    let reference = read(&ref_path)?;
    let (doc, report) = publishing::open(&doc_path)?;
    for w in &report {
        err!("import: warning: {w}");
    }
    let mut fonts = publishing::system_fonts();
    let out = publishing::publish_doc(&doc, &mut fonts, BuildMode::Release, true);
    print_diagnostics(&out.diagnostics);
    let Some(ours) = out.swf else {
        return Err(CliError::PublishFailed);
    };
    create_dir(&out_dir)?;
    let ours_path = out_dir.join("ours.swf");
    std::fs::write(&ours_path, &ours).map_err(|e| CliError::io(&ours_path, "writing", e))?;

    let renderer = software_renderer()?;
    let started = Instant::now();
    let result =
        compare::compare_swfs(&renderer, &ours, &reference, expected, &settings, &out_dir)?;
    let mut text = format!(
        "document: {}\nreference: {}\nFlash 8 log: {}\nours: {} ({} bytes)\n",
        doc_path.display(),
        ref_path.display(),
        trace_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_owned()),
        ours_path.display(),
        ours.len()
    );
    text.push_str(&diagnostics_section(
        &report_lines(&report),
        &out.diagnostics,
    ));
    text.push('\n');
    text.push_str(&result.report());
    let report_path = out_dir.join("report.txt");
    std::fs::write(&report_path, &text).map_err(|e| CliError::io(&report_path, "writing", e))?;
    print_text(&result.report());
    out!(
        "compared in {:.2} s; report: {}",
        started.elapsed().as_secs_f64(),
        report_path.display()
    );
    if result.pixels_pass() && result.traces_pass() {
        Ok(())
    } else {
        if let Some(w) = result.worst_frame() {
            err!(
                "worst frame {}: {} of the pixels differ",
                w.frame,
                percent(w.diff.share())
            );
        }
        Err(CliError::Mismatch)
    }
}

/// The import warnings as lines.
pub fn report_lines(report: &jazari_xfl::ImportReport) -> Vec<String> {
    report.iter().map(|w| format!("import: {w}")).collect()
}

/// The report section listing import warnings and publish diagnostics.
pub fn diagnostics_section(import: &[String], diagnostics: &[Diagnostic]) -> String {
    let mut text = String::new();
    let (errors, warnings, infos) = publishing::severity_counts(diagnostics);
    text.push_str(&format!(
        "import warnings: {}; publish diagnostics: {errors} error{}, {warnings} warning{}, {infos} note{}\n",
        import.len(),
        plural(errors),
        plural(warnings),
        plural(infos)
    ));
    for line in import {
        text.push_str(&format!("  {line}\n"));
    }
    for d in diagnostics {
        text.push_str(&format!("  {d}\n"));
    }
    text
}

/// Creates the software renderer, refusing any hardware adapter, and
/// prints which adapter it is.
pub fn software_renderer() -> Result<OffscreenRenderer> {
    let started = Instant::now();
    let renderer =
        OffscreenRenderer::new().map_err(|e| CliError::preview("the software renderer", &e))?;
    let info = renderer.adapter_info();
    if !is_software_adapter(&info) {
        return Err(CliError::Preview {
            what: "the renderer".to_owned(),
            message: format!(
                "refusing to render on a hardware adapter: {}",
                adapter_summary(&info)
            ),
        });
    }
    err!(
        "adapter: {} (ready in {:.2} s)",
        adapter_summary(&info),
        started.elapsed().as_secs_f64()
    );
    Ok(renderer)
}

fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for d in diagnostics {
        err!("{d}");
    }
}

fn print_text(text: &str) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    // A closed pipe (`| head`) is not an error worth reporting.
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| CliError::io(path, "reading", e))
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| CliError::io(path, "creating", e))
}

fn scale(args: &Args) -> Result<f32> {
    let scale: f32 = args.number("scale", 1.0)?;
    if !(scale.is_finite() && (0.05..=8.0).contains(&scale)) {
        return Err(CliError::Usage("--scale must be 0.05 to 8".to_owned()));
    }
    Ok(scale)
}

fn usage() -> CliError {
    CliError::Usage("wrong number of arguments".to_owned())
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
