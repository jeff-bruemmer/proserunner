//! `rules test`: every rule's `[examples]`, checked against the rule.
//!
//! Examples are Markdown, read with quoted text and code blocks checked,
//! so an example shows exactly what its rule matches. For a rule that
//! keeps to comments, an example is a comment's text, which is Markdown
//! too.

use std::ops::Range;

use crate::lint::{self, Issue};
use crate::prose;
use crate::rules::{EntryKind, Fix, Rule, RuleSet};
use crate::scan::{self, Format, Options, Scanned};

const OPTIONS: Options<'static> = Options {
    quoted_text: true,
    code_blocks: true,
    mask: None,
    front_matter: scan::FRONT_MATTER,
};

/// An example its rule gets wrong, or a rule with no `bad` example.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Failure {
    /// The rule's id.
    pub rule: String,
    /// Where the rule came from.
    pub origin: String,
    /// What went wrong.
    pub problem: String,
}

/// An example as rules read it: Markdown.
fn read(text: &str) -> Scanned {
    scan::scan(Format::Markdown, text, OPTIONS)
}

/// Lints `text` with every rule, as `rule` reads it, keeping `rule`'s
/// issues.
fn issues(rules: &RuleSet, rule: &Rule, text: &str) -> Vec<Issue> {
    lint::scanned(rules, text, read(text))
        .issues
        .into_iter()
        .filter(|i| i.rule == rule.index)
        .collect()
}

/// Why a bad example can't show anything: it has no text where the rule
/// applies. `None` when it has some.
fn out_of_scope(rule: &Rule, text: &str) -> Option<String> {
    let blocks = read(text).blocks;
    if blocks.iter().any(|b| rule.applies_to(b)) {
        return None;
    }
    let levels = if rule.heading_levels.is_empty() {
        String::new()
    } else {
        let levels: Vec<String> = rule.heading_levels.iter().map(u8::to_string).collect();
        format!(" (headings count at level {} only)", levels.join(" or "))
    };
    if rule.scope.is_empty() {
        return Some(format!(
            "bad example {text:?} has no prose to check{levels}"
        ));
    }
    let scopes: Vec<&str> = rule.scope.iter().map(|s| s.as_str()).collect();
    Some(format!(
        "bad example {text:?} has no {} text{levels}; examples are Markdown, so a heading is `# …`, a list item `- …` and code a ``` fence",
        scopes.join(" or ")
    ))
}

/// Checks every rule's examples, in rule order. A rule must report an
/// issue in each `bad` example and none in a `good` one, and each `fix`
/// pair's `before` must become its `after`.
#[must_use]
pub fn run(rules: &RuleSet) -> Vec<Failure> {
    let mut failures = Vec::new();
    for rule in rules.rules() {
        let mut fail = |problem: String| {
            failures.push(Failure {
                rule: rule.id.clone(),
                origin: rule.origin.clone(),
                problem,
            })
        };
        let ex = &rule.examples;

        if ex.bad.is_empty() {
            fail("has no `bad` examples, so nothing shows that it matches".into());
        }
        for bad in &ex.bad {
            if issues(rules, rule, bad).is_empty() {
                fail(
                    out_of_scope(rule, bad)
                        .unwrap_or_else(|| format!("bad example {bad:?} got no issues")),
                );
            }
        }
        for good in &ex.good {
            let found = issues(rules, rule, good);
            if !found.is_empty() {
                let matches: Vec<String> =
                    found.iter().map(|i| format!("{:?}", i.matched)).collect();
                fail(format!(
                    "good example {good:?} got issues: {}",
                    matches.join(", ")
                ));
            }
        }
        for Fix { before, after } in &ex.fix {
            let found = issues(rules, rule, before);
            let fixed = apply(before, &edits(rules, before, &found));
            if fixed != *after {
                fail(format!("fixing {before:?} gave {fixed:?}, not {after:?}"));
            }
        }
    }
    failures
}

/// A replacement for the source bytes `span`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    span: Range<usize>,
    replacement: String,
}

/// What a `fix` pair checks: the edit for each issue that has one, in
/// source order and without overlaps.
fn edits(rules: &RuleSet, source: &str, issues: &[Issue]) -> Vec<Edit> {
    let mut edits: Vec<Edit> = issues
        .iter()
        .filter_map(|issue| edit(rules, source, issue))
        .collect();
    edits.sort_by_key(|e| (e.span.start, e.span.end));

    let mut kept: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if kept.last().is_none_or(|k| edit.span.start >= k.span.end) {
            kept.push(edit);
        }
    }
    kept
}

/// `issue` replaced by its first suggestion with the match's case. A term
/// keeps its own case, as does a suggestion that changes only case. `None`
/// when there's no suggestion, or the source span holds markup
/// (`very* unique`).
fn edit(rules: &RuleSet, source: &str, issue: &Issue) -> Option<Edit> {
    let rule = rules.get(issue.rule)?;
    let wording = rule.wording(issue.entry)?;
    let suggestions = wording.suggestions(&issue.matched, &rule.name, &issue.args);
    let suggestion = suggestions.into_iter().next()?;
    let original = source.get(issue.span.clone())?;
    if prose::normalize(original) != issue.matched {
        return None;
    }
    let as_written = rule
        .entry(issue.entry)
        .is_some_and(|e| e.kind == EntryKind::Term)
        || suggestion.to_lowercase() == issue.matched.to_lowercase();
    Some(Edit {
        span: issue.span.clone(),
        replacement: if as_written {
            suggestion
        } else {
            preserve_case(&issue.matched, &suggestion)
        },
    })
}

/// `source` with `edits`, which are in order and don't overlap, applied.
fn apply(source: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for edit in edits {
        out.push_str(&source[at..edit.span.start]);
        out.push_str(&edit.replacement);
        at = edit.span.end;
    }
    out.push_str(&source[at..]);
    out
}

/// `Utilize` → `Use`, `UTILIZE` → `USE`, `utilize` → `use`.
fn preserve_case(matched: &str, replacement: &str) -> String {
    let letters: Vec<char> = matched.chars().filter(|c| c.is_alphabetic()).collect();
    let all_upper = letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    if all_upper {
        return replacement.to_uppercase();
    }
    let starts_upper = matched.chars().next().is_some_and(char::is_uppercase);
    let mut chars = replacement.chars();
    match chars.next() {
        Some(first) if starts_upper && first.is_lowercase() => {
            first.to_uppercase().chain(chars).collect()
        }
        _ => replacement.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::rules::{self, RuleSource};

    #[test]
    fn case_is_preserved() {
        assert_eq!(preserve_case("Utilize", "use"), "Use");
        assert_eq!(preserve_case("UTILIZE", "use"), "USE");
        assert_eq!(preserve_case("utilize", "use"), "use");
        assert_eq!(preserve_case("In order to", "to"), "To");
        assert_eq!(preserve_case("a", "GitHub"), "GitHub");
    }

    #[test]
    fn apply_in_order() {
        let edits = vec![
            Edit {
                span: 0..7,
                replacement: "Use".into(),
            },
            Edit {
                span: 12..16,
                replacement: "now".into(),
            },
        ];
        assert_eq!(apply("Utilize it, asap.", &edits), "Use it, now.");
    }

    #[test]
    fn examples_read_code_and_quotes() {
        let problems = run_one(
            r#"schema = 1
            message = "No foo."
            scope = ["code"]
            words = ["foo"]
            [examples]
            bad = ["```\nfoo\n```"]
            "#,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let problems = run_one(
            r#"schema = 1
            message = "No bar."
            words = ["bar"]
            [examples]
            bad = ['say "bar" now']
            "#,
        );
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn examples_out_of_scope_say_why() {
        let problems = run_one(
            r#"schema = 1
            message = "No foo."
            scope = ["heading"]
            words = ["foo"]
            [examples]
            bad = ["A foo."]
            "#,
        );
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].starts_with("bad example \"A foo.\" has no heading text"),
            "{problems:?}"
        );
    }

    #[test]
    fn examples_without_prose_say_so() {
        let problems = run_one(
            r#"schema = 1
            message = "No foo."
            words = ["foo"]
            [examples]
            bad = ["<!-- foo -->"]
            "#,
        );
        assert_eq!(
            problems,
            ["bad example \"<!-- foo -->\" has no prose to check"]
        );
    }

    fn run_one(text: &str) -> Vec<String> {
        let compiled = RuleSet::compile([RuleSource {
            id: "t/x".into(),
            origin: "x.toml".into(),
            text: Cow::Borrowed(text),
        }]);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        run(&compiled.rules)
            .into_iter()
            .map(|f| f.problem)
            .collect()
    }

    #[test]
    fn passing_examples() {
        let problems = run_one(
            r#"schema = 1
            words = [{ match = "utilize", suggest = ["use"] }]
            [examples]
            bad = ["We utilize it."]
            good = ["We use it."]
            fix = [["Utilize it.", "Use it."]]
            "#,
        );
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn failing_examples() {
        let problems = run_one(
            r#"schema = 1
            words = [{ match = "utilize", suggest = ["use"] }]
            [examples]
            bad = ["We use it."]
            good = ["We utilize it."]
            fix = [["Utilize it.", "Employ it."]]
            "#,
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn terms_and_groups_fix_as_written() {
        let problems = run_one(
            r#"schema = 1
            terms = ["GitHub"]
            patterns = [{ match = '(\d+)%', suggest = ["{1} percent"] }]
            [examples]
            bad = ["On GITHUB."]
            good = ["On GitHub."]
            fix = [["GITHUB is 50% off.", "GitHub is 50 percent off."]]
            "#,
        );
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn checks_have_examples_too() {
        let problems = run_one(
            r###"schema = 1
            scope = ["heading"]
            heading-levels = [2]
            capitalization = { style = "sentence" }
            consistent = [["advisor", "adviser"]]
            [examples]
            bad = ["## Getting Started", "## An advisor and an Adviser"]
            good = ["# Getting Started", "## Getting started"]
            fix = [["## Getting Started", "## Getting started"], ["## An advisor and an Adviser", "## An advisor and an Advisor"]]
            "###,
        );
        // Both checks report "Adviser", and a fix pair applies the first
        // issue on a span: `consistent`'s.
        assert!(problems.is_empty(), "{problems:?}");
        let problems = run_one(
            r###"schema = 1
            heading-levels = [2]
            capitalization = {}
            [examples]
            bad = ["# Getting Started"]
            "###,
        );
        assert_eq!(
            problems,
            [
                "bad example \"# Getting Started\" has no prose to check (headings count at level 2 only)"
            ]
        );
        let problems = run_one(
            r###"schema = 1
            scope = ["heading"]
            heading-levels = [2, 3]
            capitalization = {}
            [examples]
            bad = ["# Getting Started"]
            "###,
        );
        assert!(
            problems[0].starts_with(
                "bad example \"# Getting Started\" has no heading text (headings count at level 2 or 3 only);"
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn rules_need_bad_examples() {
        let problems = run_one("schema = 1\nmessage = \"m\"\nwords = [\"a\"]");
        assert_eq!(
            problems,
            ["has no `bad` examples, so nothing shows that it matches"]
        );
    }

    #[test]
    fn builtin_examples_pass() {
        let compiled = RuleSet::compile(rules::builtin());
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        let failures = run(&compiled.rules);
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
