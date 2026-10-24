//! The built-in rules, embedded from `defaults/` by `build.rs`.

use std::borrow::Cow;

use super::RuleSource;

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

/// The id prefix of the built-in rules.
pub const DEFAULT_SOURCE: &str = "default";

/// A built-in rule file.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BuiltinFile {
    /// Its file name without `.toml`.
    pub stem: &'static str,
    /// Its TOML.
    pub text: &'static str,
}

/// The built-in rules, in name order.
pub fn builtin() -> impl Iterator<Item = RuleSource<'static>> {
    BUILTIN.iter().map(|file| {
        let id = format!("{DEFAULT_SOURCE}/{}", file.stem);
        RuleSource {
            origin: id.clone(),
            id,
            text: Cow::Borrowed(file.text),
        }
    })
}

/// The raw text of every built-in rule, for `rules export`.
pub fn builtin_files() -> &'static [BuiltinFile] {
    BUILTIN
}
