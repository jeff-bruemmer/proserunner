//! `plain`: one issue per line, for editors and `grep`. Each line ends
//! with its rule's id, so `grep default/hedging` finds one rule's issues.
//! Hidden issues, with `--show-suppressed`, come last.

use std::io::{self, Write};

use crate::report::Report;

/// Writes `report`, then the issues it hid, if it kept them, each marked
/// `[hidden]` in place of a number, with why after its rule's id.
pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    for i in &report.issues {
        writeln!(
            out,
            "{}:{}:{}: [{}] \"{}\" -> {} ({})",
            i.file,
            i.line,
            i.column,
            i.number,
            i.shown(),
            i.message,
            i.rule.id
        )?;
    }
    for i in &report.suppressed {
        let reason = i.hidden.as_ref().map_or("", |h| h.reason.as_str());
        writeln!(
            out,
            "{}:{}:{}: [hidden] \"{}\" -> {} ({}; {reason})",
            i.file,
            i.line,
            i.column,
            i.shown(),
            i.message,
            i.rule.id
        )?;
    }
    Ok(())
}
