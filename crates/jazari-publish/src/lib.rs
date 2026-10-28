//! Publishes an Al-Jazari [`Document`] as SWF version 8 bytes.
//!
//! Two builds: [`BuildMode::Release`] for Test Movie and File › Publish, and
//! [`BuildMode::Stage`] for the editor's stage view (user scripts omitted,
//! every clip stopped, a `jz_goto` callback on the root). A failed script or
//! element is reported and skipped; the publish continues.
//!
//! See `docs/ARCHITECTURE.md`, section 6, and `docs/research/swf-writer.md`.
//!
//! # Structure
//!
//! | module | role |
//! |---|---|
//! | `movie` | root timeline: header tags, scenes, exports, classes |
//! | `timeline` | layers, depth bands, keyframe expansion, frame planning |
//! | `display` | display list diffing into Place/Modify/Replace/Remove |
//! | `symbol` | movie clips (DefineSprite) and buttons (DefineButton2) |
//! | `shape` | DefineShape3/4 |
//! | `graphic` | graphic symbol frame arithmetic |
//! | `tween` | motion tween interpolation |
//! | `group` | group flattening |
//! | `filter` | filters and blend modes |
//! | `script` | AS2 through `jazari-as2` |
//! | `text`, `bitmap`, `mask` | text, bitmaps and masks |
//! | `tags`, `validate` | tag encoding with round-trip checks, parse-back |
//! | `context` | shared state, characters, diagnostics |

#![warn(missing_docs)]

mod bitmap;
mod context;
mod display;
mod error;
mod filter;
mod graphic;
mod group;
mod mask;
mod movie;
mod script;
mod shape;
mod symbol;
mod tags;
mod text;
mod timeline;
mod tween;
mod units;
mod validate;

use std::panic::{AssertUnwindSafe, catch_unwind};

use jazari_xfl::Document;
use swf::{Compression, Header, Rectangle};

/// Compiler diagnostics and publish warnings share one type.
pub use jazari_as2::{Diagnostic, Origin, Severity};

pub use error::PublishError;

/// Which SWF to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuildMode {
    /// The real movie, with all scripts.
    #[default]
    Release,
    /// The stage-view build: no user scripts, `stop()` everywhere, and an
    /// `ExternalInterface` callback `jz_goto(frame)` on the root timeline.
    Stage,
}

/// Options of [`publish`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublishOptions {
    /// Release or stage build.
    pub mode: BuildMode,
    /// zlib-compress the SWF (`CWS`).
    pub compress: bool,
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            mode: BuildMode::Release,
            compress: true,
        }
    }
}

/// The result of [`publish`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublishOutput {
    /// The SWF bytes, or `None` when publishing failed as a whole (the
    /// reason is in `diagnostics`).
    pub swf: Option<Vec<u8>>,
    /// Compiler errors and publish warnings, with their origins, in the
    /// order they were found.
    pub diagnostics: Vec<Diagnostic>,
    /// For each root frame of the SWF, in order: the scene index and the
    /// 0-based frame within that scene (scenes are concatenated). The stage
    /// view calls `jz_goto(i + 1)` to show entry `i`. Empty only for a
    /// document without scenes (whose SWF has one empty frame).
    pub frame_map: Vec<(usize, u32)>,
}

/// Publishes a document. Never panics on document content; the same
/// document and options always give the same bytes.
pub fn publish(
    doc: &Document,
    opts: &PublishOptions,
    fonts: &jazari_text::FontLibrary,
    as2: &jazari_as2::Compiler,
) -> PublishOutput {
    match catch_unwind(AssertUnwindSafe(|| publish_inner(doc, opts, fonts, as2))) {
        Ok(out) => out,
        Err(panic) => {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_owned());
            PublishOutput {
                swf: None,
                diagnostics: vec![Diagnostic {
                    severity: Severity::Error,
                    message: format!("internal error in the publisher (please report it): {what}"),
                    origin: Origin::default(),
                    line: 0,
                    column: 0,
                }],
                frame_map: Vec::new(),
            }
        }
    }
}

fn publish_inner(
    doc: &Document,
    opts: &PublishOptions,
    fonts: &jazari_text::FontLibrary,
    as2: &jazari_as2::Compiler,
) -> PublishOutput {
    let mut ctx = context::Ctx::new(doc, opts.mode, fonts, as2);
    let movie = movie::build(&mut ctx);
    let header = movie_header(doc, opts, movie.num_frames, &mut ctx);
    let swf = match tags::write_movie(&header, &movie.tags) {
        Ok(bytes) => match validate::validate(&bytes, Some(movie.tags.len())) {
            Ok(report) => {
                for p in report.problems {
                    ctx.error(Origin::default(), format!("internal check failed: {p}"));
                }
                Some(bytes)
            }
            Err(e) => {
                ctx.error(
                    Origin::default(),
                    format!("the published SWF does not read back: {e}"),
                );
                None
            }
        },
        Err(e) => {
            ctx.error(Origin::default(), format!("cannot write the SWF: {e}"));
            None
        }
    };
    PublishOutput {
        swf,
        diagnostics: ctx.take_diagnostics(),
        frame_map: movie.frame_map,
    }
}

/// The SWF header: version 8, the stage in twips, the frame rate in 8.8.
fn movie_header(
    doc: &Document,
    opts: &PublishOptions,
    num_frames: u16,
    ctx: &mut context::Ctx<'_>,
) -> Header {
    let origin = Origin {
        file: Some("DOMDocument.xml".to_owned()),
        ..Origin::default()
    };
    let size = |v: f64, what: &str, ctx: &mut context::Ctx<'_>| {
        if !(1.0..=2880.0).contains(&v) {
            ctx.warn(
                origin.clone(),
                format!("stage {what} {v} px is outside 1..2880 px and was clamped"),
            );
        }
        units::px_to_twips(if v.is_finite() {
            v.clamp(1.0, 2880.0)
        } else {
            550.0
        })
    };
    let w = size(doc.width, "width", ctx);
    let h = size(doc.height, "height", ctx);
    let fps = doc.frame_rate;
    if !(0.01..=120.0).contains(&fps) {
        ctx.warn(
            origin.clone(),
            format!("frame rate {fps} is outside 0.01..120 fps and was clamped"),
        );
    }
    let fps = if fps.is_finite() {
        fps.clamp(0.01, 120.0)
    } else {
        12.0
    };
    Header {
        compression: if opts.compress {
            Compression::Zlib
        } else {
            Compression::None
        },
        version: tags::SWF_VERSION,
        stage_size: Rectangle {
            x_min: swf::Twips::ZERO,
            x_max: w,
            y_min: swf::Twips::ZERO,
            y_max: h,
        },
        frame_rate: units::fixed8(fps),
        num_frames,
    }
}
