//! Al-Jazari command line (binary `jazari-cli`): `dump`, `publish`,
//! `render` and `compare`. See [`jazari_cli::commands::USAGE`] and
//! `docs/ARCHITECTURE.md`, section 11.
//!
//! Rendering always uses the software adapter.

use std::process::ExitCode;

use jazari_cli::commands::{USAGE, run};
use jazari_cli::{CliError, err};

fn main() -> ExitCode {
    // Library logs (wgpu, Ruffle outside the player's own capture) go to
    // stderr; RUST_LOG overrides the default level.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            err!("jazari-cli: {e}");
            if matches!(e, CliError::Usage(_)) {
                err!();
                err!("{USAGE}");
            }
            ExitCode::from(e.exit_code())
        }
    }
}
