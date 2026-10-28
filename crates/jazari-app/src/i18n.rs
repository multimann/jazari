//! User-facing strings.
//!
//! Every string the user sees goes through `tr!`: a key looked
//! up in the table of the current language, optionally with `{name}`
//! placeholders filled in. Only English exists in milestone 1; Arabic is a
//! second table later. A missing key shows the key itself, so a gap is
//! visible instead of silent.
//!
//! ```ignore
//! ui.label(tr!("stage.no_document"));
//! ui.label(tr!("output.opened", path = path.display(), count = 3));
//! ```

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

/// Looks a key up, or formats it with named arguments.
///
/// `tr!("key")` returns a `&'static str`; `tr!("key", name = value, ..)`
/// returns a `String` with every `{name}` replaced by `value`'s `Display`.
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::lookup($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::format(
            $crate::i18n::lookup($key),
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

/// The English table: `(key, text)`.
mod en;

/// Returns the text for `key` in the current language, or `key` itself when
/// the table has no such entry.
pub fn lookup(key: &'static str) -> &'static str {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let table = TABLE.get_or_init(|| en::ENTRIES.iter().copied().collect());
    table.get(key).copied().unwrap_or(key)
}

/// Replaces each `{name}` in `template` with the matching argument.
/// Unknown placeholders are left as they are; `{{` and `}}` are not special.
pub fn format(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let replaced = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, value)| (close, value.to_string()))
        });
        match replaced {
            Some((close, value)) => {
                out.push_str(&value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn known_keys_resolve_and_unknown_keys_echo() {
        assert_eq!(lookup("menu.file"), "File");
        assert_eq!(lookup("no.such.key"), "no.such.key");
        assert_eq!(tr!("menu.file"), "File");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            format("a {x} b {y} c {x}", &[("x", &1), ("y", &"two")]),
            "a 1 b two c 1"
        );
        assert_eq!(format("keep {z} and {", &[("x", &1)]), "keep {z} and {");
        assert_eq!(format("{x}{x}", &[("x", &"é")]), "éé");
        let s = tr!("app.frame_of", frame = 3, total = 10);
        assert!(s.contains('3') && s.contains("10"), "{s}");
    }

    #[test]
    fn table_has_unique_non_empty_entries() {
        let mut seen = HashSet::new();
        for (key, text) in en::ENTRIES {
            assert!(seen.insert(key), "duplicate key {key}");
            assert!(!text.trim().is_empty(), "empty text for {key}");
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._".contains(c)),
                "badly formed key {key}"
            );
        }
    }

    /// Every `tr!("key"...)` in the crate's sources has an entry, and every
    /// entry is used somewhere.
    #[test]
    fn every_used_key_exists_and_every_entry_is_used() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut used = HashSet::new();
        let mut files = vec![src];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("read src dir") {
                    files.push(entry.expect("dir entry").path());
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("read source");
                collect_keys(&text, &mut used);
            }
        }
        let table: HashSet<&str> = en::ENTRIES.iter().map(|(k, _)| *k).collect();
        let mut missing: Vec<_> = used
            .iter()
            .filter(|k| !table.contains(k.as_str()))
            .collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "keys without an English entry: {missing:?}"
        );
        let mut unused: Vec<_> = table
            .iter()
            .filter(|k| !used.contains(**k) && !k.starts_with("test."))
            .collect();
        unused.sort();
        assert!(unused.is_empty(), "entries never used: {unused:?}");
    }

    /// Finds `tr!(` followed (after optional white space) by a string
    /// literal, skipping comment lines such as the macro's documentation.
    fn collect_keys(text: &str, out: &mut HashSet<String>) {
        // Spelled in two parts so this line is not found itself.
        const OPEN: &str = concat!("tr", "!(");
        for (i, _) in text.match_indices(OPEN) {
            let line_start = text[..i].rfind('\n').map_or(0, |n| n + 1);
            if text[line_start..i].trim_start().starts_with("//") {
                continue;
            }
            // `include_str!(` and the like are not the macro.
            if text[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let rest = text[i + OPEN.len()..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = rest.find('"') {
                out.insert(rest[..end].to_owned());
            }
        }
    }
}
