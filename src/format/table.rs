//! A Markdown (GFM) table, padded so it also lines up in a terminal.
//! Numbers are right-aligned.

use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

use crate::report::Report;

const HEADINGS: [&str; 8] = [
    "#", "File", "Line", "Col", "Match", "Message", "Rule", "Level",
];

/// Columns holding numbers.
const RIGHT: [bool; 8] = [true, false, true, true, false, false, false, false];

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    if report.issues.is_empty() {
        return Ok(());
    }
    let rows: Vec<[String; 8]> = report
        .issues
        .iter()
        .map(|i| {
            [
                i.number.to_string(),
                cell(i.file),
                i.line.to_string(),
                i.column.to_string(),
                cell(&i.shown()),
                cell(&i.message),
                cell(&i.rule.id),
                i.level.to_string(),
            ]
        })
        .collect();

    // At least three wide, so every delimiter cell is valid GFM.
    let mut widths = HEADINGS.map(|h| h.width().max(3));
    for row in &rows {
        for (w, c) in widths.iter_mut().zip(row) {
            *w = (*w).max(c.width());
        }
    }

    let headings = HEADINGS.map(str::to_owned);
    write_row(out, &headings, &widths)?;
    let delimiters: Vec<String> = widths
        .iter()
        .zip(RIGHT)
        .map(|(&w, right)| {
            if right {
                format!("{}:", "-".repeat(w - 1))
            } else {
                "-".repeat(w)
            }
        })
        .collect();
    writeln!(out, "| {} |", delimiters.join(" | "))?;
    for row in &rows {
        write_row(out, row, &widths)?;
    }
    Ok(())
}

fn write_row(out: &mut dyn Write, row: &[String; 8], widths: &[usize; 8]) -> io::Result<()> {
    let cells: Vec<String> = row
        .iter()
        .zip(widths)
        .zip(RIGHT)
        .map(|((c, &w), right)| {
            let pad = " ".repeat(w.saturating_sub(c.width()));
            if right {
                format!("{pad}{c}")
            } else {
                format!("{c}{pad}")
            }
        })
        .collect();
    writeln!(out, "| {} |", cells.join(" | "))
}

/// A pipe ends a cell, even in a code span, so it's escaped.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::cell;

    #[test]
    fn pipes_are_escaped() {
        assert_eq!(cell("a | b"), "a \\| b");
    }
}
