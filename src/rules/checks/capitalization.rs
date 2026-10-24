//! `capitalization = { style = "sentence", except = [...] }`: words in a
//! block, usually a heading, that the style writes in another case. Each
//! word is an issue of its own, so `accept` and ignores work on words.

use std::fmt::{self, Display, Formatter};
use std::ops::Range;

use serde::Deserialize;

use super::{Check, Description, Found, Spec};
use crate::prose;
use crate::scan::{Block, PLACEHOLDER};
use crate::settings;

pub(super) const SPEC: Spec = Spec {
    key: "capitalization",
    vars: &["style", "form"],
    message: "Use {style} case: “{suggestion}”.",
    suggest: &["{form}"],
};

/// Articles, coordinating conjunctions, and prepositions of four letters
/// or fewer: lowercase in title case, unless first or last.
const MINOR: &[&str] = &[
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "off",
    "on", "onto", "or", "out", "over", "per", "so", "the", "to", "up", "upon", "via", "vs", "with",
    "yet",
];

/// How a block's words are capitalized.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CapitalizationTable {
    /// The case to use: by default, "sentence".
    #[serde(default)]
    pub style: Style,
    /// Words and phrases to leave as written, such as names:
    /// "Proserunner", "Visual Studio Code".
    #[serde(default)]
    pub except: Vec<String>,
}

/// A capitalization style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Style {
    /// The first word capitalized, and the rest lowercase.
    #[default]
    Sentence,
    /// Every word capitalized but articles, conjunctions and short
    /// prepositions in the middle.
    Title,
}

impl Style {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Style::Sentence => "sentence",
            Style::Title => "title",
        }
    }
}

impl Display for Style {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Capitalization {
    style: Style,
    /// Normalized.
    except: Vec<String>,
}

pub(super) fn compile(table: Option<&CapitalizationTable>) -> Result<Option<Check>, String> {
    let Some(t) = table else {
        return Ok(None);
    };
    let except: Vec<String> = t
        .except
        .iter()
        .map(|e| {
            let e = prose::normalize(e);
            if e.is_empty() {
                return Err("`except` can't hold empty text".to_owned());
            }
            Ok(e)
        })
        .collect::<Result<_, _>>()?;
    Ok(Some(Check::Capitalization(Capitalization {
        style: t.style,
        except,
    })))
}

impl Capitalization {
    /// Each word the style writes in another case. A word with a capital
    /// after its first letter (`GitHub`, `API`, `iOS`) is a name and left
    /// alone, as are `I` and words that don't start with a letter. A word
    /// that starts a sentence is capitalized; one after a colon may be
    /// either in sentence case, and is capitalized in title case, as is
    /// the last word.
    pub(super) fn block(&self, block: &Block, out: &mut Vec<Found>) {
        let text = &block.text;
        let kept = self.kept(text);
        let words: Vec<Range<usize>> = prose::words(text).collect();
        let mut sentences = prose::sentences(text).peekable();
        let mut sentence_end = 0;
        let mut prev_end = 0;
        for (i, w) in words.iter().enumerate() {
            // The first word of a sentence, unless something the rules
            // can't see comes before it, which may be the first.
            let (mut opens, mut after_hidden) = (false, false);
            while let Some(s) = sentences.next_if(|s| s.start <= w.start) {
                after_hidden = hidden(&text[sentence_end..w.start]);
                opens = !after_hidden;
                sentence_end = s.end;
            }
            let after_colon = text[prev_end..w.start].contains(':');
            prev_end = w.end;
            if kept.iter().any(|k| k.start <= w.start && w.end <= k.end) {
                continue;
            }
            let at = Place {
                opens,
                after_colon,
                after_hidden,
                last: i + 1 == words.len(),
            };
            if let Some(form) = self.form(&text[w.clone()], at) {
                out.push(Found {
                    range: w.clone(),
                    args: vec![self.style.as_str().into(), form],
                });
            }
        }
    }

    /// Where `except` phrases are in `text`, as whole words.
    fn kept(&self, text: &str) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        for phrase in &self.except {
            for (i, _) in text.match_indices(phrase.as_str()) {
                let end = i + phrase.len();
                let before = text[..i].chars().next_back();
                let after = text[end..].chars().next();
                if !before.is_some_and(prose::is_word_char)
                    && !after.is_some_and(prose::is_word_char)
                {
                    out.push(i..end);
                }
            }
        }
        out
    }

    /// `word` as the style writes it at `at`, unless it's written so.
    fn form(&self, word: &str, at: Place) -> Option<String> {
        let mut chars = word.chars();
        let first = chars.next()?;
        let cased = first.is_uppercase() || first.is_lowercase();
        if !cased || chars.any(char::is_uppercase) || is_pronoun_i(word) {
            return None;
        }
        let minor = || MINOR.iter().any(|m| word.eq_ignore_ascii_case(m));
        let upper = match self.style {
            Style::Sentence if at.opens => true,
            Style::Sentence if at.after_colon || at.after_hidden => return None,
            Style::Sentence => false,
            Style::Title if at.opens || at.after_colon || at.last || !minor() => true,
            Style::Title if at.after_hidden => return None,
            Style::Title => false,
        };
        (first.is_uppercase() != upper).then(|| recase(word, upper))
    }

    pub(super) fn describe(&self) -> Description {
        use std::fmt::Write as _;

        let style = self.style;
        let mut finds = format!("words not in {style} case");
        if !self.except.is_empty() {
            let kept: Vec<String> = self.except.iter().map(|e| settings::quote(e)).collect();
            let _ = write!(finds, ", leaving {} as written", kept.join(", "));
        }
        Description {
            written: format!("capitalization = {{ style = \"{style}\" }}"),
            finds,
        }
    }
}

/// Where a word is in its block.
#[derive(Clone, Copy)]
struct Place {
    /// It's the first word of a sentence.
    opens: bool,
    after_colon: bool,
    /// It starts a sentence after text the rules can't see, so it may
    /// not be the first word.
    after_hidden: bool,
    last: bool,
}

/// Whether `gap`, from a sentence's end or the block's start, holds text
/// the rules can't see: a placeholder for inline code, quoted text or the
/// like.
fn hidden(gap: &str) -> bool {
    gap.contains(PLACEHOLDER)
}

fn is_pronoun_i(word: &str) -> bool {
    word == "I" || word.starts_with("I'") || word.starts_with("I’")
}

/// `word` with its first letter in upper or lower case.
fn recase(word: &str, upper: bool) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if upper {
        first.to_uppercase().chain(chars).collect()
    } else {
        first.to_lowercase().chain(chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::lint;
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{Format, Options};

    fn found(table: &str, source: &str) -> Vec<(String, String)> {
        let set = compile_one(&format!(
            "schema = 1\nscope = [\"heading\"]\ncapitalization = {table}"
        ))
        .unwrap();
        lint::document(&set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (i.matched, i.args[1].clone()))
            .collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(a, b)| ((*a).into(), (*b).into()))
            .collect()
    }

    #[test]
    fn sentence_case() {
        let style = "{ style = \"sentence\", except = [\"Visual Studio Code\", \"npm\"] }";
        assert_eq!(
            found(
                style,
                "# Getting Started With GitHub and the API\n\n## install it. Then Run It\n\nNot A Heading.\n\n## npm in Visual Studio Code: Notes and I"
            ),
            pairs(&[
                ("Started", "started"),
                ("With", "with"),
                ("install", "Install"),
                ("Run", "run"),
                ("It", "it"),
            ])
        );
        assert!(found(style, "# Built-In tools, well-Known words, 3D and `Code`").is_empty());
        assert!(found(style, "# `proserunner` is fast\n\n## “Quoted” start").is_empty());
        assert_eq!(
            found(style, "# Done. “Quoted” Start Here\n\n## “The” Start"),
            pairs(&[("Here", "here")])
        );
    }

    #[test]
    fn title_case() {
        assert_eq!(
            found(
                "{ style = \"title\" }",
                "# a Guide To the Tools of the trade\n\n## Ending With: an Example to Look at\n\n## “Quoted” the end"
            ),
            pairs(&[
                ("a", "A"),
                ("To", "to"),
                ("trade", "Trade"),
                ("With", "with"),
                ("an", "An"),
                ("at", "At"),
                ("end", "End"),
            ])
        );
    }

    #[test]
    fn the_default_message_suggests_the_form() {
        let set = compile_one("schema = 1\ncapitalization = {}").unwrap();
        let w = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        let args = ["sentence".into(), "started".into()];
        assert_eq!(
            w.render("Started", "R", &args),
            "Use sentence case: “started”."
        );
        assert_eq!(w.suggestions("Started", "R", &args), ["started"]);
    }

    #[test]
    fn empty_exceptions_are_errors() {
        let e = compile_one("schema = 1\ncapitalization = { except = [\" \"] }").unwrap_err();
        assert!(
            e.contains("`capitalization`: `except` can't hold empty text"),
            "{e}"
        );
    }
}
