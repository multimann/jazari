//! Reading XFL containers into the model.
//!
//! The rules are in `docs/research/xfl-format.md` (recommendation and
//! corrections): match elements by local name, accept any attribute order,
//! apply schema defaults, never abort on unknown content, and key library
//! symbols by the `name` inside each file.
//!
//! Layout of this module:
//!
//! - [`store`]: the folder or ZIP holding the files, with tolerant lookup;
//! - [`xml`]: a small owned XML tree;
//! - [`ctx`] and [`values`]: the import report, locations and value parsing;
//! - [`document`], [`timeline`], [`element`], [`shape`], [`text`],
//!   [`color`], [`filter`], [`publish`]: one module per part of the format;
//! - [`edges`] and [`dat`]: the edge path grammar and bitmap media;
//! - [`refs`]: dangling-reference checks after everything is read.

mod color;
mod ctx;
mod dat;
mod document;
mod edges;
mod element;
mod filter;
mod publish;
mod refs;
mod shape;
mod store;
mod text;
mod timeline;
mod values;
mod xml;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::model::{ContainerKind, Document, SourceInfo};
use crate::{Error, ImportReport, Result};

use store::Store;

/// Magic bytes of a pre-CS5 binary FLA (an OLE2 compound file).
const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
/// Content of the `.xfl` stub file of an uncompressed XFL folder.
const XFL_STUB: &str = "PROXY-CS5";
/// Largest stub file recognised.
const MAX_STUB_BYTES: u64 = 64;

/// Detects the container at `path` without reading the document.
///
/// A folder with `DOMDocument.xml`, a `.xfl` stub (at most 64 bytes, text
/// `PROXY-CS5`, BOM tolerated) or a `DOMDocument.xml` file gives
/// [`ContainerKind::Folder`]; a ZIP (`50 4B 03 04`) gives
/// [`ContainerKind::Zip`]. A binary FLA (`D0 CF 11 E0`) is
/// [`crate::Error::LegacyFla`]; anything else is [`crate::Error::NotXfl`].
pub fn sniff(path: &Path) -> Result<ContainerKind> {
    let meta = std::fs::metadata(path).map_err(|source| io_error(path, source))?;
    if meta.is_dir() {
        return if has_document(path) {
            Ok(ContainerKind::Folder)
        } else {
            Err(Error::NotXfl {
                path: path.to_owned(),
            })
        };
    }
    let head = read_head(path, MAX_STUB_BYTES as usize + 1)?;
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        return Ok(ContainerKind::Zip);
    }
    if head.starts_with(&OLE2_MAGIC) {
        return Err(Error::LegacyFla {
            path: path.to_owned(),
        });
    }
    if is_document_file(path) {
        return Ok(ContainerKind::Folder);
    }
    if meta.len() <= MAX_STUB_BYTES && is_stub(&head) {
        let dir = parent_dir(path);
        return if has_document(&dir) {
            Ok(ContainerKind::Folder)
        } else {
            Err(Error::MissingDocument { path: dir })
        };
    }
    Err(Error::NotXfl {
        path: path.to_owned(),
    })
}

/// Opens any supported container: dispatches on [`sniff`] to
/// [`open_folder`] or [`open_fla`].
pub fn open(path: &Path) -> Result<(Document, ImportReport)> {
    match sniff(path)? {
        ContainerKind::Zip => open_fla(path),
        ContainerKind::Folder | ContainerKind::InMemory => open_folder(path),
    }
}

/// Reads an uncompressed XFL folder. `dir` may also be the `.xfl` stub or the
/// `DOMDocument.xml` inside it.
pub fn open_folder(dir: &Path) -> Result<(Document, ImportReport)> {
    let meta = std::fs::metadata(dir).map_err(|source| io_error(dir, source))?;
    let folder = if meta.is_dir() {
        dir.to_owned()
    } else {
        let head = read_head(dir, MAX_STUB_BYTES as usize + 1)?;
        if head.starts_with(&OLE2_MAGIC) {
            return Err(Error::LegacyFla {
                path: dir.to_owned(),
            });
        }
        let stub = meta.len() <= MAX_STUB_BYTES && is_stub(&head);
        if !stub && !is_document_file(dir) {
            return Err(Error::NotXfl {
                path: dir.to_owned(),
            });
        }
        parent_dir(dir)
    };
    let folder = std::path::absolute(&folder).unwrap_or(folder);
    let mut store = Store::folder(&folder)?;
    let source = SourceInfo {
        path: dir.to_owned(),
        container: ContainerKind::Folder,
        base_dir: folder.clone(),
    };
    document::read_document(&mut store, &folder, source)
}

/// Reads a CS5+ `.fla` ZIP archive.
pub fn open_fla(path: &Path) -> Result<(Document, ImportReport)> {
    let head = read_head(path, 8)?;
    if head.starts_with(&OLE2_MAGIC) {
        return Err(Error::LegacyFla {
            path: path.to_owned(),
        });
    }
    if !head.starts_with(b"PK") {
        return Err(Error::NotXfl {
            path: path.to_owned(),
        });
    }
    let mut store = Store::zip(path)?;
    let base_dir = parent_dir(path);
    let source = SourceInfo {
        path: path.to_owned(),
        container: ContainerKind::Zip,
        base_dir: std::path::absolute(&base_dir).unwrap_or(base_dir),
    };
    document::read_document(&mut store, path, source)
}

fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}

/// Reads up to `n` bytes from the start of a file.
fn read_head(path: &Path, n: usize) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|source| io_error(path, source))?;
    let mut head = Vec::with_capacity(n);
    file.take(n as u64)
        .read_to_end(&mut head)
        .map_err(|source| io_error(path, source))?;
    Ok(head)
}

/// Whether `bytes` are the `.xfl` stub text (a BOM and whitespace allowed).
fn is_stub(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    std::str::from_utf8(bytes)
        .is_ok_and(|s| s.trim_matches(|c: char| c.is_whitespace() || c == '\0') == XFL_STUB)
}

/// Whether the file is named `DOMDocument.xml` (any case).
fn is_document_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("DOMDocument.xml"))
}

/// Whether `dir` contains `DOMDocument.xml` (any case).
fn has_document(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.filter_map(|e| e.ok()).any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case("DOMDocument.xml"))
        })
    })
}

/// The folder holding `path`; `.` for a bare relative file name.
fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_owned(),
        _ => PathBuf::from("."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_text_is_recognised() {
        assert!(is_stub(b"PROXY-CS5"));
        assert!(is_stub(b"\xEF\xBB\xBFPROXY-CS5\r\n"));
        assert!(!is_stub(b"PROXY-CS6"));
        assert!(!is_stub(b"\xFF\xFE"));
    }

    #[test]
    fn parent_of_a_bare_name_is_the_current_folder() {
        assert_eq!(parent_dir(Path::new("doc.xfl")), PathBuf::from("."));
        assert_eq!(parent_dir(Path::new("a/doc.xfl")), PathBuf::from("a"));
    }
}
