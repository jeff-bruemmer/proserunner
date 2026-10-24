//! GitHub Actions workflow commands: one `::warning file=…::message` line
//! per issue, which the runner shows as an annotation on the line. Hints
//! are notices. Paths are relative to the working directory, which in a
//! workflow is the repository.

use std::io::{self, Write};
use std::path::Path;

use super::located::Located;
use crate::report::Report;
use crate::rules::Level;

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>, cwd: &Path) -> io::Result<()> {
    for i in &report.issues {
        let command = match i.level {
            Level::Hint => "notice",
            Level::Warning => "warning",
            Level::Error => "error",
        };
        let mut params = Vec::new();
        let mut message = format!("\"{}\" -> {}", i.shown(), i.message);
        match Located::of(i.path, cwd) {
            Some(at) => {
                let file = match at {
                    Located::Relative(rel) => rel,
                    Located::Absolute(path) => path.display().to_string(),
                };
                params.push(format!("file={}", property(&file)));
                params.push(format!("line={}", i.line));
                params.push(format!("endLine={}", i.end_line));
                // GitHub reads columns only within one line.
                if i.line == i.end_line {
                    params.push(format!("col={}", i.column));
                    params.push(format!("endColumn={}", i.end_column));
                }
            }
            None => message = format!("{}:{}:{}: {message}", i.file, i.line, i.column),
        }
        let title = format!("{} ({})", i.rule.name, i.rule.id);
        params.push(format!("title={}", property(&title)));
        writeln!(out, "::{command} {}::{}", params.join(","), data(&message))?;
    }
    Ok(())
}

/// Escapes a message, as `@actions/core` does.
fn data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escapes a parameter value, which also can't hold `:` or `,`.
fn property(s: &str) -> String {
    data(s).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(data("100% done\r\nok"), "100%25 done%0D%0Aok");
        assert_eq!(property("a: b, c"), "a%3A b%2C c");
    }
}
