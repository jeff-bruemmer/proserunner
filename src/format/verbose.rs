//! A Markdown report: each issue with its line, its suggestions and its
//! rule's explanation, then counts by rule and by file. Locations are absolute,
//! so an assistant or a person can open them from anywhere.

use std::collections::BTreeMap;
use std::io::{self, Write};

use crate::english;
use crate::report::{Report, Reported};

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>) -> io::Result<()> {
    writeln!(out, "# Proserunner results")?;
    writeln!(out)?;
    let issues = &report.issues;
    let mut files: Vec<&str> = issues.iter().map(|i| i.file).collect();
    files.dedup();
    if issues.is_empty() {
        return writeln!(out, "No issues found.");
    }
    writeln!(
        out,
        "{} in {}.",
        english::count(issues.len(), "issue"),
        english::count(files.len(), "file")
    )?;

    for i in issues {
        writeln!(out)?;
        issue(out, i)?;
    }

    writeln!(out)?;
    writeln!(out, "## Summary")?;
    writeln!(out)?;
    writeln!(out, "- **Issues:** {}", issues.len())?;
    writeln!(out, "- **By rule:**")?;
    let mut by_rule: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
    for i in issues {
        by_rule.entry(&i.rule.id).or_insert((&i.rule.name, 0)).1 += 1;
    }
    for (id, (name, n)) in by_rule {
        writeln!(out, "  - {name} ({}): {n}", code(id))?;
    }
    writeln!(out, "- **By file:**")?;
    for file in files {
        let n = issues.iter().filter(|i| i.file == file).count();
        writeln!(out, "  - {}: {}", code(file), english::count(n, "issue"))?;
    }
    Ok(())
}

fn issue(out: &mut dyn Write, i: &Reported<'_>) -> io::Result<()> {
    writeln!(
        out,
        "## {}. {} ({})",
        i.number,
        i.rule.name,
        code(&i.rule.id)
    )?;
    writeln!(out)?;
    let file = i
        .path
        .map_or_else(|| i.file.to_owned(), |p| p.display().to_string());
    writeln!(
        out,
        "{} · {}",
        code(&format!("{file}:{}:{}", i.line, i.column)),
        i.level
    )?;
    writeln!(out)?;
    for line in i.line_text().lines() {
        let line = line.trim();
        if line.is_empty() {
            writeln!(out, ">")?;
        } else {
            writeln!(out, "> {line}")?;
        }
    }
    writeln!(out)?;
    writeln!(out, "**Problem:** {}: {}", code(&i.shown()), i.message)?;

    if !i.suggestions.is_empty() {
        let quoted: Vec<String> = i.suggestions.iter().map(|s| code(s)).collect();
        writeln!(out)?;
        writeln!(out, "**Suggestions:** {}.", quoted.join(", "))?;
    }
    if let Some(why) = i.rule.explain.as_deref().map(str::trim) {
        writeln!(out)?;
        writeln!(out, "**Why:** {why}")?;
    }
    writeln!(out)?;
    writeln!(out, "---")
}

/// A code span that holds `s` whatever backticks it has.
fn code(s: &str) -> String {
    let longest = s.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    if longest == 0 {
        return format!("`{s}`");
    }
    let fence = "`".repeat(longest + 1);
    format!("{fence} {s} {fence}")
}

#[cfg(test)]
mod tests {
    use super::code;

    #[test]
    fn code_spans_hold_backticks() {
        assert_eq!(code("a.md"), "`a.md`");
        assert_eq!(code("a `b` c"), "`` a `b` c ``");
    }
}
