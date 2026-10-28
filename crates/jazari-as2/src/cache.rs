//! Cache of compiled scripts.
//!
//! The key is a SHA-256 of the compiler settings and the script text; an
//! entry also remembers the hash of every file the compile read (`#include`
//! and class files) and every `#include` file it did not find, and is used
//! only while those files are unchanged (or still missing). Class files that
//! did not exist at compile time are not tracked.

use crate::backend::{Compiled, sha256};
use crate::{CompilerOptions, SceneContext};
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// Entries kept before the cache is emptied, so a long editing session does
/// not grow without bound.
const MAX_ENTRIES: usize = 4096;

/// Thread-safe map from script hash to compile result.
#[derive(Debug, Default)]
pub(crate) struct Cache {
    entries: Mutex<HashMap<[u8; 32], Compiled>>,
}

impl Cache {
    /// The key of `text` compiled with `options` in `scenes`.
    pub(crate) fn key(
        options: &CompilerOptions,
        text: &str,
        scenes: Option<&SceneContext>,
    ) -> [u8; 32] {
        let mut data = Vec::with_capacity(text.len() + 256);
        data.push(options.swf_version);
        let mut push_field = |field: &[u8]| {
            data.extend_from_slice(&(field.len() as u64).to_le_bytes());
            data.extend_from_slice(field);
        };
        for path in &options.class_paths {
            push_field(path.as_os_str().as_encoded_bytes());
        }
        push_field(b"|");
        if let Some(base) = &options.include_base {
            push_field(base.as_os_str().as_encoded_bytes());
        }
        push_field(options.legacy_encoding.as_deref().unwrap_or("").as_bytes());
        if let Some(scenes) = scenes {
            push_field(&(scenes.current as u64).to_le_bytes());
            for (name, first) in &scenes.scenes {
                push_field(name.as_bytes());
                push_field(&first.to_le_bytes());
            }
        }
        push_field(b"|");
        push_field(text.as_bytes());
        sha256(&data)
    }

    /// The cached result for `key`, if its dependencies are unchanged.
    pub(crate) fn get(&self, key: &[u8; 32]) -> Option<Compiled> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.get(key)?;
        if entry.dependencies.iter().all(|d| d.is_current()) {
            Some(entry.clone())
        } else {
            entries.remove(key);
            None
        }
    }

    /// Stores a result.
    pub(crate) fn insert(&self, key: [u8; 32], compiled: Compiled) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(key, compiled);
    }

    /// Number of entries.
    pub(crate) fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Removes every entry.
    pub(crate) fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}
