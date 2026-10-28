//! Command-line arguments.
//!
//! ```text
//! jazari [--open <doc>] [--frame N] [--screenshot <png>] [--quit-after-screenshot]
//! jazari <doc>
//! ```
//!
//! The screenshot options exist so the app can be checked automatically:
//! once the document is open and the window has settled, the window is
//! captured to a PNG and, with `--quit-after-screenshot`, the app closes.
//!
//! Developer options, not listed in `--help`:
//! - `--test-swf <file.swf>`: Test Movie plays this file instead of
//!   publishing the document, and starts once at launch;
//! - `--stage-swf <file.swf>`: the stage view plays this file instead of the
//!   stage build;
//! - `--screenshot-delay <seconds>`: how long the window must stay idle
//!   before the screenshot (default 3);
//! - `--select-layer <n|name>`: with `--frame`, the layer to select (1-based
//!   index from the top, or a name);
//! - `--test-movie`: run Control › Test Movie once the document is open;
//! - `--show-about`: open Help › About at startup;
//! - `--open-menu <name>`: open a menu (`file`, `control`, `settings`,
//!   `window` or `help`) at startup;
//! - `--dump-output`: print the Output panel to stdout when the app exits.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// Parsed command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    /// Document to open at startup.
    pub open: Option<PathBuf>,
    /// 1-based frame to show once the document is open.
    pub frame: Option<u32>,
    /// Where to save a screenshot of the window.
    pub screenshot: Option<PathBuf>,
    /// Close the app once the screenshot is saved.
    pub quit_after_screenshot: bool,
    /// Print the usage and exit.
    pub help: bool,
    /// Print the version and exit.
    pub version: bool,
    /// Developer: SWF for Test Movie instead of a publish.
    pub test_swf: Option<PathBuf>,
    /// Developer: SWF for the stage view instead of the stage build.
    pub stage_swf: Option<PathBuf>,
    /// Developer: idle time before the screenshot, in seconds.
    pub screenshot_delay: Option<f32>,
    /// Developer: the layer `--frame` selects.
    pub select_layer: Option<String>,
    /// Developer: run Test Movie once the document is open.
    pub test_movie: bool,
    /// Developer: open the About window at startup.
    pub show_about: bool,
    /// Developer: a menu to open at startup.
    pub open_menu: Option<String>,
    /// Developer: print the Output panel at exit.
    pub dump_output: bool,
    /// Developer: publish the document to this file once it is open
    /// (the File › Publish… code path, without the dialog).
    pub publish_to: Option<PathBuf>,
}

/// A command line that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    /// An option needs a value and none was given.
    MissingValue(String),
    /// A numeric option got something else.
    BadNumber {
        /// The option.
        flag: String,
        /// What was given.
        value: String,
    },
    /// An option this program does not know.
    Unknown(String),
    /// Two documents were given.
    TwoDocuments,
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ArgError::MissingValue(flag) => tr!("cli.missing_value", flag = flag),
            ArgError::BadNumber { flag, value } => {
                tr!("cli.bad_number", flag = flag, value = value)
            }
            ArgError::Unknown(arg) => tr!("cli.unknown", arg = arg),
            ArgError::TwoDocuments => tr!("cli.two_documents").to_owned(),
        };
        f.write_str(&text)
    }
}

/// The usage text for `--help`.
pub fn usage() -> String {
    tr!("cli.usage", version = env!("CARGO_PKG_VERSION"))
}

/// Parses the arguments after the program name. `--flag=value` and
/// `--flag value` are both accepted; a bare argument is the document.
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Args, ArgError> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let Some(text) = arg.to_str() else {
            // A non-UTF-8 argument can only be a path.
            set_document(&mut out, PathBuf::from(arg))?;
            continue;
        };
        if !text.starts_with("--") {
            set_document(&mut out, PathBuf::from(arg))?;
            continue;
        }
        let (flag, inline) = match text.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(OsString::from(value))),
            None => (text.to_owned(), None),
        };
        let value = |it: &mut I::IntoIter| -> Result<OsString, ArgError> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| ArgError::MissingValue(flag.clone()))
        };
        match flag.as_str() {
            "--open" => {
                let v = value(&mut it)?;
                set_document(&mut out, PathBuf::from(v))?;
            }
            "--frame" => {
                let v = value(&mut it)?;
                let n = number::<u32>(&flag, &v)?;
                if n == 0 {
                    return Err(ArgError::BadNumber {
                        flag: flag.clone(),
                        value: "0".to_owned(),
                    });
                }
                out.frame = Some(n);
            }
            "--screenshot" => out.screenshot = Some(PathBuf::from(value(&mut it)?)),
            "--quit-after-screenshot" => out.quit_after_screenshot = true,
            "--test-swf" => out.test_swf = Some(PathBuf::from(value(&mut it)?)),
            "--stage-swf" => out.stage_swf = Some(PathBuf::from(value(&mut it)?)),
            "--screenshot-delay" => {
                let v = value(&mut it)?;
                let secs = number::<f32>(&flag, &v)?;
                if !(0.0..=600.0).contains(&secs) {
                    return Err(ArgError::BadNumber {
                        flag: flag.clone(),
                        value: secs.to_string(),
                    });
                }
                out.screenshot_delay = Some(secs);
            }
            "--select-layer" => {
                let v = value(&mut it)?;
                out.select_layer = Some(v.to_string_lossy().into_owned());
            }
            "--show-about" => out.show_about = true,
            "--test-movie" => out.test_movie = true,
            "--open-menu" => {
                let v = value(&mut it)?;
                out.open_menu = Some(v.to_string_lossy().to_lowercase());
            }
            "--dump-output" => out.dump_output = true,
            "--publish-to" => out.publish_to = Some(PathBuf::from(value(&mut it)?)),
            "--help" => out.help = true,
            "--version" => out.version = true,
            _ => return Err(ArgError::Unknown(text.to_owned())),
        }
    }
    Ok(out)
}

fn set_document(out: &mut Args, path: PathBuf) -> Result<(), ArgError> {
    if out.open.is_some() {
        return Err(ArgError::TwoDocuments);
    }
    out.open = Some(path);
    Ok(())
}

fn number<T: std::str::FromStr>(flag: &str, value: &OsString) -> Result<T, ArgError> {
    let text = value.to_string_lossy();
    text.trim().parse().map_err(|_| ArgError::BadNumber {
        flag: flag.to_owned(),
        value: text.into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Args, ArgError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn empty_command_line() {
        assert_eq!(p(&[]), Ok(Args::default()));
    }

    #[test]
    fn all_public_options() {
        let a = p(&[
            "--open",
            "doc/xfl",
            "--frame",
            "12",
            "--screenshot=shot.png",
            "--quit-after-screenshot",
        ])
        .expect("valid");
        assert_eq!(a.open, Some(PathBuf::from("doc/xfl")));
        assert_eq!(a.frame, Some(12));
        assert_eq!(a.screenshot, Some(PathBuf::from("shot.png")));
        assert!(a.quit_after_screenshot);
    }

    #[test]
    fn bare_path_is_the_document() {
        let a = p(&["C:\\docs\\a.fla"]).expect("valid");
        assert_eq!(a.open, Some(PathBuf::from("C:\\docs\\a.fla")));
        assert_eq!(p(&["a", "--open", "b"]), Err(ArgError::TwoDocuments));
    }

    #[test]
    fn developer_options() {
        let a = p(&[
            "--test-swf",
            "m.swf",
            "--stage-swf=s.swf",
            "--screenshot-delay",
            "1.5",
            "--select-layer",
            "طبقة",
            "--show-about",
            "--test-movie",
            "--dump-output",
            "--publish-to",
            "out.swf",
        ])
        .expect("valid");
        assert_eq!(a.publish_to, Some(PathBuf::from("out.swf")));
        assert_eq!(a.select_layer.as_deref(), Some("طبقة"));
        assert!(a.show_about && a.dump_output && a.test_movie);
        assert_eq!(a.test_swf, Some(PathBuf::from("m.swf")));
        assert_eq!(a.stage_swf, Some(PathBuf::from("s.swf")));
        assert_eq!(a.screenshot_delay, Some(1.5));
    }

    #[test]
    fn errors() {
        assert_eq!(
            p(&["--frame"]),
            Err(ArgError::MissingValue("--frame".to_owned()))
        );
        assert!(matches!(
            p(&["--frame", "x"]),
            Err(ArgError::BadNumber { .. })
        ));
        assert!(matches!(
            p(&["--frame", "0"]),
            Err(ArgError::BadNumber { .. })
        ));
        assert!(matches!(
            p(&["--frame", "-3"]),
            Err(ArgError::BadNumber { .. })
        ));
        assert!(matches!(
            p(&["--screenshot-delay", "NaN"]),
            Err(ArgError::BadNumber { .. })
        ));
        assert_eq!(
            p(&["--bogus"]),
            Err(ArgError::Unknown("--bogus".to_owned()))
        );
        // Messages go through the string table.
        assert!(ArgError::Unknown("--x".into()).to_string().contains("--x"));
    }
}
