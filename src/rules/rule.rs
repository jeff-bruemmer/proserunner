//! A compiled rule: what each of its entries and checks matches, and what
//! its issues say.

use std::borrow::Cow;

use regex::Regex;

use super::checks::Check;
use super::rule_file::{Case, Examples, Level};
use super::rule_set::{self, PieceMatcher, RuleIndex};
use super::template::Template;
use crate::scan::{Block, Scope};

/// Which list an entry came from, which decides how it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryKind {
    /// A `words` entry: matched literally.
    Word,
    /// A `patterns` entry: a regex.
    Pattern,
    /// A `terms` entry: reported unless written exactly as the term.
    Term,
}

/// What in a rule found an issue. Entries sort before checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[expect(clippy::exhaustive_enums, reason = "closed: entries or checks")]
pub enum EntryRef {
    /// A `words` or `patterns` entry: an index into [`Rule::entries`].
    Entry(u32),
    /// A check, such as `repeat`: an index into the rule's checks.
    Check(u32),
}

/// What an issue says: its message, and what to write instead.
#[derive(Debug)]
#[non_exhaustive]
pub struct Wording {
    /// The message for each issue.
    pub message: Template,
    /// Best first.
    pub suggest: Vec<Template>,
}

impl Wording {
    /// The message for `matched`, with a check's values or a pattern's
    /// groups in `args`.
    pub fn render(&self, matched: &str, rule: &str, args: &[String]) -> Cow<'_, str> {
        let suggestion = if self.message.uses_suggestion() {
            self.suggestions(matched, rule, args).join(", ")
        } else {
            String::new()
        };
        self.message
            .render(&values(matched, &suggestion, rule, args))
    }

    /// What to write instead of `matched`, best first.
    pub fn suggestions(&self, matched: &str, rule: &str, args: &[String]) -> Vec<String> {
        if self.suggest.is_empty() {
            return Vec::new();
        }
        let values = values(matched, "", rule, args);
        self.suggest
            .iter()
            .map(|s| s.render(&values).into_owned())
            .collect()
    }
}

/// A template's values in slot order: the common ones, then `args`.
fn values<'a>(
    matched: &'a str,
    suggestion: &'a str,
    rule: &'a str,
    args: &'a [String],
) -> Vec<&'a str> {
    let mut values = vec![matched, suggestion, rule];
    values.extend(args.iter().map(String::as_str));
    values
}

/// A `words`, `patterns` or `terms` entry, with the rule's settings filled
/// in where it has none of its own.
#[derive(Debug)]
#[non_exhaustive]
pub struct CompiledEntry {
    /// The entry as written, for `explain`: a word, a pattern, or a term.
    pub source: String,
    /// A term's other spellings, as written.
    pub variants: Vec<String>,
    /// Which list it's from.
    pub kind: EntryKind,
    /// The entry's own case, or the rule's. A term matches any case.
    pub case: Case,
    /// The entry's own level, or the rule's.
    pub level: Level,
    /// What its issues say.
    pub wording: Wording,
}

/// One `except` item, ready to compare with a match.
#[derive(Debug)]
#[non_exhaustive]
pub enum Exception {
    /// Normalized like matches, and lowercased when the rule ignores case.
    Text(String),
    /// Must match the whole match, with the rule's case.
    Pattern {
        /// As written, for `explain`.
        source: String,
        /// Anchored at both ends.
        regex: Regex,
    },
}

#[derive(Debug)]
pub(crate) struct CompiledCheck {
    pub check: Check,
    pub wording: Wording,
}

/// How one piece of an entry matches, as `explain` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Form {
    /// What finds its matches.
    pub engine: Engine,
    /// The literal searched for, or the regex as compiled.
    pub text: String,
    /// Whether a match must start at a word boundary.
    pub left: bool,
    /// Whether a match must end at a word boundary.
    pub right: bool,
}

/// What finds a piece's matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Engine {
    /// The shared Aho-Corasick automaton.
    Literal,
    /// A linear-time regex, run where the shared `RegexSet` says it matches.
    Regex,
    /// A backtracking regex (lookaround or backreferences), with a budget.
    Backtracking,
}

/// One rule file, compiled: its settings, its entries and checks, and what
/// their issues say. The [`RuleSet`](super::RuleSet) that holds it does
/// the matching.
#[derive(Debug)]
pub struct Rule {
    /// Its place in the set that compiled it.
    pub index: RuleIndex,
    /// `<source>/<file stem>`, such as `default/hedging`.
    pub id: String,
    /// The name people see.
    pub name: String,
    /// Where it came from, for error messages: a path, or the id for a
    /// built-in.
    pub origin: String,
    /// How serious its issues are, unless an entry or config says
    /// otherwise.
    pub level: Level,
    /// Why it exists.
    pub explain: Option<String>,
    /// Whether matching ignores case, unless an entry says otherwise.
    pub case: Case,
    /// The blocks it applies to; empty for all prose.
    pub scope: Vec<Scope>,
    /// The heading levels it applies to; empty for all of them.
    pub heading_levels: Vec<u8>,
    /// Matches never to report.
    pub except: Vec<Exception>,
    /// Its `words`, `patterns` and `terms`, in that order.
    pub entries: Vec<CompiledEntry>,
    pub(crate) checks: Vec<CompiledCheck>,
    /// Text that `proserunner rules test` checks it against.
    pub examples: Examples,
}

impl Rule {
    /// Whether the rule checks `block`: its scope covers the block, and a
    /// heading is at one of its heading levels.
    pub fn applies_to(&self, block: &Block) -> bool {
        let scoped = self.scope.is_empty() || self.scope.contains(&block.kind);
        let leveled = block.kind != Scope::Heading
            || self.heading_levels.is_empty()
            || block
                .level
                .is_some_and(|l| self.heading_levels.contains(&l));
        scoped && leveled
    }

    /// True when `matched` (normalized, as found) is one of the rule's
    /// exceptions.
    pub fn is_excepted(&self, matched: &str) -> bool {
        if self.except.is_empty() {
            return false;
        }
        let folded = (self.case == Case::Insensitive).then(|| matched.to_lowercase());
        let text = folded.as_deref().unwrap_or(matched);
        self.except.iter().any(|e| match e {
            Exception::Text(t) => t == text,
            Exception::Pattern { regex, .. } => regex.is_match(matched),
        })
    }

    /// True when entry `at` is a term and `matched` is written exactly as
    /// the term is.
    pub fn is_term(&self, at: EntryRef, matched: &str) -> bool {
        self.entry(at)
            .is_some_and(|e| e.kind == EntryKind::Term && e.source == matched)
    }

    /// The entry `at` refers to, unless it's a check.
    pub fn entry(&self, at: EntryRef) -> Option<&CompiledEntry> {
        match at {
            EntryRef::Entry(i) => self.entries.get(i as usize),
            EntryRef::Check(_) => None,
        }
    }

    /// The level of issues found by `at`: an entry's own, or the rule's.
    pub fn level_of(&self, at: EntryRef) -> Level {
        self.entry(at).map_or(self.level, |e| e.level)
    }

    /// The pattern or word behind a match, as written: entry `at`'s, or
    /// want `tag` of check `at`.
    pub fn pattern(&self, at: EntryRef, tag: u32) -> Option<Cow<'_, str>> {
        match at {
            EntryRef::Entry(i) => self
                .entries
                .get(i as usize)
                .map(|e| Cow::Borrowed(e.source.as_str())),
            EntryRef::Check(i) => {
                let wants = self.checks.get(i as usize)?.check.wants();
                let want = wants.into_iter().nth(tag as usize)?;
                Some(Cow::Owned(want.into_source()))
            }
        }
    }

    /// What issues found by `at` say, or `None` when `at` isn't one of
    /// this rule's.
    pub fn wording(&self, at: EntryRef) -> Option<&Wording> {
        match at {
            EntryRef::Entry(i) => self.entries.get(i as usize).map(|e| &e.wording),
            EntryRef::Check(i) => self.checks.get(i as usize).map(|c| &c.wording),
        }
    }

    /// How entry `entry` matches: one form per brace expansion of a word
    /// or a term's spellings, or the pattern's regex.
    pub fn forms(&self, entry: usize) -> Result<Vec<Form>, String> {
        let Some(e) = self.entries.get(entry) else {
            return Ok(Vec::new());
        };
        let pieces = rule_set::pieces(e.kind, &e.source, &e.variants, e.case)?;
        Ok(pieces
            .into_iter()
            .map(|p| {
                let (engine, text) = match p.matcher {
                    PieceMatcher::Literal(word) => (Engine::Literal, word),
                    PieceMatcher::Regex(r) => (Engine::Regex, r.as_str().to_owned()),
                    PieceMatcher::Fancy(r) => (Engine::Backtracking, r.as_str().to_owned()),
                };
                Form {
                    engine,
                    text,
                    left: p.left,
                    right: p.right,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording_joins_suggestions_and_appends_args() {
        let wording = Wording {
            message: Template::parse("{rule}: {suggestion} for {match}, {n}", &["n"]).unwrap(),
            suggest: vec![Template::literal("because"), Template::literal("since")],
        };
        assert_eq!(
            wording.render("as", "Wordiness", &["2".into()]),
            "Wordiness: because, since for as, 2"
        );
    }

    #[test]
    fn suggestions_fill_in_groups() {
        let wording = Wording {
            message: Template::parse("Prefer “{suggestion}”.", &["1"]).unwrap(),
            suggest: vec![Template::suggestion("{1} percent", &["1"]).unwrap()],
        };
        let args = ["50".to_owned()];
        assert_eq!(wording.suggestions("50%", "R", &args), ["50 percent"]);
        assert_eq!(wording.render("50%", "R", &args), "Prefer “50 percent”.");
    }

    #[test]
    fn entries_sort_before_checks() {
        assert!(EntryRef::Entry(u32::MAX) < EntryRef::Check(0));
    }
}
