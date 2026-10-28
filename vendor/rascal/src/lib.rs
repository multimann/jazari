mod error;
mod internal;
#[cfg(test)]
mod patch_tests;
mod program;
mod provider;
mod sources;
mod swf;
#[cfg(test)]
mod tests;

pub use error::{Diagnostic, Error};
pub use program::{
    CompileOptions, CompiledProgram, OptimizationOptions, Program, ProgramBuilder, Scenes,
    SwfOptions,
};
pub use provider::{FileSystemSourceProvider, SourceProvider};
