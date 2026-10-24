//! Any text, in every format, scans and lints without panicking, and every
//! span it reports lies on character boundaries of the source. Sentences
//! lie on character boundaries of their block, in order, and only
//! headings have a level. Checkstyle output parses as XML.
//!
//! The first byte picks the options (bit 2 quoted text, bit 3 code
//! blocks) and the format: bits 0–1, then 4–7, index `Format::all()`.
//! The rest is the document.

#![no_main]

use std::io;
use std::iter;
use std::path::Path;
use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use proserunner::format::{self, Output};
use proserunner::report::{self, Document};
use proserunner::rules::{self, RuleSet, RuleSource};
use proserunner::scan::{self, Format, Options, Scope};
use proserunner::{lint, prose};

/// The built-in rules, and one that matches every word, so that spans all
/// over each document get checked.
static RULES: LazyLock<RuleSet> = LazyLock::new(|| {
    let mut sources: Vec<RuleSource<'static>> = rules::builtin().collect();
    sources.push(RuleSource::new(
        "fuzz/words",
        "schema = 1\nmessage = \"{match}\"\npatterns = ['\\w+']\nrepeat = true",
    ));
    RuleSet::compile(sources).rules
});

fn on_boundaries(source: &str, start: usize, end: usize) {
    assert!(
        start <= end && end <= source.len(),
        "{start}..{end} of {}",
        source.len()
    );
    assert!(
        source.is_char_boundary(start) && source.is_char_boundary(end),
        "{start}..{end} splits a character"
    );
}

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let Ok(source) = str::from_utf8(rest) else {
        return;
    };
    let index = usize::from((first & 3) | ((first >> 4) << 2));
    let formats: Vec<Format> = Format::all().collect();
    let format = formats[index % formats.len()];
    let mut options = Options::default();
    options.quoted_text = first & 4 != 0;
    options.code_blocks = first & 8 != 0;

    let scanned = scan::scan(format, source, options);
    for block in &scanned.blocks {
        for (i, c) in block.text().char_indices() {
            let range = block.source_range(source, i..i + c.len_utf8());
            on_boundaries(source, range.start, range.end);
        }
        assert_eq!(block.level().is_some(), block.kind() == Scope::Heading);
        let mut last = 0;
        for s in prose::sentences(block.text()) {
            on_boundaries(block.text(), s.start, s.end);
            assert!(last <= s.start && s.start < s.end, "{s:?} after {last}");
            last = s.end;
        }
    }
    for d in &scanned.directives {
        for word in iter::once(&d.action).chain(&d.ids) {
            on_boundaries(source, word.at, word.at);
        }
        on_boundaries(source, d.end, d.end);
    }

    let linted = lint::document(&RULES, format, source, options);
    for issue in &linted.issues {
        on_boundaries(source, issue.span.start, issue.span.end);
    }
    let documents = [Document::new(
        "fuzz".into(),
        None,
        source.to_owned(),
        linted.issues,
    )];
    let report = report::build(&RULES, &documents);
    for output in Output::all() {
        format::write(&mut io::sink(), &report, output, Path::new("/")).unwrap();
        format::write_styled(&mut io::sink(), &report, output, Path::new("/")).unwrap();
    }

    // XML 1.0 can't hold every character a document can, so Checkstyle
    // output must escape or replace each one and still parse.
    let mut xml = Vec::new();
    format::write(&mut xml, &report, Output::Checkstyle, Path::new("/")).unwrap();
    let xml = String::from_utf8(xml).unwrap();
    let parsed = roxmltree::Document::parse(&xml).unwrap();
    let errors = parsed.descendants().filter(|n| n.has_tag_name("error"));
    assert_eq!(errors.count(), report.issues.len());
});
