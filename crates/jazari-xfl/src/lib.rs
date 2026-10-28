//! XFL reader and document model for Al-Jazari.
//!
//! [`model`] is the in-memory authoring document. [`open`] reads an XFL
//! folder or a CS5+ `.fla` archive into it and returns an [`ImportReport`]
//! next to the [`model::Document`]. This crate has no SWF, GUI or Ruffle
//! dependency.
//!
//! See `docs/ARCHITECTURE.md`, section 5, and `docs/research/xfl-format.md`.

#![warn(missing_docs)]

mod error;
pub mod model;
mod reader;
mod report;

pub use error::{Error, Result};
pub use model::Document;
pub use reader::{open, open_fla, open_folder, sniff};
pub use report::{ImportReport, ImportWarning, LayerRef, Location, TimelineRef, WarningKind};
