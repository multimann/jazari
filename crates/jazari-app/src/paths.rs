//! Where settings, the window layout and the log live.
//!
//! In order (`docs/ARCHITECTURE.md`, section 2; `docs/research/app-shell.md`,
//! item 6):
//!
//! 1. the `JAZARI_HOME` environment variable, when set and not empty;
//! 2. `JazariData/` next to the executable, when it can be created and
//!    written (the portable default). Inside an AppImage the executable sits
//!    on a read-only mount, so the folder next to the `.AppImage` file is
//!    used instead, and only when it already exists;
//! 3. the platform configuration folder: `dirs::config_dir()/Al-Jazari`;
//! 4. as a last resort, `Al-Jazari` in the temporary folder.

use std::path::{Path, PathBuf};

/// The folder next to the executable that makes the app portable.
pub const PORTABLE_DIR: &str = "JazariData";
/// The folder name under the platform configuration folder.
pub const PLATFORM_DIR: &str = "Al-Jazari";
/// The environment variable that overrides the data folder.
pub const HOME_ENV: &str = "JAZARI_HOME";

/// Why a data folder was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDirKind {
    /// `JAZARI_HOME`.
    Env,
    /// `JazariData` next to the executable (or the AppImage).
    Portable,
    /// The platform configuration folder.
    Platform,
    /// The temporary folder: nothing else was writable.
    Temporary,
}

/// The folder holding the settings, the window layout and the log.
#[derive(Debug, Clone, PartialEq)]
pub struct DataDir {
    /// The folder.
    pub dir: PathBuf,
    /// How it was found.
    pub kind: DataDirKind,
}

impl DataDir {
    /// `settings.toml`.
    pub fn settings_file(&self) -> PathBuf {
        self.dir.join("settings.toml")
    }

    /// eframe's RON file with the window and dock layout. eframe takes a file
    /// path here, not a folder (research F15).
    pub fn ui_state_file(&self) -> PathBuf {
        self.dir.join("ui-state.ron")
    }

    /// The log of fatal startup errors.
    pub fn log_file(&self) -> PathBuf {
        self.dir.join("jazari.log")
    }
}

/// What the resolution looks at, gathered from the process environment by
/// [`Environment::current`] (tests build it by hand).
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// `JAZARI_HOME`.
    pub jazari_home: Option<PathBuf>,
    /// The executable's folder.
    pub exe_dir: Option<PathBuf>,
    /// `APPIMAGE`: the path of the running `.AppImage` file.
    pub appimage: Option<PathBuf>,
    /// `dirs::config_dir()`.
    pub config_dir: Option<PathBuf>,
    /// `std::env::temp_dir()`.
    pub temp_dir: PathBuf,
}

impl Environment {
    /// Reads the real environment.
    pub fn current() -> Self {
        let non_empty = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        Self {
            jazari_home: non_empty(HOME_ENV).map(PathBuf::from),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
            appimage: non_empty("APPIMAGE").map(PathBuf::from),
            config_dir: dirs::config_dir(),
            temp_dir: std::env::temp_dir(),
        }
    }
}

/// Chooses the data folder. `writable` creates a folder if needed and says
/// whether files can be written in it ([`ensure_writable`] in the app).
pub fn resolve(env: &Environment, writable: &dyn Fn(&Path) -> bool) -> DataDir {
    if let Some(home) = &env.jazari_home {
        // The override is honoured even when it cannot be written: the user
        // asked for it, and saving reports the problem.
        let _ = writable(home);
        return DataDir {
            dir: home.clone(),
            kind: DataDirKind::Env,
        };
    }
    let portable = match (&env.appimage, &env.exe_dir) {
        (Some(appimage), _) => appimage
            .parent()
            .map(|base| base.join(PORTABLE_DIR))
            .filter(|dir| dir.is_dir()),
        (None, Some(exe_dir)) => Some(exe_dir.join(PORTABLE_DIR)),
        (None, None) => None,
    };
    if let Some(dir) = portable.filter(|dir| writable(dir)) {
        return DataDir {
            dir,
            kind: DataDirKind::Portable,
        };
    }
    if let Some(dir) = env
        .config_dir
        .as_ref()
        .map(|c| c.join(PLATFORM_DIR))
        .filter(|dir| writable(dir))
    {
        return DataDir {
            dir,
            kind: DataDirKind::Platform,
        };
    }
    let dir = env.temp_dir.join(PLATFORM_DIR);
    let _ = writable(&dir);
    DataDir {
        dir,
        kind: DataDirKind::Temporary,
    }
}

/// Creates `dir` if needed and checks that a file can be written in it.
pub fn ensure_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    fn env() -> Environment {
        Environment {
            jazari_home: None,
            exe_dir: Some(PathBuf::from("/opt/jazari")),
            appimage: None,
            config_dir: Some(PathBuf::from("/home/u/.config")),
            temp_dir: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn jazari_home_wins() {
        let mut e = env();
        e.jazari_home = Some(PathBuf::from("/data/jz"));
        let d = resolve(&e, &|_| true);
        assert_eq!(d.dir, PathBuf::from("/data/jz"));
        assert_eq!(d.kind, DataDirKind::Env);
        // Even when it cannot be written.
        assert_eq!(resolve(&e, &|_| false).kind, DataDirKind::Env);
    }

    #[test]
    fn portable_next_to_the_executable_when_writable() {
        let d = resolve(&env(), &|_| true);
        assert_eq!(d.dir, Path::new("/opt/jazari").join(PORTABLE_DIR));
        assert_eq!(d.kind, DataDirKind::Portable);
        assert_eq!(d.settings_file(), d.dir.join("settings.toml"));
        assert_eq!(d.ui_state_file(), d.dir.join("ui-state.ron"));
    }

    #[test]
    fn platform_folder_when_the_executable_folder_is_read_only() {
        let tried = RefCell::new(Vec::new());
        let writable = |p: &Path| {
            tried.borrow_mut().push(p.to_path_buf());
            !p.starts_with("/opt")
        };
        let d = resolve(&env(), &writable);
        assert_eq!(d.dir, Path::new("/home/u/.config").join(PLATFORM_DIR));
        assert_eq!(d.kind, DataDirKind::Platform);
        assert_eq!(
            tried.borrow()[0],
            Path::new("/opt/jazari").join(PORTABLE_DIR)
        );
    }

    #[test]
    fn temporary_folder_as_a_last_resort() {
        let d = resolve(&env(), &|_| false);
        assert_eq!(d.dir, Path::new("/tmp").join(PLATFORM_DIR));
        assert_eq!(d.kind, DataDirKind::Temporary);
        let mut e = env();
        e.exe_dir = None;
        e.config_dir = None;
        assert_eq!(resolve(&e, &|_| true).kind, DataDirKind::Temporary);
    }

    #[test]
    fn appimage_is_portable_only_with_an_existing_folder() {
        let root = scratch("appimage");
        let mut e = env();
        e.appimage = Some(root.join("Al-Jazari.AppImage"));
        // No JazariData next to the AppImage: platform folder.
        assert_eq!(resolve(&e, &|_| true).kind, DataDirKind::Platform);
        std::fs::create_dir_all(root.join(PORTABLE_DIR)).expect("mkdir");
        let d = resolve(&e, &|_| true);
        assert_eq!(d.kind, DataDirKind::Portable);
        assert_eq!(d.dir, root.join(PORTABLE_DIR));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_writable_creates_the_folder() {
        let root = scratch("writable");
        let dir = root.join("a").join("b");
        assert!(ensure_writable(&dir));
        assert!(dir.is_dir());
        assert_eq!(
            std::fs::read_dir(&dir).expect("list").count(),
            0,
            "probe removed"
        );
        // A file in the way: not a usable folder.
        let file = root.join("file");
        std::fs::write(&file, b"x").expect("write");
        assert!(!ensure_writable(&file.join("sub")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A fresh folder under the system temporary folder.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jazari-app-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }
}
