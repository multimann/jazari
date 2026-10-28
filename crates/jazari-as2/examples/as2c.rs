//! Compiles an ActionScript 2 file and prints the disassembled bytecode.
//!
//! ```text
//! cargo run -p jazari-as2 --example as2c -- [--frame|--button|--clip] FILE
//! cargo run -p jazari-as2 --example as2c -- --hook
//! ```

use jazari_as2::disasm::{disassemble, listing};
use jazari_as2::{Compiler, CompilerOptions, Origin, stage_goto_hook};
use std::process::ExitCode;

fn print(actions: &[u8]) {
    match disassemble(actions, 8) {
        Ok(instructions) => print!("{}", listing(&instructions)),
        Err(e) => eprintln!("disassembly failed: {e}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--hook") {
        return match stage_goto_hook() {
            Ok(actions) => {
                print(&actions);
                ExitCode::SUCCESS
            }
            Err(diagnostics) => {
                for d in diagnostics {
                    eprintln!("{d}");
                }
                ExitCode::FAILURE
            }
        };
    }
    let (mode, path) = match args.as_slice() {
        [path] => ("--frame", path),
        [mode, path] => (mode.as_str(), path),
        _ => {
            eprintln!("usage: as2c [--frame|--button|--clip] FILE | --hook");
            return ExitCode::FAILURE;
        }
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let options = CompilerOptions {
        include_base: std::path::Path::new(path).parent().map(Into::into),
        ..CompilerOptions::default()
    };
    let compiler = Compiler::new(options);
    let origin = Origin {
        file: Some(path.clone()),
        ..Origin::default()
    };
    let result = match mode {
        "--frame" => compiler.compile_frame(&source, &origin).map(|actions| {
            print(&actions);
        }),
        "--button" => compiler
            .compile_button_handlers(&source, &origin)
            .map(|handlers| {
                for handler in handlers {
                    println!("on {:?} key {:?}:", handler.conditions, handler.key_code);
                    print(&handler.actions);
                }
            }),
        "--clip" => compiler
            .compile_clip_handlers(&source, &origin)
            .map(|handlers| {
                for handler in handlers {
                    println!("on {:?} key {:?}:", handler.events, handler.key_code);
                    print(&handler.actions);
                }
            }),
        other => {
            eprintln!("unknown mode {other}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(diagnostics) => {
            for d in diagnostics {
                eprintln!("{d}");
            }
            ExitCode::FAILURE
        }
    }
}
