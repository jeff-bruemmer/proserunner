//! Checkstyle XML, which reviewdog, Jenkins and other CI tools read. Every
//! file checked has a `<file>` element, with or without issues. Hints are
//! `info`. Checkstyle has no end positions, so the message quotes the
//! match. Paths are relative to the working directory when under it.

use std::io::{self, Write};
use std::path::Path;
use std::ptr;

use super::located::Located;
use crate::report::{Report, Reported};
use crate::rules::Level;

/// The format version other tools that write Checkstyle XML give.
const VERSION: &str = "4.3";

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>, cwd: &Path) -> io::Result<()> {
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(out, r#"<checkstyle version="{VERSION}">"#)?;
    let mut issues = report.issues.iter().peekable();
    for doc in &report.files {
        let name = match Located::of(doc.path.as_deref(), cwd) {
            Some(Located::Relative(rel)) => rel,
            Some(Located::Absolute(path)) => path.display().to_string(),
            None => doc.display.clone(),
        };
        // Issues come in file order, each pointing at its file's display.
        let mut errors = Vec::new();
        while let Some(i) = issues.next_if(|i| ptr::eq(i.file, doc.display.as_str())) {
            errors.push(i);
        }
        if errors.is_empty() {
            writeln!(out, r#"  <file name="{}"/>"#, escape(&name))?;
            continue;
        }
        writeln!(out, r#"  <file name="{}">"#, escape(&name))?;
        for i in errors {
            error(out, i)?;
        }
        writeln!(out, "  </file>")?;
    }
    writeln!(out, "</checkstyle>")
}

fn error(out: &mut dyn Write, i: &Reported<'_>) -> io::Result<()> {
    let severity = match i.level {
        Level::Hint => "info",
        Level::Warning => "warning",
        Level::Error => "error",
    };
    let message = format!("\"{}\" -> {}", i.shown(), i.message);
    writeln!(
        out,
        r#"    <error line="{}" column="{}" severity="{severity}" message="{}" source="{}"/>"#,
        i.line,
        i.column,
        escape(&message),
        escape(&i.rule.id)
    )
}

/// Escapes text for an attribute value. Line breaks and tabs are written
/// as references, which a parser keeps; in an attribute it would turn the
/// characters themselves into spaces. XML 1.0 can't hold other control
/// characters at all, even as references, so they become U+FFFD.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\u{0}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(
            escape(r#"a <b> & "c" 'd'"#),
            "a &lt;b&gt; &amp; &quot;c&quot; &apos;d&apos;"
        );
        assert_eq!(escape("one\r\ntwo\tthree"), "one&#13;&#10;two&#9;three");
        assert_eq!(escape("form\u{C}feed\u{FFFF}"), "form\u{FFFD}feed\u{FFFD}");
        assert_eq!(escape("naïve 𝒜"), "naïve 𝒜");
    }
}
