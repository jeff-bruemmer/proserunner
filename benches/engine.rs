//! `cargo bench`: the speed targets. Linting 5k lines must stay under
//! 5 ms in every format whose parser allows it (asciidoc-parser and
//! markdown-rs alone take longer); compiling the built-in rules is the
//! fixed cost of every run, and 500 rules show how both grow with the
//! rule count.

#![expect(missing_docs, reason = "criterion_group! makes an undocumented pub fn")]

use std::hint;

use criterion::{Criterion, criterion_group, criterion_main};
use proserunner::lint;
use proserunner::rules::{self, RuleSet, RuleSource};
use proserunner::scan::{Format, Options};

const XLARGE: &str = include_str!("../resources/benchmark-data/xlarge.md");
const MEDIUM: &str = include_str!("../resources/benchmark-data/medium.md");
const DOCS: &str = include_str!("../resources/benchmark-data/docs.md");
const ORG: &str = include_str!("../resources/formats/drivel.org");
const HTML: &str = include_str!("../resources/formats/drivel.html");
const ADOC: &str = include_str!("../resources/formats/drivel.adoc");
const MDX: &str = include_str!("../resources/formats/drivel.mdx");
const TYPST: &str = include_str!("../resources/formats/drivel.typ");

/// `text` repeated to at least 5,000 lines.
fn five_k_lines(text: &str) -> String {
    text.repeat(5000 / text.lines().count() + 1)
}

/// The first 2 KB of `text`, ending at a line break, as `scripts/check-speed`
/// cuts its sample.
fn two_kb(text: &str) -> &str {
    let cut = text[..2048.min(text.len())]
        .rfind('\n')
        .map_or(2048, |i| i + 1);
    &text[..cut]
}

/// The built-in rules, then generated ones up to 500, shaped like a large
/// style: about 20 words each, some phrases and brace expansions, patterns
/// in every fifth rule and case sensitivity in every tenth. The generated
/// words hold digits, so they never occur in the corpora: issues stay the
/// built-ins', and what's measured is the cost of the extra rules.
fn five_hundred_rules() -> Vec<RuleSource<'static>> {
    use std::fmt::Write as _;

    let mut sources: Vec<RuleSource<'static>> = rules::builtin().collect();
    for i in sources.len()..500 {
        let mut words: Vec<String> = (0..17).map(|w| format!("\"zq{i}x{w}\"")).collect();
        words.push(format!("\"zq{i} phrase of words\""));
        words.push(format!("\"zq{i}y{{a,b,c}}\""));
        let mut text = format!(
            "schema = 1\nmessage = \"Avoid {{match}}.\"\nwords = [{}]\n",
            words.join(", ")
        );
        if i % 5 == 0 {
            let _ = writeln!(
                text,
                "patterns = ['\\bzq{i}p\\d+\\b', '\\bzq{i}q(?:ing|ed)\\b']"
            );
        }
        if i % 10 == 0 {
            text.push_str("case = \"sensitive\"\n");
        }
        let id = format!("bench/rule-{i}");
        sources.push(RuleSource::new(id, text));
    }
    sources
}

/// The built-in rules, and one more with every check on.
fn with_checks() -> Vec<RuleSource<'static>> {
    let mut sources: Vec<RuleSource<'static>> = rules::builtin().collect();
    sources.push(RuleSource::new(
        "bench/checks",
        r#"schema = 1
repeat = true
consistent = [["e-mail", "email"], ["advisor{,s}", "adviser{,s}"], ["toward", "towards"]]
limit = { max = 30 }
acronyms = { except = ["URL", "JSON", "TOML"] }
metric = { max = 12 }
"#,
    ));
    sources.push(RuleSource::new(
        "bench/headings",
        "schema = 1\nscope = [\"heading\"]\ncapitalization = {}\nlimit = { max = 8, per = \"block\" }\n",
    ));
    sources
}

fn compile(c: &mut Criterion) {
    c.bench_function("compile built-in rules", |b| {
        b.iter(|| RuleSet::compile(hint::black_box(rules::builtin())))
    });
    c.bench_function("compile 500 rules", |b| {
        b.iter(|| RuleSet::compile(hint::black_box(five_hundred_rules())))
    });
}

fn lint_5k_lines(c: &mut Criterion) {
    let set = RuleSet::compile(rules::builtin()).rules;
    let org = five_k_lines(ORG);
    let docs = five_k_lines(DOCS);
    let html = five_k_lines(HTML);
    let adoc = five_k_lines(ADOC);
    let mdx = five_k_lines(MDX);
    let typst = five_k_lines(TYPST);
    for (name, format, text) in [
        ("lint 5k lines", Format::Markdown, XLARGE),
        ("lint 5k lines (org)", Format::Org, org.as_str()),
        ("lint 5k lines (docs)", Format::Markdown, docs.as_str()),
        ("lint 5k lines (html)", Format::Html, html.as_str()),
        ("lint 5k lines (asciidoc)", Format::Asciidoc, adoc.as_str()),
        ("lint 5k lines (mdx)", Format::Mdx, mdx.as_str()),
        ("lint 5k lines (typst)", Format::Typst, typst.as_str()),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| lint::document(&set, format, hint::black_box(text), Options::default()))
        });
    }

    let compiled = RuleSet::compile(with_checks());
    assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
    let checked = compiled.rules;
    c.bench_function("lint 5k lines (checks)", |b| {
        b.iter(|| {
            lint::document(
                &checked,
                Format::Markdown,
                hint::black_box(docs.as_str()),
                Options::default(),
            )
        })
    });

    let compiled = RuleSet::compile(five_hundred_rules());
    assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
    let many = compiled.rules;
    c.bench_function("lint 5k lines (500 rules)", |b| {
        b.iter(|| {
            lint::document(
                &many,
                Format::Markdown,
                hint::black_box(XLARGE),
                Options::default(),
            )
        })
    });
}

/// A whole run's work on a 2 KB file, less process start and I/O.
fn compile_and_lint(c: &mut Criterion) {
    let sample = two_kb(MEDIUM);
    c.bench_function("compile + lint 2 KB", |b| {
        b.iter(|| {
            let set = RuleSet::compile(rules::builtin()).rules;
            lint::document(
                &set,
                Format::Markdown,
                hint::black_box(sample),
                Options::default(),
            )
        })
    });
}

criterion_group!(benches, compile, lint_5k_lines, compile_and_lint);
criterion_main!(benches);
