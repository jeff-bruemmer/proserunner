//! Turns rule files into a [`RuleSet`]: validated rules plus one shared
//! matcher for all of them.
//!
//! Literal `words` from every rule go into one Aho-Corasick automaton per
//! case mode; `patterns` go into one `RegexSet`. Linting a block is then
//! about one scan of its text, however many rules there are.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};

use aho_corasick::{AhoCorasick, AhoCorasickKind, MatchKind};
use regex::{Regex, RegexSet, RegexSetBuilder};

use super::checks::{self, Check, Want};
use super::expand;
use super::rule::{CompiledCheck, CompiledEntry, EntryKind, EntryRef, Exception, Rule, Wording};
use super::rule_file::{self, Case, Entry, Except, Level, RuleFile, SCHEMA, Term};
use super::rule_source::RuleSource;
use super::template::{COMMON, Template};
use crate::error::Error;
use crate::prose;
use crate::scan::Scope;
use crate::settings;
use crate::typo;

/// Backtracking budget for patterns that need lookaround or backreferences.
const BACKTRACK_LIMIT: usize = 100_000;

/// How large the shared `RegexSet` may grow: regex's own default.
const SET_SIZE_LIMIT: usize = 10 * (1 << 20);

/// Default message for entries that suggest replacements.
const SUGGEST_MESSAGE: &str = "Prefer: {suggestion}.";

/// A rule's place in the [`RuleSet`] that compiled it. Another set reads
/// it as one of its own rules, or as none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleIndex(u32);

impl RuleIndex {
    pub(crate) fn new(index: usize) -> Self {
        RuleIndex(index as u32)
    }

    /// For tables of the caller's own, one entry per rule.
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// What [`RuleSet::compile`] made of its sources.
#[derive(Debug)]
#[non_exhaustive]
#[must_use = "rule files that failed are in `errors`"]
pub struct Compiled {
    /// Every rule that compiled.
    pub rules: RuleSet,
    /// One per rule file left out.
    pub errors: Vec<Error>,
    /// Problems that don't stop any rule from loading, such as patterns
    /// running slower than they could.
    pub warnings: Vec<String>,
}

/// Which rule entry a matcher pattern belongs to, or which check wants
/// it, whether a match must sit on word boundaries, and whether its
/// issues need the pattern's groups.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target {
    pub rule: RuleIndex,
    pub entry: EntryRef,
    /// For a check, which of its wants; 0 for an entry.
    pub tag: u32,
    pub left: bool,
    pub right: bool,
    pub captures: bool,
}

/// One automaton for a case mode, and the targets of each of its
/// patterns.
pub(crate) struct Literals {
    pub automaton: AhoCorasick,
    /// Indexed by pattern.
    pub targets: Vec<Vec<Target>>,
}

/// A regex and the target its matches go to.
pub(crate) struct Targeted<R> {
    pub target: Target,
    pub regex: R,
    /// The group each placeholder reads, in [`capture_slots`] order, when
    /// the target needs the groups.
    pub groups: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Matcher {
    pub insensitive: Option<Literals>,
    pub sensitive: Option<Literals>,
    /// Which of `set_members` can match a block. `None` runs them all:
    /// there are none, or together they were too large for one set.
    pub set: Option<RegexSet>,
    pub set_members: Vec<Targeted<Regex>>,
    pub fancy: Vec<Targeted<fancy_regex::Regex>>,
    /// The rules that have checks.
    pub checked: Vec<RuleIndex>,
}

/// Compiled rules and one shared matcher for all of them.
pub struct RuleSet {
    pub(crate) rules: Vec<Rule>,
    pub(crate) matcher: Matcher,
    /// The ids of rule files that didn't compile, so config that names
    /// one isn't taken for a typo.
    pub(crate) left_out: Vec<String>,
}

impl Debug for RuleSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.rules.iter().map(|r| &r.id))
            .finish()
    }
}

impl RuleSet {
    /// Compiles every source. A broken rule file is returned as an error and
    /// left out; the rest still compile.
    pub fn compile<'text>(sources: impl IntoIterator<Item = RuleSource<'text>>) -> Compiled {
        Self::compile_with(sources, SET_SIZE_LIMIT)
    }

    /// [`compile`](Self::compile), with the shared `RegexSet` limited to
    /// `set_limit` bytes.
    pub(crate) fn compile_with<'text>(
        sources: impl IntoIterator<Item = RuleSource<'text>>,
        set_limit: usize,
    ) -> Compiled {
        let mut rules: Vec<Rule> = Vec::new();
        let mut errors = Vec::new();
        let mut left_out = Vec::new();
        let mut parts = Parts::default();

        for source in sources {
            if rules.iter().any(|r| r.id == source.id) {
                errors.push(Error::rule(
                    source.origin,
                    format!("another rule already has the id `{}`", source.id),
                ));
                continue;
            }
            match compile_rule(&source, RuleIndex::new(rules.len())) {
                Ok((rule, rule_parts)) => {
                    rules.push(rule);
                    parts.append(rule_parts);
                }
                Err(message) => {
                    errors.push(Error::rule(source.origin, message));
                    left_out.push(source.id);
                }
            }
        }

        let mut warnings = Vec::new();
        let set = match parts.regex_set(set_limit) {
            Ok(set) => set,
            // Each regex compiled alone, so only their combined size fails.
            Err(_) => {
                warnings.push(unfiltered_warning(&rules, &parts.regexes));
                None
            }
        };
        let rules = match parts.build(set) {
            Ok(matcher) => RuleSet {
                rules,
                matcher,
                left_out,
            },
            Err(message) => {
                errors.push(Error::rule("rules", message));
                left_out.extend(rules.into_iter().map(|r| r.id));
                RuleSet {
                    rules: Vec::new(),
                    matcher: Matcher::default(),
                    left_out,
                }
            }
        };
        Compiled {
            rules,
            errors,
            warnings,
        }
    }

    /// Every rule, in index order.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The rule at `index`, or `None` if this set has none there.
    pub fn get(&self, index: RuleIndex) -> Option<&Rule> {
        self.rules.get(index.as_usize())
    }

    /// How many rules it holds.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// True when it holds no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rule a person means by `query`: its id, or its name alone
    /// (`hedging` for `default/hedging`) when only one rule has that name.
    /// The error says why nothing matched.
    pub fn find(&self, query: &str) -> Result<&Rule, String> {
        if let Some(rule) = self.rules.iter().find(|r| r.id == query) {
            return Ok(rule);
        }
        let named: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.id.rsplit('/').next() == Some(query))
            .collect();
        match named.as_slice() {
            [rule] => Ok(rule),
            [] => {
                // A name alone is compared with names, an id with ids.
                let hint = if query.contains('/') {
                    typo::closest(query, self.rules.iter().map(|r| r.id.as_str()))
                } else {
                    let names = self.rules.iter().filter_map(|r| r.id.rsplit('/').next());
                    typo::closest(query, names).and_then(|name| {
                        self.rules
                            .iter()
                            .map(|r| r.id.as_str())
                            .find(|id| id.rsplit('/').next() == Some(name))
                    })
                };
                let hint = hint
                    .map(|c| format!(" (did you mean `{c}`?)"))
                    .unwrap_or_default();
                Err(format!("no rule has the id `{query}`{hint}"))
            }
            several => {
                let ids: Vec<String> = several.iter().map(|r| format!("`{}`", r.id)).collect();
                Err(format!(
                    "several rules are named `{query}`: {}",
                    ids.join(", ")
                ))
            }
        }
    }
}

/// Matcher inputs collected from every rule before building.
#[derive(Default)]
struct Parts {
    insensitive: Vec<(String, Target)>,
    sensitive: Vec<(String, Target)>,
    regexes: Vec<Targeted<Regex>>,
    fancy: Vec<Targeted<fancy_regex::Regex>>,
    checked: Vec<RuleIndex>,
}

impl Parts {
    /// Adds one piece for `target`; a literal goes to the automaton for
    /// its case.
    fn push(&mut self, matcher: PieceMatcher, target: Target, case: Case) {
        match matcher {
            PieceMatcher::Literal(word) if case == Case::Insensitive => {
                self.insensitive.push((word, target));
            }
            PieceMatcher::Literal(word) => self.sensitive.push((word, target)),
            PieceMatcher::Regex(regex) => {
                let groups = groups(target, regex.capture_names());
                self.regexes.push(Targeted {
                    target,
                    regex,
                    groups,
                });
            }
            PieceMatcher::Fancy(regex) => {
                let groups = groups(target, regex.capture_names());
                self.fancy.push(Targeted {
                    target,
                    regex,
                    groups,
                });
            }
        }
    }

    fn append(&mut self, mut other: Parts) {
        self.insensitive.append(&mut other.insensitive);
        self.sensitive.append(&mut other.sensitive);
        self.regexes.append(&mut other.regexes);
        self.fancy.append(&mut other.fancy);
        self.checked.append(&mut other.checked);
    }

    /// One set for every linear regex, so each runs only in blocks it can
    /// match. `None` when there are none.
    fn regex_set(&self, limit: usize) -> Result<Option<RegexSet>, regex::Error> {
        if self.regexes.is_empty() {
            return Ok(None);
        }
        RegexSetBuilder::new(self.regexes.iter().map(|t| t.regex.as_str()))
            .size_limit(limit)
            .build()
            .map(Some)
    }

    fn build(self, set: Option<RegexSet>) -> Result<Matcher, String> {
        Ok(Matcher {
            insensitive: literals(self.insensitive, Case::Insensitive)?,
            sensitive: literals(self.sensitive, Case::Sensitive)?,
            set,
            set_members: self.regexes,
            fancy: self.fancy,
            checked: self.checked,
        })
    }
}

/// Why every regex runs on every block: together they're too large for
/// one set. Names the rule with the most, the likeliest to split or trim.
fn unfiltered_warning(rules: &[Rule], regexes: &[Targeted<Regex>]) -> String {
    let mut counts: HashMap<RuleIndex, usize> = HashMap::new();
    for t in regexes {
        *counts.entry(t.target.rule).or_default() += 1;
    }
    let most = counts
        .into_iter()
        .max_by_key(|&(rule, n)| (n, Reverse(rule)))
        .and_then(|(rule, n)| Some((rules.get(rule.as_usize())?, n)));
    let culprit = most
        .map(|(rule, n)| format!("; `{}` has the most ({n})", rule.id))
        .unwrap_or_default();
    format!(
        "the rules' patterns are too large to filter together, so each one runs on every block, which is slower{culprit}"
    )
}

/// One automaton for `words`. Equal literals share a pattern, so a match
/// reports every rule entry that wrote it.
fn literals(words: Vec<(String, Target)>, case: Case) -> Result<Option<Literals>, String> {
    if words.is_empty() {
        return Ok(None);
    }
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut targets: Vec<Vec<Target>> = Vec::new();
    for (word, target) in words {
        // Insensitive patterns are lowercased here and searched against
        // lowercased text (see `lint`): building with
        // `ascii_case_insensitive` takes four times as long.
        let word = if case == Case::Insensitive {
            word.to_ascii_lowercase()
        } else {
            word
        };
        let i = *index.entry(word.clone()).or_insert_with(|| {
            patterns.push(word);
            targets.push(Vec::new());
            patterns.len() - 1
        });
        targets[i].push(target);
    }
    // A contiguous NFA builds several times faster than a DFA for a few
    // thousand patterns and searches nearly as fast; building happens on
    // every run, so it wins.
    let automaton = AhoCorasick::builder()
        .match_kind(MatchKind::Standard)
        .kind(Some(AhoCorasickKind::ContiguousNFA))
        .build(&patterns)
        .map_err(|e| e.to_string())?;
    Ok(Some(Literals { automaton, targets }))
}

/// A display name from a file stem: `needless-variants` → `Needless variants`.
fn name_from_stem(stem: &str) -> String {
    let spaced = stem.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => spaced,
    }
}

/// One rule, and what it adds to the shared matcher. `index` is the
/// rule's place in the set.
fn compile_rule(source: &RuleSource<'_>, index: RuleIndex) -> Result<(Rule, Parts), String> {
    let file: RuleFile = rule_file::parse(&source.text)?;
    settings::check_schema(file.schema, SCHEMA)?;
    let checks = checks::compile(&file)?;
    if file.words.is_empty()
        && file.patterns.is_empty()
        && file.terms.is_empty()
        && checks.is_empty()
    {
        let keys: Vec<String> = checks::keys().map(|k| format!("`{k}`")).collect();
        return Err(format!(
            "the rule has nothing to match: add `words`, `patterns`, `terms`, or a check ({})",
            keys.join(", ")
        ));
    }
    if !file.heading_levels.is_empty()
        && !file.scope.is_empty()
        && !file.scope.contains(&Scope::Heading)
    {
        return Err(
            "`heading-levels` chooses headings, but `scope` has no \"heading\": add it, or drop `heading-levels`"
                .into(),
        );
    }

    let RuleFile {
        name,
        message,
        level,
        explain,
        case,
        scope,
        heading_levels,
        except,
        words,
        patterns,
        terms,
        examples,
        ..
    } = file;
    let mut rule_message = RuleMessage::new(message.as_deref());
    let mut entries = Vec::new();
    let mut parts = Parts::default();
    let drafts = words
        .into_iter()
        .map(|e| Ok(Draft::entry(EntryKind::Word, e, case, level)))
        .chain(
            patterns
                .into_iter()
                .map(|e| Ok(Draft::entry(EntryKind::Pattern, e, case, level))),
        )
        .chain(terms.into_iter().map(|t| Draft::term(t, level)));
    for draft in drafts {
        let draft = draft?;
        let pieces = pieces(draft.kind, &draft.source, &draft.variants, draft.case)?;
        let groups = match (draft.kind, pieces.first()) {
            (EntryKind::Pattern, Some(piece)) => group_vars(&draft.source, &piece.matcher)?,
            _ => Vec::new(),
        };
        let vars: Vec<&str> = groups.iter().map(String::as_str).collect();
        let wording = draft.wording(&vars, &mut rule_message)?;
        let captures =
            wording.message.uses_vars() || wording.suggest.iter().any(Template::uses_vars);
        for piece in pieces {
            let target = Target {
                rule: index,
                entry: EntryRef::Entry(entries.len() as u32),
                tag: 0,
                left: piece.left,
                right: piece.right,
                captures,
            };
            parts.push(piece.matcher, target, draft.case);
        }
        entries.push(CompiledEntry {
            source: draft.source,
            variants: draft.variants,
            kind: draft.kind,
            case: draft.case,
            level: draft.level,
            wording,
        });
    }

    for (i, check) in checks.iter().enumerate() {
        add_wants(&mut parts, check, index, i as u32)?;
    }
    let checks: Vec<CompiledCheck> = checks
        .into_iter()
        .map(|check| {
            let wording = check_wording(&check, message.as_deref())?;
            Ok(CompiledCheck { check, wording })
        })
        .collect::<Result<_, String>>()?;
    if checks.is_empty() {
        rule_message.check_unused()?;
    } else {
        parts.checked.push(index);
    }

    let except: Vec<Exception> = except
        .iter()
        .map(|e| exception(e, case))
        .collect::<Result<_, _>>()?;

    let stem = source.id.rsplit('/').next().unwrap_or(&source.id);
    let rule = Rule {
        index,
        id: source.id.clone(),
        name: name.unwrap_or_else(|| name_from_stem(stem)),
        origin: source.origin.clone(),
        level,
        explain,
        case,
        scope,
        heading_levels,
        except,
        entries,
        checks,
        examples,
    };
    Ok((rule, parts))
}

/// An entry from `words`, `patterns` or `terms`, with the rule's settings
/// filled in where it has none of its own.
struct Draft {
    kind: EntryKind,
    /// The word or pattern, or the term normalized.
    source: String,
    variants: Vec<String>,
    case: Case,
    level: Level,
    message: Option<String>,
    suggest: Vec<String>,
}

impl Draft {
    /// An entry, with the rule's `case` and `level` where it has none.
    fn entry(kind: EntryKind, e: Entry, case: Case, level: Level) -> Draft {
        Draft {
            kind,
            source: e.pattern,
            variants: Vec::new(),
            case: e.case.unwrap_or(case),
            level: e.level.unwrap_or(level),
            message: e.message,
            suggest: e.suggest,
        }
    }

    /// A term matches any case, and suggests itself.
    fn term(t: Term, level: Level) -> Result<Draft, String> {
        let term = prose::normalize(&t.term);
        if term.is_empty() {
            return Err("terms can't be empty".into());
        }
        Ok(Draft {
            kind: EntryKind::Term,
            source: term,
            variants: t.variants,
            case: Case::Insensitive,
            level: t.level.unwrap_or(level),
            message: t.message,
            suggest: Vec::new(),
        })
    }

    /// Its message and suggestions, whose placeholders are the common ones
    /// and then `vars`, a pattern's groups. Errors name the entry.
    fn wording(&self, vars: &[&str], rule_message: &mut RuleMessage) -> Result<Wording, String> {
        let at = |e: String| format!("`{}`: {e}", self.source);
        let suggest = match self.kind {
            EntryKind::Term => vec![Template::literal(&self.source)],
            _ => self
                .suggest
                .iter()
                .map(|s| {
                    if s.trim().is_empty() {
                        return Err("suggestions can't be empty".to_owned());
                    }
                    Template::suggestion(s, vars)
                })
                .collect::<Result<_, _>>()
                .map_err(at)?,
        };
        let message = match self.message.as_deref() {
            Some(m) => Template::parse(m, vars).map_err(at)?,
            None => match rule_message.parse(vars).map_err(at)? {
                Some(t) => t,
                None if !suggest.is_empty() => Template::parse(SUGGEST_MESSAGE, &[])?,
                None => {
                    return Err(format!(
                        "`{}` has no message: add `message` to the rule or to this entry",
                        self.source
                    ));
                }
            },
        };
        if message.uses_suggestion() && suggest.is_empty() {
            return Err(at(
                "the message uses {suggestion}, but the entry has no `suggest`".into(),
            ));
        }
        Ok(Wording { message, suggest })
    }
}

/// The rule's `message`, parsed once for every entry without groups, and
/// again for each that has some.
struct RuleMessage<'file> {
    written: Option<&'file str>,
    common: Option<Result<Template, String>>,
    /// Whether an entry used it, so it was checked.
    used: bool,
}

impl<'file> RuleMessage<'file> {
    fn new(written: Option<&'file str>) -> Self {
        RuleMessage {
            written,
            common: None,
            used: false,
        }
    }

    /// The message with placeholders `vars`, or `None` when the rule has
    /// none.
    fn parse(&mut self, vars: &[&str]) -> Result<Option<Template>, String> {
        let Some(m) = self.written else {
            return Ok(None);
        };
        self.used = true;
        if vars.is_empty() {
            let common = self.common.get_or_insert_with(|| Template::parse(m, &[]));
            return common.clone().map(Some);
        }
        Template::parse(m, vars).map(Some)
    }

    /// A message no entry or check used is still checked, so a mistake in
    /// it is never silent.
    fn check_unused(&mut self) -> Result<(), String> {
        if !self.used {
            self.parse(&[])?;
        }
        Ok(())
    }
}

/// The placeholders a pattern's groups fill. A group can't take the name
/// of a common placeholder.
fn group_vars(pattern: &str, matcher: &PieceMatcher) -> Result<Vec<String>, String> {
    let slots = match matcher {
        PieceMatcher::Regex(r) => capture_slots(r.capture_names()),
        PieceMatcher::Fancy(r) => capture_slots(r.capture_names()),
        PieceMatcher::Literal(_) => Vec::new(),
    };
    slots
        .into_iter()
        .map(|Slot { name, .. }| {
            if COMMON.contains(&name.as_ref()) {
                return Err(format!(
                    "`{pattern}`: a group can't be named `{name}`, since every message has {{{name}}}"
                ));
            }
            Ok(name.into_owned())
        })
        .collect()
}

/// A placeholder a pattern's groups fill, and the group it reads.
struct Slot<'regex> {
    name: Cow<'regex, str>,
    group: usize,
}

/// The groups a match reads for `target`'s placeholders, once, so
/// matching doesn't work them out again in every block.
fn groups<'a>(target: Target, names: impl Iterator<Item = Option<&'a str>>) -> Vec<usize> {
    if !target.captures {
        return Vec::new();
    }
    capture_slots(names).iter().map(|s| s.group).collect()
}

/// Each placeholder a pattern's groups fill: `{1}` to `{n}`, then each
/// named group by its name.
fn capture_slots<'a>(names: impl Iterator<Item = Option<&'a str>>) -> Vec<Slot<'a>> {
    let names: Vec<Option<&str>> = names.collect();
    let numbered = (1..names.len()).map(|group| Slot {
        name: Cow::Owned(group.to_string()),
        group,
    });
    let named = names.iter().enumerate().filter_map(|(group, n)| {
        Some(Slot {
            name: Cow::Borrowed((*n)?),
            group,
        })
    });
    numbered.chain(named).collect()
}

/// An `except` item, compiled with the rule's case.
fn exception(e: &Except, case: Case) -> Result<Exception, String> {
    match e {
        Except::Text(t) => {
            let t = prose::normalize(t);
            Ok(Exception::Text(if case == Case::Insensitive {
                t.to_lowercase()
            } else {
                t
            }))
        }
        Except::Pattern(p) => {
            // Parsed alone first, so `a)|(b` can't escape the anchors.
            regex_syntax::parse(p).map_err(|e| format!("invalid except pattern `{p}`: {e}"))?;
            let flags = if case == Case::Insensitive {
                "(?i)"
            } else {
                ""
            };
            let regex = Regex::new(&format!("{flags}^(?:{p})$"))
                .map_err(|e| format!("invalid except pattern `{p}`: {e}"))?;
            Ok(Exception::Pattern {
                source: p.clone(),
                regex,
            })
        }
    }
}

/// Adds what check `check` of rule `rule` wants found to the shared
/// matcher. Its patterns' groups come with each match.
fn add_wants(parts: &mut Parts, check: &Check, rule: RuleIndex, at: u32) -> Result<(), String> {
    let key = check.spec().key;
    for (tag, want) in check.wants().into_iter().enumerate() {
        let (piece, case) = match &want {
            Want::Literal(word, case) => (literal_piece(word, word, *case), *case),
            Want::Pattern(pattern) => (pattern_piece(pattern, Case::Sensitive), Case::Sensitive),
        };
        let piece = piece.map_err(|e| format!("`{key}`: {e}"))?;
        let target = Target {
            rule,
            entry: EntryRef::Check(at),
            tag: tag as u32,
            left: piece.left,
            right: piece.right,
            captures: matches!(want, Want::Pattern(_)),
        };
        parts.push(piece.matcher, target, case);
    }
    Ok(())
}

/// A check's message and suggestions: the rule's message, or the check's
/// own default, with the check's placeholders.
fn check_wording(check: &Check, rule_message: Option<&str>) -> Result<Wording, String> {
    let spec = check.spec();
    let at = |e: String| format!("`{}`: {e}", spec.key);
    let suggest: Vec<Template> = spec
        .suggest
        .iter()
        .map(|s| Template::suggestion(s, spec.vars))
        .collect::<Result<_, _>>()
        .map_err(at)?;
    let message = Template::parse(rule_message.unwrap_or(spec.message), spec.vars).map_err(at)?;
    if message.uses_suggestion() && suggest.is_empty() {
        return Err(format!(
            "`{}` has no suggestions, so its message can't use {{suggestion}}",
            spec.key
        ));
    }
    Ok(Wording { message, suggest })
}

/// One thing an entry matches: a `words` entry has one per brace
/// expansion; a term one per spelling; a pattern has one.
pub(super) struct Piece {
    pub matcher: PieceMatcher,
    pub left: bool,
    pub right: bool,
}

pub(super) enum PieceMatcher {
    /// Normalized; searched by the automaton for the entry's case mode.
    Literal(String),
    Regex(Regex),
    Fancy(fancy_regex::Regex),
}

/// What an entry compiles to. The compiler and `explain` both use this,
/// so what `explain` shows is what runs. `variants` are a term's other
/// spellings.
pub(super) fn pieces(
    kind: EntryKind,
    source: &str,
    variants: &[String],
    case: Case,
) -> Result<Vec<Piece>, String> {
    match kind {
        EntryKind::Word => word_pieces(source, case),
        EntryKind::Pattern => Ok(vec![pattern_piece(source, case)?]),
        EntryKind::Term => {
            let mut out = vec![literal_piece(source, source, case)?];
            for v in variants {
                out.extend(word_pieces(v, case)?);
            }
            Ok(out)
        }
    }
}

fn word_pieces(pattern: &str, case: Case) -> Result<Vec<Piece>, String> {
    expand::expand(pattern)?
        .iter()
        .map(|word| literal_piece(word, pattern, case))
        .collect()
}

/// One literal word, matched on word edges where it has them. `written`
/// is the entry it came from, for errors.
fn literal_piece(word: &str, written: &str, case: Case) -> Result<Piece, String> {
    let word = prose::normalize(word);
    let (Some(first), Some(last)) = (word.chars().next(), word.chars().next_back()) else {
        return Err(format!("`{written}`: words can't be empty"));
    };
    let (left, right) = (prose::is_word_char(first), prose::is_word_char(last));
    let matcher = if case == Case::Insensitive && !word.is_ascii() {
        // Aho-Corasick folds ASCII only; Unicode case folding needs a regex.
        let source = format!("(?i){}", regex::escape(&word));
        PieceMatcher::Regex(Regex::new(&source).map_err(|e| e.to_string())?)
    } else {
        PieceMatcher::Literal(word)
    };
    Ok(Piece {
        matcher,
        left,
        right,
    })
}

fn pattern_piece(pattern: &str, case: Case) -> Result<Piece, String> {
    let source = if case == Case::Insensitive {
        format!("(?i){pattern}")
    } else {
        pattern.to_owned()
    };
    let matcher = match Regex::new(&source) {
        Ok(regex) => PieceMatcher::Regex(regex),
        Err(linear) => {
            // Only lookaround and backreferences justify backtracking.
            let fancy = fancy_regex::RegexBuilder::new(&source)
                .backtrack_limit(BACKTRACK_LIMIT)
                .build()
                .map_err(|_| format!("invalid pattern `{pattern}`: {linear}"))?;
            PieceMatcher::Fancy(fancy)
        }
    };
    Ok(Piece {
        matcher,
        left: false,
        right: false,
    })
}

/// A rule file's text as the rule `test/rule`.
#[cfg(test)]
fn source(text: &str) -> RuleSource<'_> {
    RuleSource {
        id: "test/rule".into(),
        origin: "test/rule".into(),
        text: Cow::Borrowed(text),
    }
}

/// Compiles one rule file's text, or says why it didn't load.
#[cfg(test)]
pub(crate) fn compile_one(text: &str) -> Result<RuleSet, String> {
    let compiled = RuleSet::compile([source(text)]);
    match compiled.errors.into_iter().next() {
        Some(e) => Err(e.to_string()),
        None => Ok(compiled.rules),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Engine, EntryRef, Form};

    #[test]
    fn a_rule_needs_a_matcher() {
        let e = compile_one("schema = 1\nmessage = \"m\"").unwrap_err();
        assert!(e.contains("nothing to match"), "{e}");
        assert!(e.contains("`repeat`, `consistent`,"), "{e}");
        assert!(e.ends_with("`acronyms`, `metric`)"), "{e}");
    }

    #[test]
    fn heading_levels_need_headings_in_scope() {
        let e = compile_one(
            "schema = 1\nmessage = \"m\"\nwords = [\"a\"]\nscope = [\"paragraph\"]\nheading-levels = [1]",
        )
        .unwrap_err();
        assert!(e.contains("`heading-levels` chooses headings"), "{e}");
        compile_one("schema = 1\nmessage = \"m\"\nwords = [\"a\"]\nheading-levels = [1]").unwrap();
    }

    #[test]
    fn check_messages_name_the_check() {
        let e = compile_one(
            "schema = 1\nmessage = \"{count} words\"\nrepeat = true\nlimit = { max = 3 }",
        )
        .unwrap_err();
        assert!(
            e.contains("`repeat`: unknown placeholder `{count}` in message"),
            "{e}"
        );
        compile_one("schema = 1\nmessage = \"{count} words\"\nlimit = { max = 3 }").unwrap();
    }

    #[test]
    fn check_wants_share_the_matcher() {
        let set = compile_one(
            "schema = 1\nconsistent = [[\"a b\", \"ab\"]]\nacronyms = {}\nwords = [{ match = \"x\", message = \"m\" }]",
        )
        .unwrap();
        let m = &set.matcher;
        let literals = &m.insensitive.as_ref().unwrap().targets;
        let entries: Vec<(EntryRef, u32)> = literals
            .iter()
            .flatten()
            .map(|t| (t.entry, t.tag))
            .collect();
        assert_eq!(
            entries,
            [
                (EntryRef::Entry(0), 0),
                (EntryRef::Check(0), 0),
                (EntryRef::Check(0), 1)
            ]
        );
        let patterns: Vec<(EntryRef, u32, bool)> = m
            .set_members
            .iter()
            .map(|m| (m.target.entry, m.target.tag, m.target.captures))
            .collect();
        assert_eq!(
            patterns,
            [(EntryRef::Check(1), 0, true), (EntryRef::Check(1), 1, true)]
        );
    }

    #[test]
    fn entries_without_any_message_are_errors() {
        let e = compile_one("schema = 1\nwords = [\"a\"]").unwrap_err();
        assert!(e.contains("has no message"), "{e}");
    }

    #[test]
    fn suggestions_get_a_default_message() {
        let set =
            compile_one("schema = 1\nwords = [{ match = \"a\", suggest = [\"b\"] }]").unwrap();
        let w = set.rules[0].wording(EntryRef::Entry(0)).unwrap();
        assert_eq!(w.render("a", "r", &[]), "Prefer: b.");
    }

    #[test]
    fn suggestion_placeholder_needs_suggestions() {
        let e =
            compile_one("schema = 1\nmessage = \"Use {suggestion}\"\nwords = [\"a\"]").unwrap_err();
        assert!(e.contains("has no `suggest`"), "{e}");
    }

    #[test]
    fn unsupported_schema_is_an_error() {
        let e = compile_one("schema = 2\nmessage = \"m\"\nwords = [\"a\"]").unwrap_err();
        assert!(e.contains("schema 2"), "{e}");
    }

    #[test]
    fn invalid_patterns_name_the_pattern() {
        let e = compile_one("schema = 1\nmessage = \"m\"\npatterns = [\"(a\"]").unwrap_err();
        assert!(e.contains("`(a`"), "{e}");
    }

    #[test]
    fn lookaround_falls_back_to_backtracking() {
        let set =
            compile_one("schema = 1\nmessage = \"m\"\npatterns = [\"fast(?= track)\"]").unwrap();
        assert_eq!(set.matcher.fancy.len(), 1);
        assert!(set.matcher.set.is_none());
    }

    #[test]
    fn broken_rules_are_left_out() {
        let good = source("schema = 1\nmessage = \"m\"\nwords = [\"a\"]");
        let bad = RuleSource {
            id: "test/bad".into(),
            origin: "bad.toml".into(),
            text: Cow::Borrowed("schema = 1\nwords = [\"b\"]"),
        };
        let compiled = RuleSet::compile([bad, good]);
        assert_eq!(compiled.errors.len(), 1);
        let set = compiled.rules;
        assert_eq!(set.len(), 1);
        let targets = &set.matcher.insensitive.as_ref().unwrap().targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0][0].rule, RuleIndex::new(0));
    }

    #[test]
    fn patterns_too_large_to_filter_together_still_load() {
        let sources = [("test/few", "['a\\d']"), ("test/many", "['b\\d', 'c\\d']")]
            .into_iter()
            .map(|(id, patterns)| RuleSource {
                id: id.into(),
                origin: id.into(),
                text: Cow::Owned(format!(
                    "schema = 1\nmessage = \"m\"\npatterns = {patterns}"
                )),
            });
        let Compiled {
            rules: set,
            errors,
            warnings,
        } = RuleSet::compile_with(sources, 1);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(set.len(), 2);
        assert!(set.matcher.set.is_none());
        assert_eq!(set.matcher.set_members.len(), 3);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("`test/many` has the most (2)"),
            "{warnings:?}"
        );
    }

    #[test]
    fn forms_show_what_runs() {
        let set = compile_one(
            r#"schema = 1
            message = "m"
            words = ["synerg{y,ies}", "etc.", "über"]
            patterns = ['\d+%', 'fast(?= track)']"#,
        )
        .unwrap();
        let rule = &set.rules[0];
        let forms: Vec<Vec<Form>> = (0..5).map(|i| rule.forms(i).unwrap()).collect();
        let literal = |text: &str, right| Form {
            engine: Engine::Literal,
            text: text.into(),
            left: true,
            right,
        };
        assert_eq!(
            forms[0],
            [literal("synergy", true), literal("synergies", true)]
        );
        assert_eq!(forms[1], [literal("etc.", false)]);
        assert_eq!(forms[2][0].engine, Engine::Regex);
        assert_eq!(forms[2][0].text, "(?i)über");
        assert_eq!(forms[3][0].text, "(?i)\\d+%");
        assert_eq!(forms[4][0].engine, Engine::Backtracking);
    }

    #[test]
    fn group_placeholders_need_groups() {
        let e =
            compile_one("schema = 1\nwords = [{ match = \"a\", message = \"{1}\" }]").unwrap_err();
        assert_eq!(
            e,
            "test/rule: `a`: unknown placeholder `{1}` in message \
             (only a pattern's groups are numbered, and this has none)"
        );
        let e =
            compile_one("schema = 1\nmessage = \"{2}\"\npatterns = ['(a)', '(b)(c)']").unwrap_err();
        assert!(e.contains("`(a)`: unknown placeholder `{2}`"), "{e}");
        let e = compile_one("schema = 1\npatterns = [{ match = '(a)', suggest = [\"{x}\"] }]")
            .unwrap_err();
        assert!(e.contains("`{x}` in suggestion"), "{e}");
        let e = compile_one("schema = 1\nmessage = \"m\"\npatterns = ['(?<rule>a)']").unwrap_err();
        assert!(e.contains("a group can't be named `rule`"), "{e}");
    }

    #[test]
    fn a_rule_message_with_groups_suits_patterns_with_them() {
        let set = compile_one(
            "schema = 1\nmessage = \"No {1}.\"\npatterns = ['(a)b', '(c)(d)', { match = 'e', message = \"m\" }]",
        )
        .unwrap();
        let targets: Vec<bool> = set
            .matcher
            .set_members
            .iter()
            .map(|m| m.target.captures)
            .collect();
        assert_eq!(targets, [true, true, false]);
    }

    #[test]
    fn an_unused_rule_message_is_still_checked() {
        let e = compile_one(
            "schema = 1\nmessage = \"{mtach}\"\nwords = [{ match = \"a\", message = \"m\" }]",
        )
        .unwrap_err();
        assert!(e.contains("did you mean `{match}`?"), "{e}");
    }

    #[test]
    fn except_patterns_stay_anchored() {
        let e = compile_one(
            "schema = 1\nmessage = \"m\"\nwords = [\"a\"]\nexcept = [{ pattern = 'a)|(b' }]",
        )
        .unwrap_err();
        assert!(e.contains("invalid except pattern `a)|(b`"), "{e}");
        let set = compile_one(
            "schema = 1\nmessage = \"m\"\nwords = [\"a\"]\nexcept = [{ pattern = 'x|y' }]",
        )
        .unwrap();
        let rule = &set.rules[0];
        assert!(rule.is_excepted("X"));
        assert!(!rule.is_excepted("xy"));
    }

    #[test]
    fn terms_need_text_and_bring_a_message() {
        let e = compile_one("schema = 1\nterms = [\" \"]").unwrap_err();
        assert!(e.contains("terms can't be empty"), "{e}");
        let set =
            compile_one("schema = 1\nterms = [{ term = \"a{b}\", match = [\"a b\"] }]").unwrap();
        let rule = &set.rules[0];
        let w = rule.wording(EntryRef::Entry(0)).unwrap();
        assert_eq!(w.suggestions("ab", "R", &[]), ["a{b}"]);
        let forms = rule.forms(0).unwrap();
        assert_eq!(
            forms.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(),
            ["a{b}", "a b"]
        );
    }

    #[test]
    fn rules_are_found_by_id_or_name() {
        let sources = ["default/hedging", "default/jargon", "project/jargon"]
            .into_iter()
            .map(|id| RuleSource {
                id: id.into(),
                origin: id.into(),
                text: Cow::Borrowed("schema = 1\nmessage = \"m\"\nwords = [\"a\"]"),
            });
        let set = RuleSet::compile(sources).rules;
        let index = |query| set.find(query).map(|r| r.index.as_usize());
        assert_eq!(index("default/jargon"), Ok(1));
        assert_eq!(index("hedging"), Ok(0));
        assert_eq!(
            set.find("jargon").unwrap_err(),
            "several rules are named `jargon`: `default/jargon`, `project/jargon`"
        );
        assert_eq!(
            set.find("default/hedgin").unwrap_err(),
            "no rule has the id `default/hedgin` (did you mean `default/hedging`?)"
        );
        assert_eq!(
            set.find("hedgin").unwrap_err(),
            "no rule has the id `hedgin` (did you mean `default/hedging`?)"
        );
    }

    #[test]
    fn names_come_from_stems() {
        assert_eq!(name_from_stem("needless-variants"), "Needless variants");
    }
}
