//! `plain`: one issue per line, for editors and `grep`.

use std::io::{self, Write};

use crate::report::Report;

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    for i in &report.issues {
        writeln!(
            out,
            "{}:{}:{}: [{}] \"{}\" -> {}",
            i.file,
            i.line,
            i.column,
            i.number,
            i.shown(),
            i.message
        )?;
    }
    Ok(())
}
