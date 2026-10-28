//! Tests for the Al-Jazari patches listed in `PATCHES.md`.
//!
//! The expected bytecode was checked against what the Flash 8 IDE compiles for the same
//! source (disassembled from a SWF it published).

use crate::internal::as2::lexer::Lexer;
use crate::internal::as2::lexer::tokens::{Keyword, TokenKind};
use crate::internal::as2::parser::unescape;
use crate::internal::span::FileId;
use crate::{CompileOptions, ProgramBuilder, SourceProvider};

/// Serves one script, `main.as`, from memory.
struct OneScript(&'static str);

impl SourceProvider for OneScript {
    fn load(&self, path: &str) -> Result<String, std::io::Error> {
        if path == "main.as" {
            Ok(self.0.to_owned())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found",
            ))
        }
    }

    fn is_file(&self, path: &str) -> bool {
        path == "main.as"
    }
}

fn build(source: &'static str) -> Result<crate::Program, crate::Error> {
    let mut builder = ProgramBuilder::new(OneScript(source))
        .with_compile_options(CompileOptions::default().with_swf_version(8));
    builder.add_script("main.as");
    builder.build()
}

/// The compiled script as PCode text.
fn pcode(source: &'static str) -> String {
    let program = build(source).unwrap_or_else(|e| panic!("{}", e.to_string_plain()));
    program
        .compile()
        .initializer
        .map(|actions| actions.to_string())
        .unwrap_or_default()
}

fn error_messages(source: &'static str) -> Vec<String> {
    match build(source) {
        Ok(_) => panic!("expected `{source}` to fail"),
        Err(e) => e.diagnostics().into_iter().map(|d| d.message).collect(),
    }
}

// ---- 1. postfix ++/-- on members and indexes

#[test]
fn postfix_increment_on_member_statement() {
    // Flash 8: Push "o"; GetVariable; Push "a", "o"; GetVariable; Push "a"; GetMember;
    // Increment; SetMember
    assert_eq!(
        pcode("o.a++;"),
        "ConstantPool \"o\", \"a\"\n\
         Push constant0\n\
         GetVariable\n\
         Push constant1, constant0\n\
         GetVariable\n\
         Push constant1\n\
         GetMember\n\
         Increment\n\
         SetMember\n"
    );
}

#[test]
fn postfix_decrement_on_member_value_is_the_old_value() {
    // Flash 8 pushes the old value first, then updates the member.
    assert_eq!(
        pcode("var x = o.a--;"),
        "ConstantPool \"x\", \"o\", \"a\"\n\
         Push constant0, constant1\n\
         GetVariable\n\
         Push constant2\n\
         GetMember\n\
         Push constant1\n\
         GetVariable\n\
         Push constant2, constant1\n\
         GetVariable\n\
         Push constant2\n\
         GetMember\n\
         Decrement\n\
         SetMember\n\
         DefineLocal\n"
    );
}

#[test]
fn postfix_increment_on_index_and_this() {
    let code = pcode("arr[i]++;\nthis._x++;");
    assert!(code.contains("GetMember\nIncrement\nSetMember\n"), "{code}");
    assert_eq!(code.matches("SetMember").count(), 2, "{code}");
}

#[test]
fn postfix_on_member_inside_function() {
    let code = pcode("this.onEnterFrame = function () { this._x++; };");
    assert!(code.contains("Increment\nSetMember"), "{code}");
}

#[test]
fn postfix_on_call_result_is_rejected_like_flash() {
    let message = "Left side of assignment operator must be variable or property.";
    assert_eq!(error_messages("f()++;"), vec![message.to_owned()]);
    assert_eq!(error_messages("getTimer()++;"), vec![message.to_owned()]);
}

#[test]
fn postfix_on_parenthesized_member() {
    let code = pcode("(o.a)++;");
    assert!(code.contains("GetMember\nIncrement\nSetMember"), "{code}");
}

// ---- 2. do-while

#[test]
fn do_while_lexes_do_as_keyword() {
    let kinds: Vec<TokenKind> = Lexer::new("do", FileId::new(0))
        .into_vec()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(kinds, vec![TokenKind::Keyword(Keyword::Do)]);
}

#[test]
fn do_while_runs_body_then_tests_condition() {
    // Flash 8: body; condition; If <start>
    assert_eq!(
        pcode("var n = 0;\ndo {\n\tn++;\n} while (n < 3);"),
        "ConstantPool \"n\"\n\
         Push constant0, 0\n\
         DefineLocal\n\
         loc0000:Push constant0, constant0\n\
         GetVariable\n\
         Increment\n\
         SetVariable\n\
         loc0001:Push constant0\n\
         GetVariable\n\
         Push 3\n\
         Less2\n\
         If loc0000\n\
         loc0002:\n"
    );
}

#[test]
fn do_while_continue_tests_condition_and_break_leaves() {
    let code = pcode("do { if (a) continue; if (b) break; } while (c);");
    // `continue` jumps to the condition (loc0001), `break` past the loop (loc0002).
    assert!(code.contains("Jump loc0001"), "{code}");
    assert!(code.contains("Jump loc0002"), "{code}");
    assert!(code.contains("loc0001:Push"), "{code}");
    assert!(code.trim_end().ends_with("loc0002:"), "{code}");
}

#[test]
fn do_while_braceless_body_and_no_trailing_semicolon() {
    let code = pcode("do n++; while (n < 10)\ntrace(n);");
    assert!(code.contains("If loc0000"), "{code}");
    assert!(code.contains("Trace"), "{code}");
}

#[test]
fn do_while_without_while_is_an_error() {
    assert!(build("do { trace(1); }").is_err());
}

// ---- 3. string escapes

#[test]
fn escapes_unicode_hex_octal() {
    assert_eq!(unescape(r"\u0645\u0631"), "مر");
    assert_eq!(unescape(r"\u00e9\u00E9"), "éé");
    assert_eq!(unescape(r"\x41\x4a\xE9"), "AJé");
    assert_eq!(unescape(r"\101\60x"), "A0x");
    assert_eq!(unescape(r"\377\400"), "\u{ff} 0");
    assert_eq!(unescape(r"a\bb\fc"), "a\u{8}b\u{c}c");
}

#[test]
fn escapes_like_flash_8_when_malformed() {
    // Observed in Flash 8 output: short \u / \x lose the backslash; unknown escapes keep
    // only the backslash.
    assert_eq!(unescape(r"\u12"), "u12");
    assert_eq!(unescape(r"\x4"), "x4");
    assert_eq!(unescape(r"\q\z"), r"\\");
    assert_eq!(unescape(r"\8\9"), r"\\");
}

#[test]
fn escapes_surrogates_and_nul() {
    assert_eq!(unescape(r"\uD83D\uDE00"), "😀");
    assert_eq!(unescape(r"\uD83Dx"), "\u{fffd}x");
    assert_eq!(unescape(r"ab\0cd"), "ab");
}

#[test]
fn escapes_reach_the_constant_pool() {
    let code = pcode(r#"trace("\u0645\x41\101\n");"#);
    assert!(code.starts_with("ConstantPool \"مAA\n\"\n"), "{code:?}");
}

// ---- 4. panics turned into errors

#[test]
fn at_sign_is_a_syntax_error_not_a_panic() {
    let tokens = Lexer::new("@foo;", FileId::new(0)).into_vec();
    assert_eq!(tokens[0].kind, TokenKind::Unknown);
    assert_eq!(tokens[0].raw, "@foo");
    assert!(build("var a = 1;\n@foo;").is_err());
    assert!(build("@").is_err());
}

#[test]
fn invalid_inline_pcode_is_an_error_not_a_panic() {
    assert_eq!(
        error_messages("@PCode { NotAnAction }"),
        vec!["Invalid inline PCode.".to_owned()]
    );
}

#[test]
fn assignment_to_non_lvalue_is_an_error_not_a_panic() {
    let message = "Left side of assignment operator must be variable or property.";
    for source in [
        "\"abc\" = 5;",
        "5++;",
        "--5;",
        "++getTimer();",
        "f() = 1;",
        "null = 1;",
        "x = (1 + 2) -= 3;",
    ] {
        assert_eq!(error_messages(source), vec![message.to_owned()], "{source}");
    }
}

#[test]
fn assignments_to_lvalues_still_compile() {
    for source in [
        "x = 1;",
        "o.a = 1;",
        "o[k] += 1;",
        "_x = 5;",
        "_x++;",
        "function f(a) { a++; return a; }",
    ] {
        assert!(build(source).is_ok(), "{source}");
    }
}

#[test]
fn special_property_delete_call_new_do_not_panic() {
    let code = pcode("delete _x;\n_x();\nnew _y();");
    assert!(code.contains("CallMethod"), "{code}");
    assert!(code.contains("NewMethod"), "{code}");
}

// ---- 5. structured diagnostics

#[test]
fn diagnostics_carry_path_message_and_span() {
    let error = build("var a = 1;\n\"abc\" = 5;").err().expect("error");
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path.as_deref(), Some("main.as"));
    assert_eq!(
        diagnostics[0].message,
        "Left side of assignment operator must be variable or property."
    );
    assert_eq!(diagnostics[0].span, Some(11..16));
}

#[test]
fn diagnostics_for_missing_files_have_no_span() {
    let mut builder = ProgramBuilder::new(OneScript(""));
    builder.add_script("missing.as");
    let error = builder.build().err().expect("error");
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path.as_deref(), Some("missing.as"));
    assert_eq!(diagnostics[0].span, None);
}
