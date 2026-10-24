//! `{"version": 1, "issues": [...]}`. Lines and columns are 1-based;
//! columns count characters, and `end_column` is just past the match.
//! Byte offsets are exact, for tools that edit the file.

use std::io::{self, Write};

use serde::Serialize;

use crate::report::Report;

/// Bumped only when a field changes meaning or goes away.
const VERSION: u32 = 1;

#[derive(Serialize)]
struct Output<'report> {
    version: u32,
    issues: Vec<Issue<'report>>,
}

#[derive(Serialize)]
struct Issue<'report> {
    number: usize,
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
}

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    let output = Output {
        version: VERSION,
        issues: report
            .issues
            .iter()
            .map(|i| Issue {
                number: i.number,
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
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut *out, &output)?;
    writeln!(out)
}
