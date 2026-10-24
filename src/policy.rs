//! What applies to each file once the rules have matched: inline control,
//! levels, `accept`, `[[overrides]]` and ignores. Pure.
//!
//! Filtering after matching means turning a rule off, or accepting one of
//! its matches, never changes what another rule finds.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::{self, Config, Paths, RuleSettings, Setting, Sourced};
use crate::english;
use crate::error::{Error, Result};
use crate::ignores::{self, Ignored, Ignores};
use crate::lint::{Exhausted, Issue, Linted};
use crate::report::LineIndex;
use crate::rules::{self, Level, RuleIndex, RuleSet};
use crate::scan::{Control, ControlError};
use crate::typo;

/// What config and ignores make of one rule set's issues.
#[derive(Debug)]
pub struct Policy {
    ids: Ids,
    /// Per rule: its own level, for when config sets none.
    own: Vec<Level>,
    /// Per rule: the level config sets, which replaces the rule's and its
    /// entries', or `off`, with the file that set it.
    levels: Vec<Option<Sourced<Setting>>>,
    /// Per rule: accepted match keys, each with the file that accepts it.
    rule_accept: Vec<HashMap<String, String>>,
    /// Match keys accepted for every rule, each with the file that
    /// accepts it.
    accept: HashMap<String, String>,
    overrides: Vec<Scoped>,
    ignored: HashMap<PathBuf, Vec<Ignored>>,
    /// Whether [`Policy::apply`] keeps what it hides.
    keep_suppressed: bool,
}

/// Rule ids, and the rule sources this run loads.
#[derive(Debug)]
struct Ids {
    /// Per rule: its id.
    ids: Vec<String>,
    index: HashMap<String, RuleIndex>,
    prefixes: HashSet<String>,
    /// Rules whose files didn't compile, which were reported then.
    left_out: HashSet<String>,
}

impl Ids {
    /// The rule `id` names. An id from a loaded source that names no rule
    /// is an error (a typo would otherwise do nothing), and so is one a
    /// typo away from a loaded rule (`defualt/hedging`), or a name with no
    /// source (`hedging`). An id from a source this run doesn't load is
    /// `None`, since a project may choose its own rules, and so is a rule
    /// whose file didn't compile.
    fn find(&self, id: &str) -> Result<Option<RuleIndex>, String> {
        if let Some(&i) = self.index.get(id) {
            return Ok(Some(i));
        }
        if self.left_out.contains(id) {
            return Ok(None);
        }
        let Some((prefix, _)) = id.split_once('/') else {
            return Err(self.not_an_id(id));
        };
        let ids = || self.ids.iter().map(String::as_str);
        if !self.prefixes.contains(prefix) {
            return match typo::closest_within(id, ids(), 2) {
                Some(c) => Err(format!(
                    "no rule has the id `{id}`, and no rules come from `{prefix}` (did you mean `{c}`?)"
                )),
                None => Ok(None),
            };
        }
        let hint = typo::closest(id, ids())
            .map(|c| format!(" (did you mean `{c}`?)"))
            .unwrap_or_default();
        Err(format!("no rule has the id `{id}`{hint}"))
    }

    /// Why `name`, which has no source, isn't an id, with the ids it may
    /// mean: those of the rules with that name, or of the closest name.
    fn not_an_id(&self, name: &str) -> String {
        let ids = || self.ids.iter().map(String::as_str);
        // A name a rule has is the closest to itself.
        let named: Vec<String> = typo::closest(name, ids().map(rules::id_name))
            .map(|close| {
                ids()
                    .filter(|id| rules::id_name(id) == close)
                    .map(|id| format!("`{id}`"))
                    .collect()
            })
            .unwrap_or_default();
        let hint = if named.is_empty() {
            ", such as `default/hedging`".to_owned()
        } else {
            format!(" (did you mean {}?)", english::or_list(&named))
        };
        format!("`{name}`: rule ids look like `source/name`{hint}")
    }

    /// Checks a `[rule."…"]` key: an id as [`Ids::find`] does, or a
    /// `source/*` pattern, whose source may be one this run doesn't load,
    /// unless it's a typo away from one it does.
    fn check_key(&self, key: &str) -> Result<(), String> {
        let Some(source) = config::pattern_source(key) else {
            return self.find(key).map(|_| ());
        };
        if self.prefixes.contains(source) {
            return Ok(());
        }
        match typo::closest_within(source, self.prefixes.iter().map(String::as_str), 2) {
            Some(c) => Err(format!(
                "no rules come from `{source}` (did you mean `{c}/*`?)"
            )),
            None => Ok(()),
        }
    }

    /// Adds an error to `errors` for each key of `tables` that
    /// [`Ids::check_key`] rejects, naming the file it's in.
    fn check_keys<'a>(
        &self,
        tables: &'a BTreeMap<String, RuleSettings>,
        origin: impl Fn(&'a RuleSettings) -> &'a str,
        errors: &mut Vec<Error>,
    ) {
        for (key, settings) in tables {
            if let Err(m) = self.check_key(key) {
                errors.push(Error::config(origin(settings), m));
            }
        }
    }
}

/// An `[[overrides]]` entry, with rule ids resolved.
#[derive(Debug)]
struct Scoped {
    paths: Paths,
    /// The file it's in, as shown.
    origin: String,
    accept: HashSet<String>,
    levels: HashMap<RuleIndex, Setting>,
    rule_accept: HashMap<RuleIndex, HashSet<String>>,
}

/// Why [`Policy::apply`] hid an issue, borrowing from the policy, so an
/// issue it drops costs no copies.
enum HiddenBy<'policy> {
    Directive(usize),
    Off(&'policy str),
    Accepted(&'policy str),
    Ignored(&'policy str),
}

impl HiddenBy<'_> {
    fn to_suppression(&self) -> Suppression {
        match *self {
            HiddenBy::Directive(at) => Suppression::Directive { at },
            HiddenBy::Off(origin) => Suppression::Off {
                origin: origin.to_owned(),
            },
            HiddenBy::Accepted(origin) => Suppression::Accepted {
                origin: origin.to_owned(),
            },
            HiddenBy::Ignored(origin) => Suppression::Ignored {
                origin: origin.to_owned(),
            },
        }
    }
}

impl Policy {
    /// Checks every rule id the config names, and reports each that's
    /// wrong. An id from a loaded source that names no rule is an error;
    /// one from a source this run doesn't load, or of a rule file that
    /// didn't compile, is skipped. `[rule."source/*"]` sets every rule
    /// from a source, and a rule's own table wins over it.
    pub fn try_new(config: &Config, rules: &RuleSet, ignores: Ignores) -> Result<Policy> {
        let ids = Ids {
            ids: rules.rules().iter().map(|r| r.id.clone()).collect(),
            index: rules
                .rules()
                .iter()
                .map(|r| (r.id.clone(), r.index))
                .collect(),
            prefixes: config
                .rules
                .value
                .iter()
                .map(|d| d.prefix().to_owned())
                .collect(),
            left_out: rules.left_out.iter().cloned().collect(),
        };
        let mut errors = Vec::new();
        ids.check_keys(&config.rule, origin, &mut errors);
        for o in &config.overrides {
            ids.check_keys(&o.rule, |_| o.origin.as_str(), &mut errors);
        }
        Error::all(errors)?;

        let mut levels = Vec::with_capacity(rules.len());
        let mut rule_accept = Vec::with_capacity(rules.len());
        for rule in rules.rules() {
            let settings = config.rule_settings(&rule.id);
            levels.push(settings.level);
            rule_accept.push(
                settings
                    .accept
                    .into_iter()
                    .map(|a| (ignores::key(&a.value), a.origin))
                    .collect(),
            );
        }

        let mut overrides = Vec::new();
        for o in &config.overrides {
            let mut scoped = Scoped {
                paths: o.paths.clone(),
                origin: o.origin.clone(),
                accept: o.accept.iter().map(|a| ignores::key(a)).collect(),
                levels: HashMap::new(),
                rule_accept: HashMap::new(),
            };
            for rule in rules.rules() {
                let settings = o.rule_settings(&rule.id);
                if let Some(level) = settings.level {
                    scoped.levels.insert(rule.index, level.value);
                }
                if !settings.accept.is_empty() {
                    scoped.rule_accept.insert(
                        rule.index,
                        settings
                            .accept
                            .iter()
                            .map(|a| ignores::key(&a.value))
                            .collect(),
                    );
                }
            }
            overrides.push(scoped);
        }

        let mut accept: HashMap<String, String> = config
            .accept
            .iter()
            .map(|a| (ignores::key(&a.value), a.origin.clone()))
            .collect();
        accept.extend(ignores.accept);
        Ok(Policy {
            ids,
            own: rules.rules().iter().map(|r| r.level).collect(),
            levels,
            rule_accept,
            accept,
            overrides,
            ignored: ignores.issues,
            keep_suppressed: false,
        })
    }

    /// Whether [`Policy::apply`] keeps the issues it hides, with why, in
    /// [`Applied::suppressed`]. It doesn't by default, since that costs a
    /// copy of each one.
    pub fn set_keep_suppressed(&mut self, keep: bool) {
        self.keep_suppressed = keep;
    }

    /// A rule's level outside any override, or `None` when it's off.
    pub fn level(&self, rule: RuleIndex) -> Option<Level> {
        match self.levels.get(rule.as_usize()).and_then(Option::as_ref) {
            Some(set) => set.value.level(),
            None => self.own.get(rule.as_usize()).copied(),
        }
    }

    /// The level config sets for a rule under the overrides `here`, the
    /// last one that sets it winning, with the file that set it; `None`
    /// when config sets none.
    fn setting_here<'p>(
        &'p self,
        here: &[&'p Scoped],
        rule: RuleIndex,
    ) -> Option<(Setting, &'p str)> {
        here.iter()
            .rev()
            .find_map(|o| Some((*o.levels.get(&rule)?, o.origin.as_str())))
            .or_else(|| {
                let set = self.levels.get(rule.as_usize())?.as_ref()?;
                Some((set.value, set.origin.as_str()))
            })
    }

    /// The file that accepts match key `k` for `rule` under the overrides
    /// `here`, if one does.
    fn accepted_by<'p>(&'p self, here: &[&'p Scoped], rule: RuleIndex, k: &str) -> Option<&'p str> {
        self.accept
            .get(k)
            .or_else(|| self.rule_accept.get(rule.as_usize())?.get(k))
            .map(String::as_str)
            .or_else(|| {
                here.iter()
                    .find(|o| {
                        o.accept.contains(k)
                            || o.rule_accept.get(&rule).is_some_and(|a| a.contains(k))
                    })
                    .map(|o| o.origin.as_str())
            })
    }

    /// What hides `issue` under the overrides `here`, if anything: inline
    /// control, a level of `off`, `accept`, or an ignore in `ignored`, the
    /// file's, the first of these that does. `setting` is what
    /// [`Policy::setting_here`] gives for its rule. `line` is the issue's
    /// line, which only an ignore with a `line` needs.
    fn hidden_by<'p>(
        &'p self,
        issue: &Issue,
        setting: Option<(Setting, &'p str)>,
        control: &Control<RuleIndex>,
        here: &[&'p Scoped],
        ignored: &'p [Ignored],
        line: &dyn Fn() -> Option<usize>,
    ) -> Option<HiddenBy<'p>> {
        let r = issue.rule;
        if let Some(at) = control.off_by(r, issue.span.start) {
            return Some(HiddenBy::Directive(at));
        }
        if let Some((Setting::Off, origin)) = setting {
            return Some(HiddenBy::Off(origin));
        }
        let k = ignores::key(&issue.matched);
        if let Some(origin) = self.accepted_by(here, r, &k) {
            return Some(HiddenBy::Accepted(origin));
        }
        let id = self.ids.ids.get(r.as_usize()).map(String::as_str);
        ignored
            .iter()
            .find(|g| {
                g.matched == k
                    && g.rule.as_deref().is_none_or(|rule| Some(rule) == id)
                    && g.line.is_none_or(|n| Some(n) == line())
            })
            .map(|g| HiddenBy::Ignored(&g.origin))
    }

    /// What survives in the file at `path` (absolute; `None` for standard
    /// input): the issues, each at the level that applies there, and the
    /// patterns that gave up where their rule is on. A directive that
    /// names an unknown rule is an error, as in config.
    ///
    /// `linted` comes from the rule set this policy was made for. Indexes
    /// from another set read as other rules, or as none, and never panic.
    pub fn apply(
        &self,
        path: Option<&Path>,
        source: &str,
        linted: Linted,
    ) -> Result<Applied, ControlError> {
        let control = Control::try_new(&linted.directives, source, |id| self.ids.find(id))?;
        let here: Vec<&Scoped> = match path {
            Some(p) => self
                .overrides
                .iter()
                .filter(|o| o.paths.is_match(p))
                .collect(),
            None => Vec::new(),
        };
        let ignored: &[Ignored] = path
            .and_then(|p| self.ignored.get(p))
            .map_or(&[], Vec::as_slice);
        let lines = ignored
            .iter()
            .any(|g| g.line.is_some())
            .then(|| LineIndex::new(source));

        let exhausted: Vec<Exhausted> = linted
            .exhausted
            .into_iter()
            .filter(|e| {
                control.allows(e.rule, e.at)
                    && self.setting_here(&here, e.rule).map(|(set, _)| set) != Some(Setting::Off)
            })
            .collect();
        let mut issues = Vec::with_capacity(linted.issues.len());
        let mut suppressed = Vec::new();
        for mut issue in linted.issues {
            let line = || {
                lines
                    .as_ref()
                    .map(|l| l.position(source, issue.span.start).line)
            };
            let setting = self.setting_here(&here, issue.rule);
            let hidden_by = self.hidden_by(&issue, setting, &control, &here, ignored, &line);
            if hidden_by.is_some() && !self.keep_suppressed {
                continue;
            }
            if let Some(level) = setting.and_then(|(set, _)| set.level()) {
                issue.level = level;
            }
            match hidden_by {
                None => issues.push(issue),
                Some(why) => suppressed.push(Suppressed {
                    issue,
                    why: why.to_suppression(),
                }),
            }
        }
        Ok(Applied {
            issues,
            exhausted,
            suppressed,
        })
    }
}

/// What [`Policy::apply`] keeps of a linted file.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Applied {
    /// The issues to report, each at the level that applies in the file.
    pub issues: Vec<Issue>,
    /// The patterns that gave up where their rule is on.
    pub exhausted: Vec<Exhausted>,
    /// What it hid, with why, in source order, when
    /// [`Policy::set_keep_suppressed`] asked for it; otherwise empty.
    pub suppressed: Vec<Suppressed>,
}

/// An issue [`Policy::apply`] hid, and why.
#[derive(Debug)]
#[non_exhaustive]
pub struct Suppressed {
    /// The issue, at its rule's own level, or the one config sets.
    pub issue: Issue,
    /// Why it wasn't reported.
    pub why: Suppression,
}

/// Why an issue wasn't reported. When several things hide one, it's the
/// first of these.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Suppression {
    /// An inline control comment turned its rule off.
    Directive {
        /// Where the comment's directive starts in the source.
        at: usize,
    },
    /// Config turned its rule off.
    Off {
        /// The file that did, as shown.
        origin: String,
    },
    /// Config or an ignore list accepts its match.
    Accepted {
        /// The file that does, as shown.
        origin: String,
    },
    /// An ignore list ignores it in this file.
    Ignored {
        /// The list, as shown.
        origin: String,
    },
}

impl Suppression {
    /// Its kind, as JSON output names it: `directive`, `off`, `accepted`
    /// or `ignored`.
    pub fn kind(&self) -> &'static str {
        match self {
            Suppression::Directive { .. } => "directive",
            Suppression::Off { .. } => "off",
            Suppression::Accepted { .. } => "accepted",
            Suppression::Ignored { .. } => "ignored",
        }
    }
}

/// The file a `[rule."…"]` table came from.
fn origin(settings: &RuleSettings) -> &str {
    settings
        .level
        .as_ref()
        .map(|l| l.origin.as_str())
        .or_else(|| settings.accept.first().map(|a| a.origin.as_str()))
        .unwrap_or("config")
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::config::{self, Layer, LayerKind};
    use crate::ctx::Ctx;
    use crate::lint;
    use crate::rules::{EntryRef, RuleSource};
    use crate::scan::{Format, Options};

    fn ctx() -> Ctx {
        Ctx::new("/p".into(), Some("/home/me".into()), None, false)
    }

    fn rules() -> RuleSet {
        let sources = [
            (
                "default/jargon",
                "schema = 1\nmessage = \"Jargon.\"\nwords = [\"synergy\", \"leverage\"]",
            ),
            (
                "default/hedging",
                "schema = 1\nmessage = \"Hedge.\"\nwords = [\"I think\"]",
            ),
        ]
        .into_iter()
        .map(|(id, text)| RuleSource {
            id: id.into(),
            origin: id.into(),
            text: Cow::Borrowed(text),
        });
        let compiled = RuleSet::compile(sources);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        compiled.rules
    }

    fn policy(text: &str, ignores: Ignores) -> Result<(RuleSet, Policy)> {
        let layer = Layer {
            kind: LayerKind::Project,
            base: "/p".into(),
            within: Some("/p".into()),
            origin: "config.toml".into(),
            group: 0,
            file: config::parse(text).unwrap(),
        };
        let config = config::merge(&ctx(), vec![layer])?;
        let rules = rules();
        let policy = Policy::try_new(&config, &rules, ignores)?;
        Ok((rules, policy))
    }

    fn found(rules: &RuleSet, policy: &Policy, path: &str, source: &str) -> Vec<(String, Level)> {
        let linted = lint::document(rules, Format::Markdown, source, Options::default());
        policy
            .apply(Some(Path::new(path)), source, linted)
            .unwrap()
            .issues
            .into_iter()
            .map(|i| (i.matched, i.level))
            .collect()
    }

    const TEXT: &str = "I think synergy and leverage.";

    #[test]
    fn levels_and_off() {
        let (r, p) = policy(
            "schema = 1\n[rule.\"default/hedging\"]\nlevel = \"off\"\n[rule.\"default/jargon\"]\nlevel = \"error\"",
            Ignores::default(),
        )
        .unwrap();
        assert_eq!(
            found(&r, &p, "/p/a.md", TEXT),
            [
                ("synergy".into(), Level::Error),
                ("leverage".into(), Level::Error)
            ]
        );
    }

    #[test]
    fn config_levels_replace_entry_levels() {
        let sources = vec![RuleSource {
            id: "default/jargon".into(),
            origin: "default/jargon".into(),
            text: Cow::Borrowed(
                "schema = 1\nmessage = \"m\"\nwords = [\"synergy\", { match = \"leverage\", level = \"error\" }]",
            ),
        }];
        let compiled = RuleSet::compile(sources);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        let rules = compiled.rules;
        let found = |config: &str| {
            let layer = Layer {
                kind: LayerKind::Project,
                base: "/p".into(),
                within: Some("/p".into()),
                origin: "config.toml".into(),
                group: 0,
                file: config::parse(config).unwrap(),
            };
            let config = config::merge(&ctx(), vec![layer]).unwrap();
            let policy = Policy::try_new(&config, &rules, Ignores::default()).unwrap();
            let levels: Vec<Level> = super::tests::found(&rules, &policy, "/p/legal/a.md", TEXT)
                .into_iter()
                .map(|(_, level)| level)
                .collect();
            levels
        };
        assert_eq!(found("schema = 1"), [Level::Warning, Level::Error]);
        assert_eq!(
            found("schema = 1\n[rule.\"default/jargon\"]\nlevel = \"hint\""),
            [Level::Hint, Level::Hint]
        );
        assert_eq!(
            found(
                "schema = 1\n[[overrides]]\npaths = [\"legal/\"]\nrule.\"default/jargon\".level = \"warning\""
            ),
            [Level::Warning, Level::Warning]
        );
        assert!(found("schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"").is_empty());
    }

    #[test]
    fn accept_is_whole_match_and_case_insensitive() {
        let (r, p) = policy(
            "schema = 1\naccept = [\"SYNERGY\", \"lever\"]\n[rule.\"default/hedging\"]\naccept = [\"i  think\"]",
            Ignores::default(),
        )
        .unwrap();
        assert_eq!(
            found(&r, &p, "/p/a.md", TEXT),
            [("leverage".into(), Level::Warning)]
        );
    }

    #[test]
    fn overrides_apply_to_their_paths_and_the_last_wins() {
        let (r, p) = policy(
            r#"schema = 1
            [rule."default/hedging"]
            level = "off"
            [[overrides]]
            paths = ["legal/"]
            rule."default/hedging".level = "error"
            accept = ["leverage"]
            [[overrides]]
            paths = ["legal/old/"]
            rule."default/hedging".level = "hint"
            "#,
            Ignores::default(),
        )
        .unwrap();
        assert_eq!(found(&r, &p, "/p/a.md", TEXT).len(), 2);
        assert_eq!(
            found(&r, &p, "/p/legal/a.md", TEXT),
            [
                ("I think".into(), Level::Error),
                ("synergy".into(), Level::Warning)
            ]
        );
        assert_eq!(
            found(&r, &p, "/p/legal/old/a.md", TEXT)[0],
            ("I think".into(), Level::Hint)
        );
    }

    #[test]
    fn misspelled_rule_ids_are_errors_with_a_hint() {
        let e = policy(
            "schema = 1\n[rule.\"default/hedgin\"]\nlevel = \"off\"",
            Ignores::default(),
        )
        .err()
        .unwrap()
        .to_string();
        assert_eq!(
            e,
            "config.toml: no rule has the id `default/hedgin` (did you mean `default/hedging`?)"
        );
        // Rules from a source this run doesn't load are fine.
        policy(
            "schema = 1\n[rule.\"custom/x\"]\nlevel = \"off\"",
            Ignores::default(),
        )
        .unwrap();
        // Unless the source is a typo away from a loaded rule.
        let e = policy(
            "schema = 1\n[rule.\"defualt/hedging\"]\nlevel = \"off\"",
            Ignores::default(),
        )
        .err()
        .unwrap()
        .to_string();
        assert_eq!(
            e,
            "config.toml: no rule has the id `defualt/hedging`, and no rules come from `defualt` (did you mean `default/hedging`?)"
        );
    }

    #[test]
    fn a_name_alone_gets_the_ids_it_may_mean() {
        let e = |text: &str| policy(text, Ignores::default()).err().unwrap().to_string();
        assert_eq!(
            e("schema = 1\n[rule.hedging]\nlevel = \"off\""),
            "config.toml: `hedging`: rule ids look like `source/name` (did you mean `default/hedging`?)"
        );
        assert_eq!(
            e("schema = 1\n[rule.hedgin]\nlevel = \"off\""),
            "config.toml: `hedgin`: rule ids look like `source/name` (did you mean `default/hedging`?)"
        );
        assert_eq!(
            e("schema = 1\n[rule.zzz]\nlevel = \"off\""),
            "config.toml: `zzz`: rule ids look like `source/name`, such as `default/hedging`"
        );
    }

    #[test]
    fn every_unknown_id_is_reported_at_once() {
        let e = policy(
            "schema = 1\n[rule.\"default/hedgin\"]\nlevel = \"off\"\n[rule.jargon]\nlevel = \"off\"\n\
             [[overrides]]\npaths = [\"x/\"]\nrule.\"defualt/*\".level = \"off\"",
            Ignores::default(),
        )
        .err()
        .unwrap();
        let lines: Vec<String> = e.each().iter().map(Error::to_string).collect();
        assert_eq!(
            lines,
            [
                "config.toml: no rule has the id `default/hedgin` (did you mean `default/hedging`?)",
                "config.toml: `jargon`: rule ids look like `source/name` (did you mean `default/jargon`?)",
                "config.toml: no rules come from `defualt` (did you mean `default/*`?)",
            ]
        );
    }

    #[test]
    fn suppressed_issues_say_why() {
        let mut ignores = Ignores::default();
        ignores.issues.insert(
            "/p/a.md".into(),
            vec![Ignored {
                rule: None,
                matched: "leverage".into(),
                line: None,
                origin: "ignores.toml".into(),
            }],
        );
        let (r, mut p) = policy(
            "schema = 1\naccept = [\"synergy\"]\n[rule.\"default/hedging\"]\nlevel = \"off\"",
            ignores,
        )
        .unwrap();
        let source =
            "I think synergy and leverage.\n\n<!-- proserunner-disable-next-line -->\nsynergy\n";
        let why = |p: &Policy| {
            let linted = lint::document(&r, Format::Markdown, source, Options::default());
            let applied = p.apply(Some(Path::new("/p/a.md")), source, linted).unwrap();
            assert!(applied.issues.is_empty());
            applied
                .suppressed
                .into_iter()
                .map(|s| (s.issue.matched, s.why))
                .collect::<Vec<_>>()
        };
        assert!(why(&p).is_empty(), "kept only when asked");
        p.set_keep_suppressed(true);
        assert_eq!(
            why(&p),
            [
                (
                    "I think".into(),
                    Suppression::Off {
                        origin: "config.toml".into()
                    }
                ),
                (
                    "synergy".into(),
                    Suppression::Accepted {
                        origin: "config.toml".into()
                    }
                ),
                (
                    "leverage".into(),
                    Suppression::Ignored {
                        origin: "ignores.toml".into()
                    }
                ),
                (
                    "synergy".into(),
                    Suppression::Directive {
                        at: source.find("proserunner-").unwrap()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_hidden_issue_has_the_level_config_sets() {
        let (r, mut p) = policy(
            "schema = 1\naccept = [\"synergy\"]\n[rule.\"default/jargon\"]\nlevel = \"error\"",
            Ignores::default(),
        )
        .unwrap();
        p.set_keep_suppressed(true);
        let linted = lint::document(&r, Format::Markdown, TEXT, Options::default());
        let applied = p.apply(None, TEXT, linted).unwrap();
        let hidden: Vec<(&str, Level)> = applied
            .suppressed
            .iter()
            .map(|s| (s.issue.matched.as_str(), s.issue.level))
            .collect();
        assert_eq!(hidden, [("synergy", Level::Error)]);
    }

    #[test]
    fn ignores_match_file_rule_and_line() {
        let mut ignores = Ignores::default();
        ignores
            .accept
            .insert("i think".into(), "ignores.toml".into());
        ignores.issues.insert(
            "/p/a.md".into(),
            vec![
                Ignored {
                    rule: Some("default/jargon".into()),
                    matched: "synergy".into(),
                    line: None,
                    origin: "ignores.toml".into(),
                },
                Ignored {
                    rule: None,
                    matched: "leverage".into(),
                    line: Some(3),
                    origin: "ignores.toml".into(),
                },
            ],
        );
        let (r, p) = policy("schema = 1", ignores).unwrap();
        let source = "I think synergy and leverage.\n\nLeverage again.";
        assert_eq!(
            found(&r, &p, "/p/a.md", source),
            [("leverage".into(), Level::Warning)]
        );
        assert_eq!(found(&r, &p, "/p/b.md", source).len(), 3);
    }

    #[test]
    fn inline_control_turns_rules_off_by_position() {
        let (r, p) = policy("schema = 1", Ignores::default()).unwrap();
        let source = "synergy\n\n<!-- proserunner-disable default/jargon -->\n\nsynergy, I think\n\n<!-- proserunner-enable -->\n<!-- proserunner-disable-next-line -->\nsynergy\n\nsynergy";
        let lines: Vec<usize> = {
            let index = LineIndex::new(source);
            let linted = lint::document(&r, Format::Markdown, source, Options::default());
            p.apply(None, source, linted)
                .unwrap()
                .issues
                .iter()
                .map(|i| index.position(source, i.span.start).line)
                .collect()
        };
        assert_eq!(lines, [1, 5, 11]);
    }

    #[test]
    fn patterns_that_gave_up_count_only_where_their_rule_is_on() {
        let source = "a\n\n<!-- proserunner-disable default/jargon -->\n\nb";
        let b = source.rfind('b').unwrap();
        let kept = |rules: &RuleSet, policy: &Policy, at| {
            let mut linted = lint::document(rules, Format::Markdown, source, Options::default());
            linted.exhausted = vec![Exhausted {
                rule: RuleIndex::new(0),
                entry: EntryRef::Entry(0),
                tag: 0,
                at,
            }];
            policy.apply(None, source, linted).unwrap().exhausted.len()
        };
        let (r, on) = policy("schema = 1", Ignores::default()).unwrap();
        assert_eq!(kept(&r, &on, 0), 1);
        assert_eq!(kept(&r, &on, b), 0);
        let (r, off) = policy(
            "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"",
            Ignores::default(),
        )
        .unwrap();
        assert_eq!(kept(&r, &off, 0), 0);
    }

    #[test]
    fn directives_check_ids_like_config() {
        let (r, p) = policy(
            "schema = 1\nrules = [\"default\", { path = \"x\", prefix = \"mine\" }]",
            Ignores::default(),
        )
        .unwrap();
        let apply = |source: &str| {
            let linted = lint::document(&r, Format::Markdown, source, Options::default());
            p.apply(None, source, linted)
        };
        let e = apply("<!-- proserunner-disable default/jargn -->").unwrap_err();
        assert_eq!(
            e.message,
            "no rule has the id `default/jargn` (did you mean `default/jargon`?)"
        );
        assert_eq!(e.at, "<!-- proserunner-disable ".len());
        assert!(apply("<!-- proserunner-disable mine/x -->").is_err());
        // A source this run doesn't load leaves the directive alone.
        assert_eq!(
            apply("<!-- proserunner-disable other/x -->\nsynergy")
                .unwrap()
                .issues
                .len(),
            1
        );
    }

    #[test]
    fn rules_whose_files_didnt_compile_are_skipped_not_misspelled() {
        let sources = [
            (
                "mine/ok",
                "schema = 1\nmessage = \"m\"\nwords = [\"synergy\"]",
            ),
            ("mine/broken", "schema = 1\nwords = ["),
        ]
        .into_iter()
        .map(|(id, text)| RuleSource::new(id, text));
        let compiled = RuleSet::compile(sources);
        assert_eq!(compiled.errors.len(), 1, "{:?}", compiled.errors);
        let rules = compiled.rules;
        let layer = Layer {
            kind: LayerKind::Project,
            base: "/p".into(),
            within: Some("/p".into()),
            origin: "config.toml".into(),
            group: 0,
            file: config::parse(
                "schema = 1\nrules = [{ path = \"x\", prefix = \"mine\" }]\n\
                 [rule.\"mine/broken\"]\nlevel = \"error\"",
            )
            .unwrap(),
        };
        let config = config::merge(&ctx(), vec![layer]).unwrap();
        let policy = Policy::try_new(&config, &rules, Ignores::default()).unwrap();
        let source = "<!-- proserunner-disable mine/broken -->\nsynergy";
        let linted = lint::document(&rules, Format::Markdown, source, Options::default());
        assert_eq!(policy.apply(None, source, linted).unwrap().issues.len(), 1);
        // A typo is still one.
        let source = "<!-- proserunner-disable mine/brokn -->";
        let linted = lint::document(&rules, Format::Markdown, source, Options::default());
        assert!(policy.apply(None, source, linted).is_err());
    }
}
