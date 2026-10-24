//! `{"version": 1, "issues": [...]}`. Lines and columns are 1-based;
//! columns count characters, and `end_column` is just past the match.
//! Byte offsets are exact, for tools that edit the file. With
//! `--show-suppressed`, `"suppressed"` lists the issues hidden, each with
//! `hidden_by` and `reason`, and no number.

use std::io::{self, Write};

use serde::Serialize;

use crate::report::{Report, Reported};

/// Bumped only when a field changes meaning or goes away.
const VERSION: u32 = 1;

#[derive(Serialize)]
struct Output<'report> {
    version: u32,
    issues: Vec<Issue<'report>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suppressed: Vec<Issue<'report>>,
}

#[derive(Serialize)]
struct Issue<'report> {
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<usize>,
    file: &'report str,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    byte_start: usize,
    byte_end: usize,
    rule: &'report str,
    name: &'report str,
    level: &'static str,
    #[serde(rename = "match")]
    matched: &'report str,
    message: &'report str,
    suggestions: &'report [String],
    /// For a hidden issue, what hid it: `directive`, `off`, `accepted`
    /// or `ignored`.
    #[serde(skip_serializing_if = "Option::is_none")]
    hidden_by: Option<&'static str>,
    /// For a hidden issue, what hid it, for people.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'report str>,
}

impl<'report> Issue<'report> {
    fn new(i: &'report Reported<'_>) -> Self {
        Issue {
            number: i.hidden.is_none().then_some(i.number),
            file: i.file,
            line: i.line,
            column: i.column,
            end_line: i.end_line,
            end_column: i.end_column,
            byte_start: i.span.start,
            byte_end: i.span.end,
            rule: &i.rule.id,
            name: &i.rule.name,
            level: i.level.as_str(),
            matched: i.matched,
            message: &i.message,
            suggestions: &i.suggestions,
            hidden_by: i.hidden.as_ref().map(|h| h.why.kind()),
            reason: i.hidden.as_ref().map(|h| h.reason.as_str()),
        }
    }
}

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    let output = Output {
        version: VERSION,
        issues: report.issues.iter().map(Issue::new).collect(),
        suppressed: report.suppressed.iter().map(Issue::new).collect(),
    };
    serde_json::to_writer_pretty(&mut *out, &output)?;
    writeln!(out)
}
