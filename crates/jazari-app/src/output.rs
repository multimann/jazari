//! The Output panel's log: traces, player messages, the import report and
//! compiler and publish diagnostics, each with an optional location.

use std::collections::VecDeque;

use crate::doc::DocLocation;

/// Most entries kept; the oldest go first.
pub const MAX_ENTRIES: usize = 20_000;

/// How serious an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// `trace()` output of the movie.
    Trace,
    /// Information.
    Info,
    /// Something was approximated or skipped.
    Warning,
    /// Something failed.
    Error,
}

/// Where an entry comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// The application itself.
    App,
    /// The XFL reader's import report.
    Import,
    /// The AS2 compiler and the publisher.
    Publish,
    /// The stage view's player.
    Stage,
    /// The Test Movie player.
    TestMovie,
}

/// One line of the Output panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Severity.
    pub level: Level,
    /// Origin.
    pub source: Source,
    /// The message.
    pub text: String,
    /// What clicking it selects.
    pub location: Option<DocLocation>,
    /// How many times in a row the same entry arrived.
    pub repeats: u32,
}

impl Entry {
    /// A new entry without a location.
    pub fn new(level: Level, source: Source, text: impl Into<String>) -> Self {
        Self {
            level,
            source,
            text: text.into(),
            location: None,
            repeats: 1,
        }
    }

    /// The same entry pointing at `location`.
    pub fn at(mut self, location: Option<DocLocation>) -> Self {
        self.location = location;
        self
    }
}

/// Which entries the panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    /// Movie traces.
    pub traces: bool,
    /// Information.
    pub info: bool,
    /// Warnings.
    pub warnings: bool,
    /// Errors.
    pub errors: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            traces: true,
            info: true,
            warnings: true,
            errors: true,
        }
    }
}

impl Filter {
    /// Whether an entry passes.
    pub fn accepts(&self, e: &Entry) -> bool {
        match e.level {
            Level::Trace => self.traces,
            Level::Info => self.info,
            Level::Warning => self.warnings,
            Level::Error => self.errors,
        }
    }
}

/// The log.
#[derive(Debug, Default)]
pub struct OutputLog {
    entries: VecDeque<Entry>,
}

impl OutputLog {
    /// Adds an entry. An entry equal to the last one (same level, source,
    /// text and location) only increases its repeat count, so a message
    /// sent every frame does not flood the panel.
    pub fn push(&mut self, entry: Entry) {
        if let Some(last) = self.entries.back_mut()
            && last.level == entry.level
            && last.source == entry.source
            && last.text == entry.text
            && last.location == entry.location
        {
            last.repeats = last.repeats.saturating_add(1);
            return;
        }
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entry `i` (0 = oldest).
    pub fn get(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// All entries, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// How many entries of `level` there are (repeats counted once).
    pub fn count(&self, level: Level) -> usize {
        self.entries.iter().filter(|e| e.level == level).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_entries_are_folded() {
        let mut log = OutputLog::default();
        for _ in 0..3 {
            log.push(Entry::new(Level::Warning, Source::Stage, "same"));
        }
        log.push(Entry::new(Level::Warning, Source::TestMovie, "same"));
        assert_eq!(log.len(), 2);
        assert_eq!(log.get(0).map(|e| e.repeats), Some(3));
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn the_log_is_bounded() {
        let mut log = OutputLog::default();
        for i in 0..MAX_ENTRIES + 5 {
            log.push(Entry::new(Level::Trace, Source::TestMovie, i.to_string()));
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        assert_eq!(log.get(0).map(|e| e.text.as_str()), Some("5"));
    }

    #[test]
    fn filter() {
        let f = Filter {
            traces: false,
            ..Filter::default()
        };
        assert!(!f.accepts(&Entry::new(Level::Trace, Source::TestMovie, "t")));
        assert!(f.accepts(&Entry::new(Level::Error, Source::Publish, "e")));
    }
}
