//! `metric = { formula = "flesch-kincaid", max = 10, per = "document" }`:
//! text that reads above a school grade, by a readability formula.

use std::fmt::{self, Display, Formatter};

use serde::Deserialize;

use super::{Check, Covered, Description, Doc, Found, FoundIn, Spec};
use crate::prose::{self, Counts};
use crate::scan::Block;

pub(super) const SPEC: Spec = Spec {
    key: "metric",
    vars: &["score", "limit", "formula"],
    message: "{formula} grade level {score}; aim for {limit} or lower.",
    suggest: &[],
};

/// The formulas need a sample to mean anything.
const MIN_WORDS: u32 = 100;

/// A readability score to keep under a limit.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct MetricTable {
    /// The formula, each of which gives a U.S. school grade: by default,
    /// "flesch-kincaid".
    #[serde(default)]
    pub formula: Formula,
    /// The highest grade allowed.
    pub max: f64,
    /// What to score: by default, the whole "document".
    #[serde(default)]
    pub per: MetricPer,
    /// Text with fewer words than this isn't scored: 100 by default.
    #[serde(default = "min_words")]
    pub min_words: u32,
}

fn min_words() -> u32 {
    MIN_WORDS
}

/// A readability formula, which scores text as a school grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Formula {
    /// Words per sentence and syllables per word.
    #[default]
    FleschKincaid,
    /// Words per sentence, and the share of words with three syllables or
    /// more.
    GunningFog,
    /// Letters and sentences per 100 words.
    ColemanLiau,
    /// Characters per word and words per sentence.
    AutomatedReadability,
}

impl Formula {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Formula::FleschKincaid => "flesch-kincaid",
            Formula::GunningFog => "gunning-fog",
            Formula::ColemanLiau => "coleman-liau",
            Formula::AutomatedReadability => "automated-readability",
        }
    }

    /// What messages call it.
    fn title(self) -> &'static str {
        match self {
            Formula::FleschKincaid => "Flesch-Kincaid",
            Formula::GunningFog => "Gunning fog",
            Formula::ColemanLiau => "Coleman-Liau",
            Formula::AutomatedReadability => "Automated readability",
        }
    }

    /// The grade for `c`, which has words.
    fn score(self, c: &Counts) -> f64 {
        let words = c.words as f64;
        let sentences = c.sentences.max(1) as f64;
        let per_sentence = words / sentences;
        match self {
            Formula::FleschKincaid => {
                0.39 * per_sentence + 11.8 * (c.syllables as f64 / words) - 15.59
            }
            Formula::GunningFog => 0.4 * (per_sentence + 100.0 * c.polysyllables as f64 / words),
            Formula::ColemanLiau => {
                0.0588 * (100.0 * c.characters as f64 / words)
                    - 0.296 * (100.0 * sentences / words)
                    - 15.8
            }
            Formula::AutomatedReadability => {
                4.71 * (c.characters as f64 / words) + 0.5 * per_sentence - 21.43
            }
        }
    }
}

impl Display for Formula {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// What a grade is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MetricPer {
    /// The whole document: every block the rule applies to. Its issue is
    /// at the first block's first sentence.
    #[default]
    Document,
    /// Each block.
    Block,
}

impl MetricPer {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricPer::Document => "document",
            MetricPer::Block => "block",
        }
    }
}

impl Display for MetricPer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Metric {
    formula: Formula,
    max: f64,
    per: MetricPer,
    min_words: u32,
}

pub(super) fn compile(table: Option<&MetricTable>) -> Result<Option<Check>, String> {
    let Some(t) = table else {
        return Ok(None);
    };
    if !t.max.is_finite() {
        return Err("`max` must be a number".into());
    }
    Ok(Some(Check::Metric(Metric {
        formula: t.formula,
        max: t.max,
        per: t.per,
        min_words: t.min_words,
    })))
}

impl Metric {
    pub(super) fn is_document(&self) -> bool {
        self.per == MetricPer::Document
    }

    pub(super) fn block(&self, block: &Block, out: &mut Vec<Found>) {
        if self.per != MetricPer::Block {
            return;
        }
        let text = &block.text;
        if let Some(args) = self.over(&Counts::of(text)) {
            out.push(Found {
                range: prose::trimmed(text, 0..text.len()),
                args,
            });
        }
    }

    pub(super) fn document(&self, doc: &Doc<'_>, out: &mut Vec<FoundIn>) {
        let mut counts = Counts::default();
        for covered in doc.blocks {
            counts += Counts::of(&covered.block.text);
        }
        let Some(args) = self.over(&counts) else {
            return;
        };
        // The first sentence there is: a block can be all hidden text,
        // such as a quote.
        let first = doc.blocks.iter().find_map(|&Covered { index, block }| {
            prose::sentences(&block.text)
                .next()
                .map(|range| (index, range))
        });
        if let Some((block, range)) = first {
            out.push(FoundIn {
                block,
                found: Found { range, args },
            });
        }
    }

    /// The issue's values when `counts` are scored and over the limit. The
    /// score is compared as shown, to one decimal place.
    fn over(&self, counts: &Counts) -> Option<Vec<String>> {
        if counts.words == 0 || counts.words < self.min_words as usize {
            return None;
        }
        let score = (self.formula.score(counts) * 10.0).round() / 10.0;
        (score > self.max).then(|| {
            vec![
                format!("{score:.1}"),
                self.max.to_string(),
                self.formula.title().into(),
            ]
        })
    }

    pub(super) fn describe(&self) -> Description {
        let per = self.per;
        Description {
            written: format!(
                "metric = {{ formula = \"{}\", max = {}, per = \"{per}\", min-words = {} }}",
                self.formula, self.max, self.min_words
            ),
            finds: format!(
                "a {per} of {} words or more above grade {} by {}",
                self.min_words,
                self.max,
                self.formula.title()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint;
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{Format, Options};

    fn found(metric: &str, source: &str) -> Vec<(String, Vec<String>)> {
        let set = compile_one(&format!("schema = 1\nmetric = {metric}")).unwrap();
        lint::document(&set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (i.matched, i.args))
            .collect()
    }

    const EASY: &str = "The cat sat on the mat. It was a good day.";
    const HARD: &str = "Organizational interdependencies necessitate comprehensive \
        reconsideration of institutional responsibilities. Administrative \
        complexity notwithstanding, implementation remains problematic.";

    #[test]
    fn formulas_score_grades() {
        let easy = Counts::of(EASY);
        let hard = Counts::of(HARD);
        for formula in [
            Formula::FleschKincaid,
            Formula::GunningFog,
            Formula::ColemanLiau,
            Formula::AutomatedReadability,
        ] {
            let (e, h) = (formula.score(&easy), formula.score(&hard));
            assert!(e < 5.0 && h > 15.0, "{formula:?}: {e} {h}");
        }
        // 0.39 × 11/2 + 11.8 × 11/11 − 15.59
        assert!((Formula::FleschKincaid.score(&easy) - (-1.645)).abs() < 1e-9);
    }

    #[test]
    fn blocks_and_documents_over_the_limit() {
        let source = format!("# Title\n\n{EASY}\n\n{HARD}\n");
        assert_eq!(
            found("{ max = 10, per = \"block\", min-words = 10 }", &source),
            [(
                HARD.split_whitespace().collect::<Vec<_>>().join(" "),
                vec!["39.4".into(), "10".into(), "Flesch-Kincaid".into()]
            )]
        );
        assert_eq!(
            found("{ max = 8.5, min-words = 20 }", &source),
            [(
                "Title".into(),
                vec!["20.5".into(), "8.5".into(), "Flesch-Kincaid".into()]
            )]
        );
        assert!(
            found("{ max = 8.5 }", &source).is_empty(),
            "under 100 words"
        );
        assert!(found("{ max = 40, per = \"block\", min-words = 1 }", &source).is_empty());
    }

    #[test]
    fn a_document_issue_skips_blocks_with_no_sentence() {
        // Quotes are hidden, so the first block has no sentence to show.
        let source = format!("\"Absolutely.\"\n\n{HARD}\n");
        let issues = found("{ max = 10, min-words = 5 }", &source);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].0.starts_with("Organizational"), "{issues:?}");
    }

    #[test]
    fn the_default_message() {
        let set = compile_one("schema = 1\nmetric = { max = 10 }").unwrap();
        let w = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        let args = ["12.3".into(), "10".into(), "Flesch-Kincaid".into()];
        assert_eq!(
            w.render("…", "R", &args),
            "Flesch-Kincaid grade level 12.3; aim for 10 or lower."
        );
    }

    #[test]
    fn metrics_must_make_sense() {
        let e = |metric: &str| compile_one(&format!("schema = 1\nmetric = {metric}")).unwrap_err();
        assert!(e("{ max = nan }").contains("`metric`: `max` must be a number"));
        assert!(e("{ max = 1, formula = \"smog\" }").contains("unknown variant `smog`"));
        assert!(e("{ formula = \"gunning-fog\" }").contains("missing field `max`"));
    }
}
