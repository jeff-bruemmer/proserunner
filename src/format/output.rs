//! The output formats: what flags and config call each one, whether the
//! summary line goes with it, and its writer.

use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};
use std::path::Path;

use clap::ValueEnum;
use serde::Deserialize;

use super::{checkstyle, github, group, json, plain, sarif, table, verbose};
use crate::report::Report;

/// How issues are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Output {
    /// Issues grouped under each file, numbered
    #[default]
    Group,
    /// One issue per line: path:line:col: [n] "match" -> message
    #[expect(
        rustdoc::broken_intra_doc_links,
        reason = "also --help's and the schema's text, which would show an escaped [n]"
    )]
    Plain,
    /// A Markdown table
    Table,
    /// A Markdown report with each issue's line, suggestions and explanation
    Verbose,
    /// Machine-readable, versioned
    Json,
    /// SARIF 2.1.0, for code scanning
    Sarif,
    /// GitHub Actions annotations
    Github,
    /// Checkstyle XML, for reviewdog, Jenkins and other CI tools
    Checkstyle,
}

/// An output format's name, whether the summary line goes with it, and
/// its writer.
struct Writer {
    output: Output,
    name: &'static str,
    /// Whether "Found 3 issues." goes to standard error after the report.
    /// Formats for programs go without, and `verbose` has its own.
    summary: bool,
    /// Whether [`write_styled`] adds ANSI styles to it.
    styles: bool,
    /// Writes a report; the path is what relative paths in it are
    /// relative to, and the flag says whether to style it.
    write: fn(&mut dyn Write, &Report<'_>, &Path, bool) -> io::Result<()>,
}

/// Every output format, in `Output` order. A new format is a variant, a
/// row, and its writer.
const WRITERS: [Writer; 8] = [
    Writer {
        output: Output::Group,
        name: "group",
        summary: true,
        styles: true,
        write: |out, report, _, styled| group::write(out, report, styled),
    },
    Writer {
        output: Output::Plain,
        name: "plain",
        summary: false,
        styles: false,
        write: |out, report, _, _| plain::write(out, report),
    },
    Writer {
        output: Output::Table,
        name: "table",
        summary: true,
        styles: false,
        write: |out, report, _, _| table::write(out, report),
    },
    Writer {
        output: Output::Verbose,
        name: "verbose",
        summary: false,
        styles: false,
        write: |out, report, _, _| verbose::write(out, report),
    },
    Writer {
        output: Output::Json,
        name: "json",
        summary: false,
        styles: false,
        write: |out, report, _, _| json::write(out, report),
    },
    Writer {
        output: Output::Sarif,
        name: "sarif",
        summary: false,
        styles: false,
        write: |out, report, cwd, _| sarif::write(out, report, cwd),
    },
    Writer {
        output: Output::Github,
        name: "github",
        summary: false,
        styles: false,
        write: |out, report, cwd, _| github::write(out, report, cwd),
    },
    Writer {
        output: Output::Checkstyle,
        name: "checkstyle",
        summary: false,
        styles: false,
        write: |out, report, cwd, _| checkstyle::write(out, report, cwd),
    },
];

const _: () = {
    let mut i = 0;
    while i < WRITERS.len() {
        assert!(
            WRITERS[i].output as usize == i,
            "WRITERS must be in `Output` order: `Output::writer` indexes it by variant"
        );
        i += 1;
    }
};

impl Output {
    /// Every output format, in `Output` order.
    pub fn all() -> impl Iterator<Item = Output> {
        WRITERS.iter().map(|w| w.output)
    }

    /// What `--output` and config call it.
    pub fn as_str(self) -> &'static str {
        self.writer().name
    }

    /// Whether the summary line ("Found 3 issues.") goes with this format.
    pub fn has_summary(self) -> bool {
        self.writer().summary
    }

    /// Whether [`write_styled`] styles this format: only `group`, the
    /// format for people. The others are the same either way.
    pub fn has_styles(self) -> bool {
        self.writer().styles
    }

    fn writer(self) -> &'static Writer {
        &WRITERS[self as usize]
    }
}

impl Display for Output {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// Writes `report` in `format`. `cwd` is what relative paths in the
/// report are relative to; SARIF and GitHub need it.
pub fn write(
    out: &mut dyn Write,
    report: &Report<'_>,
    format: Output,
    cwd: &Path,
) -> io::Result<()> {
    (format.writer().write)(out, report, cwd, false)
}

/// Like [`write()`], with ANSI styles in the formats that
/// [`Output::has_styles`] names. They're written whatever `out` is, so
/// pass a stream that drops them where they don't belong, such as
/// `anstream::AutoStream`.
pub fn write_styled(
    out: &mut dyn Write,
    report: &Report<'_>,
    format: Output,
    cwd: &Path,
) -> io::Result<()> {
    (format.writer().write)(out, report, cwd, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_names_are_the_ones_flags_and_config_take() {
        for &output in Output::value_variants() {
            let flag = output.to_possible_value().map(|v| v.get_name().to_owned());
            assert_eq!(flag.as_deref(), Some(output.as_str()));
            let config: Output = serde_json::from_value(output.as_str().into()).unwrap();
            assert_eq!(config, output);
        }
    }
}
