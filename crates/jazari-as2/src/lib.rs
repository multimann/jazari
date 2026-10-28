//! ActionScript 2 front-end for Al-Jazari, built on Rascal.
//!
//! Compiles frame scripts, button `on()` handlers and movie clip
//! `onClipEvent()` handlers to AVM1 bytecode for SWF 8. Our own tokenizer
//! splits handler blocks (Rascal does not understand them); each body is
//! compiled with everything outside it blanked to spaces so line and column
//! numbers stay true. Every compile runs inside `catch_unwind` on its own
//! thread, so a Rascal panic becomes a [`Diagnostic`].
//!
//! See `docs/ARCHITECTURE.md`, section 7, and `docs/research/rascal.md`.

#![warn(missing_docs)]

mod backend;
mod cache;
mod diagnostic;
pub mod disasm;
mod handlers;
#[cfg(test)]
mod oracle_tests;
mod scan;
mod source;
mod stage;
#[cfg(test)]
mod tests;

use backend::{Output, RawDiagnostic};
use cache::Cache;
use handlers::{InstanceKind, Keyword, Problem};
use source::{LineIndex, blank_outside, strip_bom};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

pub use diagnostic::{Diagnostic, Origin, Severity};
pub use stage::{STAGE_GOTO_SOURCE, stage_goto_hook};

/// Deepest nesting of brackets a script may have. Deeper input would only
/// come from a broken or hostile file, and Rascal's parser recurses on it.
const MAX_NESTING: usize = 1000;

/// Options of a [`Compiler`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerOptions {
    /// SWF version to compile for (always 8 in milestone 1).
    pub swf_version: u8,
    /// AS2 class path roots (from `PublishSettings` `PackagePaths`, resolved
    /// against the document folder).
    pub class_paths: Vec<PathBuf>,
    /// Base directory for `#include` (the document folder).
    pub include_base: Option<PathBuf>,
    /// Encoding label (for example `windows-1256`) for `#include` and class
    /// files that are neither UTF-8 nor marked by a byte order mark.
    /// `None` means `windows-1252`.
    pub legacy_encoding: Option<String>,
    /// Cache compiled output by a hash of the source and its includes.
    pub cache: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            swf_version: 8,
            class_paths: Vec::new(),
            include_base: None,
            legacy_encoding: None,
            cache: true,
        }
    }
}

/// The AS2 compiler. One instance is shared by a whole publish; its methods
/// take `&self` and it is safe to share between threads.
#[derive(Debug)]
pub struct Compiler {
    options: CompilerOptions,
    cache: Cache,
    /// Classes referenced by the scripts compiled so far.
    classes: Mutex<BTreeSet<String>>,
}

impl Compiler {
    /// Creates a compiler.
    pub fn new(opts: CompilerOptions) -> Self {
        Self {
            options: opts,
            cache: Cache::default(),
            classes: Mutex::new(BTreeSet::new()),
        }
    }

    /// The options this compiler was created with.
    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    /// Compiles a frame script to a DoAction payload (ending with
    /// `ActionEnd`). A script without code (empty, or only comments) compiles
    /// to `Ok` with an empty payload: write no DoAction for it.
    ///
    /// `on (...) { }` and `onClipEvent (...) { }` blocks are rejected, as in
    /// Flash 8: they belong on instances.
    pub fn compile_frame(&self, src: &str, origin: &Origin) -> Result<Vec<u8>, Vec<Diagnostic>> {
        let src = strip_bom(src);
        let tokens = scan::tokenize(src);
        let mut problems = precheck(&tokens);
        if let Some(found) = handlers::find_handler_syntax(src, &tokens) {
            problems.push(Problem {
                offset: found.offset,
                message: match found.keyword {
                    Keyword::On => "Mouse events are permitted only for button instances",
                    Keyword::OnClipEvent => {
                        "Clip events are permitted only for movie clip instances"
                    }
                }
                .to_owned(),
            });
        }
        if !problems.is_empty() {
            return Err(problem_diagnostics(src, problems, origin));
        }
        self.compile_text(src)
            .map(|output| output.actions)
            .map_err(|raw| raw_diagnostics(raw, origin))
    }

    /// Splits and compiles the `on(...)` handlers of a button instance into
    /// DefineButton2 condition actions.
    ///
    /// As in Flash 8, a handler without code is left out, and any error
    /// fails the whole instance.
    pub fn compile_button_handlers(
        &self,
        src: &str,
        origin: &Origin,
    ) -> Result<Vec<ButtonHandler>, Vec<Diagnostic>> {
        let src = strip_bom(src);
        self.compile_handlers(src, origin, InstanceKind::Button, |handler, output| {
            let (conditions, key_code) = handlers::button_conditions(handler)?;
            Ok((!output.actions.is_empty()).then_some(ButtonHandler {
                conditions,
                key_code,
                actions: output.actions,
            }))
        })
    }

    /// Splits and compiles the `onClipEvent(...)` and `on(...)` handlers of a
    /// movie clip instance into PlaceObject2 clip actions.
    ///
    /// As in Flash 8, a handler without code is kept (its actions are just
    /// `ActionEnd`), and any error fails the whole instance.
    pub fn compile_clip_handlers(
        &self,
        src: &str,
        origin: &Origin,
    ) -> Result<Vec<ClipHandler>, Vec<Diagnostic>> {
        let src = strip_bom(src);
        self.compile_handlers(src, origin, InstanceKind::MovieClip, |handler, output| {
            let (events, key_code) = handlers::clip_events(handler)?;
            let mut actions = output.actions;
            if actions.is_empty() {
                actions.push(0); // ActionEnd
            }
            Ok(Some(ClipHandler {
                events,
                key_code,
                actions,
            }))
        })
    }

    /// AS2 classes (fully qualified names) referenced by the scripts this
    /// compiler has compiled so far, in sorted order. Scripts compile
    /// against empty stubs of these classes; the classes themselves still
    /// have to be compiled into DoInitAction modules.
    pub fn referenced_classes(&self) -> Vec<String> {
        self.classes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Compiles AS2 classes into DoInitAction modules, with every class they
    /// need, in the order the modules must run. Pass
    /// [`referenced_classes`](Self::referenced_classes) plus the linkage
    /// classes of exported symbols.
    ///
    /// Each module belongs in a `DoInitAction` for a sprite exported as
    /// `__Packages.<name>`, before the first frame that uses the class.
    /// Rascal's MTASC-style `X.main(this)` entry call is dropped, as Flash
    /// has no such convention; two classes with a static `main` still fail.
    pub fn compile_classes(&self, names: &[String]) -> Result<Vec<ClassModule>, Vec<Diagnostic>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        backend::compile_classes(&self.options, names)
            .map(|modules| {
                modules
                    .into_iter()
                    .map(|m| ClassModule {
                        name: m.name,
                        actions: m.actions,
                    })
                    .collect()
            })
            .map_err(|raw| raw_diagnostics(raw, &Origin::default()))
    }

    /// Forgets every cached compile result.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Number of cached compile results.
    pub fn cached_results(&self) -> usize {
        self.cache.len()
    }

    /// Splits an instance script and compiles each handler body; `build`
    /// turns a handler and its compiled body into the result type, or into
    /// problems with its event list.
    fn compile_handlers<T>(
        &self,
        src: &str,
        origin: &Origin,
        kind: InstanceKind,
        build: impl Fn(&handlers::Handler, Output) -> Result<Option<T>, Vec<Problem>>,
    ) -> Result<Vec<T>, Vec<Diagnostic>> {
        let tokens = scan::tokenize(src);
        let mut problems = precheck(&tokens);
        let (found, split_problems) = handlers::split(src, &tokens, kind);
        let rejected = !problems.is_empty();
        problems.extend(split_problems);
        if rejected {
            // Rascal must not see these scripts (and would only repeat the
            // syntax error for every handler).
            return Err(problem_diagnostics(src, problems, origin));
        }
        let mut results = Vec::new();
        let mut diagnostics = Vec::new();
        for handler in &found {
            let text = blank_outside(src, handler.body.clone());
            match self.compile_text(&text) {
                Ok(output) => match build(handler, output) {
                    Ok(Some(result)) => results.push(result),
                    Ok(None) => {}
                    Err(event_problems) => problems.extend(event_problems),
                },
                Err(raw) => {
                    // The event list is still checked, so all problems show.
                    if let Err(event_problems) = build(handler, Output::default()) {
                        problems.extend(event_problems);
                    }
                    diagnostics.extend(raw_diagnostics(raw, origin));
                }
            }
        }
        if problems.is_empty() && diagnostics.is_empty() {
            return Ok(results);
        }
        let mut all = problem_diagnostics(src, problems, origin);
        all.extend(diagnostics);
        all.sort_by_key(|d| (d.origin.file.is_some(), d.line, d.column));
        Err(all)
    }

    /// Compiles one script text through the cache.
    fn compile_text(&self, text: &str) -> Result<Output, Vec<RawDiagnostic>> {
        let key = self.options.cache.then(|| Cache::key(&self.options, text));
        let cached = key.as_ref().and_then(|key| self.cache.get(key));
        let compiled = match cached {
            Some(compiled) => compiled,
            None => {
                let compiled = backend::compile_script(&self.options, text);
                if let Some(key) = key {
                    self.cache.insert(key, compiled.clone());
                }
                compiled
            }
        };
        if let Ok(output) = &compiled.result {
            self.classes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(output.classes.iter().cloned());
        }
        compiled.result
    }
}

/// Checks made before Rascal sees a script: `@` (which panics or confuses
/// Rascal's lexer and is not ActionScript) and runaway nesting.
fn precheck(tokens: &[scan::Token]) -> Vec<Problem> {
    let mut problems: Vec<Problem> = tokens
        .iter()
        .filter(|t| t.kind == scan::TokenKind::At)
        .map(|t| Problem {
            offset: t.start,
            message: "Unexpected '@' encountered".to_owned(),
        })
        .collect();
    let (depth, offset) = scan::max_nesting(tokens);
    if depth > MAX_NESTING {
        problems.push(Problem {
            offset,
            message: format!("Brackets are nested more than {MAX_NESTING} levels deep"),
        });
    }
    problems
}

fn problem_diagnostics(src: &str, problems: Vec<Problem>, origin: &Origin) -> Vec<Diagnostic> {
    let index = LineIndex::new(src);
    problems
        .into_iter()
        .map(|problem| {
            let (line, column) = index.position(problem.offset);
            Diagnostic {
                severity: Severity::Error,
                message: problem.message,
                origin: origin.clone(),
                line,
                column,
            }
        })
        .collect()
}

fn raw_diagnostics(raw: Vec<RawDiagnostic>, origin: &Origin) -> Vec<Diagnostic> {
    raw.into_iter()
        .map(|d| {
            let mut origin = origin.clone();
            if d.file.is_some() {
                origin.file = d.file;
            }
            Diagnostic {
                severity: Severity::Error,
                message: d.message,
                origin,
                line: d.line,
                column: d.column,
            }
        })
        .collect()
}

/// A compiled AS2 class or interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassModule {
    /// Fully qualified name (`com.example.Ball`); the module is exported as
    /// `__Packages.<name>`.
    pub name: String,
    /// DoInitAction payload, ending with `ActionEnd`.
    pub actions: Vec<u8>,
}

/// One compiled `on(...)` block of a button instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonHandler {
    /// The button events (`press`, `release`, `rollOver`, ...). For
    /// `keyPress` the key code is in the `KEY_PRESS` bits too.
    pub conditions: swf::ButtonActionCondition,
    /// Key code for `on(keyPress "<key>")`.
    pub key_code: Option<u8>,
    /// AVM1 bytecode, ending with `ActionEnd`.
    pub actions: Vec<u8>,
}

/// One compiled `onClipEvent(...)` or `on(...)` block of a movie clip instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipHandler {
    /// The clip events.
    pub events: swf::ClipEventFlag,
    /// Key code, set only for `KEY_PRESS`.
    pub key_code: Option<u8>,
    /// AVM1 bytecode, ending with `ActionEnd`.
    pub actions: Vec<u8>,
}
