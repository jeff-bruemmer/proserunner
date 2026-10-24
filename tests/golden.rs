//! The golden corpus: what the built-in rules find in `resources/`. Any
//! change to matching shows up here as a snapshot diff to review, and a
//! changed snapshot needs a "Behaviour changes" note in the changelog.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use assert_cmd::Command;

/// What a run over `files` prints in `format`, with the repository's
/// path as `[ROOT]` so snapshots hold on any machine.
fn output(files: &[&str], format: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let home = tempfile::tempdir().unwrap();
    let out = Command::cargo_bin("proserunner")
        .unwrap()
        .current_dir(root)
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("XDG_CACHE_HOME", home.path().join(".cache"))
        .args(files)
        .args(["-o", format])
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .replace(&root.display().to_string(), "[ROOT]")
}

fn plain(files: &[&str]) -> String {
    output(files, "plain")
}

#[test]
fn drivel() {
    insta::assert_snapshot!(plain(&["resources/drivel.md", "resources/more/words.md"]));
}

#[test]
fn benchmark_samples() {
    insta::assert_snapshot!(plain(&[
        "resources/benchmark-data/small.md",
        "resources/benchmark-data/medium.md"
    ]));
}

#[test]
fn org() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.org"]));
}

#[test]
fn text() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.txt"]));
}

#[test]
fn html() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.html"]));
}

#[test]
fn asciidoc() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.adoc"]));
}

#[test]
fn typst() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.typ"]));
}

#[test]
fn mdx() {
    insta::assert_snapshot!(plain(&["resources/formats/drivel.mdx"]));
}

/// Every format but `plain`, on one file, so a change to what a format
/// writes shows up as a diff too.
#[test]
fn every_format() {
    for format in [
        "group",
        "table",
        "verbose",
        "json",
        "sarif",
        "github",
        "checkstyle",
    ] {
        insta::assert_snapshot!(
            format!("format_{format}"),
            output(&["resources/drivel.md"], format)
        );
    }
}
