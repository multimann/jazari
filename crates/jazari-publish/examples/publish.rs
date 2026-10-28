//! Publishes a document to an SWF file and prints the diagnostics.
//!
//! ```text
//! cargo run -p jazari-publish --example publish -- <doc> <out.swf> [--stage] [--no-compress]
//! ```
//!
//! `<doc>` is an XFL folder, its `.xfl` stub or a CS5+ `.fla`. The exit code
//! is 0 when an SWF was written (even with warnings or script errors), 1
//! when publishing failed and 2 on bad arguments.

use std::path::PathBuf;
use std::process::ExitCode;

use jazari_publish::{BuildMode, PublishOptions, publish};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut positional = Vec::new();
    let mut opts = PublishOptions::default();
    for a in args.by_ref() {
        match a.as_str() {
            "--stage" => opts.mode = BuildMode::Stage,
            "--no-compress" => opts.compress = false,
            _ => positional.push(PathBuf::from(a)),
        }
    }
    let [doc_path, out_path] = positional.as_slice() else {
        eprintln!("usage: publish <doc> <out.swf> [--stage] [--no-compress]");
        return ExitCode::from(2);
    };
    let (doc, report) = match jazari_xfl::open(doc_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot open {}: {e}", doc_path.display());
            return ExitCode::from(1);
        }
    };
    for w in report.iter() {
        println!("import: {w}");
    }
    let mut fonts = jazari_text::FontLibrary::new();
    fonts.add_document_fonts(&doc.source.base_dir);
    let base = doc.source.base_dir.clone();
    let as2 = jazari_as2::Compiler::new(jazari_as2::CompilerOptions {
        class_paths: std::iter::once(base.clone())
            .chain(doc.publish.package_paths.iter().map(|p| base.join(p)))
            .collect(),
        include_base: Some(base),
        ..Default::default()
    });
    let started = std::time::Instant::now();
    let out = publish(&doc, &opts, &fonts, &as2);
    let elapsed = started.elapsed();
    for d in &out.diagnostics {
        println!("{d}");
    }
    let Some(swf) = out.swf else {
        eprintln!("publishing failed");
        return ExitCode::from(1);
    };
    if let Err(e) = std::fs::write(out_path, &swf) {
        eprintln!("cannot write {}: {e}", out_path.display());
        return ExitCode::from(1);
    }
    println!(
        "wrote {} ({} bytes, {} frames, {} diagnostics) in {elapsed:?}",
        out_path.display(),
        swf.len(),
        out.frame_map.len(),
        out.diagnostics.len()
    );
    ExitCode::SUCCESS
}
