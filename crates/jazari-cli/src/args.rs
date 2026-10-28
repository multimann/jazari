//! A small hand-written argument parser: positional arguments plus
//! `--flag`, `--option value` and `--option=value`, and frame lists.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::error::{CliError, Result};

/// The largest frame number a frame list may name. Rendering frame N runs N
/// frames, so this bounds the work a typo can cause.
pub const MAX_FRAME: u32 = 100_000;

/// Parsed arguments of one command.
#[derive(Debug, Default)]
pub struct Args {
    positional: Vec<OsString>,
    options: BTreeMap<String, Option<String>>,
}

impl Args {
    /// Parses `raw` (without the program and command names). `switches`
    /// are flags without a value; `valued` are options that take one.
    /// Anything else starting with `--` is an error, and so is a repeated
    /// option. `--` ends option parsing.
    pub fn parse(raw: &[OsString], switches: &[&str], valued: &[&str]) -> Result<Self> {
        let mut out = Args::default();
        let mut iter = raw.iter();
        let mut only_positional = false;
        while let Some(arg) = iter.next() {
            let text = arg.to_str();
            let flag = match text {
                Some(t) if !only_positional && t.starts_with("--") => t,
                _ => {
                    out.positional.push(arg.clone());
                    continue;
                }
            };
            if flag == "--" {
                only_positional = true;
                continue;
            }
            let (name, inline) = match flag.split_once('=') {
                Some((n, v)) => (n, Some(v.to_owned())),
                None => (flag, None),
            };
            let name = name.trim_start_matches("--");
            let value = if switches.contains(&name) {
                if inline.is_some() {
                    return Err(CliError::Usage(format!("--{name} takes no value")));
                }
                None
            } else if valued.contains(&name) {
                let value = match inline {
                    Some(v) => v,
                    None => iter
                        .next()
                        .and_then(|v| v.to_str())
                        .map(str::to_owned)
                        .ok_or_else(|| CliError::Usage(format!("--{name} needs a value")))?,
                };
                Some(value)
            } else {
                return Err(CliError::Usage(format!("unknown option --{name}")));
            };
            if out.options.insert(name.to_owned(), value).is_some() {
                return Err(CliError::Usage(format!("--{name} is given twice")));
            }
        }
        Ok(out)
    }

    /// Whether the switch `name` was given.
    pub fn has(&self, name: &str) -> bool {
        self.options.contains_key(name)
    }

    /// The value of the option `name`, if given.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.options.get(name).and_then(|v| v.as_deref())
    }

    /// The positional arguments as paths; exactly `names.len()` of them
    /// must be present (the names are used in the error message).
    pub fn paths(&self, names: &[&str]) -> Result<Vec<PathBuf>> {
        if self.positional.len() != names.len() {
            let wanted = names
                .iter()
                .map(|n| format!("<{n}>"))
                .collect::<Vec<_>>()
                .join(" ");
            return Err(CliError::Usage(format!(
                "expected {} argument(s): {wanted}, got {}",
                names.len(),
                self.positional.len()
            )));
        }
        Ok(self.positional.iter().map(PathBuf::from).collect())
    }

    /// The option `name` parsed as a number, or `default`.
    pub fn number<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T> {
        match self.value(name) {
            None => Ok(default),
            Some(v) => v
                .trim()
                .parse()
                .map_err(|_| CliError::Usage(format!("--{name}: {v:?} is not a valid number"))),
        }
    }
}

/// Which frames to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameSpec {
    /// Every frame of the movie (1 to its frame count).
    All,
    /// These 1-based frames, sorted and without duplicates.
    List(Vec<u32>),
}

impl FrameSpec {
    /// Parses `all` or a comma-separated list of frames and ranges, such as
    /// `1,5,10` or `1-12,20`. Frames are 1-based.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("all") {
            return Ok(FrameSpec::All);
        }
        let bad = |why: &str| CliError::Usage(format!("bad frame list {text:?}: {why}"));
        let number = |s: &str| -> Result<u32> {
            let n: u32 = s
                .trim()
                .parse()
                .map_err(|_| bad(&format!("{:?} is not a frame number", s.trim())))?;
            if n == 0 {
                return Err(bad("frames are numbered from 1"));
            }
            if n > MAX_FRAME {
                return Err(bad(&format!("frame {n} is above the limit of {MAX_FRAME}")));
            }
            Ok(n)
        };
        let mut frames = Vec::new();
        for item in text.split(',').filter(|s| !s.trim().is_empty()) {
            match item.split_once('-') {
                Some((a, b)) => {
                    let (a, b) = (number(a)?, number(b)?);
                    if a > b {
                        return Err(bad(&format!("range {a}-{b} runs backwards")));
                    }
                    frames.extend(a..=b);
                }
                None => frames.push(number(item)?),
            }
        }
        if frames.is_empty() {
            return Err(bad("no frames"));
        }
        frames.sort_unstable();
        frames.dedup();
        Ok(FrameSpec::List(frames))
    }

    /// The frames for a movie of `num_frames` frames (`All` = 1 to
    /// `num_frames`, at least frame 1).
    pub fn resolve(&self, num_frames: u32) -> Vec<u32> {
        match self {
            FrameSpec::All => (1..=num_frames.clamp(1, MAX_FRAME)).collect(),
            FrameSpec::List(list) => list.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_switches_options_and_positionals() {
        let a = Args::parse(
            &os(&["in.swf", "--stage", "--frames", "1,2", "out", "--scale=2"]),
            &["stage"],
            &["frames", "scale"],
        )
        .unwrap();
        assert!(a.has("stage"));
        assert_eq!(a.value("frames"), Some("1,2"));
        assert_eq!(a.number::<f32>("scale", 1.0).unwrap(), 2.0);
        assert_eq!(a.paths(&["swf", "dir"]).unwrap().len(), 2);
        assert!(a.paths(&["swf"]).is_err());
    }

    #[test]
    fn rejects_unknown_repeated_and_valueless_options() {
        assert!(Args::parse(&os(&["--nope"]), &[], &[]).is_err());
        assert!(Args::parse(&os(&["--stage", "--stage"]), &["stage"], &[]).is_err());
        assert!(Args::parse(&os(&["--frames"]), &[], &["frames"]).is_err());
        assert!(Args::parse(&os(&["--stage=1"]), &["stage"], &[]).is_err());
        let a = Args::parse(&os(&["--", "--stage"]), &["stage"], &[]).unwrap();
        assert!(!a.has("stage"));
        assert_eq!(a.paths(&["x"]).unwrap()[0], PathBuf::from("--stage"));
    }

    #[test]
    fn frame_lists() {
        assert_eq!(FrameSpec::parse("all").unwrap(), FrameSpec::All);
        assert_eq!(
            FrameSpec::parse("10, 1,5,1-3").unwrap(),
            FrameSpec::List(vec![1, 2, 3, 5, 10])
        );
        assert!(FrameSpec::parse("0").is_err());
        assert!(FrameSpec::parse("5-2").is_err());
        assert!(FrameSpec::parse("x").is_err());
        assert!(FrameSpec::parse("").is_err());
        assert!(FrameSpec::parse("1-4000000000").is_err());
        assert_eq!(FrameSpec::All.resolve(3), vec![1, 2, 3]);
        assert_eq!(FrameSpec::All.resolve(0), vec![1]);
    }
}
