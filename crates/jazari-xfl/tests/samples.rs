//! Opens every fixture under `tests/fixtures/` (including any added after the
//! hand-written checks in `fixtures.rs`), plus the samples in an external
//! folder named by `JAZARI_SAMPLES`: its `fixtures/*/xfl` and `f8/smoke-xfl`.
//!
//! Opening must never fail; warnings are printed (`cargo test -- --nocapture`
//! shows them) but do not fail the test, because a new fixture may hold
//! content milestone 1 reports as unsupported.

use std::path::{Path, PathBuf};

/// Every `<root>/<name>/xfl` folder holding a DOMDocument.xml, sorted.
fn xfl_folders_in(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let xfl = e.path().join("xfl");
            if xfl.join("DOMDocument.xml").is_file() {
                out.push(xfl);
            }
        }
    }
}

fn fixture_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    xfl_folders_in(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures"),
        &mut out,
    );
    if let Some(root) = std::env::var_os("JAZARI_SAMPLES").map(PathBuf::from) {
        xfl_folders_in(&root.join("fixtures"), &mut out);
        let smoke = root.join("f8").join("smoke-xfl");
        if smoke.join("DOMDocument.xml").is_file() {
            out.push(smoke);
        }
    }
    out
}

#[test]
fn every_sample_fixture_opens() {
    let folders = fixture_folders();
    assert!(!folders.is_empty(), "tests/fixtures holds no XFL fixture");
    let mut failures = Vec::new();
    for folder in &folders {
        match jazari_xfl::open(folder) {
            Ok((doc, report)) => {
                println!(
                    "{}: {} × {} px, {} fps, {} scene(s), {} frame(s), {} symbol(s), {} bitmap(s), {} warning(s)",
                    folder.display(),
                    doc.width,
                    doc.height,
                    doc.frame_rate,
                    doc.scenes.len(),
                    doc.scenes.first().map_or(0, |s| s.frame_count()),
                    doc.library.symbols.len(),
                    doc.library.bitmaps.len(),
                    report.len()
                );
                for w in &report {
                    println!("    {w}");
                }
                if doc.scenes.is_empty() {
                    failures.push(format!("{}: no scene", folder.display()));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", folder.display())),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
