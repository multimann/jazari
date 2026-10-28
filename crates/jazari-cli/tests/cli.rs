//! Runs the `jazari-cli` binary on fixtures: commands, output and exit
//! codes. Nothing here renders (see `golden.rs` for that).

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jazari-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join(name)
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jazari-cli"))
        .args(args)
        .env("JAZARI_FORCE_CPU", "1")
        .output()
        .expect("run jazari-cli")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn dump_summarises_a_document() {
    let doc = fixture("tweens").join("xfl");
    let out = run(&["dump".as_ref(), doc.as_os_str(), "--scripts".as_ref()]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.contains("stage: 550 x 400 px, 24 fps"), "{stdout}");
    assert!(stdout.contains("scene 1 'Scene 1': 60 frames"), "{stdout}");
    assert!(stdout.contains("layer"), "{stdout}");
    assert!(stdout.contains("motion tween (ease"), "{stdout}");
    assert!(stdout.contains("symbol 'Counter' (graphic)"), "{stdout}");
    assert!(stdout.contains("| stop();"), "{stdout}");
    assert!(stdout.contains("import report: no warnings"), "{stdout}");
}

#[test]
fn dump_swf_disassembles_actions() {
    let swf = fixture("as2").join("flash8.swf");
    let out = run(&["dump".as_ref(), "--swf".as_ref(), swf.as_os_str()]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(stdout.starts_with("SWF version 8"), "{stdout}");
    assert!(stdout.contains("DefineButton2"), "{stdout}");
    assert!(stdout.contains("on over_down_to_over_up"), "{stdout}");
    assert!(stdout.contains("onClipEvent load"), "{stdout}");
    assert!(stdout.contains("DefineFunction2"), "{stdout}");
    assert!(stdout.contains("FrameLabel \"loop\""), "{stdout}");
    assert!(stdout.contains("-- frame 12"), "{stdout}");
}

#[test]
fn publish_writes_a_readable_swf() {
    let doc = fixture("shapes-basic").join("xfl");
    let out_path = scratch("shapes-basic.swf");
    let out = run(&["publish".as_ref(), doc.as_os_str(), out_path.as_os_str()]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("1 frame"));
    let bytes = std::fs::read(&out_path).expect("published SWF");
    let info = jazari_cli::swfdump::info(&bytes, "ours").expect("readable");
    assert_eq!(info.version, 8);
    assert_eq!(info.num_frames, 1);
    assert_eq!(info.stage_px, (550.0, 400.0));

    let stage_path = scratch("shapes-basic-stage.swf");
    let out = run(&[
        "publish".as_ref(),
        doc.as_os_str(),
        stage_path.as_os_str(),
        "--stage".as_ref(),
        "--no-compress".as_ref(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let bytes = std::fs::read(&stage_path).expect("stage SWF");
    assert_eq!(&bytes[..3], b"FWS");
}

#[test]
fn bad_arguments_and_missing_files() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["frobnicate".as_ref()]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["publish".as_ref(), "only-one".as_ref()]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&[
        "render".as_ref(),
        "x.swf".as_ref(),
        "out".as_ref(),
        "--frames".as_ref(),
        "0".as_ref(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let missing = scratch("does-not-exist");
    let out = run(&[
        "publish".as_ref(),
        missing.as_os_str(),
        scratch("x.swf").as_os_str(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("cannot open"));
    let out = run(&["dump".as_ref(), "--swf".as_ref(), missing.as_os_str()]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["help".as_ref()]);
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("jazari-cli compare"));
}
