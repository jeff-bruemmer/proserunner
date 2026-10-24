//! The default format: issues grouped under each file, in aligned
//! columns that every group shares. Each row ends with its rule's id, the
//! name config and directives take. Styled, files and matches are bold,
//! positions and ids dim, and each level has its color.

use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};

use anstyle::{AnsiColor, Style};
use unicode_width::UnicodeWidthStr;

use crate::report::{Report, Reported};
use crate::rules::Level;

/// A specimen wider than this is cut short, so a long match, such as a
/// sentence over a limit, doesn't push its message off the screen.
const MAX_SPECIMEN_WIDTH: usize = 40;

/// Messages are padded to the widest, up to this, so rule ids line up
/// without one long message pushing every id across the screen.
const MAX_MESSAGE_WIDTH: usize = 50;

const GAP: &str = "  ";

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();

struct Widths {
    number: usize,
    line: usize,
    column: usize,
    level: usize,
    specimen: usize,
    message: usize,
    rule: usize,
}

fn widths(issues: &[Reported<'_>]) -> Widths {
    let max = |f: &dyn Fn(&Reported<'_>) -> usize| issues.iter().map(f).max().unwrap_or(0);
    Widths {
        number: max(&|i| format!("[{}]", i.number).len()),
        line: max(&|i| i.line.to_string().len()),
        column: max(&|i| i.column.to_string().len()),
        level: max(&|i| i.level.as_str().len()),
        specimen: max(&|i| specimen(&i.shown()).width()),
        message: max(&|i| i.message.width()).min(MAX_MESSAGE_WIDTH),
        rule: max(&|i| i.rule.id.width()),
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

/// How a level stands out: errors in bold red, warnings in yellow, hints
/// in cyan.
fn level_style(level: Level) -> Style {
    match level {
        Level::Error => BOLD.fg_color(Some(AnsiColor::Red.into())),
        Level::Warning => Style::new().fg_color(Some(AnsiColor::Yellow.into())),
        Level::Hint => Style::new().fg_color(Some(AnsiColor::Cyan.into())),
    }
}

/// `text` in `style`, then spaces to `width`, so padding is never styled.
/// It writes itself, so a row allocates nothing for its cells.
struct Cell<'text> {
    text: &'text str,
    width: usize,
    style: Style,
}

impl Display for Cell<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let pad = self.width.saturating_sub(self.text.width());
        write!(f, "{}{}{:#}{:pad$}", self.style, self.text, self.style, "")
    }
}

fn cell(text: &str, width: usize, style: Style) -> Cell<'_> {
    Cell { text, width, style }
}

/// Writes `report`, then the issues it hid, if it kept them, without
/// numbers and with why; `styled` adds ANSI styles.
pub(super) fn write(out: &mut dyn Write, report: &Report<'_>, styled: bool) -> io::Result<()> {
    rows(out, &report.issues, styled)?;
    if !report.suppressed.is_empty() {
        let style = if styled { BOLD } else { Style::new() };
        let heading = "Hidden by config, ignore lists and inline control:";
        writeln!(out)?;
        writeln!(out, "{}", cell(heading, 0, style))?;
        rows(out, &report.suppressed, styled)?;
    }
    Ok(())
}

/// One row per issue, under a line for each file. A hidden issue's row
/// has no number, and ends with why it's hidden.
fn rows(out: &mut dyn Write, issues: &[Reported<'_>], styled: bool) -> io::Result<()> {
    let w = widths(issues);
    let style = |s: Style| if styled { s } else { Style::new() };
    let mut file: Option<&str> = None;

    for i in issues {
        if file != Some(i.file) {
            writeln!(out)?;
            writeln!(out, "{}", cell(i.file, 0, style(BOLD)))?;
            file = Some(i.file);
        }
        if i.hidden.is_none() {
            let number = format!("[{}]", i.number);
            write!(out, "{}{GAP}", cell(&number, w.number, Style::new()))?;
        }
        // Right-justify lines and left-justify columns so the colons line up.
        let position = format!("{:>width$}:{}", i.line, i.column, width = w.line);
        write!(
            out,
            "{}{GAP}{}{GAP}{} -> {}{GAP}",
            cell(&position, w.line + 1 + w.column, style(DIM)),
            cell(i.level.as_str(), w.level, style(level_style(i.level))),
            cell(&specimen(&i.shown()), w.specimen, style(BOLD)),
            cell(&i.message, w.message, Style::new()),
        )?;
        match &i.hidden {
            None => writeln!(out, "{}", cell(&i.rule.id, 0, style(DIM)))?,
            Some(hidden) => writeln!(
                out,
                "{}{GAP}{}",
                cell(&i.rule.id, w.rule, style(DIM)),
                hidden.reason
            )?,
        }
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

    #[test]
    fn cells_pad_outside_their_style() {
        assert_eq!(cell("ab", 4, Style::new()).to_string(), "ab  ");
        assert_eq!(cell("ab", 4, BOLD).to_string(), "\u{1b}[1mab\u{1b}[0m  ");
        assert_eq!(cell("abcdef", 4, Style::new()).to_string(), "abcdef");
    }
}
