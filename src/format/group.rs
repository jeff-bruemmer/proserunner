//! The default format: issues grouped under each file, in aligned
//! columns that every group shares.

use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

use crate::report::{Report, Reported};

/// A specimen wider than this is cut short, so a long match, such as a
/// sentence over a limit, doesn't push its message off the screen.
const MAX_SPECIMEN_WIDTH: usize = 40;

const GAP: &str = "  ";

struct Widths {
    number: usize,
    line: usize,
    column: usize,
    specimen: usize,
}

fn widths(issues: &[Reported<'_>]) -> Widths {
    let max = |f: &dyn Fn(&Reported<'_>) -> usize| issues.iter().map(f).max().unwrap_or(0);
    Widths {
        number: max(&|i| format!("[{}]", i.number).len()),
        line: max(&|i| i.line.to_string().len()),
        column: max(&|i| i.column.to_string().len()),
        specimen: max(&|i| specimen(&i.shown()).width()),
    }
}

/// A match quoted, and cut short with "…" past [`MAX_SPECIMEN_WIDTH`].
fn specimen(s: &str) -> String {
    use unicode_width::UnicodeWidthChar as _;

    let quoted = format!("\"{s}\"");
    if quoted.width() <= MAX_SPECIMEN_WIDTH {
        return quoted;
    }
    // Room for both quotes and the ellipsis.
    let budget = MAX_SPECIMEN_WIDTH - 3;
    let mut width = 0;
    let mut cut = String::from("\"");
    for c in s.chars() {
        width += c.width().unwrap_or(0);
        if width > budget {
            break;
        }
        cut.push(c);
    }
    format!("{}…\"", cut.trim_end())
}

fn pad_right(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    let w = widths(&report.issues);
    let mut file: Option<&str> = None;

    for i in &report.issues {
        if file != Some(i.file) {
            writeln!(out)?;
            writeln!(out, "{}", i.file)?;
            file = Some(i.file);
        }
        // Right-justify lines and left-justify columns so the colons line up.
        let position = format!("{:>width$}:{}", i.line, i.column, width = w.line);
        writeln!(
            out,
            "{}{GAP}{}{GAP}{} -> {}",
            pad_right(&format!("[{}]", i.number), w.number),
            pad_right(&position, w.line + 1 + w.column),
            pad_right(&specimen(&i.shown()), w.specimen),
            i.message
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_specimens_are_cut_short() {
        assert_eq!(specimen("utilize"), "\"utilize\"");
        let fits = "x".repeat(MAX_SPECIMEN_WIDTH - 2);
        assert_eq!(specimen(&fits), format!("\"{fits}\""));
        let cut = specimen("It covers the command-line interface and much more besides.");
        assert_eq!(cut, "\"It covers the command-line interface…\"");
        assert!(cut.width() <= MAX_SPECIMEN_WIDTH);
        let wide = specimen(&"漢".repeat(30));
        assert_eq!(wide.width(), MAX_SPECIMEN_WIDTH - 1);
    }
}
