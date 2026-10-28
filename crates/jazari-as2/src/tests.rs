//! Tests of the compiler as a whole: frame scripts, handlers, diagnostics,
//! isolation, the cache and a comparison with the Flash 8 IDE's output.

use crate::disasm::{Instruction, Operand, disassemble, listing};
use crate::{Compiler, CompilerOptions, Diagnostic, Origin};
use std::path::{Path, PathBuf};
use swf::{ButtonActionCondition as Cond, ClipEventFlag as Clip};

/// An `#include` path that makes the test provider panic inside Rascal.
pub(crate) const PANIC_INCLUDE: &str = "__jazari_test_panic__.as";

fn compiler() -> Compiler {
    Compiler::new(CompilerOptions::default())
}

fn origin() -> Origin {
    Origin {
        timeline: Some("Scene 1".to_owned()),
        layer: Some("actions".to_owned()),
        frame: Some(0),
        ..Origin::default()
    }
}

fn code(actions: &[u8]) -> String {
    listing(&disassemble(actions, 8).expect("valid bytecode"))
}

fn frame(src: &str) -> String {
    code(&compiler().compile_frame(src, &origin()).expect("compiles"))
}

/// (line, column, message) of each diagnostic.
fn positions(diagnostics: &[Diagnostic]) -> Vec<(u32, u32, String)> {
    diagnostics
        .iter()
        .map(|d| (d.line, d.column, d.message.clone()))
        .collect()
}

fn frame_errors(src: &str) -> Vec<(u32, u32, String)> {
    positions(&compiler().compile_frame(src, &origin()).expect_err("fails"))
}

/// A fresh directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("jazari-as2-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dirs");
        }
        std::fs::write(path, contents).expect("write file");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- frames

#[test]
fn frame_script_compiles_to_a_do_action_payload() {
    let actions = compiler()
        .compile_frame("trace(\"hi\");\nstop();", &origin())
        .expect("compiles");
    assert_eq!(actions.last(), Some(&0), "ends with ActionEnd");
    assert_eq!(
        code(&actions),
        "ConstantPool \"hi\"\nPush \"hi\"\nTrace\nStop\nEnd\n"
    );
}

#[test]
fn frames_without_code_have_no_payload() {
    for src in ["", "  \r\n\t", "// nothing\n/* at all */"] {
        assert_eq!(
            compiler().compile_frame(src, &origin()),
            Ok(Vec::new()),
            "{src:?}"
        );
    }
}

#[test]
fn byte_order_mark_is_stripped() {
    assert_eq!(frame("\u{feff}trace(1);"), frame("trace(1);"));
    assert_eq!(
        frame_errors("\u{feff}x = ;"),
        [(1, 5, "invalid expression".to_owned())]
    );
}

#[test]
fn arabic_string_literals_survive() {
    assert!(frame("trace(\"مرحبا بالعالم\");").contains("\"مرحبا بالعالم\""));
}

#[test]
fn patched_rascal_syntax_compiles() {
    let listing = frame("this._x++;\nvar i = 3;\ndo { i--; } while (i > 0);");
    assert!(
        listing.contains("GetMember\nIncrement\nSetMember"),
        "{listing}"
    );
    assert!(listing.contains("Decrement"), "{listing}");
    // "\u0645" and "\x41", spelled with a computed backslash.
    let escapes = format!("trace(\"{b}u0645{b}x41{b}101\");", b = '\\');
    assert!(frame(&escapes).contains("\"مAA\""));
}

#[test]
fn error_positions_are_true_lines_and_character_columns() {
    let cases: &[(&str, (u32, u32), &str)] = &[
        ("var a = 1;\nvar b = ;", (2, 9), "invalid expression"),
        ("var s = \"مر\";  x = = 2;", (1, 20), "invalid expression"),
        ("trace(1", (1, 8), "invalid expression: expected `)`"),
        (
            "if (x) {\n trace(1);\n",
            (2, 11),
            "invalid if statement: expected `}`",
        ),
        (
            "var ok = 1;\r\n\"abc\" = 5;",
            (2, 1),
            "Left side of assignment operator must be variable or property.",
        ),
        ("do { trace(1); } while (", (1, 25), "invalid do-while loop"),
    ];
    for &(src, (line, column), message) in cases {
        assert_eq!(
            frame_errors(src),
            [(line, column, message.to_owned())],
            "{src:?}"
        );
    }
}

#[test]
fn diagnostics_carry_the_origin() {
    let diagnostics = compiler()
        .compile_frame("var a = 1;\nvar b = ;", &origin())
        .expect_err("fails");
    assert_eq!(diagnostics[0].origin, origin());
    assert_eq!(
        diagnostics[0].to_string(),
        "error: Scene 1 › layer 'actions' › frame 1:2:9: invalid expression"
    );
}

#[test]
fn at_sign_is_rejected_before_rascal() {
    assert_eq!(
        frame_errors("var a = 1;\n  @foo;"),
        [(2, 3, "Unexpected '@' encountered".to_owned())]
    );
    assert_eq!(
        frame_errors("@PCode { Stop }"),
        [(1, 1, "Unexpected '@' encountered".to_owned())]
    );
    assert!(frame("trace(\"a@b\"); // @x\n/* @y */").contains("\"a@b\""));
}

#[test]
fn frames_reject_handler_blocks() {
    assert_eq!(
        frame_errors("on (press) { trace(1); }"),
        [(
            1,
            1,
            "Mouse events are permitted only for button instances".to_owned()
        )]
    );
    assert_eq!(
        frame_errors("x = 1;\nonClipEvent (load) { }"),
        [(
            2,
            1,
            "Clip events are permitted only for movie clip instances".to_owned()
        )]
    );
    // A call to a function named `on` is not a handler.
    assert!(frame("on(1);").contains("CallFunction"));
}

#[test]
fn includes_resolve_against_the_include_base() {
    let dir = TempDir::new("include");
    dir.write("inc.as", "trace(\"from include\");\n");
    dir.write("sub/bad.as", "trace(1);\nvar = 3;\n");
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        ..CompilerOptions::default()
    });
    let actions = compiler
        .compile_frame("#include \"inc.as\"\ntrace(2);", &origin())
        .expect("compiles");
    assert!(code(&actions).contains("\"from include\""));

    let diagnostics = compiler
        .compile_frame("trace(0);\n#include \"sub/bad.as\"", &origin())
        .expect_err("fails");
    assert_eq!(
        positions(&diagnostics),
        [(
            2,
            5,
            "invalid variable name: expected identifier".to_owned()
        )]
    );
    assert_eq!(diagnostics[0].origin.file.as_deref(), Some("sub/bad.as"));

    assert_eq!(
        positions(
            &compiler
                .compile_frame("x = 1;\n#include \"missing.as\"", &origin())
                .expect_err("fails")
        ),
        [(
            2,
            1,
            "Could not load file 'missing.as': File not found".to_owned()
        )]
    );
}

#[test]
fn lone_carriage_returns_end_line_comments() {
    // Rascal ends `//` comments only at `\n`: with Flash 8's lone `\r` line
    // breaks the rest of the script used to vanish without an error.
    let listing = frame("// first\rtrace(1);\r// second\rtrace(2);\r");
    assert!(
        listing.contains("Push 1\nTrace") && listing.contains("Push 2\nTrace"),
        "{listing}"
    );
    let handlers = buttons("on (release) {\r\t// comment\r\ttrace(\"r\");\r}\r");
    assert_eq!(handlers.len(), 1);
    assert!(code(&handlers[0].actions).contains("\"r\""));
    assert_eq!(
        frame_errors("// c\rx = ;"),
        [(2, 5, "invalid expression".to_owned())]
    );
}

#[test]
fn windows_style_include_paths_resolve() {
    // `\u` and `\t` would otherwise be decoded as string escapes.
    let dir = TempDir::new("backslash");
    dir.write(
        "lib/util.as",
        "#include \"lib\\tail.as\"\ntrace(\"util\");\n",
    );
    dir.write("lib/tail.as", "trace(\"tail\");\n");
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        ..CompilerOptions::default()
    });
    let listing = code(
        &compiler
            .compile_frame("#include \"lib\\util.as\"", &origin())
            .expect("compiles"),
    );
    assert!(
        listing.contains("\"util\"") && listing.contains("\"tail\""),
        "{listing}"
    );
}

#[test]
fn legacy_encoded_includes_are_decoded() {
    let dir = TempDir::new("legacy");
    dir.write("ar.as", b"trace(\"\xe3\xd1\xcd\xc8\xc7\");\n"); // Windows-1256
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        legacy_encoding: Some("windows-1256".to_owned()),
        ..CompilerOptions::default()
    });
    let actions = compiler
        .compile_frame("#include \"ar.as\"", &origin())
        .expect("compiles");
    assert!(code(&actions).contains("\"مرحبا\""));
}

#[test]
fn classes_compile_against_stubs_then_into_modules() {
    let dir = TempDir::new("classes");
    dir.write(
        "com/x/Foo.as",
        "class com.x.Foo {\n\tstatic function main() { trace(\"main\"); }\n\tfunction hello() { trace(\"hi\"); }\n}\n",
    );
    let compiler = Compiler::new(CompilerOptions {
        class_paths: vec![dir.path().to_owned()],
        ..CompilerOptions::default()
    });
    let script = code(
        &compiler
            .compile_frame("var f = new com.x.Foo();\nf.hello();", &origin())
            .expect("compiles"),
    );
    assert!(script.contains("NewMethod"), "{script}");
    assert!(
        !script.contains("\"main\""),
        "no MTASC entry call: {script}"
    );
    assert_eq!(compiler.referenced_classes(), ["com.x.Foo"]);

    let modules = compiler
        .compile_classes(&compiler.referenced_classes())
        .expect("classes compile");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].name, "com.x.Foo");
    let module = code(&modules[0].actions);
    assert!(
        module.contains("\"hi\"") && module.contains("\"main\""),
        "{module}"
    );

    let missing = compiler
        .compile_classes(&["com.x.Missing".to_owned()])
        .expect_err("missing class");
    assert!(missing[0].message.contains("File not found"), "{missing:?}");
}

#[test]
fn panics_inside_rascal_become_diagnostics() {
    let compiler = compiler();
    let diagnostics = compiler
        .compile_frame(&format!("#include \"{PANIC_INCLUDE}\""), &origin())
        .expect_err("panics");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].message,
        "internal compiler error (Rascal panicked): test panic inside Rascal's build"
    );
    assert_eq!(diagnostics[0].origin, origin());
    // The compiler keeps working afterwards.
    assert!(compiler.compile_frame("trace(1);", &origin()).is_ok());
}

#[test]
fn an_action_over_64_kib_is_an_error() {
    let src = format!("var s = \"{}\";", "x".repeat(70_000));
    let errors = frame_errors(&src);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].2.starts_with("cannot encode the compiled script"),
        "{errors:?}"
    );
}

#[test]
fn runaway_nesting_is_rejected_and_deep_nesting_compiles() {
    let deep = |n: usize| format!("x = {}1{};", "(".repeat(n), ")".repeat(n));
    assert_eq!(
        frame_errors(&deep(1001)),
        [(
            1,
            1005,
            "Brackets are nested more than 1000 levels deep".to_owned()
        )]
    );
    assert!(compiler().compile_frame(&deep(400), &origin()).is_ok());
    let long_sum = format!("x = 0{};", " + 1".repeat(5_000));
    assert!(compiler().compile_frame(&long_sum, &origin()).is_ok());
}

#[test]
fn long_chains_do_not_overflow_the_compiler_stack() {
    // Each of these aborted the process (a stack overflow cannot be caught)
    // when the compiler thread had 32 MiB of stack.
    let deepest = format!("x = {}1{};", "(".repeat(1000), ")".repeat(1000));
    assert!(compiler().compile_frame(&deepest, &origin()).is_ok());
    let else_ifs = format!("if (a) {{}}{}", " else if (a) {}".repeat(5_000));
    // Too long for 16-bit jumps, but it must come back as a diagnostic.
    assert!(compiler().compile_frame(&else_ifs, &origin()).is_err());
    let sum = format!("x = a{};", " + a".repeat(10_000));
    assert!(compiler().compile_frame(&sum, &origin()).is_ok());
}

#[test]
fn cache_reuses_results_and_watches_includes() {
    let dir = TempDir::new("cache");
    dir.write("inc.as", "trace(\"one\");\n");
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        ..CompilerOptions::default()
    });
    let src = "#include \"inc.as\"";
    let first = compiler.compile_frame(src, &origin()).expect("compiles");
    let second = compiler.compile_frame(src, &origin()).expect("compiles");
    assert_eq!(first, second);
    assert_eq!(compiler.cached_results(), 1);

    dir.write("inc.as", "trace(\"two\");\n");
    let third = compiler.compile_frame(src, &origin()).expect("compiles");
    assert!(code(&third).contains("\"two\""));
    assert_eq!(compiler.cached_results(), 1);

    // Errors are cached too, and still get each caller's origin.
    let other = Origin {
        frame: Some(4),
        ..origin()
    };
    assert!(compiler.compile_frame("x = ;", &origin()).is_err());
    let again = compiler.compile_frame("x = ;", &other).expect_err("fails");
    assert_eq!(again[0].origin, other);
    assert_eq!(compiler.cached_results(), 2);

    compiler.clear_cache();
    assert_eq!(compiler.cached_results(), 0);
    let uncached = Compiler::new(CompilerOptions {
        cache: false,
        ..CompilerOptions::default()
    });
    assert!(uncached.compile_frame("trace(1);", &origin()).is_ok());
    assert_eq!(uncached.cached_results(), 0);
}

#[test]
fn cached_missing_include_error_ends_when_the_file_appears() {
    let dir = TempDir::new("late-include");
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        ..CompilerOptions::default()
    });
    let src = "#include \"late.as\"";
    assert!(compiler.compile_frame(src, &origin()).is_err());
    assert!(compiler.compile_frame(src, &origin()).is_err());
    assert_eq!(compiler.cached_results(), 1);
    dir.write("late.as", "trace(\"late\");\n");
    let actions = compiler
        .compile_frame(src, &origin())
        .expect("compiles now");
    assert!(code(&actions).contains("\"late\""));
}

#[test]
fn included_handler_blocks_are_rejected() {
    // Rascal would compile them as a call to `on` and a plain block.
    let dir = TempDir::new("include-handler");
    dir.write("h.as", "// handlers\non (press) { trace(1); }\n");
    let compiler = Compiler::new(CompilerOptions {
        include_base: Some(dir.path().to_owned()),
        ..CompilerOptions::default()
    });
    let errors = positions(
        &compiler
            .compile_frame("x = 1;\n#include \"h.as\"", &origin())
            .expect_err("fails"),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!((errors[0].0, errors[0].1), (2, 1));
    assert!(errors[0].2.contains("line 2"), "{errors:?}");
    let errors = compiler
        .compile_button_handlers("on (release) {\n\t#include \"h.as\"\n}", &origin())
        .expect_err("fails");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn one_compiler_serves_many_threads() {
    fn assert_sync<T: Sync + Send>() {}
    assert_sync::<Compiler>();
    let compiler = compiler();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let compiler = &compiler;
                scope.spawn(move || compiler.compile_frame(&format!("trace({i});"), &origin()))
            })
            .collect();
        for handle in handles {
            assert!(handle.join().expect("no panic").is_ok());
        }
    });
}

// ---------------------------------------------------------------- buttons

fn buttons(src: &str) -> Vec<crate::ButtonHandler> {
    compiler()
        .compile_button_handlers(src, &origin())
        .expect("compiles")
}

fn button_errors(src: &str) -> Vec<(u32, u32, String)> {
    positions(
        &compiler()
            .compile_button_handlers(src, &origin())
            .expect_err("fails"),
    )
}

#[test]
fn button_handler_of_the_as2_fixture() {
    let handlers = buttons("on (release) {\n\ttrace(\"my_btn: release\");\n}\n");
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].conditions, Cond::OVER_DOWN_TO_OVER_UP);
    assert_eq!(handlers[0].key_code, None);
    assert_eq!(
        code(&handlers[0].actions),
        "ConstantPool \"my_btn: release\"\nPush \"my_btn: release\"\nTrace\nEnd\n"
    );
}

#[test]
fn button_records_match_flash_8() {
    // The probe instance `btn_all`; Flash 8 wrote these seven records, in
    // this order, and dropped the handlers with empty bodies.
    let src = "on (press, release, releaseOutside, rollOver, rollOut, dragOver, dragOut) {\n\ttrace(\"all\");\n}\n\
               on (keyPress \"<Enter>\") { trace(\"enter\"); }\n\
               on (keyPress \"a\") { trace(\"a\"); }\n\
               on (keyPress \"<Left>\") { trace(\"left\"); }\n\
               on (keyPress \"<PageUp>\") {}\n\
               on (keyPress \"<Space>\") { trace(\"space\"); }\n\
               on (keyPress \"<Escape>\") { trace(\"esc\"); }\n\
               on (keyPress \"A\") { trace(\"A\"); }\n\
               on (keyPress \"<Home>\") {} on (keyPress \"<End>\") {} on (keyPress \"<Insert>\") {}\n";
    let handlers = buttons(src);
    let summary: Vec<(u16, Option<u8>)> = handlers
        .iter()
        .map(|h| (h.conditions.bits(), h.key_code))
        .collect();
    assert_eq!(
        summary,
        [
            (0x1ff, None),
            (13 << 9, Some(13)),
            (97 << 9, Some(97)),
            (1 << 9, Some(1)),
            (32 << 9, Some(32)),
            (19 << 9, Some(19)),
            (65 << 9, Some(65)),
        ]
    );
    assert!(code(&handlers[4].actions).contains("\"space\""));
}

#[test]
fn button_handlers_without_code_are_left_out() {
    assert_eq!(
        buttons("on (press) {\n\t// nothing\n}\non (release) { /* x */ }\n"),
        []
    );
    assert_eq!(buttons("// only a comment\n"), []);
}

#[test]
fn button_errors_keep_true_positions() {
    assert_eq!(
        button_errors("on (release) {\n\ttrace(1;\n}\n"),
        [(2, 9, "invalid expression: expected `)`".to_owned())]
    );
    assert_eq!(
        button_errors("// header\n\non (press) {\n\tx = 1;\n\n\tx = = 2;\n}"),
        [(6, 6, "invalid expression".to_owned())]
    );
    // Multi-byte characters blanked before the body on the same line count
    // as one column each.
    assert_eq!(
        button_errors("/* زر */ on (press) { x = = 2; }"),
        [(1, 27, "invalid expression".to_owned())]
    );
    assert_eq!(
        clip_errors("onClipEvent (load) { s = \"م\"; } on (keyPress \"<Enter>\") { y = ; }"),
        [(1, 63, "invalid expression".to_owned())]
    );
}

#[test]
fn button_rejects_clip_events_stray_code_and_bad_keys() {
    assert_eq!(
        button_errors("onClipEvent (load) { trace(3); }"),
        [(
            1,
            1,
            "Clip events are permitted only for movie clip instances".to_owned()
        )]
    );
    assert_eq!(
        button_errors("trace(\"x\");\non (release) { trace(4); }"),
        [(1, 1, "Statement must appear within on handler".to_owned())]
    );
    assert_eq!(
        button_errors("on (foo) { trace(1); }"),
        [(1, 5, "Invalid mouse event specified.".to_owned())]
    );
    assert_eq!(
        button_errors("on (keyPress \"<Foo>\") { trace(1); }"),
        [(1, 14, "Invalid key code".to_owned())]
    );
    let arabic = button_errors("on (keyPress \"م\") { trace(1); }");
    assert!(arabic[0].2.contains("U+0645"), "{arabic:?}");
}

#[test]
fn every_error_of_an_instance_is_reported() {
    assert_eq!(
        button_errors("on (foo) {\n\tx = ;\n}\non (press) {\n\ty = ;\n}"),
        [
            (1, 5, "Invalid mouse event specified.".to_owned()),
            (2, 6, "invalid expression".to_owned()),
            (5, 6, "invalid expression".to_owned()),
        ]
    );
}

#[test]
fn instance_scripts_strip_the_bom_and_reject_at_signs() {
    assert_eq!(buttons("\u{feff}on (press) { trace(1); }").len(), 1);
    assert_eq!(
        button_errors("on (press) {\n\t@x;\n}"),
        [(2, 2, "Unexpected '@' encountered".to_owned())]
    );
}

// ---------------------------------------------------------------- clips

fn clips(src: &str) -> Vec<crate::ClipHandler> {
    compiler()
        .compile_clip_handlers(src, &origin())
        .expect("compiles")
}

fn clip_errors(src: &str) -> Vec<(u32, u32, String)> {
    positions(
        &compiler()
            .compile_clip_handlers(src, &origin())
            .expect_err("fails"),
    )
}

#[test]
fn clip_handlers_of_the_as2_fixture() {
    let handlers = clips(AS2_CLIP);
    let events: Vec<Clip> = handlers.iter().map(|h| h.events).collect();
    assert_eq!(events, [Clip::LOAD, Clip::ENTER_FRAME]);
    assert!(code(&handlers[1].actions).contains("GetMember\nIncrement\nSetMember"));
}

#[test]
fn every_clip_event_compiles_like_flash_8() {
    // The probe instance `mc_all`: Flash 8 kept the empty handlers.
    let src = "onClipEvent (load) { trace(\"load\"); }\nonClipEvent (enterFrame) { }\n\
               onClipEvent (unload) { trace(1); }\nonClipEvent (mouseMove) { trace(2); }\n\
               onClipEvent (mouseDown) { trace(3); }\nonClipEvent (mouseUp) { trace(4); }\n\
               onClipEvent (keyDown) { trace(5); }\nonClipEvent (keyUp) { trace(6); }\n\
               onClipEvent (data) { trace(7); }\nonClipEvent (initialize) { trace(8); }\n\
               onClipEvent (construct) { trace(9); }\non (press) { trace(10); }\n\
               on (release, releaseOutside) { trace(11); }\n\
               on (rollOver) {} on (rollOut) {} on (dragOver) {} on (dragOut) {}\n\
               on (keyPress \"<Space>\") { trace(12); }\non (keyPress \"x\") { trace(13); }\n";
    let handlers = clips(src);
    let summary: Vec<(Clip, Option<u8>)> =
        handlers.iter().map(|h| (h.events, h.key_code)).collect();
    assert_eq!(
        summary,
        [
            (Clip::LOAD, None),
            (Clip::ENTER_FRAME, None),
            (Clip::UNLOAD, None),
            (Clip::MOUSE_MOVE, None),
            (Clip::MOUSE_DOWN, None),
            (Clip::MOUSE_UP, None),
            (Clip::KEY_DOWN, None),
            (Clip::KEY_UP, None),
            (Clip::DATA, None),
            (Clip::INITIALIZE, None),
            (Clip::CONSTRUCT, None),
            (Clip::PRESS, None),
            (Clip::RELEASE | Clip::RELEASE_OUTSIDE, None),
            (Clip::ROLL_OVER, None),
            (Clip::ROLL_OUT, None),
            (Clip::DRAG_OVER, None),
            (Clip::DRAG_OUT, None),
            (Clip::KEY_PRESS, Some(32)),
            (Clip::KEY_PRESS, Some(120)),
        ]
    );
    assert_eq!(handlers[1].actions, [0], "empty handler is just ActionEnd");
}

#[test]
fn braces_in_strings_and_comments_stay_in_the_body() {
    let src = "// a comment with } brace\nonClipEvent (load) {\n\tvar s = \"}{\"; /* } */\n\tif (true) { trace(s); }\n\ttrace('it\\'s }');\n}\n";
    let handlers = clips(src);
    assert_eq!(handlers.len(), 1);
    let listing = code(&handlers[0].actions);
    assert!(
        listing.contains("\"}{\"") && listing.contains("\"it's }\""),
        "{listing}"
    );
}

#[test]
fn clip_errors_match_flash_8_messages() {
    assert_eq!(
        clip_errors("onClipEvent (load, enterFrame) { trace(1); }"),
        [(1, 18, "')' expected".to_owned())]
    );
    assert_eq!(
        clip_errors("trace(\"x\");\nonClipEvent (load) { trace(1); }"),
        [(
            1,
            1,
            "Statement must appear within on/onClipEvent handler".to_owned()
        )]
    );
    assert_eq!(
        clip_errors("onClipEvent (press) { trace(1); }"),
        [(1, 14, "Invalid movie clip event specified.".to_owned())]
    );
    assert_eq!(
        clip_errors("onClipEvent (load) {\n\ton (release) { trace(1); }\n}\n"),
        [(
            2,
            2,
            "on handlers may not nest within other on handlers".to_owned()
        )]
    );
    let component = clip_errors("on (load) { trace(1); }");
    assert!(
        component[0].2.contains("component event listener"),
        "{component:?}"
    );
}

// ---------------------------------------------------------------- Flash 8 oracle

/// The scripts of the `as2` fixture (C:\dev\samples\fixtures\as2), as the
/// Flash 8 IDE saved them.
const AS2_FRAMES: [(usize, &str); 4] = [
    (
        0,
        "var passes = 0;\nfunction greet(who) {\n\treturn \"Hello, \" + who + \"!\";\n}\nfunction add(a, b) {\n\treturn a + b;\n}\ntrace(\"frame 1: \" + greet(\"Flash 8\"));\ntrace(\"frame 1: add(2, 3) = \" + add(2, 3));\ntrace(\"frame 1: typeof my_btn = \" + typeof my_btn + \", ticker_mc._name = \" + ticker_mc._name);\nscoreText = \"score: 0\";\n",
    ),
    (
        4,
        "passes++;\ntrace(\"frame 5 [loop]: pass \" + passes);\nscoreText = \"score: \" + (passes * 10);\n",
    ),
    (
        7,
        "if (passes < 2) {\n\ttrace(\"frame 8: gotoAndPlay(\\\"loop\\\")\");\n\tgotoAndPlay(\"loop\");\n} else {\n\ttrace(\"frame 8: continue\");\n}\n",
    ),
    (
        11,
        "trace(\"frame 12 [end]: _currentframe = \" + _currentframe + \", scoreText = \" + scoreText);\nstop();\n",
    ),
];
const AS2_BUTTON: &str = "on (release) {\n\ttrace(\"my_btn: release\");\n}\n";
const AS2_CLIP: &str = "onClipEvent (load) {\n\tthis.n = 0;\n\ttrace(\"ticker_mc: load\");\n}\nonClipEvent (enterFrame) {\n\tif (this.n < 3) {\n\t\tthis.n++;\n\t\ttrace(\"ticker_mc: enterFrame \" + this.n);\n\t}\n}\n";

/// The listing in a form two compilers can agree on: no constant pools,
/// adjacent pushes merged, whole numbers as integers, registers named after
/// what they hold, jumps as instruction indices.
pub(crate) fn normalized(actions: &[u8]) -> Vec<String> {
    let instructions: Vec<Instruction> = disassemble(actions, 8)
        .expect("valid bytecode")
        .into_iter()
        .filter(|i| i.name != "ConstantPool")
        .collect();
    let mut registers: Vec<Vec<(u8, String)>> = vec![Vec::new()];
    let mut out: Vec<String> = Vec::new();
    let mut merged_push: Option<(usize, Vec<String>)> = None;
    for (index, instruction) in instructions.iter().enumerate() {
        registers.truncate(instruction.depth + 1);
        let operand = |o: &Operand| match o {
            Operand::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
            Operand::Register(r) => registers
                .last()
                .and_then(|regs| regs.iter().find(|(reg, _)| reg == r))
                .map_or(format!("r{r}"), |(_, name)| format!("<{name}>")),
            other => other.to_string(),
        };
        if instruction.name == "Push" {
            let values = instruction.operands.iter().map(operand);
            match &mut merged_push {
                Some((depth, pushed)) if *depth == instruction.depth => pushed.extend(values),
                _ => {
                    if let Some((depth, pushed)) = merged_push.take() {
                        out.push(format!("{depth}:Push {}", pushed.join(", ")));
                    }
                    merged_push = Some((instruction.depth, values.collect()));
                }
            }
            continue;
        }
        if let Some((depth, pushed)) = merged_push.take() {
            out.push(format!("{depth}:Push {}", pushed.join(", ")));
        }
        let text = match instruction.name.as_str() {
            "If" | "Jump" => {
                // Jumps only occur at one level in these scripts; an If or
                // Jump action is 5 bytes long.
                let Some(Operand::Int(delta)) = instruction.operands.first() else {
                    panic!("jump without offset");
                };
                let target = (instruction.offset as i64 + 5 + i64::from(*delta)) as usize;
                let to = instructions[index..]
                    .iter()
                    .chain(instructions[..index].iter())
                    .position(|i| i.depth == instruction.depth && i.offset == target)
                    .map_or("end".to_owned(), |p| format!("+{p}"));
                format!("{} {to}", instruction.name)
            }
            _ => {
                let ops: Vec<String> = instruction.operands.iter().map(operand).collect();
                format!("{} {}", instruction.name, ops.join(", "))
            }
        };
        out.push(format!("{}:{}", instruction.depth, text.trim_end()));
        if instruction.name == "DefineFunction2" {
            registers.push(instruction.registers.clone());
        } else if instruction.name == "DefineFunction" {
            registers.push(Vec::new());
        }
    }
    if let Some((depth, pushed)) = merged_push.take() {
        out.push(format!("{depth}:Push {}", pushed.join(", ")));
    }
    out
}

#[test]
fn normalization_merges_pushes_and_resolves_jumps() {
    let a = normalized(
        &compiler()
            .compile_frame("if (x) { trace(1); }", &origin())
            .expect("ok"),
    );
    assert_eq!(
        a,
        [
            "0:Push \"x\"",
            "0:GetVariable",
            "0:Not",
            "0:If +3",
            "0:Push 1",
            "0:Trace",
            "0:End"
        ]
    );
}

/// Compares our output for the `as2` fixture with the SWF the Flash 8 IDE
/// published. Skipped when the fixture is not on this machine.
#[test]
fn as2_fixture_matches_flash_8() {
    let path = Path::new(r"C:\dev\samples\fixtures\as2\as2.swf");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skipped: {} not found", path.display());
        return;
    };
    let buf = swf::decompress_swf(&bytes[..]).expect("valid SWF");
    let parsed = swf::parse_swf(&buf).expect("valid SWF");

    let mut frame = 0usize;
    let mut flash_frames = Vec::new();
    let mut flash_button = None;
    let mut flash_clip = None;
    for tag in &parsed.tags {
        match tag {
            swf::Tag::DoAction(data) => flash_frames.push((frame, data.to_vec())),
            swf::Tag::ShowFrame => frame += 1,
            swf::Tag::DefineButton2(button) => {
                flash_button = button
                    .actions
                    .first()
                    .map(|a| (a.conditions, a.action_data.to_vec()));
            }
            swf::Tag::PlaceObject(place) => {
                if let Some(actions) = &place.clip_actions {
                    flash_clip = Some(
                        actions
                            .records
                            .iter()
                            .map(|a| (a.events, a.action_data.to_vec()))
                            .collect::<Vec<_>>(),
                    );
                }
            }
            _ => {}
        }
    }

    let compiler = compiler();
    assert_eq!(flash_frames.len(), AS2_FRAMES.len());
    for ((flash_frame, flash), (our_frame, src)) in flash_frames.iter().zip(AS2_FRAMES) {
        assert_eq!(*flash_frame, our_frame);
        let ours = compiler.compile_frame(src, &origin()).expect("compiles");
        assert_eq!(
            normalized(&ours),
            normalized(flash),
            "frame {}",
            our_frame + 1
        );
    }

    let (conditions, flash) = flash_button.expect("the fixture has a button");
    let ours = compiler
        .compile_button_handlers(AS2_BUTTON, &origin())
        .expect("compiles");
    assert_eq!(ours[0].conditions, conditions);
    assert_eq!(normalized(&ours[0].actions), normalized(&flash));

    let flash = flash_clip.expect("the fixture has clip actions");
    let ours = compiler
        .compile_clip_handlers(AS2_CLIP, &origin())
        .expect("compiles");
    assert_eq!(ours.len(), flash.len());
    for (our, (events, actions)) in ours.iter().zip(&flash) {
        assert_eq!(our.events, *events);
        assert_eq!(normalized(&our.actions), normalized(actions));
    }
}

/// The scenes of the scenes fixture: Intro (root frames 1-4), Main (5-9) and
/// Outro (10-13).
fn fixture_scenes(current: usize) -> crate::SceneContext {
    crate::SceneContext {
        scenes: vec![
            ("Intro".to_owned(), 0),
            ("Main".to_owned(), 4),
            ("Outro".to_owned(), 9),
        ],
        current,
    }
}

fn scene_frame(src: &str, current: usize) -> String {
    let scenes = fixture_scenes(current);
    code(
        &compiler()
            .compile_frame_in(src, &origin(), Some(&scenes))
            .expect("compiles"),
    )
}

#[test]
fn scene_gotos_compile_as_flash_8_does() {
    // tests/fixtures/scenes (MANIFEST.md, "SWF"): Flash 8's compiled forms.
    let next = scene_frame("nextScene();", 0);
    let prev = scene_frame("prevScene();", 2);
    assert_eq!(next, prev);
    assert!(next.contains("GotoFrame 4"), "{next}");
    assert!(!next.contains("Play"), "{next}");
    let bare = scene_frame("gotoAndPlay(4);", 1);
    assert!(
        bare.contains("GotoFrame 7") && bare.contains("Play"),
        "{bare}"
    );
    let named = scene_frame("gotoAndPlay(\"Intro\", 3);", 1);
    assert!(named.contains("GotoFrame 2"), "{named}");
    let label = scene_frame("gotoAndPlay(\"Outro\", \"outro\");", 1);
    assert!(
        label.contains("GotoLabel") && label.contains("outro"),
        "{label}"
    );
    // The same script in a symbol timeline (no scenes) is unaffected, and
    // the cache keeps the two apart.
    assert!(frame("gotoAndPlay(4);").contains("GotoFrame 3"));
    assert!(scene_frame("gotoAndPlay(4);", 1).contains("GotoFrame 7"));
}
