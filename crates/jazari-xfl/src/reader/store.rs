//! The files of a document: an XFL folder on disk or the entries of a `.fla`
//! ZIP archive, behind one interface with tolerant path lookup.
//!
//! Paths are normalised to `/` separators without a leading `./`. Lookup
//! tries the exact path, then a case-insensitive match; callers can also ask
//! for a unique basename match inside a folder, because library file names
//! do not always match the `Include` hrefs (`docs/research/xfl-format.md`,
//! F03).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Largest single entry read into memory (a guard against ZIP bombs).
const MAX_ENTRY_BYTES: u64 = 1 << 30;
/// Deepest folder level listed in an XFL folder.
const MAX_FOLDER_DEPTH: usize = 32;
/// Most files listed in an XFL folder.
const MAX_FOLDER_FILES: usize = 200_000;

/// The files of a document.
pub(crate) struct Store {
    /// Normalised paths, in listing order.
    paths: Vec<String>,
    /// Path → index into `paths` (first one wins).
    exact: HashMap<String, usize>,
    /// Lowercased path → index into `paths` (first one wins).
    lower: HashMap<String, usize>,
    backend: Backend,
}

enum Backend {
    /// Files on disk: one real path per entry of `paths`.
    Folder(Vec<PathBuf>),
    /// A ZIP archive: one archive index per entry of `paths`.
    Zip {
        archive: Box<zip::ZipArchive<BufReader<File>>>,
        indices: Vec<usize>,
    },
}

impl Store {
    /// Lists the files of an XFL folder.
    pub fn folder(root: &Path) -> Result<Self> {
        let mut paths = Vec::new();
        let mut real = Vec::new();
        list_folder(root, "", 0, &mut paths, &mut real)?;
        Ok(Self::new(paths, Backend::Folder(real)))
    }

    /// Opens a ZIP archive and lists its entries (directories excluded).
    pub fn zip(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        let archive = zip::ZipArchive::new(BufReader::new(file)).map_err(|source| Error::Zip {
            path: path.to_owned(),
            source,
        })?;
        let mut paths = Vec::new();
        let mut indices = Vec::new();
        for (i, name) in archive.file_names().enumerate() {
            let Ok(name) = name else { continue };
            if name.ends_with('/') || name.ends_with('\\') {
                continue;
            }
            paths.push(normalize(&name));
            indices.push(i);
        }
        Ok(Self::new(
            paths,
            Backend::Zip {
                archive: Box::new(archive),
                indices,
            },
        ))
    }

    fn new(paths: Vec<String>, backend: Backend) -> Self {
        let mut exact = HashMap::with_capacity(paths.len());
        let mut lower = HashMap::with_capacity(paths.len());
        for (i, p) in paths.iter().enumerate() {
            exact.entry(p.clone()).or_insert(i);
            lower.entry(p.to_lowercase()).or_insert(i);
        }
        Self {
            paths,
            exact,
            lower,
            backend,
        }
    }

    /// All normalised paths.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// The stored path that matches `wanted` exactly or, failing that,
    /// case-insensitively.
    pub fn find(&self, wanted: &str) -> Option<&str> {
        let wanted = normalize(wanted);
        self.exact
            .get(&wanted)
            .or_else(|| self.lower.get(&wanted.to_lowercase()))
            .map(|&i| self.paths[i].as_str())
    }

    /// The only path under `folder` (for example `LIBRARY/`) whose file name
    /// equals the file name of `wanted`, ignoring case. `None` when there is
    /// no such path or more than one.
    pub fn find_by_basename(&self, folder: &str, wanted: &str) -> Option<&str> {
        let wanted = normalize(wanted);
        let base = wanted.rsplit('/').next()?.to_lowercase();
        let folder = folder.to_lowercase();
        let mut found = self.paths.iter().filter(|p| {
            let lp = p.to_lowercase();
            lp.starts_with(&folder) && lp.rsplit('/').next() == Some(base.as_str())
        });
        let first = found.next()?;
        found.next().is_none().then_some(first.as_str())
    }

    /// Reads a file by its stored path (as returned by [`Store::find`]).
    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let index = *self
            .exact
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_owned()))?;
        match &mut self.backend {
            Backend::Folder(real) => {
                let file = File::open(&real[index])?;
                read_limited(file)
            }
            Backend::Zip { archive, indices } => {
                let entry = archive.by_index(indices[index]).map_err(io::Error::other)?;
                read_limited(entry)
            }
        }
    }
}

/// Reads at most [`MAX_ENTRY_BYTES`]; a larger entry is an error.
fn read_limited(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX_ENTRY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the file is larger than 1 GiB",
        ));
    }
    Ok(out)
}

/// Lists `dir` recursively into normalised relative paths and real paths.
fn list_folder(
    dir: &Path,
    prefix: &str,
    depth: usize,
    paths: &mut Vec<String>,
    real: &mut Vec<PathBuf>,
) -> Result<()> {
    let io_err = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(io_err)?
        .filter_map(|e| e.ok())
        .collect();
    // A stable order makes "first match wins" deterministic across systems.
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if paths.len() >= MAX_FOLDER_FILES {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth < MAX_FOLDER_DEPTH {
                // Unreadable subfolders are skipped, not fatal.
                let _ = list_folder(&entry.path(), &format!("{rel}/"), depth + 1, paths, real);
            }
        } else if kind.is_file() || kind.is_symlink() {
            paths.push(rel);
            real.push(entry.path());
        }
    }
    Ok(())
}

/// Normalises a path: `\` to `/`, no leading `./` or `/`, no empty segments.
pub(crate) fn normalize(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_separators_and_dots() {
        assert_eq!(normalize("LIBRARY\\art\\Ball.xml"), "LIBRARY/art/Ball.xml");
        assert_eq!(normalize("./bin//M 1.dat"), "bin/M 1.dat");
        assert_eq!(normalize("/DOMDocument.xml"), "DOMDocument.xml");
    }

    fn memory_store(paths: &[&str]) -> Store {
        let paths: Vec<String> = paths.iter().map(|p| normalize(p)).collect();
        let real = paths.iter().map(PathBuf::from).collect();
        Store::new(paths, Backend::Folder(real))
    }

    #[test]
    fn finds_exact_then_case_insensitive() {
        let s = memory_store(&[
            "DOMDocument.xml",
            "LIBRARY/Art/Ball.xml",
            "library/art/ball.xml",
        ]);
        assert_eq!(s.find("LIBRARY/Art/Ball.xml"), Some("LIBRARY/Art/Ball.xml"));
        assert_eq!(s.find("library/art/ball.xml"), Some("library/art/ball.xml"));
        assert_eq!(
            s.find("Library\\ART\\ball.XML"),
            Some("LIBRARY/Art/Ball.xml")
        );
        assert_eq!(s.find("domdocument.xml"), Some("DOMDocument.xml"));
        assert_eq!(s.find("missing.xml"), None);
    }

    #[test]
    fn basename_match_must_be_unique() {
        let s = memory_store(&[
            "LIBRARY/a/Ball.xml",
            "LIBRARY/b/Box.xml",
            "LIBRARY/c/box.xml",
        ]);
        assert_eq!(
            s.find_by_basename("LIBRARY/", "Ball.xml"),
            Some("LIBRARY/a/Ball.xml")
        );
        assert_eq!(
            s.find_by_basename("LIBRARY/", "x/ball.XML"),
            Some("LIBRARY/a/Ball.xml")
        );
        assert_eq!(
            s.find_by_basename("LIBRARY/", "Box.xml"),
            None,
            "two matches"
        );
        assert_eq!(s.find_by_basename("bin/", "Ball.xml"), None);
    }
}
