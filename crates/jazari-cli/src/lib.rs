//! The Al-Jazari command line (`docs/ARCHITECTURE.md`, section 11), as a
//! library shared by the `jazari-cli` binary and its golden tests.
//!
//! ```text
//! jazari-cli dump <doc> [--scripts]
//! jazari-cli dump --swf <file.swf> [--full]
//! jazari-cli publish <doc> <out.swf> [--stage] [--no-compress] [--strict]
//! jazari-cli render <file.swf> <outdir> [--frames 1,5,10|all] [--scale 1]
//! jazari-cli compare <doc> <reference.swf> <outdir> [--frames ...]
//! ```
//!
//! | module | role |
//! |---|---|
//! | [`commands`] | the commands, their output and exit codes |
//! | [`args`] | hand-written argument and frame-list parsing |
//! | [`docdump`] | document model summary |
//! | [`swfdump`] | SWF tag dump with AVM1 disassembly |
//! | [`publishing`] | open and publish as the app does |
//! | [`compare`] | render two SWFs, diff frames and traces, report |
//! | [`imagediff`] | pixel statistics, diff image, side-by-side composite |
//! | [`traces`] | `trace()` sequences, Flash 8 logs, line diffs |
//!
//! Rendering always uses the software adapter (WARP, lavapipe or
//! llvmpipe), through one [`jazari_preview::OffscreenRenderer`] per process.

#![warn(missing_docs)]

/// Like `println!`, but a closed stdout (`jazari-cli ... | head`) is
/// ignored instead of aborting the command halfway.
#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => {{
        use ::std::io::Write as _;
        let _ = ::std::writeln!(::std::io::stdout(), $($arg)*);
    }};
}

/// Like `eprintln!`, but a closed stderr is ignored.
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {{
        use ::std::io::Write as _;
        let _ = ::std::writeln!(::std::io::stderr(), $($arg)*);
    }};
}

pub mod args;
pub mod commands;
pub mod compare;
pub mod docdump;
pub mod error;
pub mod imagediff;
pub mod publishing;
pub mod swfdump;
pub mod traces;

pub use error::{CliError, Result};
