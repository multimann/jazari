//! `settings.toml` in the data folder: the rendering preference and the
//! recent documents.
//!
//! The rendering preference is what the user chose. The mode actually used
//! is [`active_mode`]: `JAZARI_FORCE_CPU` always wins, and that forced value
//! is never written back, because only the user's own choice is saved.

use std::fmt;
use std::path::{Path, PathBuf};

use jazari_preview::RenderMode;
use serde::{Deserialize, Serialize};

/// How many recent documents are kept.
pub const MAX_RECENT: usize = 10;

/// The renderer the user prefers. It takes effect at the next start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RenderPref {
    /// A hardware adapter.
    Gpu,
    /// The software adapter (WARP, lavapipe or llvmpipe). The default, so a
    /// fresh install never puts load on a GPU unless asked to.
    #[default]
    Cpu,
}

impl RenderPref {
    /// The preview's render mode for this preference (before the
    /// environment override).
    pub fn mode(self) -> RenderMode {
        match self {
            RenderPref::Gpu => RenderMode::Gpu,
            RenderPref::Cpu => RenderMode::Cpu,
        }
    }
}

/// The mode to start with: CPU whenever `forced` (`JAZARI_FORCE_CPU`),
/// otherwise the preference.
pub fn active_mode(pref: RenderPref, forced: bool) -> RenderMode {
    if forced { RenderMode::Cpu } else { pref.mode() }
}

/// Everything saved in `settings.toml`. Unknown keys are ignored and missing
/// keys take their defaults, so older and newer files both load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    /// The renderer preference.
    pub rendering: RenderPref,
    /// Recently opened documents, most recent first.
    pub recent: Vec<PathBuf>,
}

/// Why settings could not be read or written.
#[derive(Debug)]
pub enum SettingsError {
    /// Reading or writing the file failed.
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The file is not valid TOML for these settings.
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },
    /// The settings could not be turned into TOML.
    Serialize(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SettingsError::Io { path, source } => {
                tr!("settings.io_error", path = path.display(), error = source)
            }
            SettingsError::Parse { path, message } => {
                tr!(
                    "settings.parse_error",
                    path = path.display(),
                    error = message
                )
            }
            SettingsError::Serialize(message) => tr!("settings.serialize_error", error = message),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// Reads `path`. A missing file gives the defaults without a problem; an
    /// unreadable or broken file gives the defaults and the problem, and a
    /// broken file is kept as `settings.toml.bad` so the next save does not
    /// destroy it silently.
    pub fn load(path: &Path) -> (Settings, Option<SettingsError>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Settings::default(), None);
            }
            Err(source) => {
                return (
                    Settings::default(),
                    Some(SettingsError::Io {
                        path: path.to_owned(),
                        source,
                    }),
                );
            }
        };
        match Settings::from_toml(&text) {
            Ok(settings) => (settings, None),
            Err(message) => {
                let _ = std::fs::copy(path, path.with_extension("toml.bad"));
                (
                    Settings::default(),
                    Some(SettingsError::Parse {
                        path: path.to_owned(),
                        message,
                    }),
                )
            }
        }
    }

    /// Parses settings from TOML text.
    pub fn from_toml(text: &str) -> Result<Settings, String> {
        let mut settings: Settings = toml::from_str(text).map_err(|e| e.to_string())?;
        settings.normalise_recent();
        Ok(settings)
    }

    /// The settings as TOML text.
    pub fn to_toml(&self) -> Result<String, SettingsError> {
        toml::to_string_pretty(self).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    /// Writes `path` (through a temporary file, so a crash never leaves half
    /// a file), creating its folder if needed.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let text = self.to_toml()?;
        let io = |source| SettingsError::Io {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)
    }

    /// Puts `path` at the top of the recent list. Paths that cannot be
    /// written as UTF-8 text are not remembered.
    pub fn add_recent(&mut self, path: &Path) {
        if path.to_str().is_none() {
            return;
        }
        self.recent.retain(|p| !same_path(p, path));
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    /// Removes duplicates and empty entries and applies the size limit.
    fn normalise_recent(&mut self) {
        let mut kept: Vec<PathBuf> = Vec::new();
        for p in self.recent.drain(..) {
            if !p.as_os_str().is_empty() && !kept.iter().any(|k| same_path(k, &p)) {
                kept.push(p);
            }
        }
        kept.truncate(MAX_RECENT);
        self.recent = kept;
    }
}

/// Path equality as the file system sees it: on Windows, case and the
/// kind of slash do not matter.
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        let norm = |p: &Path| {
            p.as_os_str()
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase()
        };
        norm(a) == norm(b)
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::tests::scratch;

    #[test]
    fn defaults_are_cpu_and_no_recent_documents() {
        let s = Settings::default();
        assert_eq!(s.rendering, RenderPref::Cpu);
        assert!(s.recent.is_empty());
    }

    #[test]
    fn round_trip_through_a_file() {
        let dir = scratch("settings-roundtrip");
        let path = dir.join("sub").join("settings.toml");
        let mut s = Settings {
            rendering: RenderPref::Gpu,
            recent: Vec::new(),
        };
        s.add_recent(Path::new("C:/docs/a/xfl"));
        s.add_recent(Path::new("C:/docs/مرحبا.fla"));
        s.save(&path).expect("save");
        let (loaded, problem) = Settings::load(&path);
        assert!(problem.is_none(), "{problem:?}");
        assert_eq!(loaded, s);
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("rendering = \"gpu\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_not_a_problem() {
        let dir = scratch("settings-missing");
        let (s, problem) = Settings::load(&dir.join("settings.toml"));
        assert_eq!(s, Settings::default());
        assert!(problem.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_file_gives_defaults_and_is_kept() {
        let dir = scratch("settings-broken");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "rendering = [unclosed").expect("write");
        let (s, problem) = Settings::load(&path);
        assert_eq!(s, Settings::default());
        assert!(matches!(problem, Some(SettingsError::Parse { .. })));
        assert!(dir.join("settings.toml.bad").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_and_missing_keys_are_tolerated() {
        let s = Settings::from_toml("future_option = 3\n").expect("parse");
        assert_eq!(s, Settings::default());
        let s = Settings::from_toml("rendering = \"gpu\"\n").expect("parse");
        assert_eq!(s.rendering, RenderPref::Gpu);
        assert!(Settings::from_toml("rendering = \"quantum\"\n").is_err());
    }

    #[test]
    fn recent_list_is_deduplicated_and_bounded() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.add_recent(Path::new(&format!("/d/{i}")));
        }
        s.add_recent(Path::new("/d/3"));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], PathBuf::from("/d/3"));
        assert_eq!(
            s.recent
                .iter()
                .filter(|p| p.as_path() == Path::new("/d/3"))
                .count(),
            1
        );
        if cfg!(windows) {
            let mut w = Settings::default();
            w.add_recent(Path::new("C:/Docs/A"));
            w.add_recent(Path::new("c:\\docs\\a\\"));
            assert_eq!(w.recent.len(), 1, "same folder, other spelling");
        }
        let s = Settings::from_toml("recent = [\"/a\", \"/a\", \"\", \"/b\"]\n").expect("parse");
        assert_eq!(s.recent, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn forced_cpu_wins_and_is_never_saved() {
        assert_eq!(active_mode(RenderPref::Gpu, true), RenderMode::Cpu);
        assert_eq!(active_mode(RenderPref::Cpu, true), RenderMode::Cpu);
        assert_eq!(active_mode(RenderPref::Gpu, false), RenderMode::Gpu);
        assert_eq!(active_mode(RenderPref::Cpu, false), RenderMode::Cpu);

        // A GPU preference survives a forced-CPU session: the settings hold
        // only the preference, and saving writes it unchanged.
        let dir = scratch("settings-forced");
        let path = dir.join("settings.toml");
        let s = Settings {
            rendering: RenderPref::Gpu,
            ..Settings::default()
        };
        s.save(&path).expect("save");
        let (loaded, _) = Settings::load(&path);
        assert_eq!(active_mode(loaded.rendering, true), RenderMode::Cpu);
        loaded.save(&path).expect("save again");
        let (again, _) = Settings::load(&path);
        assert_eq!(again.rendering, RenderPref::Gpu);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
