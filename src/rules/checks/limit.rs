//! `limit = { max = 30, unit = "words", per = "sentence" }`: sentences or
//! blocks that are too long.

use std::fmt::{self, Display, Formatter};
use std::ops::Range;

use serde::Deserialize;

use super::{Check, Description, Found, Spec};
use crate::prose;
use crate::scan::{Block, PLACEHOLDER};

pub(super) const SPEC: Spec = Spec {
    key: "limit",
    vars: &["count", "limit", "unit"],
    message: "{count} {unit}; keep it to {limit} or fewer.",
    suggest: &[],
};

/// How long a sentence or block may be.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct LimitTable {
    /// The most allowed: 1 or more.
    pub max: u32,
    /// What to count: by default, "words".
    #[serde(default)]
    pub unit: Unit,
    /// What each count is of: by default, each "sentence".
    #[serde(default)]
    pub per: Per,
}

/// What a limit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Unit {
    /// Words, with `it's`, `well-known` and `3.5` one word each.
    #[default]
    Words,
    /// Sentences, which don't end at abbreviations such as `e.g.` and
    /// `Dr.`.
    Sentences,
    /// Characters as the rules see them, spaces included. Hidden text,
    /// such as a URL or a quotation, is one, as "…" shows it.
    Characters,
}

impl Unit {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Unit::Words => "words",
            Unit::Sentences => "sentences",
            Unit::Characters => "characters",
        }
    }

    fn count(self, text: &str) -> usize {
        match self {
            Unit::Words => prose::words(text).count(),
            Unit::Sentences => prose::sentences(text).count(),
            Unit::Characters => characters(text),
        }
    }
}

/// How many characters `text` shows: each run of placeholders, which is
/// one thing hidden, counts once.
fn characters(text: &str) -> usize {
    let mut previous = None;
    text.chars()
        .filter(|&c| {
            let repeat = c == PLACEHOLDER && previous == Some(PLACEHOLDER);
            previous = Some(c);
            !repeat
        })
        .count()
}

impl Display for Unit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// What a limit's count is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Per {
    /// Each sentence.
    #[default]
    Sentence,
    /// Each block: a paragraph, heading, list item or table cell.
    Block,
}

impl Per {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Per::Sentence => "sentence",
            Per::Block => "block",
        }
    }
}

impl Display for Per {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Limit {
    max: u32,
    unit: Unit,
    per: Per,
}

pub(super) fn compile(table: Option<&LimitTable>) -> Result<Option<Check>, String> {
    let Some(t) = table else {
        return Ok(None);
    };
    if t.max == 0 {
        return Err("`max` must be 1 or more".into());
    }
    if (t.unit, t.per) == (Unit::Sentences, Per::Sentence) {
        return Err("a sentence is one sentence: count sentences per block".into());
    }
    Ok(Some(Check::Limit(Limit {
        max: t.max,
        unit: t.unit,
        per: t.per,
    })))
}

impl Limit {
    /// Each sentence or block over the limit, all of it.
    pub(super) fn block(&self, block: &Block, out: &mut Vec<Found>) {
        let text = &block.text;
        let mut check = |range: Range<usize>| {
            let count = self.unit.count(&text[range.clone()]);
            if count > self.max as usize {
                out.push(Found {
                    range,
                    args: vec![
                        count.to_string(),
                        self.max.to_string(),
                        self.unit.as_str().into(),
                    ],
                });
            }
        };
        match self.per {
            Per::Sentence => prose::sentences(text).for_each(check),
            Per::Block => check(prose::trimmed(text, 0..text.len())),
        }
    }

    pub(super) fn describe(&self) -> Description {
        Description {
            written: format!(
                "limit = {{ max = {}, unit = \"{}\", per = \"{}\" }}",
                self.max, self.unit, self.per
            ),
            finds: format!("a {} of more than {} {}", self.per, self.max, self.unit),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lint;
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{Format, Options};

    fn found(limit: &str, source: &str) -> Vec<(String, Vec<String>)> {
        let set = compile_one(&format!("schema = 1\nlimit = {limit}")).unwrap();
        lint::document(&set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (i.matched, i.args))
            .collect()
    }

    #[test]
    fn sentences_over_the_limit() {
        assert_eq!(
            found(
                "{ max = 4 }",
                "One two three four. One two three four five, six.\n\n- A well-known, 3.5 point list item."
            ),
            [
                (
                    "One two three four five, six.".into(),
                    vec!["6".into(), "4".into(), "words".into()]
                ),
                (
                    "A well-known, 3.5 point list item.".into(),
                    vec!["6".into(), "4".into(), "words".into()]
                )
            ]
        );
    }

    #[test]
    fn blocks_over_the_limit() {
        let source = "# A heading of six words here\n\nOne. Two. Three.\n\nOne. Two.";
        let matched =
            |limit| -> Vec<String> { found(limit, source).into_iter().map(|(m, _)| m).collect() };
        assert_eq!(
            matched("{ max = 2, unit = \"sentences\", per = \"block\" }"),
            ["One. Two. Three."]
        );
        assert_eq!(
            matched("{ max = 25, unit = \"characters\", per = \"block\" }"),
            ["A heading of six words here"]
        );
        assert_eq!(
            matched("{ max = 5, per = \"block\" }"),
            ["A heading of six words here"]
        );
    }

    #[test]
    fn hidden_text_is_one_character() {
        let source = "See https://example.com/a/long/path for it.\n\n\
                      He said \"a long quoted passage\" and left.";
        let counts: Vec<String> = found("{ max = 5, unit = \"characters\" }", source)
            .into_iter()
            .map(|(_, args)| args[0].clone())
            .collect();
        // "See … for it." and "He said … and left.", as shown.
        assert_eq!(counts, ["13", "19"]);
    }

    #[test]
    fn the_default_message() {
        let set = compile_one("schema = 1\nlimit = { max = 30 }").unwrap();
        let w = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        let args = ["31".into(), "30".into(), "words".into()];
        assert_eq!(
            w.render("…", "R", &args),
            "31 words; keep it to 30 or fewer."
        );
    }

    #[test]
    fn limits_must_make_sense() {
        let e = |limit: &str| compile_one(&format!("schema = 1\nlimit = {limit}")).unwrap_err();
        assert!(e("{ max = 0 }").contains("`limit`: `max` must be 1 or more"));
        assert!(e("{ max = 2, unit = \"sentences\" }").contains("count sentences per block"));
        assert!(e("{ max = 2, per = \"page\" }").contains("unknown variant `page`"));
        assert!(e("{ max = 2, units = \"words\" }").contains("unknown field `units`"));
    }
}
