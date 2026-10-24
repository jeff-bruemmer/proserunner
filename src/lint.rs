//! Matches every rule against prose blocks in one pass per matcher, runs
//! the checks that need a whole document, then resolves each rule's
//! matches on its own. Pure: no I/O.

use std::cmp::Reverse;
use std::ops::Range;

use crate::prose;
use crate::rules::checks::{Candidate, Covered, Doc, Found};
use crate::rules::{EntryRef, Level, RuleIndex, RuleSet, Target};
use crate::scan::{self, Block, Directive, Format, Options, Scanned};

/// A match a rule reports: where it is, what found it, and how serious it
/// is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Issue {
    /// The rule that found it, in the set that linted.
    pub rule: RuleIndex,
    /// The rule's entry or check that found it.
    pub entry: EntryRef,
    /// Source bytes.
    pub span: Range<usize>,
    /// The matched text as the rules saw it, with whitespace collapsed.
    pub matched: String,
    /// Values for a check's own message placeholders, such as a count, or
    /// a pattern's groups.
    pub args: Vec<String>,
    /// The entry's level or the rule's, until config changes it for this
    /// file.
    pub level: Level,
}

#[derive(Debug)]
struct Hit {
    rule: RuleIndex,
    entry: EntryRef,
    start: usize,
    end: usize,
    args: Vec<String>,
}

impl Hit {
    /// What check `check` of rule `rule` found.
    fn of_check(rule: RuleIndex, check: u32, found: Found) -> Hit {
        Hit {
            rule,
            entry: EntryRef::Check(check),
            start: found.range.start,
            end: found.range.end,
            args: found.args,
        }
    }
}

/// A backtracking pattern that ran out of budget in a block, so it may have
/// missed matches there.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Exhausted {
    /// The rule whose pattern gave up.
    pub rule: RuleIndex,
    /// The entry the pattern is, or the check that wants it.
    pub entry: EntryRef,
    /// For a check, which of its wants; 0 for an entry.
    pub tag: u32,
    /// The source offset where the block starts.
    pub at: usize,
}

/// A linted document: its issues in source order, the inline control
/// directives that [`Policy::apply`](crate::policy::Policy::apply) honors,
/// and the patterns that gave up.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Linted {
    /// Every rule's, as found: config, ignores and inline control haven't
    /// had their say.
    pub issues: Vec<Issue>,
    /// From the document's comments, in source order.
    pub directives: Vec<Directive>,
    /// In block order, at most one per pattern and block.
    pub exhausted: Vec<Exhausted>,
}

/// Scans `source` as `format` and matches every rule against its prose.
/// Markup and masked text never match, and each issue's span is source
/// bytes. [`Policy::apply`](crate::policy::Policy::apply) then decides
/// which issues to keep.
///
/// ```
/// use proserunner::lint;
/// use proserunner::rules::{self, RuleSet};
/// use proserunner::scan::{Format, Options};
///
/// let rules = RuleSet::compile(rules::builtin()).rules;
/// let source = "We utilize `utilize` here.";
/// let linted = lint::document(&rules, Format::Markdown, source, Options::default());
/// let matched: Vec<&str> = linted.issues.iter().map(|i| &source[i.span.clone()]).collect();
/// assert_eq!(matched, ["utilize"]);
/// ```
#[must_use]
pub fn document(rules: &RuleSet, format: Format, source: &str, options: Options<'_>) -> Linted {
    scanned(rules, source, scan::scan(format, source, options))
}

/// Lints what a scan of `source` found.
#[must_use]
pub fn scanned(rules: &RuleSet, source: &str, scanned: Scanned) -> Linted {
    let blocks = &scanned.blocks;
    let mut linted = Linted {
        directives: scanned.directives,
        ..Linted::default()
    };
    let mut scratch = String::new();
    let mut candidates = Vec::new();
    let mut hits: Vec<Vec<Hit>> = Vec::with_capacity(blocks.len());
    for (i, block) in blocks.iter().enumerate() {
        let mut found = Vec::new();
        let mut matched = Matched {
            block: i as u32,
            hits: &mut found,
            candidates: &mut candidates,
            exhausted: &mut linted.exhausted,
        };
        block_hits(rules, block, source, &mut scratch, &mut matched);
        hits.push(found);
    }
    document_hits(rules, blocks, candidates, &mut hits);
    for (block, hits) in blocks.iter().zip(hits) {
        resolve(rules, block, source, hits, &mut linted.issues);
    }
    // A block can sit inside another, as a footnote sits inside its
    // paragraph, so blocks aren't always in source order.
    linted.issues.sort_by_key(|i| (i.span.start, i.rule));
    linted
}

/// Where one block's matches go.
struct Matched<'out, 'block> {
    block: u32,
    hits: &'out mut Vec<Hit>,
    /// What checks' wants found, for their `document` hooks.
    candidates: &'out mut Vec<Candidate<'block>>,
    exhausted: &'out mut Vec<Exhausted>,
}

/// What every rule's entries and block checks match in one block, and
/// what check wants find there. `scratch` is reused between blocks for
/// the lowercased text.
fn block_hits<'block>(
    rules: &RuleSet,
    block: &'block Block,
    source: &str,
    scratch: &mut String,
    out: &mut Matched<'_, 'block>,
) {
    let text = block.text.as_str();
    let m = &rules.matcher;
    let at = block.source_offset(0);

    let mut push = |t: &Target, start: usize, end: usize, args: Vec<String>| {
        if !bounded(text, start, end, t) {
            return;
        }
        match t.entry {
            EntryRef::Entry(_) => out.hits.push(Hit {
                rule: t.rule,
                entry: t.entry,
                start,
                end,
                args,
            }),
            EntryRef::Check(check) => out.candidates.push(Candidate {
                rule: t.rule,
                check,
                tag: t.tag,
                block: out.block,
                at: block.source_offset(start),
                range: start..end,
                text: &text[start..end],
                args,
            }),
        }
    };

    if let Some(literals) = &m.insensitive {
        // Same length as `text`, so offsets carry over.
        scratch.clear();
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        for found in literals.automaton.find_overlapping_iter(scratch.as_str()) {
            for t in &literals.targets[found.pattern().as_usize()] {
                push(t, found.start(), found.end(), Vec::new());
            }
        }
    }
    if let Some(literals) = &m.sensitive {
        for found in literals.automaton.find_overlapping_iter(text) {
            for t in &literals.targets[found.pattern().as_usize()] {
                push(t, found.start(), found.end(), Vec::new());
            }
        }
    }
    // Without a set, every regex runs: slower, but nothing is missed.
    let candidates = m.set.as_ref().map(|set| set.matches(text));
    for (i, member) in m.set_members.iter().enumerate() {
        let (t, regex) = (&member.target, &member.regex);
        if candidates.as_ref().is_some_and(|c| !c.matched(i)) {
            continue;
        }
        if t.captures {
            for caps in regex.captures_iter(text) {
                let args = group_args(&member.groups, |g| caps.get(g).map(|m| m.as_str()));
                push(t, caps.get_match().start(), caps.get_match().end(), args);
            }
            continue;
        }
        for found in regex.find_iter(text) {
            push(t, found.start(), found.end(), Vec::new());
        }
    }
    let mut exhausted = Vec::new();
    for member in &m.fancy {
        let (t, regex) = (&member.target, &member.regex);
        // A pattern that exceeds its backtracking budget stops matching
        // in this block rather than stalling the run, and says so.
        for found in regex.captures_iter(text) {
            match found {
                Ok(caps) => {
                    let Some(whole) = caps.get(0) else { continue };
                    let args = group_args(&member.groups, |g| caps.get(g).map(|m| m.as_str()));
                    push(t, whole.start(), whole.end(), args);
                }
                Err(_) => {
                    exhausted.push(Exhausted {
                        rule: t.rule,
                        entry: t.entry,
                        tag: t.tag,
                        at,
                    });
                    break;
                }
            }
        }
    }
    out.exhausted.append(&mut exhausted);

    let mut found = Vec::new();
    for &index in &m.checked {
        let Some(rule) = rules.get(index) else {
            continue;
        };
        if !rule.applies_to(block) {
            continue;
        }
        for (i, c) in rule.checks.iter().enumerate() {
            c.check.block(block, source, &mut found);
            out.hits
                .extend(found.drain(..).map(|f| Hit::of_check(index, i as u32, f)));
        }
    }
}

/// What checks find across the document, added to the hits of the block
/// each is in. Candidates in blocks a rule doesn't apply to, or that it
/// excepts, are dropped first.
fn document_hits(
    rules: &RuleSet,
    blocks: &[Block],
    mut candidates: Vec<Candidate<'_>>,
    hits: &mut [Vec<Hit>],
) {
    candidates.retain(|c| {
        let rule = rules.get(c.rule);
        let block = blocks.get(c.block as usize);
        rule.zip(block)
            .is_some_and(|(rule, block)| rule.applies_to(block) && !rule.is_excepted(c.text))
    });
    candidates.sort_by_key(|c| (c.rule, c.check, c.at, c.tag));
    let mut found = Vec::new();
    for &index in &rules.matcher.checked {
        let Some(rule) = rules.get(index) else {
            continue;
        };
        if !rule.checks.iter().any(|c| c.check.is_document()) {
            continue;
        }
        let covered: Vec<Covered<'_>> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| rule.applies_to(b))
            .map(|(i, block)| Covered {
                index: i as u32,
                block,
            })
            .collect();
        for (i, c) in rule.checks.iter().enumerate() {
            if !c.check.is_document() {
                continue;
            }
            let key = (index, i as u32);
            let start = candidates.partition_point(|c| (c.rule, c.check) < key);
            let end = candidates.partition_point(|c| (c.rule, c.check) <= key);
            let doc = Doc {
                blocks: &covered,
                candidates: &candidates[start..end],
            };
            c.check.document(&doc, &mut found);
            for f in found.drain(..) {
                if let Some(hits) = hits.get_mut(f.block as usize) {
                    hits.push(Hit::of_check(index, i as u32, f.found));
                }
            }
        }
    }
}

/// Per rule: drop empty and out-of-scope matches, keep the leftmost-longest
/// that don't overlap, then drop exceptions and terms written right.
///
/// Matches overlap among a rule's entries, or among one check's: each
/// check finds another kind of problem, so a sentence over a `limit`
/// doesn't hide a word in it.
fn resolve(rules: &RuleSet, block: &Block, source: &str, mut hits: Vec<Hit>, out: &mut Vec<Issue>) {
    hits.retain(|h| h.end > h.start);
    hits.sort_by_key(|h| (h.rule, group(h.entry), h.start, Reverse(h.end), h.entry));

    let first = out.len();
    let mut last: Option<(RuleIndex, Option<u32>, usize)> = None;
    for h in hits {
        let at = (h.rule, group(h.entry));
        if let Some((rule, group, end)) = last
            && (rule, group) == at
            && h.start < end
        {
            continue;
        }
        last = Some((at.0, at.1, h.end));

        let Some(rule) = rules.get(h.rule) else {
            continue;
        };
        if !rule.applies_to(block) {
            continue;
        }
        let matched = &block.text[h.start..h.end];
        if rule.is_excepted(matched) || rule.is_term(h.entry, matched) {
            continue;
        }
        out.push(Issue {
            rule: h.rule,
            entry: h.entry,
            span: block.source_range(source, h.start..h.end),
            matched: one_placeholder_each(matched),
            args: h.args,
            level: rule.level_of(h.entry),
        });
    }
    out[first..].sort_by_key(|i| (i.span.start, i.rule));
}

/// `matched` with each run of placeholders made one. Hidden text such as
/// a URL is a placeholder per byte in a block, to keep its offsets, but
/// it's one thing hidden.
fn one_placeholder_each(matched: &str) -> String {
    let mut out = String::with_capacity(matched.len());
    for c in matched.chars() {
        if !(c == scan::PLACEHOLDER && out.ends_with(scan::PLACEHOLDER)) {
            out.push(c);
        }
    }
    out
}

/// Which matches of a rule can overlap: `None` for its entries, or the
/// check's index.
fn group(entry: EntryRef) -> Option<u32> {
    match entry {
        EntryRef::Entry(_) => None,
        EntryRef::Check(i) => Some(i),
    }
}

/// A match's values for its pattern's group placeholders, from `groups`
/// in slot order. A group that took no part is empty.
fn group_args<'t>(groups: &[usize], group: impl Fn(usize) -> Option<&'t str>) -> Vec<String> {
    groups
        .iter()
        .map(|&g| group(g).unwrap_or_default().to_owned())
        .collect()
}

/// Whether a match sits on word boundaries, where its entry asks for them.
/// A word doesn't start right after a hyphen, so `known` doesn't match the
/// end of `well-known`, and isn't part of a dotted name, so `github`
/// doesn't match in `github.com` or `docs.github.io`.
fn bounded(text: &str, start: usize, end: usize, t: &Target) -> bool {
    let left_ok = !t.left || {
        let mut before = text[..start].chars().rev();
        match before.next() {
            None => true,
            Some('.') => !before.next().is_some_and(prose::is_word_char),
            Some(c) => !prose::is_word_char(c) && c != '-',
        }
    };
    let right_ok = !t.right || {
        let mut after = text[end..].chars();
        match after.next() {
            None => true,
            Some('.') => !after.next().is_some_and(prose::is_word_char),
            Some(c) => !prose::is_word_char(c),
        }
    };
    left_ok && right_ok
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::rules::RuleSource;

    fn rules(files: &[(&str, &str)]) -> RuleSet {
        let sources = files.iter().map(|(id, text)| RuleSource {
            id: (*id).into(),
            origin: (*id).into(),
            text: Cow::Owned((*text).into()),
        });
        let compiled = RuleSet::compile(sources);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        compiled.rules
    }

    fn found(set: &RuleSet, source: &str) -> Vec<(String, String)> {
        document(set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (set.rules[i.rule.as_usize()].id.clone(), i.matched))
            .collect()
    }

    fn words_rule(words: &str) -> String {
        format!("schema = 1\nmessage = \"m\"\nwords = {words}")
    }

    #[test]
    fn words_match_on_boundaries() {
        let set = rules(&[("t/w", &words_rule(r#"["leverage", "etc."]"#))]);
        assert_eq!(
            found(&set, "We leverage it, etc. and so on."),
            [
                ("t/w".into(), "leverage".into()),
                ("t/w".into(), "etc.".into())
            ]
        );
        assert!(found(&set, "Leverages and deleverage.").is_empty());
    }

    #[test]
    fn hyphenated_words_dont_match_their_second_half() {
        let set = rules(&[("t/w", &words_rule(r#"["known"]"#))]);
        assert!(found(&set, "It's well-known.").is_empty());
        assert_eq!(found(&set, "Known-good.").len(), 1);
    }

    #[test]
    fn phrases_match_across_wrapped_lines() {
        let set = rules(&[("t/w", &words_rule(r#"["due to the fact that"]"#))]);
        let issues = document(
            &set,
            Format::Markdown,
            "It failed due to the fact\nthat it rained.",
            Options::default(),
        )
        .issues;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].matched, "due to the fact that");
        assert_eq!(issues[0].span, 10..30);
    }

    #[test]
    fn longest_match_wins_within_a_rule() {
        let set = rules(&[(
            "t/w",
            &words_rule(r#"["thought leader{,s}", "in order", "in order to"]"#),
        )]);
        assert_eq!(
            found(&set, "Thought leaders work in order to win.")
                .into_iter()
                .map(|(_, m)| m)
                .collect::<Vec<_>>(),
            ["Thought leaders", "in order to"]
        );
    }

    #[test]
    fn rules_overlap_each_other() {
        let set = rules(&[
            ("t/a", &words_rule(r#"["at the end of the day"]"#)),
            ("t/b", &words_rule(r#"["the end"]"#)),
        ]);
        assert_eq!(found(&set, "At the end of the day.").len(), 2);
    }

    #[test]
    fn case_sensitive_rules() {
        let set = rules(&[(
            "t/c",
            "schema = 1\nmessage = \"m\"\ncase = \"sensitive\"\nwords = [\"TODO\"]",
        )]);
        assert_eq!(found(&set, "TODO: fix. todo later.").len(), 1);
    }

    #[test]
    fn unicode_case_folding() {
        let set = rules(&[("t/u", &words_rule(r#"["über"]"#))]);
        assert_eq!(found(&set, "Über alles.").len(), 1);
        assert!(found(&set, "Überall.").is_empty());
    }

    #[test]
    fn patterns_are_used_as_written() {
        let set = rules(&[(
            "t/p",
            "schema = 1\nmessage = \"m\"\ncase = \"sensitive\"\npatterns = ['\\d+ ?%']",
        )]);
        assert_eq!(found(&set, "Up 50 % today.")[0].1, "50 %");
    }

    #[test]
    fn patterns_match_without_a_set() {
        let source = RuleSource {
            id: "t/p".into(),
            origin: "t/p".into(),
            text: Cow::Borrowed("schema = 1\nmessage = \"m\"\npatterns = ['\\d+%', 'x+']"),
        };
        let compiled = RuleSet::compile_with([source], 1);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        let set = compiled.rules;
        assert!(set.matcher.set.is_none());
        assert_eq!(
            found(&set, "Up 50% today, xx.")
                .into_iter()
                .map(|(_, m)| m)
                .collect::<Vec<_>>(),
            ["50%", "xx"]
        );
    }

    #[test]
    fn lookaround_patterns_work() {
        let set = rules(&[(
            "t/p",
            "schema = 1\nmessage = \"m\"\npatterns = ['fast(?= track)']",
        )]);
        assert_eq!(found(&set, "A fast track. A fast car.").len(), 1);
    }

    #[test]
    fn patterns_that_run_out_of_budget_say_so() {
        let set = rules(&[(
            "t/b",
            "schema = 1\nmessage = \"m\"\npatterns = ['(a|aa)+\\1b', 'z']",
        )]);
        let a = "a".repeat(40);
        let source = format!("Fine z.\n\n{a} z\n\n{a}\n");
        let linted = document(&set, Format::Markdown, &source, Options::default());
        assert_eq!(linted.issues.len(), 2);
        let at = source.find('a').unwrap();
        assert_eq!(
            linted.exhausted,
            [
                Exhausted {
                    rule: RuleIndex::new(0),
                    entry: EntryRef::Entry(0),
                    tag: 0,
                    at
                },
                Exhausted {
                    rule: RuleIndex::new(0),
                    entry: EntryRef::Entry(0),
                    tag: 0,
                    at: at + a.len() + 4
                }
            ]
        );
    }

    #[test]
    fn except_drops_whole_matches() {
        let set = rules(&[(
            "t/e",
            "schema = 1\nmessage = \"m\"\nwords = [\"leverage\", \"leverage partners\"]\nexcept = [\"Leverage Partners\"]",
        )]);
        assert!(found(&set, "Ask leverage partners.").is_empty());
        assert_eq!(found(&set, "Leverage it.").len(), 1);
    }

    #[test]
    fn scope_limits_rules() {
        let set = rules(&[(
            "t/h",
            "schema = 1\nmessage = \"m\"\nscope = [\"heading\"]\npatterns = ['\\.$']",
        )]);
        assert_eq!(found(&set, "# Title.\n\nBody.\n").len(), 1);
    }

    #[test]
    fn markup_hides_nothing_and_adds_nothing() {
        let set = rules(&[("t/w", &words_rule(r#"["very unique", "utilize"]"#))]);
        assert_eq!(found(&set, "A *very* unique idea.").len(), 1);
        assert!(found(&set, "Call `utilize()` or see [docs](http://x/utilize).").is_empty());
        assert!(found(&set, "very `x` unique").is_empty());
    }

    #[test]
    fn spans_never_split_characters() {
        // Found by fuzzing: a pattern ending inside a masked quote ended
        // inside a character.
        let set = rules(&[(
            "t/s",
            "schema = 1\nmessage = \"m\"\npatterns = ['\\x{1A}{2}']",
        )]);
        let src = "A \"éé\" b.";
        let issues = document(&set, Format::Markdown, src, Options::default()).issues;
        assert_eq!(issues[0].span, 2..5);
        assert!(issues.iter().all(|i| src.get(i.span.clone()).is_some()));
    }

    #[test]
    fn words_dont_match_inside_dotted_names() {
        let set = rules(&[("t/w", &words_rule(r#"["github", "main", "v2"]"#))]);
        assert!(found(&set, "See github.com, docs.github.io and main.rs.").is_empty());
        assert!(found(&set, "Ship v2.1 today.").is_empty());
        assert_eq!(
            found(&set, "On github. The main... github's v2.")
                .into_iter()
                .map(|(_, m)| m)
                .collect::<Vec<_>>(),
            ["github", "main", "github", "v2"]
        );
    }

    #[test]
    fn entries_have_their_own_case_and_level() {
        let set = rules(&[(
            "t/e",
            r#"schema = 1
            message = "m"
            words = [{ match = "US", case = "sensitive" }, { match = "utilize", level = "error" }]
            patterns = [{ match = 'todo', case = "insensitive" }]
            case = "sensitive"
            "#,
        )]);
        let issues = document(
            &set,
            Format::Markdown,
            "The US and us utilize TODO.",
            Options::default(),
        )
        .issues;
        assert_eq!(
            issues
                .iter()
                .map(|i| (i.matched.as_str(), i.level))
                .collect::<Vec<_>>(),
            [
                ("US", Level::Warning),
                ("utilize", Level::Error),
                ("TODO", Level::Warning)
            ]
        );
    }

    #[test]
    fn groups_fill_messages_and_suggestions() {
        let set = rules(&[(
            "t/g",
            r#"schema = 1
            message = "Write “{1} percent”."
            patterns = ['(\d+)%', '(?<n>\d+) ?per ?cent(?= off)']
            "#,
        )]);
        let linted = document(
            &set,
            Format::Markdown,
            "Up 50% and 20 per cent off.",
            Options::default(),
        );
        let args: Vec<&[String]> = linted.issues.iter().map(|i| i.args.as_slice()).collect();
        assert_eq!(args, [&["50".to_owned()][..], &["20".into(), "20".into()]]);
        let rule = &set.rules[0];
        let i = &linted.issues[1];
        assert_eq!(
            rule.wording(i.entry)
                .unwrap()
                .render(&i.matched, "", &i.args),
            "Write “20 percent”."
        );
    }

    #[test]
    fn hidden_text_is_one_placeholder_in_a_match() {
        let set = rules(&[(
            "t/see",
            "schema = 1\nmessage = \"m\"\npatterns = ['See .+ for']",
        )]);
        let issues = document(
            &set,
            Format::Markdown,
            "See https://example.com/a/b/c for more.",
            Options::default(),
        )
        .issues;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].matched, "See \u{1A} for");
    }

    #[test]
    fn patterns_without_group_placeholders_skip_captures() {
        let set = rules(&[("t/g", "schema = 1\nmessage = \"m\"\npatterns = ['(\\d+)%']")]);
        let issues = document(&set, Format::Markdown, "Up 50%.", Options::default()).issues;
        assert!(issues[0].args.is_empty());
    }

    #[test]
    fn except_patterns_match_whole_matches() {
        let set = rules(&[(
            "t/e",
            r#"schema = 1
            message = "m"
            words = ["smith", "mr smith", "ms smith"]
            except = [{ pattern = 'm[rs] smith' }]
            "#,
        )]);
        assert!(found(&set, "Ask Mr Smith or Ms Smith.").is_empty());
        assert_eq!(found(&set, "Ask Smith.").len(), 1);
    }

    #[test]
    fn terms_report_any_other_spelling() {
        let set = rules(&[(
            "t/t",
            r#"schema = 1
            case = "sensitive"
            terms = ["GitHub", "GitHub Actions", { term = "macOS", match = ["mac os{, x}"] }, "Ångström"]
            "#,
        )]);
        let matched =
            |source| -> Vec<String> { found(&set, source).into_iter().map(|(_, m)| m).collect() };
        assert!(matched("GitHub Actions on GitHub, macOS and Ångström.").is_empty());
        assert_eq!(
            matched("Github actions, GITHUB, Mac OS X, macos and ångström."),
            ["Github actions", "GITHUB", "Mac OS X", "macos", "ångström"]
        );
        // A term written right still claims its span.
        assert!(matched("GitHub Actions").is_empty());
        assert!(matched("See github.com.").is_empty());
        let issues = document(&set, Format::Markdown, "On github.", Options::default()).issues;
        let rule = &set.rules[0];
        let w = rule.wording(issues[0].entry).unwrap();
        assert_eq!(w.render("github", "", &[]), "Prefer: GitHub.");
    }

    #[test]
    fn heading_levels_choose_headings() {
        let set = rules(&[(
            "t/h",
            "schema = 1\nmessage = \"m\"\nheading-levels = [0, 2]\nwords = [\"foo\"]",
        )]);
        let lines = |source: &str| -> Vec<usize> {
            document(&set, Format::Org, source, Options::default())
                .issues
                .iter()
                .map(|i| source[..i.span.start].matches('\n').count() + 1)
                .collect()
        };
        assert_eq!(
            lines("#+TITLE: foo\n* foo\n** foo\n*** foo\nfoo\n"),
            [1, 3, 5]
        );
    }

    #[test]
    fn check_patterns_that_give_up_say_so() {
        let set = rules(&[("t/a", "schema = 1\nacronyms = { match = '(a|aa)+\\1b' }")]);
        let source = "a".repeat(40);
        let linted = document(&set, Format::Markdown, &source, Options::default());
        assert_eq!(
            linted.exhausted,
            [Exhausted {
                rule: RuleIndex::new(0),
                entry: EntryRef::Check(0),
                tag: 0,
                at: 0
            }]
        );
        let rule = &set.rules[0];
        assert_eq!(
            rule.pattern(EntryRef::Check(0), 0).as_deref(),
            Some("(a|aa)+\\1b")
        );
        assert_eq!(rule.pattern(EntryRef::Check(0), 9), None);
    }

    #[test]
    fn checks_dont_hide_each_other() {
        let set = rules(&[(
            "t/c",
            r#"schema = 1
            consistent = [["advisor", "adviser"]]
            limit = { max = 3 }
            words = [{ match = "the adviser", message = "m" }]
            "#,
        )]);
        let issues = document(
            &set,
            Format::Markdown,
            "An advisor.\n\nThe adviser met an adviser.",
            Options::default(),
        )
        .issues;
        assert_eq!(
            issues
                .iter()
                .map(|i| (i.entry, i.matched.as_str()))
                .collect::<Vec<_>>(),
            [
                (EntryRef::Entry(0), "The adviser"),
                (EntryRef::Check(1), "The adviser met an adviser."),
                (EntryRef::Check(0), "adviser"),
                (EntryRef::Check(0), "adviser")
            ]
        );
    }

    #[test]
    fn checks_resolve_like_entries() {
        let set = rules(&[(
            "t/r",
            "schema = 1\nrepeat = true\nwords = [{ match = \"is is a\", message = \"m\" }]\nexcept = [\"had had\"]\nscope = [\"paragraph\"]",
        )]);
        let issues = document(&set, Format::Markdown, "The the the.", Options::default()).issues;
        assert_eq!(issues.len(), 1, "overlapping matches of one check");
        assert_eq!(issues[0].entry, EntryRef::Check(0));
        // An entry's match and a check's both stand.
        let issues = document(
            &set,
            Format::Markdown,
            "This is is a test.",
            Options::default(),
        )
        .issues;
        assert_eq!(
            issues
                .iter()
                .map(|i| (i.entry, i.matched.as_str()))
                .collect::<Vec<_>>(),
            [
                (EntryRef::Entry(0), "is is a"),
                (EntryRef::Check(0), "is is")
            ]
        );
        assert!(found(&set, "He had had enough.").is_empty());
        assert!(found(&set, "# The the").is_empty());
    }
}
