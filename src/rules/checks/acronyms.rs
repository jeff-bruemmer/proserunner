//! `acronyms = { except = [...], match = '…', definitions = ['…'] }`: an
//! acronym used before it's defined, as in “Portable Document Format
//! (PDF)”. Only its first use is reported, and a use in a dotted name,
//! such as `README.md`, isn't one.

use std::collections::{HashMap, HashSet};
use std::iter;

use serde::Deserialize;

use super::{Candidate, Check, Description, Doc, Found, FoundIn, Spec, Want};
use crate::prose;
use crate::settings;

pub(super) const SPEC: Spec = Spec {
    key: "acronyms",
    vars: &["acronym"],
    message: "Define “{acronym}” where it's first used, as in “Full Name ({acronym})”.",
    suggest: &[],
};

/// Three capital letters or more, with a plural `s`: `PDF`, `APIs`.
const ACRONYM: &str = r"\b([A-Z]{3,})s?\b";

/// A word in parentheses after a word: “Portable Document Format (PDF)”.
const DEFINITION: &str = r"\w \(([^()\s]+?)s?\)";

/// Acronyms that must be defined before they're used.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AcronymsTable {
    /// Acronyms that need no definition, such as "HTML" or "NASA".
    #[serde(default)]
    pub except: Vec<String>,
    /// A regex for an acronym where it's used, matched with case. Its
    /// first group, if it has one, is the acronym, so `APIs` can be a use
    /// of `API`. By default, three capital letters or more, with a plural
    /// `s`.
    #[serde(rename = "match")]
    pub pattern: Option<String>,
    /// Regexes for where an acronym is defined, matched with case. The
    /// first group of each is the acronym it defines. By default, a word
    /// in parentheses after a word: "Portable Document Format (PDF)".
    pub definitions: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Acronyms {
    except: Vec<String>,
    pattern: String,
    definitions: Vec<String>,
}

pub(super) fn compile(table: Option<&AcronymsTable>) -> Option<Check> {
    let t = table?;
    Some(Check::Acronyms(Acronyms {
        except: t.except.clone(),
        pattern: t.pattern.clone().unwrap_or_else(|| ACRONYM.into()),
        definitions: t
            .definitions
            .clone()
            .unwrap_or_else(|| vec![DEFINITION.into()]),
    }))
}

/// The acronym a match stands for: its first group, or all of it.
fn acronym<'a>(c: &'a Candidate<'_>) -> &'a str {
    c.args
        .first()
        .filter(|g| !g.is_empty())
        .map_or(c.text, String::as_str)
}

/// Whether a match is joined to a word by a period, as in `README.md`.
fn in_dotted_name(doc: &Doc<'_>, c: &Candidate<'_>) -> bool {
    let Ok(i) = doc.blocks.binary_search_by_key(&c.block, |b| b.index) else {
        return false;
    };
    let text = &doc.blocks[i].block.text;
    dot_then_word(text[c.range.end..].chars()) || dot_then_word(text[..c.range.start].chars().rev())
}

/// Whether `chars` start with a period and then a word character.
fn dot_then_word(mut chars: impl Iterator<Item = char>) -> bool {
    chars.next() == Some('.') && chars.next().is_some_and(prose::is_word_char)
}

impl Acronyms {
    /// The use pattern is tag 0; definitions follow.
    pub(super) fn wants(&self) -> Vec<Want> {
        iter::once(&self.pattern)
            .chain(&self.definitions)
            .map(|p| Want::Pattern(p.clone()))
            .collect()
    }

    /// Each acronym's first use, unless a definition starts at or before
    /// it. A use inside its own definition counts as defined.
    pub(super) fn document(&self, doc: &Doc<'_>, out: &mut Vec<FoundIn>) {
        let mut defined: HashMap<&str, usize> = HashMap::new();
        let definitions = doc.candidates.iter().filter(|c| c.tag > 0);
        for c in definitions {
            defined.entry(acronym(c)).or_insert(c.at);
        }
        let mut seen = HashSet::new();
        let uses = doc
            .candidates
            .iter()
            .filter(|c| c.tag == 0 && !in_dotted_name(doc, c));
        for c in uses {
            let name = acronym(c);
            if self.except.iter().any(|e| e == name) || !seen.insert(name) {
                continue;
            }
            if defined.get(name).is_none_or(|&at| at > c.at) {
                out.push(FoundIn {
                    block: c.block,
                    found: Found {
                        range: c.range.clone(),
                        args: vec![name.to_owned()],
                    },
                });
            }
        }
    }

    pub(super) fn describe(&self) -> Description {
        let mut lines = vec![format!("acronym: regex {}", settings::quote(&self.pattern))];
        for d in &self.definitions {
            lines.push(format!("defined by: regex {}", settings::quote(d)));
        }
        if !self.except.is_empty() {
            let except: Vec<String> = self.except.iter().map(|e| settings::quote(e)).collect();
            lines.push(format!("except {}", except.join(", ")));
        }
        lines.push("an acronym's first use, unless it's defined by then".into());
        Description {
            written: "acronyms".into(),
            finds: lines.join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lint;
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{Format, Options};

    fn found(rule: &str, source: &str) -> Vec<(String, String)> {
        let set = compile_one(rule).unwrap();
        lint::document(&set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (i.matched, i.args[0].clone()))
            .collect()
    }

    const RULE: &str = "schema = 1\nacronyms = { except = [\"HTML\"] }";

    #[test]
    fn acronyms_need_a_definition_first() {
        assert_eq!(
            found(
                RULE,
                "The APIs and HTML.\n\nAn application programming interface (API) is an API. Use a PDF.\n\nThen PDF (Portable Document Format) and the URL, a URL."
            ),
            [
                ("APIs".into(), "API".into()),
                ("PDF".into(), "PDF".into()),
                ("URL".into(), "URL".into()),
            ]
        );
        assert!(
            found(
                RULE,
                "Portable Document Formats (PDFs) and a PDF. A Uniform Resource Locator (URL)."
            )
            .is_empty()
        );
        assert!(found(RULE, "Not an acronym: Api, A.P.I. or AB.").is_empty());
        assert!(found(RULE, "See README.md, CHANGELOG.md and file.TXT.").is_empty());
        assert_eq!(found(RULE, "See the README. Then CSV.").len(), 2);
    }

    #[test]
    fn patterns_and_scope_are_the_rules() {
        let rule = r#"schema = 1
            scope = ["paragraph"]
            acronyms = { match = '\b[A-Z]{2,}\b', definitions = ['\(([A-Z]{2,})\): '] }"#;
        assert_eq!(
            found(
                rule,
                "# The UI\n\nThe (UI): user interface. The UI, and an OS (operating system)."
            ),
            [("OS".into(), "OS".into())]
        );
    }

    #[test]
    fn the_default_message() {
        let set = compile_one(RULE).unwrap();
        let w = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        assert_eq!(
            w.render("APIs", "R", &["API".into()]),
            "Define “API” where it's first used, as in “Full Name (API)”."
        );
    }

    #[test]
    fn invalid_patterns_name_the_check() {
        let e = compile_one("schema = 1\nacronyms = { match = '(' }").unwrap_err();
        assert!(e.contains("`acronyms`: invalid pattern `(`"), "{e}");
    }
}
