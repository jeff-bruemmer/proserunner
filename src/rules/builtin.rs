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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::rules::{Engine, RuleSet};

    /// A word in two built-in rules is one phrase reported twice, so each
    /// belongs to one rule.
    #[test]
    fn no_two_builtin_rules_share_a_word() {
        let compiled = RuleSet::compile(builtin());
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        let mut owners: HashMap<String, &str> = HashMap::new();
        let mut shared = Vec::new();
        for rule in compiled.rules.rules() {
            for entry in 0..rule.entries.len() {
                let forms = rule.forms(entry).unwrap();
                let words = forms.iter().filter(|f| f.engine == Engine::Literal);
                for word in words {
                    let key = word.text.to_lowercase();
                    match owners.insert(key.clone(), &rule.id) {
                        Some(other) if other != rule.id => {
                            shared.push(format!("{key:?}: {other} and {}", rule.id));
                        }
                        _ => {}
                    }
                }
            }
        }
        assert!(shared.is_empty(), "{shared:#?}");
    }
}
