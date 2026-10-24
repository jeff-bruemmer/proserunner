//! SARIF 2.1.0, the format code scanning reads (GitHub, GitLab, Azure
//! DevOps, editors' SARIF viewers). Hints are notes.
//!
//! Columns are UTF-16 code units, SARIF's default, because consumers that
//! ignore `columnKind` assume it. Paths under the working directory are
//! relative to `%SRCROOT%`, which is the working directory.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use super::located::Located;
use crate::report::{self, Report, Reported};
use crate::rules::{Level, Rule};

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const INFORMATION_URI: &str = "https://github.com/jeff-bruemmer/proserunner";
const SRCROOT: &str = "%SRCROOT%";

#[derive(Serialize)]
struct Log<'report> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: [Run<'report>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Run<'report> {
    tool: Tool<'report>,
    original_uri_base_ids: BTreeMap<&'static str, Uri>,
    results: Vec<SarifResult<'report>>,
}

#[derive(Serialize)]
struct Tool<'report> {
    driver: Driver<'report>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Driver<'report> {
    name: &'static str,
    semantic_version: &'static str,
    information_uri: &'static str,
    rules: Vec<Descriptor<'report>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor<'report> {
    id: &'report str,
    name: &'report str,
    short_description: Text<'report>,
    full_description: Text<'report>,
    help: Text<'report>,
    default_configuration: Configuration,
}

#[derive(Serialize)]
struct Configuration {
    level: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult<'report> {
    rule_id: &'report str,
    rule_index: usize,
    level: &'static str,
    message: Text<'report>,
    locations: [Location<'report>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Location<'report> {
    physical_location: PhysicalLocation<'report>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation<'report> {
    artifact_location: ArtifactLocation,
    region: Region<'report>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactLocation {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri_base_id: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Region<'report> {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
    snippet: Text<'report>,
}

#[derive(Serialize)]
struct Text<'report> {
    text: &'report str,
}

#[derive(Serialize)]
struct Uri {
    uri: String,
}

pub(super) fn write(out: &mut dyn Write, report: &Report<'_>, cwd: &Path) -> io::Result<()> {
    // Only the rules with results, in id order; results point at them by
    // index.
    let rules: BTreeMap<&str, &Rule> = report
        .issues
        .iter()
        .map(|i| (i.rule.id.as_str(), i.rule))
        .collect();
    let index: BTreeMap<&str, usize> = rules.keys().enumerate().map(|(n, &id)| (id, n)).collect();

    let log = Log {
        schema: SCHEMA,
        version: "2.1.0",
        runs: [Run {
            tool: Tool {
                driver: Driver {
                    name: "proserunner",
                    semantic_version: env!("CARGO_PKG_VERSION"),
                    information_uri: INFORMATION_URI,
                    rules: rules.values().map(|r| descriptor(r)).collect(),
                },
            },
            original_uri_base_ids: BTreeMap::from([(
                SRCROOT,
                Uri {
                    uri: format!("{}/", file_uri(cwd).trim_end_matches('/')),
                },
            )]),
            results: report
                .issues
                .iter()
                .map(|i| result(i, index.get(i.rule.id.as_str()).copied().unwrap_or(0), cwd))
                .collect(),
        }],
    };
    serde_json::to_writer_pretty(&mut *out, &log)?;
    writeln!(out)
}

fn descriptor(rule: &Rule) -> Descriptor<'_> {
    let explain = rule
        .explain
        .as_deref()
        .map_or(rule.name.as_str(), str::trim);
    Descriptor {
        id: &rule.id,
        name: &rule.name,
        short_description: Text { text: &rule.name },
        full_description: Text { text: explain },
        help: Text { text: explain },
        default_configuration: Configuration {
            level: level(rule.level),
        },
    }
}

fn result<'a>(i: &'a Reported<'a>, rule_index: usize, cwd: &Path) -> SarifResult<'a> {
    let artifact_location = match Located::of(i.path, cwd) {
        Some(Located::Relative(rel)) => ArtifactLocation {
            uri: encode(&rel, /*keep_colon=*/ false),
            uri_base_id: Some(SRCROOT),
        },
        Some(Located::Absolute(path)) => ArtifactLocation {
            uri: file_uri(path),
            uri_base_id: None,
        },
        None => ArtifactLocation {
            uri: "stdin".into(),
            uri_base_id: None,
        },
    };
    SarifResult {
        rule_id: &i.rule.id,
        rule_index,
        level: level(i.level),
        message: Text { text: &i.message },
        locations: [Location {
            physical_location: PhysicalLocation {
                artifact_location,
                region: Region {
                    start_line: i.line,
                    start_column: utf16_column(i.source, i.span.start),
                    end_line: i.end_line,
                    end_column: utf16_column(i.source, i.span.end),
                    snippet: Text {
                        text: i.source.get(i.span.clone()).unwrap_or(i.matched),
                    },
                },
            },
        }],
    }
}

fn level(level: Level) -> &'static str {
    match level {
        Level::Hint => "note",
        Level::Warning => "warning",
        Level::Error => "error",
    }
}

/// The 1-based UTF-16 column of byte `offset`.
fn utf16_column(source: &str, offset: usize) -> usize {
    let before = source.get(..offset).unwrap_or("");
    let start = report::line_start(source, before.rfind('\n').map_or(0, |n| n + 1));
    before.get(start..).unwrap_or("").encode_utf16().count() + 1
}

/// A `file://` URI for an absolute path.
fn file_uri(path: &Path) -> String {
    let slashed = path.display().to_string().replace('\\', "/");
    let rooted = if slashed.starts_with('/') {
        slashed
    } else {
        format!("/{slashed}")
    };
    format!("file://{}", encode(&rooted, /*keep_colon=*/ true))
}

/// Percent-encodes everything but unreserved characters and `/`. A colon
/// is kept only in absolute paths (Windows drives): in a relative one it
/// would read as a URI scheme.
fn encode(path: &str, keep_colon: bool) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/')
            || (keep_colon && b == b':');
        if keep {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_utf16_units() {
        let src = "line\n𝒜 naïve x";
        let x = src.find('x').unwrap();
        // 𝒜 is two UTF-16 units; ï is one.
        assert_eq!(utf16_column(src, x), 10);
        assert_eq!(utf16_column(src, 0), 1);
    }

    #[test]
    fn uris_are_encoded() {
        assert_eq!(
            encode("docs/a b#1.md", /*keep_colon=*/ false),
            "docs/a%20b%231.md"
        );
        assert_eq!(encode("x:y.md", /*keep_colon=*/ false), "x%3Ay.md");
        assert_eq!(
            file_uri(Path::new("/home/me/naïve.md")),
            "file:///home/me/na%C3%AFve.md"
        );
    }
}
