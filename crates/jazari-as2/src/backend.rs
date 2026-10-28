//! Compiling one script with Rascal.
//!
//! Rascal only builds whole SWF files, so each script is served from memory
//! under [`SCRIPT_PATH`], built into a tiny uncompressed SWF and the DoAction
//! payload is read back out of it. The build runs on its own thread with a
//! large stack inside `catch_unwind`, so neither a Rascal panic nor deep
//! recursion can take the application down.

use crate::source::{LineIndex, prepare_for_rascal, strip_bom};
use crate::{CompilerOptions, handlers, scan};
use rascal::{CompileOptions, OptimizationOptions, ProgramBuilder, SourceProvider, SwfOptions};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The virtual path of the script being compiled. It cannot name a real file
/// (`<` and `>` are not allowed in Windows file names).
pub(crate) const SCRIPT_PATH: &str = "<script>";

/// Stack size of the compiler thread. Rascal's parser, resolver and code
/// generator recurse once per nesting level, per binary operator and per
/// `else if`, and a stack overflow aborts the whole process (it cannot be
/// caught). With 32 MiB a chain of about 5000 `+` or 3000 `else if` already
/// overflowed; 256 MiB moves that point to where Rascal takes minutes anyway.
/// The stack is only reserved, not committed, so this costs address space.
const STACK_SIZE: usize = 256 << 20;

/// Stack size tried when the system refuses [`STACK_SIZE`].
const FALLBACK_STACK_SIZE: usize = 32 << 20;

/// A file a compiled script depends on, with the hash of what was read, or
/// `None` for an `#include` file that was missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dependency {
    pub(crate) path: PathBuf,
    pub(crate) hash: Option<[u8; 32]>,
}

impl Dependency {
    /// Whether the file still has the same content (or is still missing).
    pub(crate) fn is_current(&self) -> bool {
        std::fs::read(&self.path).ok().map(|bytes| sha256(&bytes)) == self.hash
    }
}

/// A successfully compiled script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Output {
    /// The DoAction payload (ending with `ActionEnd`), empty when the script
    /// has no code.
    pub(crate) actions: Vec<u8>,
    /// AS2 classes the script refers to (fully qualified names).
    pub(crate) classes: BTreeSet<String>,
}

/// A compiler message before it is given its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawDiagnostic {
    /// `None` for the script itself, else the `#include` or class path.
    pub(crate) file: Option<String>,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) message: String,
}

impl RawDiagnostic {
    pub(crate) fn without_position(message: impl Into<String>) -> Self {
        Self {
            file: None,
            line: 0,
            column: 0,
            message: message.into(),
        }
    }
}

/// The result of one compile with the files it read.
#[derive(Debug, Clone)]
pub(crate) struct Compiled {
    pub(crate) result: Result<Output, Vec<RawDiagnostic>>,
    pub(crate) dependencies: Vec<Dependency>,
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Compiles `text` as a frame script (or a blanked handler body).
///
/// Class files are served as empty stubs (`class a.b.C {}`), so the script
/// compiles against the class names only: the classes themselves go into
/// their own DoInitAction modules, and Rascal's MTASC-style `X.main(this)`
/// call is not added to the script.
pub(crate) fn compile_script(options: &CompilerOptions, text: &str) -> Compiled {
    let task = || {
        let script = prepare_for_rascal(text);
        let provider = Provider::new(options, &script, ClassMode::Stub);
        let mut builder = ProgramBuilder::new(ByRef(&provider)).with_compile_options(
            CompileOptions::default()
                .with_swf_version(options.swf_version)
                .with_optimizations(OptimizationOptions::full()),
        );
        builder.add_script(SCRIPT_PATH);
        let result = match builder.build() {
            Ok(program) => program
                .compile()
                .to_swf(&SwfOptions::default())
                .map_err(|e| vec![RawDiagnostic::without_position(swf_error_message(&e))])
                .and_then(|swf| {
                    do_action_payload(&swf).map_err(|m| vec![RawDiagnostic::without_position(m)])
                })
                .map(|actions| Output {
                    actions,
                    classes: provider.classes.borrow().clone(),
                }),
            Err(error) => Err(provider.diagnostics(&error)),
        };
        Compiled {
            result,
            dependencies: provider.dependencies(),
        }
    };
    isolate(task).unwrap_or_else(|panic| Compiled {
        result: Err(vec![RawDiagnostic::without_position(format!(
            "internal compiler error (Rascal panicked): {panic}"
        ))]),
        dependencies: Vec::new(),
    })
}

/// A compiled class or interface: the DoInitAction payload of its
/// `__Packages.<name>` module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) name: String,
    pub(crate) actions: Vec<u8>,
}

/// Compiles AS2 classes (with the classes they need) into modules, in the
/// order Rascal emits them, which is the order they must run in.
pub(crate) fn compile_classes(
    options: &CompilerOptions,
    names: &[String],
) -> Result<Vec<Module>, Vec<RawDiagnostic>> {
    let task = || {
        let provider = Provider::new(options, "", ClassMode::Real);
        provider.expect_classes(names);
        let mut builder = ProgramBuilder::new(ByRef(&provider)).with_compile_options(
            CompileOptions::default()
                .with_swf_version(options.swf_version)
                .with_optimizations(OptimizationOptions::full()),
        );
        for name in names {
            builder.add_class(name);
        }
        match builder.build() {
            Ok(program) => program
                .compile()
                .to_swf(&SwfOptions::default())
                .map_err(|e| vec![RawDiagnostic::without_position(swf_error_message(&e))])
                .and_then(|swf| {
                    modules(&swf).map_err(|m| vec![RawDiagnostic::without_position(m)])
                }),
            Err(error) => Err(provider.diagnostics(&error)),
        }
    };
    isolate(task).unwrap_or_else(|panic| {
        Err(vec![RawDiagnostic::without_position(format!(
            "internal compiler error (Rascal panicked): {panic}"
        ))])
    })
}

/// The class modules of an SWF built by Rascal. Its DoAction, which only
/// holds an MTASC-style `X.main(this)` call, is left out.
fn modules(swf_bytes: &[u8]) -> Result<Vec<Module>, String> {
    let read_error = |e: swf::error::Error| format!("cannot read Rascal's output: {e}");
    let buf = swf::decompress_swf(swf_bytes).map_err(read_error)?;
    let parsed = swf::parse_swf(&buf).map_err(read_error)?;
    let mut names = BTreeMap::new();
    let mut result = Vec::new();
    for tag in &parsed.tags {
        match tag {
            swf::Tag::ExportAssets(assets) => {
                for asset in assets {
                    let name = asset.name.to_str_lossy(swf::UTF_8);
                    let name = name.strip_prefix("__Packages.").unwrap_or(&name).to_owned();
                    names.insert(asset.id, name);
                }
            }
            swf::Tag::DoInitAction { id, action_data } => {
                let name = names
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("Rascal's module {id} has no exported name"))?;
                result.push(Module {
                    name,
                    actions: action_data.to_vec(),
                });
            }
            _ => {}
        }
    }
    Ok(result)
}

/// Runs `task` on a thread with a large stack and catches its panics.
/// Returns the panic message on a panic.
pub(crate) fn isolate<T: Send>(task: impl FnOnce() -> T + Send) -> Result<T, String> {
    let task = Mutex::new(Some(task));
    let run = || {
        let task = task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        match task {
            Some(task) => catch_unwind(AssertUnwindSafe(task)).map_err(|p| panic_message(&*p)),
            None => Err("the compiler task was already taken".to_owned()),
        }
    };
    std::thread::scope(|scope| {
        let spawn = |stack_size| {
            std::thread::Builder::new()
                .name("jazari-as2".to_owned())
                .stack_size(stack_size)
                .spawn_scoped(scope, run)
        };
        match spawn(STACK_SIZE).or_else(|_| spawn(FALLBACK_STACK_SIZE)) {
            Ok(handle) => handle.join().unwrap_or_else(|p| Err(panic_message(&*p))),
            Err(error) => {
                log::warn!("cannot start the AS2 compiler thread ({error}); compiling inline");
                run()
            }
        }
    })
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

fn swf_error_message(error: &swf::error::Error) -> String {
    format!("cannot encode the compiled script: {error}")
}

/// Takes the DoAction payload out of an SWF built by Rascal. A script without
/// code has no DoAction, which gives an empty payload.
fn do_action_payload(swf_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let read_error = |e: swf::error::Error| format!("cannot read Rascal's output: {e}");
    let buf = swf::decompress_swf(swf_bytes).map_err(read_error)?;
    let parsed = swf::parse_swf(&buf).map_err(read_error)?;
    Ok(parsed
        .tags
        .iter()
        .find_map(|tag| match tag {
            swf::Tag::DoAction(data) => Some(data.to_vec()),
            _ => None,
        })
        .unwrap_or_default())
}

/// How class files are served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassMode {
    /// As `class a.b.C {}` / `interface a.b.C {}` stubs.
    Stub,
    /// As they are.
    Real,
}

/// A file the provider has loaded.
#[derive(Debug, Clone)]
struct Loaded {
    text: String,
    dependency: Option<Dependency>,
}

/// Serves the script from memory, `#include` files from the include base and
/// class files from the class paths.
pub(crate) struct Provider<'a> {
    options: &'a CompilerOptions,
    script: &'a str,
    mode: ClassMode,
    /// Rascal path to the loaded file.
    loaded: RefCell<BTreeMap<String, Loaded>>,
    /// Rascal paths `is_file` answered `true` for: class files.
    class_files: RefCell<BTreeSet<String>>,
    /// `#include` files that were not found, so that a cached error is
    /// dropped once the file appears.
    missing: RefCell<Vec<Dependency>>,
    /// Classes requested while compiling, fully qualified.
    pub(crate) classes: RefCell<BTreeSet<String>>,
}

impl<'a> Provider<'a> {
    pub(crate) fn new(options: &'a CompilerOptions, script: &'a str, mode: ClassMode) -> Self {
        Self {
            options,
            script,
            mode,
            loaded: RefCell::new(BTreeMap::new()),
            class_files: RefCell::new(BTreeSet::new()),
            missing: RefCell::new(Vec::new()),
            classes: RefCell::new(BTreeSet::new()),
        }
    }

    /// Marks the files of these classes as class files (they are loaded
    /// without being probed with `is_file` first).
    fn expect_classes(&self, names: &[String]) {
        let mut class_files = self.class_files.borrow_mut();
        for name in names {
            class_files.insert(format!("{}.as", name.replace('.', "/")));
        }
    }

    fn find_class_file(&self, path: &str) -> Option<PathBuf> {
        if !path.ends_with(".as") {
            return None;
        }
        self.options
            .class_paths
            .iter()
            .map(|root| root.join(path))
            .find(|candidate| candidate.is_file())
    }

    /// Where an `#include` path points, whether or not the file exists.
    fn include_candidate(&self, path: &str) -> Option<PathBuf> {
        let path = Path::new(path);
        if path.is_absolute() {
            return Some(path.to_owned());
        }
        self.options
            .include_base
            .as_ref()
            .map(|base| base.join(path))
    }

    fn find_include(&self, path: &str) -> Option<PathBuf> {
        let candidate = self.include_candidate(path)?;
        if candidate.is_file() {
            return Some(candidate);
        }
        self.missing.borrow_mut().push(Dependency {
            path: candidate,
            hash: None,
        });
        None
    }

    fn read(&self, file: &Path) -> std::io::Result<(String, Dependency)> {
        let bytes = std::fs::read(file)?;
        let dependency = Dependency {
            path: file.to_owned(),
            hash: Some(sha256(&bytes)),
        };
        Ok((decode(&bytes, self.options), dependency))
    }

    /// The files read, for the cache.
    pub(crate) fn dependencies(&self) -> Vec<Dependency> {
        let mut dependencies: Vec<Dependency> = self
            .loaded
            .borrow()
            .values()
            .filter_map(|loaded| loaded.dependency.clone())
            .collect();
        dependencies.extend(self.missing.borrow().iter().cloned());
        dependencies
    }

    /// Turns Rascal's errors into positioned diagnostics.
    pub(crate) fn diagnostics(&self, error: &rascal::Error) -> Vec<RawDiagnostic> {
        let loaded = self.loaded.borrow();
        let only_script = loaded.len() <= 1;
        let mut result = Vec::new();
        for diagnostic in error.diagnostics() {
            let message = clean_message(&diagnostic.message);
            let (file, text) = match diagnostic.path.as_deref() {
                Some(SCRIPT_PATH) => (None, Some(self.script)),
                Some(path) => (
                    Some(path.to_owned()),
                    loaded.get(path).map(|l| l.text.as_str()),
                ),
                None => (None, None),
            };
            let (line, column) = match (&diagnostic.span, text) {
                (Some(span), Some(text)) => LineIndex::new(text).position(span.start),
                // A syntax error at the very end has no span (Rascal leaves it
                // empty); when only the script was read, it is at its end.
                (None, None) if diagnostic.path.is_none() && only_script => {
                    LineIndex::new(self.script).end_position()
                }
                _ => (0, 0),
            };
            result.push(RawDiagnostic {
                file,
                line,
                column,
                message,
            });
        }
        if result.is_empty() {
            result.push(RawDiagnostic::without_position(clean_message(
                &error.to_string_plain(),
            )));
        }
        result
    }
}

/// Rascal's messages can be empty or span several lines ("invalid
/// expression\nexpected `)`"); make them one line.
fn clean_message(message: &str) -> String {
    let parts: Vec<&str> = message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if parts.is_empty() {
        "Syntax error.".to_owned()
    } else {
        parts.join(": ")
    }
}

/// Decodes a source file: a UTF-8 or UTF-16 byte order mark wins, then UTF-8,
/// then the configured legacy code page (Flash-era files are often ANSI).
fn decode(bytes: &[u8], options: &CompilerOptions) -> String {
    if let Some((encoding, bom_length)) = swf::Encoding::for_bom(bytes) {
        let (text, _) = encoding.decode_without_bom_handling(&bytes[bom_length..]);
        return text.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => {
            let encoding = options
                .legacy_encoding
                .as_deref()
                .and_then(|label| swf::Encoding::for_label(label.as_bytes()))
                .unwrap_or(swf::WINDOWS_1252);
            encoding.decode_without_bom_handling(bytes).0.into_owned()
        }
    }
}

/// A stub declaring the class or interface `fqn`, or `None` when `text`
/// declares neither (it is then served as it is).
fn class_stub(text: &str, fqn: &str) -> Option<String> {
    let tokens = scan::tokenize(text);
    tokens.iter().find_map(|token| {
        if token.kind != scan::TokenKind::Ident {
            return None;
        }
        match token.text(text) {
            "class" => Some(format!("class {fqn} {{}}\n")),
            "interface" => Some(format!("interface {fqn} {{}}\n")),
            _ => None,
        }
    })
}

/// Rascal has no `impl SourceProvider for &P`.
struct ByRef<'p, 'a>(&'p Provider<'a>);

impl SourceProvider for ByRef<'_, '_> {
    fn load(&self, path: &str) -> std::io::Result<String> {
        let provider = self.0;
        if path == SCRIPT_PATH {
            let text = provider.script.to_owned();
            provider.loaded.borrow_mut().insert(
                path.to_owned(),
                Loaded {
                    text: text.clone(),
                    dependency: None,
                },
            );
            return Ok(text);
        }
        #[cfg(test)]
        if path == crate::tests::PANIC_INCLUDE {
            panic!("test panic inside Rascal's build");
        }
        let is_class = provider.class_files.borrow().contains(path);
        let file = if is_class {
            provider.find_class_file(path)
        } else {
            provider.find_include(path)
        };
        let Some(file) = file else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found",
            ));
        };
        let (text, dependency) = provider.read(&file)?;
        let text = prepare_for_rascal(strip_bom(&text)).into_owned();
        provider.loaded.borrow_mut().insert(
            path.to_owned(),
            Loaded {
                text: text.clone(),
                dependency: Some(dependency),
            },
        );
        if !is_class {
            // Rascal would compile `on (press) { }` as a call to `on` and a
            // plain block, without any error (the front-end checks only the
            // script itself, not what it includes).
            if let Some(found) = handlers::find_handler_syntax(&text, &scan::tokenize(&text)) {
                let (line, _) = LineIndex::new(&text).position(found.offset);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "line {line} has an on()/onClipEvent() handler, which an included \
                         file cannot hold"
                    ),
                ));
            }
            return Ok(text);
        }
        let fqn = path.trim_end_matches(".as").replace(['/', '\\'], ".");
        provider.classes.borrow_mut().insert(fqn.clone());
        match provider.mode {
            ClassMode::Stub => Ok(class_stub(&text, &fqn).unwrap_or(text)),
            ClassMode::Real => Ok(text),
        }
    }

    fn is_file(&self, path: &str) -> bool {
        // Rascal asks only when probing for classes (`new a.b.C`, `import`,
        // type annotations, dotted paths).
        let found = self.0.find_class_file(path).is_some();
        if found {
            self.0.class_files.borrow_mut().insert(path.to_owned());
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_one_line() {
        assert_eq!(clean_message(""), "Syntax error.");
        assert_eq!(
            clean_message("invalid expression\nexpected `)`"),
            "invalid expression: expected `)`"
        );
    }

    #[test]
    fn stubs_keep_the_kind() {
        assert_eq!(
            class_stub(
                "// x\nintrinsic class a.B extends C { function f():Void; }",
                "a.B"
            ),
            Some("class a.B {}\n".to_owned())
        );
        assert_eq!(
            class_stub("interface a.I { }", "a.I"),
            Some("interface a.I {}\n".to_owned())
        );
        assert_eq!(class_stub("trace(1);", "x"), None);
    }

    #[test]
    fn decoding_follows_bom_then_utf8_then_legacy() {
        let options = CompilerOptions::default();
        assert_eq!(decode(b"\xef\xbb\xbfab", &options), "ab");
        assert_eq!(decode(b"\xff\xfea\x00b\x00", &options), "ab");
        assert_eq!(decode("مر".as_bytes(), &options), "مر");
        assert_eq!(decode(b"caf\xe9", &options), "café");
        let arabic = CompilerOptions {
            legacy_encoding: Some("windows-1256".to_owned()),
            ..CompilerOptions::default()
        };
        assert_eq!(decode(b"\xe3", &arabic), "م");
    }

    #[test]
    fn isolate_returns_values_and_catches_panics() {
        assert_eq!(isolate(|| 41 + 1), Ok(42));
        let caught = isolate(|| -> u32 { panic!("boom") });
        assert_eq!(caught, Err("boom".to_owned()));
    }
}
