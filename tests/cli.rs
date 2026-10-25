//! Black-box tests of checking and rules: argv in; exit status, stdout and
//! stderr out. Config, ignores and migration are in `config.rs`.

#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::Path;
use std::process::Stdio;

use common::{CLEAN, DIRTY, Env, Run, files, run};
use proserunner::scan::DOCUMENTS;
use serde_json::Value;

#[test]
fn no_issues_exits_0() {
    let env = Env::new();
    env.write("clean.md", CLEAN);
    let r = run(env.cmd().arg("clean.md"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "");
    assert_eq!(r.stderr, "Checked 1 file and found no issues.\n");
}

#[test]
fn issues_exit_1_with_results_on_stdout_and_summary_on_stderr() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().arg("dirty.md"));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stdout.starts_with("\ndirty.md\n[1]"), "{}", r.stdout);
    assert_eq!(r.stderr, "Checked 1 file and found 4 issues.\n");
    assert!(!r.stdout.contains("Checked"));
}

#[test]
fn the_summary_breaks_issues_down_by_level() {
    let env = Env::new();
    // An error (a repeated word), a warning and a hint.
    env.write("a.md", "We utilize the the plan, perhaps.\n");
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "Checked 1 file and found 3 issues: 1 error, 1 warning and 1 hint.\n"
    );
    assert!(r.stdout.contains("  error    \"the the\""), "{}", r.stdout);
    let r = run(env.cmd().args(["a.md", "--fail-on", "error", "-q"]));
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "");
}

#[test]
fn a_long_report_names_the_rules_that_found_the_most() {
    let env = Env::new();
    let mut text = "We utilize it, perhaps.\n\n".repeat(12);
    text.push_str("Very good.\n");
    env.write("a.md", &text);
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(
        r.stderr,
        "Checked 1 file and found 25 issues: 12 warnings and 13 hints.\n\
         The rules that found the most:\n  \
         12  default/hedging\n  \
         12  default/needless-variants\n   \
         1  default/overused-adverbs\n\
         'proserunner explain RULE' shows what a rule matches and how to turn it off.\n"
    );
    // A short report doesn't.
    env.write("b.md", "We utilize it.\n");
    let r = run(env.cmd().arg("b.md"));
    assert_eq!(r.stderr, "Checked 1 file and found 1 issue.\n");

    // With several files, it names the files too, since one that isn't
    // prose (test data, say) can be most of a first run.
    env.write("data/grid.txt", &"0 0 0 0\n\n".repeat(30));
    let r = run(env.cmd().args(["a.md", "b.md", "data"]));
    assert_eq!(
        r.stderr,
        "Checked 3 files and found 86 issues: 60 errors, 13 warnings and 13 hints.\n\
         The rules that found the most:\n  \
         60  default/repetition\n  \
         13  default/needless-variants\n  \
         12  default/hedging\n   \
         1  default/overused-adverbs\n\
         The files with the most:\n  \
         60  data/grid.txt\n  \
         25  a.md\n   \
         1  b.md\n\
         'proserunner explain RULE' shows what a rule matches and how to turn it off.\n\
         A file that isn't prose can go in .proserunnerignore, or in check.exclude in config.\n"
    );
}

#[test]
fn group_output_is_styled_only_where_color_is_wanted() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let plain = run(env.cmd().arg("dirty.md"));
    assert!(!plain.stdout.contains('\u{1b}'), "{}", plain.stdout);
    let forced = run(env.cmd().arg("dirty.md").env("CLICOLOR_FORCE", "1"));
    assert!(
        forced.stdout.contains("\u{1b}[1m\"utilize\"\u{1b}[0m"),
        "{}",
        forced.stdout
    );
    let off = run(env
        .cmd()
        .arg("dirty.md")
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1"));
    assert_eq!(off.stdout, plain.stdout);
    // Formats for programs are never styled.
    let json = run(env
        .cmd()
        .args(["dirty.md", "-o", "json"])
        .env("CLICOLOR_FORCE", "1"));
    assert!(!json.stdout.contains('\u{1b}'), "{}", json.stdout);
}

#[test]
fn a_directory_with_nothing_to_check_says_why() {
    let env = Env::new();
    env.write("docs/a.rst", "Hopefully.\n");
    env.write("docs/b.TEX", "Hopefully.\n");
    env.write("docs/Makefile", "all:\n");
    let r = run(env.cmd().arg("docs"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "proserunner: docs: no files to check; proserunner doesn't read .rst and .tex files (config's [formats] can map an extension to a format it reads)\n\
         Checked no files.\n"
    );
    fs::create_dir(env.path("empty")).unwrap();
    let r = run(env.cmd().arg("empty"));
    assert_eq!(
        r.stderr,
        "proserunner: empty: no files to check\nChecked no files.\n"
    );
}

#[test]
fn check_is_optional() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let a = run(env.cmd().args(["check", "dirty.md"]));
    let b = run(env.cmd().arg("dirty.md"));
    assert_eq!(a.code, 1);
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn global_flags_go_before_a_command_and_check_flags_dont() {
    let env = Env::new();
    env.write("team.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["-q", "rules"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout.lines().count(), 22);
    let r = run(env
        .cmd()
        .args(["--config", "team.toml", "explain", "hedging"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.starts_with("default/hedging"), "{}", r.stdout);

    // A word after a path is a path.
    let r = run(env.cmd().args(["dirty.md", "rules"]));
    assert_eq!(r.code, 2);
    assert_eq!(r.stderr, "proserunner: rules: no such file or directory\n");
    let r = run(env.cmd().args(["-o", "json", "rules"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .contains("'--output <OUTPUT>' can't come before the subcommand 'rules'"),
        "{}",
        r.stderr
    );
}

#[test]
fn a_file_named_two_ways_is_checked_once() {
    let env = Env::new();
    env.write("docs/a.md", DIRTY);
    let once = run(env.cmd().args(["docs/a.md", "-o", "plain"]));
    let r = run(env
        .cmd()
        .args([".", "docs/a.md", "./docs/a.md", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(r.stdout, once.stdout);
}

#[test]
fn group_output() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().arg("dirty.md"));
    // Columns line up across every row, and each row ends with the id
    // that config and directives take.
    assert_eq!(
        r.stdout,
        "\ndirty.md\n\
         [1]  1:4   warning  \"utilize\"              -> Prefer: use.                  default/needless-variants\n\
         [2]  1:12  warning  \"synergy\"              -> Avoid corporate-speak.        default/corporate-speak\n\
         [3]  3:11  warning  \"due to the fact that\" -> Prefer: because, since, why.  default/compression\n\
         [4]  4:17  warning  \"per your request\"     -> Jargon: say it plainly.       default/jargon\n"
    );
}

#[test]
fn plain_output_is_one_issue_per_line() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["dirty.md", "-o", "plain"]));
    let first = r.stdout.lines().next().unwrap();
    assert!(
        first.starts_with("dirty.md:1:4: [1] \"utilize\" -> "),
        "{first}"
    );
    assert!(
        r.stdout.contains(
            "dirty.md:3:11: [3] \"due to the fact that\" -> Prefer: because, since, why. (default/compression)\n"
        )
    );
    assert_eq!(r.stderr, "", "plain output has no summary line");
}

#[test]
fn json_output() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["dirty.md", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(v["version"], 1);
    let wrapped = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["match"] == "due to the fact that")
        .unwrap();
    assert_eq!(wrapped["rule"], "default/compression");
    assert_eq!(
        (wrapped["line"].as_u64(), wrapped["column"].as_u64()),
        (Some(3), Some(11))
    );
    assert_eq!(
        (wrapped["end_line"].as_u64(), wrapped["end_column"].as_u64()),
        (Some(4), Some(5))
    );
    let start = wrapped["byte_start"].as_u64().unwrap() as usize;
    let end = wrapped["byte_end"].as_u64().unwrap() as usize;
    assert_eq!(&DIRTY[start..end], "due to the fact\nthat");
    assert_eq!(
        wrapped["suggestions"],
        serde_json::json!(["because", "since", "why"])
    );
    assert_eq!(wrapped["level"], "warning");
}

#[test]
fn front_matter_issues_point_at_the_source() {
    let env = Env::new();
    // The title is the document's title, so heading rules check it, and
    // the description's spans count its escape's source bytes.
    let text = "---\ntitle: A guide.\ndescription: \"Caf\\u00E9 notes, in order to start.\"\nlayout: utilize\n---\n\nText.\n";
    env.write("a.md", text);
    let r = run(env.cmd().args(["a.md", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    let found: Vec<(&str, &str)> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let start = i["byte_start"].as_u64().unwrap() as usize;
            let end = i["byte_end"].as_u64().unwrap() as usize;
            (i["rule"].as_str().unwrap(), &text[start..end])
        })
        .collect();
    assert_eq!(
        found,
        [
            ("default/heading-periods", "."),
            ("default/compression", "in order to")
        ]
    );
    let r = run(env
        .cmd()
        .args(["--stdin-format", "mdx", "-o", "plain"])
        .write_stdin(text));
    assert_eq!(matched(&r), [".", "in order to"]);
}

#[test]
fn quiet_drops_the_summary() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["dirty.md", "-q"]));
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "");
}

#[test]
fn stdin_is_labeled() {
    let env = Env::new();
    let r = run(env.cmd().args(["-", "-o", "plain"]).write_stdin(DIRTY));
    assert_eq!(r.code, 1);
    assert!(r.stdout.starts_with("<stdin>:1:4: [1]"), "{}", r.stdout);
}

#[test]
fn stdin_must_be_text_under_the_size_limit() {
    let env = Env::new();
    let r = run(env.cmd().arg("-").write_stdin(&b"caf\xe9\n"[..]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("<stdin>: not UTF-8 text"), "{}", r.stderr);

    // A character that crosses the limit makes it too large, rather than
    // cut short and not text.
    let mut text = "a".repeat(10_000_000);
    text.push('é');
    let r = run(env.cmd().arg("-").write_stdin(text));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("<stdin>: files must be under 10 MB"),
        "{}",
        r.stderr
    );
}

/// What each issue in a plain run matched, in order.
fn matched(r: &Run) -> Vec<&str> {
    r.stdout
        .lines()
        .filter_map(|l| l.split('"').nth(1))
        .collect()
}

#[test]
fn stdin_format_chooses_the_scanner() {
    let env = Env::new();
    // Org's `=verbatim=` is a placeholder; Markdown has no such markup.
    let text = "We =utilize= synergy.\n";
    let check = |args: &[&str]| run(env.cmd().args(args).args(["-o", "plain"]).write_stdin(text));
    assert_eq!(matched(&check(&["-"])), ["utilize", "synergy"]);
    assert_eq!(
        matched(&check(&["-", "--stdin-format", "org"])),
        ["synergy"]
    );
    // With no paths, a standard-input flag reads standard input.
    let r = check(&["--stdin-format", "org"]);
    assert_eq!(matched(&r), ["synergy"]);
    assert!(r.stdout.starts_with("<stdin>:1:14: [1]"), "{}", r.stdout);
}

#[test]
fn stdin_filename_names_issues_and_chooses_the_format() {
    let env = Env::new();
    let text = "We =utilize= synergy.\n";
    let r = run(env
        .cmd()
        .args(["--stdin-filename", "notes/plan.org", "-o", "plain"])
        .write_stdin(text));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stdout.starts_with("notes/plan.org:1:14: [1] \"synergy\""),
        "{}",
        r.stdout
    );
    // `--stdin-format` wins over the name.
    let r = run(env
        .cmd()
        .args(["-", "--stdin-filename", "notes/plan.org"])
        .args(["--stdin-format", "markdown", "-o", "plain"])
        .write_stdin(text));
    assert_eq!(matched(&r), ["utilize", "synergy"]);
    assert!(
        r.stdout.starts_with("notes/plan.org:1:5: [1]"),
        "{}",
        r.stdout
    );
}

#[test]
fn stdin_flags_need_standard_input_and_a_format() {
    let env = Env::new();
    env.write("a.md", CLEAN);
    let r = run(env.cmd().args(["a.md", "--stdin-format", "org"]));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: --stdin-format is for standard input; add - to the paths to check it with them\n"
    );
    let r = run(env
        .cmd()
        .args(["--stdin-filename", "notes.docx"])
        .write_stdin(CLEAN));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .starts_with("proserunner: --stdin-filename: notes.docx: unsupported file type")
            && r.stderr.ends_with("; --stdin-format chooses one\n"),
        "{}",
        r.stderr
    );
    let r = run(env.cmd().args(["--stdin-format", "docx"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .contains("[possible values: markdown, mdx, org, text, html, asciidoc, typst]"),
        "{}",
        r.stderr
    );
}

#[test]
fn stdin_format_help_names_every_document_format() {
    let env = Env::new();
    let r = run(env.cmd().args(["check", "--help"]));
    let help = r
        .stdout
        .split("--stdin-format <FORMAT>")
        .nth(1)
        .unwrap_or("");
    let line = help.lines().nth(1).unwrap_or("");
    for format in DOCUMENTS {
        assert!(
            line.contains(format.name),
            "{} isn't in {line}",
            format.name
        );
    }
}

#[test]
fn missing_file_is_an_error() {
    let env = Env::new();
    let r = run(env.cmd().arg("missing.md"));
    assert_eq!(r.code, 2);
    assert_eq!(r.stdout, "");
    assert_eq!(
        r.stderr,
        "proserunner: missing.md: no such file or directory\n"
    );
}

#[test]
fn unsupported_file_is_an_error() {
    let env = Env::new();
    env.write("main.rs", "fn main() {}");
    let r = run(env.cmd().arg("main.rs"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("unsupported file type"), "{}", r.stderr);
}

#[test]
fn bad_flags_exit_2() {
    let env = Env::new();
    let r = run(env.cmd().args(["--frobnicate", "x.md"]));
    assert_eq!(r.code, 2);
    assert_eq!(r.stdout, "");
    assert!(r.stderr.contains("--frobnicate"), "{}", r.stderr);
}

#[test]
fn no_arguments_print_help_and_exit_2() {
    let env = Env::new();
    let r = run(&mut env.cmd());
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("Usage: proserunner"), "{}", r.stderr);
    assert!(r.stderr.contains("migrate"), "{}", r.stderr);

    // Options without paths are the same mistake as `check` without them.
    for args in [&["-o", "json"][..], &["check", "-o", "json"]] {
        let r = run(env.cmd().args(args));
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.stderr.contains("nothing to check"), "{}", r.stderr);
    }
}

#[test]
fn version() {
    let env = Env::new();
    let r = run(env.cmd().arg("--version"));
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with("proserunner 1."), "{}", r.stdout);
}

#[test]
fn directories_are_walked_in_order_with_ignores() {
    let env = Env::new();
    env.write("docs/b.md", DIRTY);
    env.write("docs/a.txt", DIRTY);
    env.write("docs/drafts/c.md", DIRTY);
    env.write("docs/old.backup.md", DIRTY);
    env.write("docs/.proserunnerignore", "drafts/\n");
    let r = run(env.cmd().args(["docs", "-o", "plain", "-e", "*.backup.md"]));
    let files: Vec<&str> = r
        .stdout
        .lines()
        .map(|l| l.split(':').next().unwrap())
        .collect();
    let mut unique = files.clone();
    unique.dedup();
    assert_eq!(unique, ["docs/a.txt", "docs/b.md"]);
}

#[test]
fn quoted_text_and_code_are_skipped_unless_asked() {
    let env = Env::new();
    env.write(
        "q.md",
        "She said \"we utilize it\" here.\n\n```\nutilize\n```\n",
    );
    assert_eq!(run(env.cmd().arg("q.md")).code, 0);
    let r = run(env
        .cmd()
        .args(["q.md", "--quoted-text", "--code-blocks", "-o", "plain"]));
    assert_eq!(r.code, 1);
    assert!(r.stdout.contains("q.md:1:14:"), "{}", r.stdout);
    assert!(r.stdout.contains("q.md:4:1:"), "{}", r.stdout);
}

#[test]
fn custom_rules() {
    let env = Env::new();
    env.write(
        "style/house.toml",
        r#"schema = 1
message = "House style: write “{suggestion}”."
words = [{ match = "e-mail", suggest = ["email"] }]
[examples]
bad = ["Send an e-mail."]
"#,
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"style\"]\n",
    );
    env.write("doc.md", "Send an e-mail today.\n");
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "doc.md:1:9: [1] \"e-mail\" -> House style: write “email”. (style/house)\n"
    );
}

#[test]
fn terms_groups_and_entry_levels() {
    let env = Env::new();
    env.write(
        "style/house.toml",
        r#"schema = 1
message = "Write “{suggestion}”."
terms = [{ term = "GitHub", level = "hint" }]
patterns = [{ match = '(\d+) ?per ?cent', suggest = ["{1}%"], level = "error" }]
[examples]
bad = ["On Github."]
good = ["On GitHub, at github.com."]
fix = [["GITHUB is 5 per cent off.", "GitHub is 5% off."]]
"#,
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"style\"]\n",
    );
    env.write("doc.md", "See github.com: Github is 20 percent off.\n");
    let r = run(env.cmd().args(["doc.md", "-o", "json"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    let issues: Vec<(&str, &str, &str)> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["match"].as_str().unwrap(),
                i["level"].as_str().unwrap(),
                i["message"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        issues,
        [
            ("Github", "hint", "Write “GitHub”."),
            ("20 percent", "error", "Write “20%”.")
        ]
    );
    assert_eq!(v["issues"][1]["suggestions"], serde_json::json!(["20%"]));

    env.write("doc.md", "On Github.\n");
    let r = run(env.cmd().args(["doc.md", "--fail-on", "warning"]));
    assert_eq!(r.code, 0, "a hint doesn't fail the run: {}", r.stdout);
    let r = run(env.cmd().args(["rules", "test", "style"]));
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
}

#[test]
fn checks_find_what_words_cant() {
    let env = Env::new();
    env.write(
        "style/consistency.toml",
        r#"schema = 1
consistent = [["advisor{,s}", "adviser{,s}"]]
acronyms = { except = ["HTML"] }
[examples]
bad = ["An advisor and an adviser.", "The API."]
good = ["The application programming interface (API) and HTML."]
fix = [["Advisors and an adviser.", "Advisors and an advisor."]]
"#,
    );
    env.write(
        "style/headings.toml",
        r####"schema = 1
scope = ["heading"]
heading-levels = [1, 2]
capitalization = { style = "sentence", except = ["Visual Studio Code"] }
limit = { max = 6, per = "block" }
[examples]
bad = ["## Getting Started"]
good = ["### Getting Started", "## Visual Studio Code setup"]
fix = [["# Getting Started", "# Getting started"]]
"####,
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"style\"]\naccept = [\"Proserunner\"]\n",
    );
    env.write(
        "doc.md",
        "# Using Proserunner With the API\n\nAn advisor read the application programming interface (API) docs.\n\n## A heading that goes on for far too long\n\nThe advisers agreed.\n",
    );
    let r = run(env.cmd().args(["doc.md", "-o", "json"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    let issues: Vec<(String, &str, &str)> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                format!("{}:{}", i["line"], i["column"]),
                i["match"].as_str().unwrap(),
                i["message"].as_str().unwrap(),
            )
        })
        .collect();
    // "Proserunner" is accepted, and "API" is defined only after its
    // first use.
    assert_eq!(
        issues,
        [
            ("1:21".into(), "With", "Use sentence case: “with”."),
            (
                "1:30".into(),
                "API",
                "Define “API” where it's first used, as in “Full Name (API)”."
            ),
            (
                "5:4".into(),
                "A heading that goes on for far too long",
                "9 words; keep it to 6 or fewer."
            ),
            (
                "7:5".into(),
                "advisers",
                "Prefer “advisors”, which this document uses first."
            ),
        ]
    );
    let r = run(env.cmd().args(["rules", "test", "style"]));
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);

    // Inline code in a long match reads as "…", and JSON keeps what the
    // rules saw.
    env.write(
        "long.md",
        "# Notes\n\nRun `make` and then, after it, run `make test`.\n",
    );
    env.write(
        "style/long.toml",
        "schema = 1\nlimit = { max = 5 }\n[examples]\nbad = [\"A b c d e f.\"]\n",
    );
    let r = run(env.cmd().args(["long.md", "-o", "plain"]));
    assert_eq!(
        r.stdout,
        "long.md:3:1: [1] \"Run … and then, after it, run ….\" -> 6 words; keep it to 5 or fewer. (style/long)\n"
    );
    let r = run(env.cmd().args(["long.md", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(
        v["issues"][0]["match"],
        "Run \u{1a} and then, after it, run \u{1a}."
    );
}

#[test]
fn broken_rules_warn_the_rest_still_run_and_the_exit_is_2() {
    let env = Env::new();
    env.write(
        "style/broken.toml",
        "schema = 1\nwords = [\"a\"]\nmesage = \"typo\"\n",
    );
    env.write(
        "style/ok.toml",
        "schema = 1\nmessage = \"No.\"\nwords = [\"synergy\"]\n",
    );
    // Config and directives that name the broken rule aren't typos.
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"style\"]\n[rule.\"style/broken\"]\nlevel = \"error\"\n",
    );
    env.write(
        "doc.md",
        "<!-- proserunner-disable style/broken -->\nSynergy.\n",
    );
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("style/broken.toml"), "{}", r.stderr);
    assert!(r.stderr.contains("unknown field `mesage`"), "{}", r.stderr);
    assert!(r.stderr.contains("1 rule file didn't load"), "{}", r.stderr);
    assert!(r.stdout.contains("\"Synergy\" -> No."), "{}", r.stdout);

    // Ignoring what the rest found is still a run with an error.
    let r = run(env.cmd().args(["doc.md", "--ignore-all"]));
    assert_eq!(r.code, 2, "{}", r.stderr);
    assert!(r.stderr.contains("1 rule file didn't load"), "{}", r.stderr);

    let r = run(env.cmd().args(["rules", "test"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
}

#[test]
fn rules_list_and_test() {
    let env = Env::new();
    let r = run(env.cmd().arg("rules"));
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout.lines().count(), 22);
    assert!(r.stdout.contains("default/cliches"), "{}", r.stdout);

    let r = run(env.cmd().args(["rules", "test"]));
    assert_eq!(r.code, 0, "{}", r.stdout);
    assert_eq!(r.stderr, "22 rules; every example passes.\n");
}

#[test]
fn rules_test_reports_failures() {
    let env = Env::new();
    env.write(
        "style/x.toml",
        "schema = 1\nmessage = \"m\"\nwords = [\"a\"]\n[examples]\nbad = [\"b\"]\n",
    );
    let r = run(env.cmd().args(["rules", "test", "style"]));
    assert_eq!(r.code, 1);
    assert!(
        r.stdout
            .contains("style/x: bad example \"b\" got no issues"),
        "{}",
        r.stdout
    );
}

#[test]
fn rules_export_writes_the_builtins_once() {
    let env = Env::new();
    let r = run(env.cmd().args(["rules", "export", "out"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(fs::read_dir(env.path("out")).unwrap().count(), 22);
    // Exported rules are ordinary rule files.
    let r = run(env.cmd().args(["rules", "test", "out"]));
    assert_eq!(r.code, 0, "{}", r.stdout);
    assert_eq!(r.stderr, "22 rules; every example passes.\n");
    let r = run(env.cmd().args(["rules", "export", "out"]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("not overwriting"), "{}", r.stderr);
}

#[test]
fn migrate_rules_converts_08_checks() {
    let env = Env::new();
    env.write(
        "old/hedging.edn",
        r#"{:name "hedging" :kind "existence" :message "Omit this phrase"
            :explanation "Say, or say not." :specimens ["I think" "thought leaders?"]}"#,
    );
    let r = run(env.cmd().args(["migrate", "rules", "old", "new"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    let toml = fs::read_to_string(env.path("new/hedging.toml")).unwrap();
    assert!(toml.contains("\"thought leaders\""), "{toml}");
    env.write("doc.md", "I think so.\n");
    env.write("c.toml", "schema = 1\nrules = [\"new\"]\n");
    let r = run(env
        .cmd()
        .args(["doc.md", "-o", "plain", "--config", "c.toml"]));
    assert_eq!(
        r.stdout,
        "doc.md:1:1: [1] \"I think\" -> Omit this phrase. (new/hedging)\n"
    );
}

#[test]
fn output_is_the_same_with_one_thread() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
    // `benchmark-data` is ignored inside `resources`, so name it too.
    let paths = [corpus.clone(), corpus.join("benchmark-data")];
    let env = Env::new();
    let many = run(env.cmd().args(&paths).args(["-o", "json"]));
    let one = run(env
        .cmd()
        .args(&paths)
        .args(["-o", "json"])
        .env("RAYON_NUM_THREADS", "1"));
    assert_eq!(many.code, 1);
    assert_eq!(many.stdout, one.stdout);
}

#[test]
fn table_output_is_a_markdown_table() {
    let env = Env::new();
    env.write("t.md", "We utilize it | twice.\n");
    let r = run(env.cmd().args(["t.md", "-o", "table"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "|   # | File | Line | Col | Match   | Message      | Rule                      | Level   |\n\
         | --: | ---- | ---: | --: | ------- | ------------ | ------------------------- | ------- |\n\
         |   1 | t.md |    1 |   4 | utilize | Prefer: use. | default/needless-variants | warning |\n"
    );
    assert_eq!(r.stderr, "Checked 1 file and found 1 issue.\n");
}

#[test]
fn github_output_is_workflow_annotations() {
    let env = Env::new();
    env.write("docs/a.md", DIRTY);
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"error\"\n[rule.\"default/corporate-speak\"]\nlevel = \"hint\"\n",
    );
    let r = run(env.cmd().args(["docs", "-o", "github"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let lines: Vec<&str> = r.stdout.lines().collect();
    assert_eq!(
        lines[0],
        "::warning file=docs/a.md,line=1,endLine=1,col=4,endColumn=11,\
         title=Needless variant (default/needless-variants)::\"utilize\" -> Prefer: use."
    );
    assert!(
        lines[1].starts_with("::notice file=docs/a.md,"),
        "{}",
        lines[1]
    );
    // A match across lines has no columns: GitHub reads them only within
    // one line.
    assert_eq!(
        lines[2],
        "::warning file=docs/a.md,line=3,endLine=4,title=Compression (default/compression)::\
         \"due to the fact that\" -> Prefer: because, since, why."
    );
    assert_eq!(
        lines[3],
        "::error file=docs/a.md,line=4,endLine=4,col=17,endColumn=33,title=Jargon (default/jargon)::\
         \"per your request\" -> Jargon: say it plainly."
    );
    assert_eq!(r.stderr, "", "annotations have no summary line");
}

#[test]
fn checkstyle_output_lists_every_file_checked() {
    let env = Env::new();
    env.write("docs/a&b.md", DIRTY);
    env.write("docs/clean.md", CLEAN);
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"default/corporate-speak\"]\nlevel = \"hint\"\n",
    );
    let r = run(env.cmd().args(["docs", "-o", "checkstyle"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <checkstyle version=\"4.3\">\n  \
         <file name=\"docs/a&amp;b.md\">\n    \
         <error line=\"1\" column=\"4\" severity=\"warning\" message=\"&quot;utilize&quot; -&gt; Prefer: use.\" source=\"default/needless-variants\"/>\n    \
         <error line=\"1\" column=\"12\" severity=\"info\" message=\"&quot;synergy&quot; -&gt; Avoid corporate-speak.\" source=\"default/corporate-speak\"/>\n    \
         <error line=\"3\" column=\"11\" severity=\"warning\" message=\"&quot;due to the fact that&quot; -&gt; Prefer: because, since, why.\" source=\"default/compression\"/>\n    \
         <error line=\"4\" column=\"17\" severity=\"warning\" message=\"&quot;per your request&quot; -&gt; Jargon: say it plainly.\" source=\"default/jargon\"/>\n  \
         </file>\n  \
         <file name=\"docs/clean.md\"/>\n\
         </checkstyle>\n"
    );
    assert_eq!(r.stderr, "", "checkstyle has no summary line");

    // A clean run still lists what it checked, and standard input is named.
    let r = run(env.cmd().args(["-", "-o", "checkstyle"]).write_stdin(CLEAN));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.contains("  <file name=\"&lt;stdin&gt;\"/>\n"),
        "{}",
        r.stdout
    );
}

#[test]
fn verbose_output_is_a_markdown_report() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["dirty.md", "-o", "verbose"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let abs = env.path("dirty.md").display().to_string();
    assert!(
        r.stdout
            .starts_with("# Proserunner results\n\n4 issues in 1 file.\n\n"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout.contains(&format!(
            "## 3. Compression (`default/compression`)\n\n`{abs}:3:11` · warning\n\n\
             > It failed due to the fact\n> that it rained, per your request.\n\n\
             **Problem:** `due to the fact that`: Prefer: because, since, why.\n\n\
             **Suggestions:** `because`, `since`, `why`.\n"
        )),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .contains("- **By file:**\n  - `dirty.md`: 4 issues\n")
    );
    assert_eq!(r.stderr, "", "the report has its own summary");

    env.write("clean.md", CLEAN);
    let r = run(env.cmd().args(["clean.md", "-o", "verbose"]));
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout, "# Proserunner results\n\nNo issues found.\n");
}

#[test]
fn sarif_output() {
    let env = Env::new();
    env.write("docs/a b.md", "# A 𝒜 title.\n\nWe utilize it.\n");
    let r = run(env.cmd().args(["docs", "-o", "sarif"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(r.stderr, "");
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(v["version"], "2.1.0");
    let run0 = &v["runs"][0];
    assert_eq!(run0["tool"]["driver"]["name"], "proserunner");
    let rules = run0["tool"]["driver"]["rules"].as_array().unwrap();
    let ids: Vec<&str> = rules.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["default/heading-periods", "default/needless-variants"]
    );

    let results = run0["results"].as_array().unwrap();
    let period = &results[0];
    assert_eq!(period["ruleId"], "default/heading-periods");
    assert_eq!(period["ruleIndex"], 0);
    assert_eq!(period["level"], "warning");
    let location = &period["locations"][0]["physicalLocation"];
    assert_eq!(location["artifactLocation"]["uri"], "docs/a%20b.md");
    assert_eq!(location["artifactLocation"]["uriBaseId"], "%SRCROOT%");
    // UTF-16 columns: 𝒜 takes two.
    assert_eq!(location["region"]["startColumn"], 13);
    assert_eq!(location["region"]["endColumn"], 14);
    let root = run0["originalUriBaseIds"]["%SRCROOT%"]["uri"]
        .as_str()
        .unwrap();
    assert!(
        root.starts_with("file:///") && root.ends_with('/'),
        "{root}"
    );

    env.write("clean.md", CLEAN);
    let r = run(env.cmd().args(["clean.md", "-o", "sarif"]));
    assert_eq!(r.code, 0);
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(v["runs"][0]["results"], serde_json::json!([]));
}

#[test]
fn config_can_choose_any_format() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[check]\noutput = \"github\"\n",
    );
    let r = run(env.cmd().arg("dirty.md"));
    assert!(r.stdout.starts_with("::warning "), "{}", r.stdout);
}

#[test]
fn explain_shows_a_rule_and_what_changes_it() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        "schema = 1\naccept = [\"Kubernetes\"]\n[rule.\"default/hedging\"]\nlevel = \"error\"\n",
    );
    run(env.cmd().args(["ignore", "add", "hopefully"]));
    let r = run(env.cmd().args(["explain", "hedging"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.starts_with(
            "default/hedging: Hedging\n\n\
             Level    error (set in .proserunner/config.toml)\n\
             Source   built in\n"
        ),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .contains("\nEntries\n  \"I think\"\n      literal, whole word\n")
    );
    assert!(r.stdout.contains("\nExamples\n  bad   "), "{}", r.stdout);
    assert!(
        r.stdout
            .contains("\"Kubernetes\"  accept, .proserunner/config.toml\n"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .contains("\"hopefully\"   ignore list, .proserunner/ignores.toml\n"),
        "{}",
        r.stdout
    );
    assert_eq!(
        r.stdout,
        run(env.cmd().args(["explain", "default/hedging"])).stdout
    );

    let r = run(env.cmd().args(["explain", "hedgin"]));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: no rule has the id `hedgin` (did you mean `default/hedging`?)\n"
    );
}

#[test]
fn help_has_examples_and_every_command() {
    let env = Env::new();
    let r = run(env.cmd().arg("--help"));
    assert_eq!(r.code, 0);
    for command in [
        "check", "explain", "ignore", "rules", "config", "init", "migrate",
    ] {
        assert!(
            r.stdout.contains(&format!("\n  {command} ")),
            "{command}: {}",
            r.stdout
        );
    }
    for heading in ["Input:", "What to check:", "Output:", "Ignoring:"] {
        assert!(r.stdout.contains(&format!("\n{heading}\n")), "{heading}");
    }
    assert!(r.stdout.contains("Examples:\n"), "{}", r.stdout);
    assert!(r.stdout.contains("Exit status: 0"), "{}", r.stdout);
    assert!(r.stdout.contains("sarif"), "{}", r.stdout);
    let r = run(env.cmd().args(["help", "explain"]));
    assert_eq!(r.code, 0);
    assert!(
        r.stdout
            .contains("Usage: proserunner explain [OPTIONS] <RULE>"),
        "{}",
        r.stdout
    );
}

#[test]
fn org_is_read_natively() {
    let env = Env::new();
    env.write(
        "notes.org",
        "#+TITLE: Notes\n* TODO Utilize the plan :work:\nWe use =utilize()= and [[https://x/utilize][the docs]].\n#+BEGIN_SRC sh\nutilize\n#+END_SRC\nWe utilize it.\n",
    );
    let r = run(env.cmd().args([".", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    // Keywords, tags, verbatim, link targets and code are skipped.
    assert_eq!(
        r.stdout,
        "notes.org:2:8: [1] \"Utilize\" -> Prefer: use. (default/needless-variants)\n\
         notes.org:7:4: [2] \"utilize\" -> Prefer: use. (default/needless-variants)\n"
    );
}

#[test]
fn inline_control_comments() {
    let env = Env::new();
    env.write(
        "a.md",
        "<!-- proserunner-disable-next-line -->\nWe utilize synergy.\n\nWe utilize synergy.\n\n<!-- proserunner-disable default/needless-variants -- house style -->\nWe utilize synergy.\n",
    );
    let r = run(env.cmd().args(["a.md", "-o", "plain"]));
    let found: Vec<&str> = r
        .stdout
        .lines()
        .map(|l| l.split(" -> ").next().unwrap())
        .collect();
    assert_eq!(
        found,
        [
            "a.md:4:4: [1] \"utilize\"",
            "a.md:4:12: [2] \"synergy\"",
            "a.md:7:12: [3] \"synergy\""
        ]
    );
}

#[test]
fn a_directive_naming_no_rule_is_an_error() {
    let env = Env::new();
    env.write(
        "b.md",
        "Text.\n<!-- proserunner-disable default/hedgin -->\n",
    );
    let r = run(env.cmd().arg("b.md"));
    assert_eq!(r.code, 2);
    assert_eq!(r.stdout, "");
    assert_eq!(
        r.stderr,
        "proserunner: b.md:2:26: no rule has the id `default/hedgin` (did you mean `default/hedging`?)\n"
    );
    env.write("c.org", "# proserunner-disabel\n");
    let r = run(env.cmd().arg("c.org"));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: c.org:1:3: `proserunner-disabel` isn't a directive (did you mean `proserunner-disable`?)\n"
    );
}

#[test]
fn a_reader_that_goes_away_is_not_an_error() {
    let env = Env::new();
    env.write("doc.md", DIRTY);
    let commands: [&[&str]; 5] = [
        &["rules"],
        &["config"],
        &["explain", "default/jargon"],
        &["doc.md", "-o", "plain"],
        &["ignore", "list"],
    ];
    for args in commands {
        let mut child = env
            .process()
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
        assert!(
            matches!(out.status.code(), Some(0 | 1)),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn the_project_is_the_one_around_the_checked_files() {
    let env = Env::new();
    env.write(
        "proj/.proserunner/config.toml",
        "schema = 1\n[rule.\"default/hedging\"]\nlevel = \"off\"\n",
    );
    env.write("proj/docs/a.md", "Perhaps.\n");
    env.write("other/.proserunner/config.toml", "schema = 1\n");
    env.write("other/b.md", "Perhaps.\n");

    // From outside, the project's config still applies to its files.
    let r = run(env.cmd().args(["proj/docs/a.md", "-o", "plain"]));
    assert_eq!((r.code, r.stdout.as_str()), (0, ""), "{}", r.stderr);
    let r = run(env.cmd_in("other").args(["../proj/docs", "-o", "plain"]));
    assert_eq!((r.code, r.stdout.as_str()), (0, ""), "{}", r.stderr);

    // One run has one config.
    let r = run(env.cmd().args(["proj/docs/a.md", "other/b.md"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("check each project on its own"),
        "{}",
        r.stderr
    );
}

#[test]
fn force_exclude_applies_excludes_and_ignore_files_to_named_files() {
    let env = Env::new();
    env.write(".proserunnerignore", "drafts/\n");
    env.write("drafts/a.md", DIRTY);
    env.write("b.tmp.md", DIRTY);
    env.write("c.md", DIRTY);
    let args = [
        "drafts/a.md",
        "b.tmp.md",
        "c.md",
        "-o",
        "plain",
        "-e",
        "*.tmp.md",
    ];

    let r = run(env.cmd().args(args));
    for file in ["drafts/a.md", "b.tmp.md", "c.md"] {
        assert!(r.stdout.contains(file), "{file}: {}", r.stdout);
    }
    let r = run(env.cmd().args(args).arg("--force-exclude"));
    assert!(!r.stdout.contains("drafts/a.md"), "{}", r.stdout);
    assert!(!r.stdout.contains("b.tmp.md"), "{}", r.stdout);
    assert!(r.stdout.contains("c.md"), "{}", r.stdout);
}

#[test]
fn force_exclude_applies_to_the_stdin_filename() {
    let env = Env::new();
    env.write(".proserunnerignore", "drafts/\n");
    let check = |name: &str| {
        run(env
            .cmd()
            .args(["--stdin-filename", name, "--force-exclude", "-o", "plain"])
            .write_stdin(DIRTY))
    };
    let r = check("drafts/a.md");
    assert_eq!((r.code, r.stdout.as_str()), (0, ""), "{}", r.stderr);
    let r = check("docs/a.md");
    assert!(r.stdout.starts_with("docs/a.md:1:4: [1]"), "{}", r.stdout);
}

#[test]
fn force_exclude_applies_git_excludes_to_named_files() {
    let env = Env::new();
    env.write(".git/info/exclude", "local.md\n");
    env.write("home/.config/git/ignore", "global.md\n");
    let files = ["local.md", "global.md", "c.md"];
    for file in files {
        env.write(file, DIRTY);
    }

    let r = run(env.cmd().args(files).args(["-o", "plain"]));
    for file in files {
        assert!(r.stdout.contains(file), "{file}: {}", r.stdout);
    }
    let r = run(env
        .cmd()
        .args(files)
        .args(["-o", "plain", "--force-exclude"]));
    assert!(!r.stdout.contains("local.md"), "{}", r.stdout);
    assert!(!r.stdout.contains("global.md"), "{}", r.stdout);
    assert!(r.stdout.contains("c.md"), "{}", r.stdout);
}

#[test]
fn exclude_patterns_keep_their_commas() {
    let env = Env::new();
    env.write("docs/a.draft.md", DIRTY);
    env.write("docs/b.tmp.md", DIRTY);
    env.write("docs/c.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["docs", "-o", "plain", "-e", "*.{draft,tmp}.md"]));
    assert!(r.stdout.contains("docs/c.md"), "{}", r.stdout);
    assert!(!r.stdout.contains("draft"), "{}", r.stdout);
    assert!(!r.stdout.contains("tmp"), "{}", r.stdout);
}

#[test]
fn flags_turn_config_switches_off() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[check]\nquoted-text = true\n",
    );
    env.write("doc.md", "They said \"we utilize it\" twice.\n");
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert!(r.stdout.contains("utilize"), "{}", r.stdout);
    let r = run(env
        .cmd()
        .args(["doc.md", "-o", "plain", "--no-quoted-text"]));
    assert_eq!(r.stdout, "", "{}", r.stderr);
    // The last of a pair wins.
    let r = run(env
        .cmd()
        .args(["doc.md", "-o", "plain", "--no-quoted-text", "--quoted-text"]));
    assert!(r.stdout.contains("utilize"), "{}", r.stdout);
}

#[test]
fn files_that_arent_utf8_are_skipped_in_directories_and_errors_when_named() {
    let env = Env::new();
    env.write("docs/a.md", DIRTY);
    fs::write(env.path("docs/latin1.md"), b"caf\xe9 synergy\n").unwrap();
    let r = run(env.cmd().args(["docs", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stderr.contains("latin1.md: skipped, not UTF-8 text"),
        "{}",
        r.stderr
    );
    assert!(r.stdout.contains("docs/a.md"), "{}", r.stdout);

    let r = run(env.cmd().args(["docs/latin1.md"]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("not UTF-8 text"), "{}", r.stderr);
}

#[test]
fn a_byte_order_mark_is_not_a_column() {
    let env = Env::new();
    env.write("doc.md", "\u{FEFF}# We utilize it\n\nWe utilize it.\n");
    env.write("doc.org", "\u{FEFF}* We utilize it\n");
    let r = run(env.cmd().args(["doc.md", "doc.org", "-o", "plain"]));
    assert_eq!(
        r.stdout,
        "doc.md:1:6: [1] \"utilize\" -> Prefer: use. (default/needless-variants)\n\
         doc.md:3:4: [2] \"utilize\" -> Prefer: use. (default/needless-variants)\n\
         doc.org:1:6: [3] \"utilize\" -> Prefer: use. (default/needless-variants)\n"
    );
    let r = run(env.cmd().args(["doc.md", "-o", "json"]));
    let json: Value = serde_json::from_str(&r.stdout).unwrap();
    // Byte offsets count the mark, so tools can edit the file.
    assert_eq!(json["issues"][0]["byte_start"], 8);
}

#[test]
fn urls_and_inch_marks_hide_nothing() {
    let env = Env::new();
    env.write(
        "doc.md",
        "See <https://example.com/utilize> and https://example.com/perhaps.\n\n\
         The board is 5\" wide and we utilize it. Then \"perhaps\".\n",
    );
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(
        r.stdout,
        "doc.md:3:29: [1] \"utilize\" -> Prefer: use. (default/needless-variants)\n"
    );
}

#[test]
fn huge_issue_ranges_cost_nothing() {
    let env = Env::new();
    env.write("doc.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["doc.md", "--ignore-issues", "1-99999999999"]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("there's no issue"), "{}", r.stderr);
}

#[test]
fn patterns_that_run_out_of_budget_are_reported() {
    let env = Env::new();
    env.write(
        "r/slow.toml",
        "schema = 1\nmessage = \"Slow.\"\npatterns = ['(a|aa)+\\1b']\n",
    );
    env.write(
        "c.toml",
        "schema = 1\nrules = [\"default\", { path = \"r\", prefix = \"mine\" }]\n",
    );
    let a = "a".repeat(40);
    env.write("doc.md", &format!("We utilize it.\n\n{a}\n\n{a}\n"));
    let r = run(env
        .cmd()
        .args(["doc.md", "-o", "plain", "--config", "c.toml"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "proserunner: doc.md:3:1: `mine/slow` gave up on the pattern `(a|aa)+\\1b` in this block and 1 other block (too much backtracking), so it may have missed issues there\n"
    );
    // Off is off: no issues, and no warning.
    env.write(
        "c.toml",
        "schema = 1\nrules = [\"default\", { path = \"r\", prefix = \"mine\" }]\n[rule.\"mine/slow\"]\nlevel = \"off\"\n",
    );
    let r = run(env
        .cmd()
        .args(["doc.md", "-o", "plain", "--config", "c.toml"]));
    assert_eq!(r.stderr, "");
}

#[test]
fn guide_and_schema_print_what_this_version_reads() {
    let env = Env::new();
    let r = run(env.cmd().arg("guide"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.starts_with(
            "'proserunner guide TOPIC' prints one of these, as Markdown:\n  overview "
        ),
        "{}",
        r.stdout
    );
    for (topic, file) in [
        ("overview", "README.md"),
        ("config", "docs/config.md"),
        ("rules", "docs/rules.md"),
        ("formats", "docs/formats.md"),
        ("upgrading", "docs/upgrading.md"),
    ] {
        let r = run(env.cmd().args(["guide", topic]));
        let page = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
        assert_eq!(r.stdout, page, "{topic}");
    }
    for kind in ["config", "rule", "ignores"] {
        let r = run(env.cmd().args(["schema", kind]));
        assert_eq!(r.code, 0, "{}", r.stderr);
        let schema: Value = serde_json::from_str(&r.stdout).unwrap();
        assert!(
            schema["title"]
                .as_str()
                .unwrap()
                .starts_with("proserunner ")
        );
    }
    let r = run(env.cmd().args(["guide", "nope"]));
    assert_eq!(r.code, 2);
}

#[test]
fn rules_export_copies_some_rules_and_says_how_to_use_them() {
    let env = Env::new();
    let r = run(env.cmd().args(["rules", "export", "mine", "hedging"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "Wrote hedging.toml to mine/.\n\
         To use it in place of the built-in, add the directory to `rules` in config, and turn the built-in off:\n  \
         rules = [\"default\", \"mine\"]\n  \
         [rule.\"default/hedging\"]\n  \
         level = \"off\"\n"
    );
    assert_eq!(files(&env.path("mine")), ["hedging.toml"]);
    let r = run(env.cmd().args(["rules", "export", "mine", "hedgin"]));
    assert_eq!(
        r.stderr,
        "proserunner: no rule has the id `hedgin` (did you mean `default/hedging`?)\n"
    );

    // Both copies on report each issue twice, so a run says so.
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"default\", \"mine\"]\n",
    );
    env.write("doc.md", "I think so.\n");
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(
        r.stderr,
        "proserunner: `mine/hedging` is the same rule as `default/hedging`, so each issue they find is reported twice; turn one off, or take its directory out of `rules`\n"
    );
    assert_eq!(r.stdout.lines().count(), 2);
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"default\", \"mine\"]\n[rule.\"default/hedging\"]\nlevel = \"off\"\n",
    );
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(r.stderr, "");
    assert_eq!(
        r.stdout,
        "doc.md:1:1: [1] \"I think\" -> Omit this phrase. (mine/hedging)\n"
    );

    let r = run(env.cmd().args(["rules", "export", "all"]));
    assert!(
        r.stderr.starts_with(
            "Wrote 22 rules to all/.\nTo use them in place of the built-ins, set `rules = [\"all\"]` in config.\n"
        ),
        "{}",
        r.stderr
    );
}

#[test]
fn rule_flag_keeps_only_some_rules() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let rules = |args: &[&str]| {
        let r = run(env.cmd().arg("dirty.md").args(args).args(["-o", "json"]));
        assert!(r.code < 2, "{}", r.stderr);
        let v: Value = serde_json::from_str(&r.stdout).unwrap();
        v["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["rule"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(rules(&["--rule", "jargon"]), ["default/jargon"]);
    assert_eq!(
        rules(&["--rule", "default/jargon", "--rule", "compression"]),
        ["default/compression", "default/jargon"]
    );
    assert_eq!(rules(&["--rule", "default/*"]).len(), 4);
    let r = run(env.cmd().args(["dirty.md", "--rule", "jargn"]));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: --rule: no rule has the id `jargn` (did you mean `default/jargon`?)\n"
    );
    let r = run(env.cmd().args(["dirty.md", "--rule", "mine/*"]));
    assert_eq!(r.stderr, "proserunner: --rule: no rules come from `mine`\n");
}

#[test]
fn show_suppressed_says_what_hid_each_issue() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        "schema = 1\naccept = [\"synergy\"]\n[rule.\"default/compression\"]\nlevel = \"off\"\n",
    );
    env.write(
        "doc.md",
        "We utilize synergy due to the fact that it works.\n\n\
         <!-- proserunner-disable-next-line -->\nWe utilize it.\n",
    );
    let r = run(env.cmd().args(["doc.md", "--show-suppressed"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "\ndoc.md\n\
         [1]  1:4  warning  \"utilize\" -> Prefer: use.  default/needless-variants\n\
         \nHidden by config, ignore lists and inline control:\n\
         \ndoc.md\n\
         1:12  warning  \"synergy\"              -> Avoid corporate-speak.        default/corporate-speak    accepted in .proserunner/config.toml\n\
         1:20  warning  \"due to the fact that\" -> Prefer: because, since, why.  default/compression        off in .proserunner/config.toml\n\
         4:4   warning  \"utilize\"              -> Prefer: use.                  default/needless-variants  off by the comment on line 3\n"
    );
    assert_eq!(
        r.stderr,
        "Checked 1 file and found 1 issue.\n\
         3 issues that config, ignore lists or inline control hide are shown last.\n"
    );

    let r = run(env
        .cmd()
        .args(["doc.md", "--show-suppressed", "-o", "plain"]));
    assert_eq!(
        r.stdout.lines().nth(1),
        Some(
            "doc.md:1:12: [hidden] \"synergy\" -> Avoid corporate-speak. (default/corporate-speak; accepted in .proserunner/config.toml)"
        )
    );
    let r = run(env
        .cmd()
        .args(["doc.md", "--show-suppressed", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    let hidden = &v["suppressed"][1];
    assert_eq!(hidden["rule"], "default/compression");
    assert_eq!(hidden["hidden_by"], "off");
    assert_eq!(hidden["reason"], "off in .proserunner/config.toml");
    assert!(hidden.get("number").is_none());
    assert_eq!(v["issues"][0]["number"], 1);
    // Without the flag, JSON has no `suppressed` at all.
    let r = run(env.cmd().args(["doc.md", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    assert!(v.get("suppressed").is_none());

    let r = run(env
        .cmd()
        .args(["doc.md", "--show-suppressed", "-o", "sarif"]));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: --show-suppressed works with -o group, plain and json, not -o sarif\n"
    );
}
