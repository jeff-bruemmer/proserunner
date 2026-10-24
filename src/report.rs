//! Orders and numbers issues once, for every output format. Pure.

use std::borrow::Cow;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::lint::Issue;
use crate::policy::{Suppressed, Suppression};
use crate::rules::{Level, Rule, RuleSet};
use crate::scan::{BOM, PLACEHOLDER};

/// A checked document and what was found in it.
#[derive(Debug)]
#[non_exhaustive]
pub struct Document {
    /// How the path is shown: relative to the working directory when it's
    /// under it, `~/…` under the home directory, otherwise as given.
    pub display: String,
    /// Absolute, or `None` for standard input.
    pub path: Option<PathBuf>,
    /// The whole text.
    pub source: String,
    /// What [`Policy::apply`](crate::policy::Policy::apply) kept, in source
    /// order.
    pub issues: Vec<Issue>,
    /// What it hid, with why, when it was asked to keep them (see
    /// [`Policy::set_keep_suppressed`](crate::policy::Policy::set_keep_suppressed)).
    /// Empty from [`Document::new`].
    pub suppressed: Vec<Suppressed>,
}

impl Document {
    /// A checked document; see the fields for each part.
    pub fn new(display: String, path: Option<PathBuf>, source: String, issues: Vec<Issue>) -> Self {
        Document {
            display,
            path,
            source,
            issues,
            suppressed: Vec::new(),
        }
    }
}

/// Line starts, for turning byte offsets into 1-based lines and columns.
#[derive(Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
}

/// A 1-based line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Position {
    /// Lines end at each `\n`.
    pub line: usize,
    /// In characters.
    pub column: usize,
}

impl LineIndex {
    /// Finds the lines of `source`.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { starts }
    }

    /// Where `offset` is. Columns count characters, not counting a byte
    /// order mark.
    pub fn position(&self, source: &str, offset: usize) -> Position {
        let line = self
            .starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        let start = line_start(source, self.starts[line]);
        let column = source.get(start..offset).map_or(0, |s| s.chars().count());
        Position {
            line: line + 1,
            column: column + 1,
        }
    }

    /// The bytes of 1-based lines `first` through `last`, without the
    /// final line ending.
    pub fn lines(&self, source: &str, first: usize, last: usize) -> Range<usize> {
        let start = line_start(
            source,
            self.starts
                .get(first.saturating_sub(1))
                .copied()
                .unwrap_or(0),
        );
        let end = self
            .starts
            .get(last)
            .map_or(source.len(), |&next| next.saturating_sub(1));
        let end = if source.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r') {
            end - 1
        } else {
            end
        };
        start..end.max(start)
    }
}

/// Where a line's text starts: after the byte order mark on the first line.
pub fn line_start(source: &str, start: usize) -> usize {
    if start == 0 && source.starts_with(BOM) {
        BOM.len()
    } else {
        start
    }
}

/// An issue as every output format prints it: numbered, placed and worded.
#[derive(Debug)]
#[non_exhaustive]
pub struct Reported<'report> {
    /// From 1, in report order; 0 for an issue in
    /// [`Report::suppressed`], which has none.
    pub number: usize,
    /// The document's path, as shown.
    pub file: &'report str,
    /// The document's absolute path; `None` for standard input.
    pub path: Option<&'report Path>,
    /// Where the match starts. Lines and columns count from 1.
    pub line: usize,
    /// In characters.
    pub column: usize,
    /// The position just past the match.
    pub end_line: usize,
    /// In characters.
    pub end_column: usize,
    /// The match's source bytes.
    pub span: Range<usize>,
    /// The rule that found it.
    pub rule: &'report Rule,
    /// The match as the rules saw it; see [`Reported::shown`].
    pub matched: &'report str,
    /// The rule's message, filled in.
    pub message: Cow<'report, str>,
    /// What to write instead, best first.
    pub suggestions: Vec<String>,
    /// The level that applies in this file.
    pub level: Level,
    /// The whole document.
    pub source: &'report str,
    /// The whole lines the match is on, as bytes of `source`.
    pub lines: Range<usize>,
    /// Why it wasn't reported, for an issue in [`Report::suppressed`];
    /// `None` for one that's reported.
    pub hidden: Option<Hidden<'report>>,
}

/// Why an issue in [`Report::suppressed`] wasn't reported.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Hidden<'report> {
    /// What hid it.
    pub why: &'report Suppression,
    /// What hid it, for people: "accepted in .proserunner/config.toml",
    /// or "off by the comment on line 4".
    pub reason: String,
}

impl<'report> Reported<'report> {
    /// The match as people read it, with "…" where the rules saw a
    /// placeholder for inline code, a URL or the like.
    pub fn shown(&self) -> Cow<'report, str> {
        shown(self.matched)
    }

    /// The whole lines the match is on.
    pub fn line_text(&self) -> &str {
        self.source.get(self.lines.clone()).unwrap_or("")
    }
}

/// `matched` as people read it, with "…" for each placeholder.
fn shown(matched: &str) -> Cow<'_, str> {
    if matched.contains(PLACEHOLDER) {
        Cow::Owned(matched.replace(PLACEHOLDER, "…"))
    } else {
        Cow::Borrowed(matched)
    }
}

/// Every issue of a run, numbered, for the output formats to print.
#[derive(Debug)]
#[non_exhaustive]
pub struct Report<'report> {
    /// Every document checked, with issues or without, in the order its
    /// issues are numbered.
    pub files: Vec<&'report Document>,
    /// In file, then source, order.
    pub issues: Vec<Reported<'report>>,
    /// The issues config, ignore lists and inline control hid, when the
    /// policy kept them, in file, then source, order. Each has a
    /// [`Reported::hidden`], and its `number` is 0, since it has none.
    /// The `group`, `plain` and `json` formats show them; the others are
    /// for tools that act on issues, and leave them out.
    pub suppressed: Vec<Reported<'report>>,
}

/// Numbers every issue in file, then source, order.
#[must_use]
pub fn build<'a>(rules: &'a RuleSet, documents: &'a [Document]) -> Report<'a> {
    let mut files: Vec<&Document> = documents.iter().collect();
    files.sort_by(|a, b| a.display.cmp(&b.display));

    let mut issues = Vec::new();
    let mut suppressed = Vec::new();
    for &doc in &files {
        let index = LineIndex::new(&doc.source);
        for issue in &doc.issues {
            if let Some(mut r) = reported(rules, doc, &index, issue) {
                r.number = issues.len() + 1;
                issues.push(r);
            }
        }
        for s in &doc.suppressed {
            if let Some(mut r) = reported(rules, doc, &index, &s.issue) {
                let reason = reason(&s.why, doc, &index);
                r.hidden = Some(Hidden {
                    why: &s.why,
                    reason,
                });
                suppressed.push(r);
            }
        }
    }
    Report {
        files,
        issues,
        suppressed,
    }
}

/// An issue in `doc`, placed and worded, numbered 0 and not hidden; `None`
/// when its rule isn't in `rules`.
fn reported<'a>(
    rules: &'a RuleSet,
    doc: &'a Document,
    index: &LineIndex,
    issue: &'a Issue,
) -> Option<Reported<'a>> {
    let rule = rules.get(issue.rule)?;
    let Position { line, column } = index.position(&doc.source, issue.span.start);
    let end = index.position(&doc.source, issue.span.end);
    let wording = rule.wording(issue.entry);
    let suggestions = wording
        .map(|w| w.suggestions(&issue.matched, &rule.name, &issue.args))
        .unwrap_or_default();
    let message = wording
        .map(|w| w.render(&shown(&issue.matched), &rule.name, &issue.args))
        .unwrap_or_default();
    // A match that ends a line ends where the next line starts.
    let last_line = if end.column == 1 && end.line > line {
        end.line - 1
    } else {
        end.line
    };
    Some(Reported {
        number: 0,
        file: &doc.display,
        path: doc.path.as_deref(),
        line,
        column,
        end_line: end.line,
        end_column: end.column,
        span: issue.span.clone(),
        rule,
        matched: &issue.matched,
        message,
        suggestions,
        level: issue.level,
        source: &doc.source,
        lines: index.lines(&doc.source, line, last_line),
        hidden: None,
    })
}

/// What hid an issue in `doc`, for people.
fn reason(why: &Suppression, doc: &Document, index: &LineIndex) -> String {
    match why {
        Suppression::Directive { at } => format!(
            "off by the comment on line {}",
            index.position(&doc.source, *at).line
        ),
        Suppression::Off { origin } => format!("off in {origin}"),
        Suppression::Accepted { origin } => format!("accepted in {origin}"),
        Suppression::Ignored { origin } => format!("ignored in {origin}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position};

    #[test]
    fn positions_count_characters() {
        let src = "first\nnaïve test\n";
        let index = LineIndex::new(src);
        let at = |offset| {
            let Position { line, column } = index.position(src, offset);
            (line, column)
        };
        assert_eq!(at(0), (1, 1));
        assert_eq!(at(src.find("test").unwrap()), (2, 7));
        assert_eq!(at(src.len()), (3, 1));
    }

    #[test]
    fn lines_drop_their_endings() {
        let src = "one\r\ntwo\nthree";
        let index = LineIndex::new(src);
        assert_eq!(&src[index.lines(src, 1, 1)], "one");
        assert_eq!(&src[index.lines(src, 1, 2)], "one\r\ntwo");
        assert_eq!(&src[index.lines(src, 3, 3)], "three");
    }
}
