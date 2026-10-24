//! What applies to each file once the rules have matched: inline control,
//! levels, `accept`, `[[overrides]]` and ignores. Pure.
//!
//! Filtering after matching means turning a rule off, or accepting one of
//! its matches, never changes what another rule finds.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::{Config, Paths, RuleSettings, Setting};
use crate::error::{Error, Result};
use crate::ignores::{self, Ignored, Ignores};
use crate::lint::{Exhausted, Issue, Linted};
use crate::report::LineIndex;
use crate::rules::{Level, RuleIndex, RuleSet};
use crate::scan::{Control, ControlError};
use crate::typo;

/// What config and ignores make of one rule set's issues.
#[derive(Debug)]
pub struct Policy {
    ids: Ids,
    /// Per rule: its own level, for when config sets none.
    own: Vec<Level>,
    /// Per rule: the level config sets, which replaces the rule's and its
    /// entries', or `off`.
    levels: Vec<Option<Setting>>,
    /// Per rule: accepted match keys.
    rule_accept: Vec<HashSet<String>>,
    /// Match keys accepted for every rule.
    accept: HashSet<String>,
    overrides: Vec<Scoped>,
    ignored: HashMap<PathBuf, Vec<Ignored>>,
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
    /// typo away from a loaded rule (`defualt/hedging`). An id from a
    /// source this run doesn't load is `None`, since a project may choose
    /// its own rules, and so is a rule whose file didn't compile.
    fn find(&self, id: &str) -> Result<Option<RuleIndex>, String> {
        if let Some(&i) = self.index.get(id) {
            return Ok(Some(i));
        }
        if self.left_out.contains(id) {
            return Ok(None);
        }
        let ids = || self.ids.iter().map(String::as_str);
        let prefix = id.split('/').next().unwrap_or(id);
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
}

/// An `[[overrides]]` entry, with rule ids resolved.
#[derive(Debug)]
struct Scoped {
    paths: Paths,
    accept: HashSet<String>,
    levels: HashMap<RuleIndex, Setting>,
    rule_accept: HashMap<RuleIndex, HashSet<String>>,
}

impl Policy {
    /// Checks every rule id the config names. An id from a loaded source
    /// that names no rule is an error; one from a source this run doesn't
    /// load, or of a rule file that didn't compile, is skipped.
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
        let find = |id: &str, origin: &str| ids.find(id).map_err(|m| Error::config(origin, m));

        let mut levels: Vec<Option<Setting>> = vec![None; rules.len()];
        let mut rule_accept = vec![HashSet::new(); rules.len()];
        for (id, settings) in &config.rule {
            let Some(i) = find(id, origin(settings))? else {
                continue;
            };
            let i = i.as_usize();
            if let Some(level) = &settings.level {
                levels[i] = Some(level.value);
            }
            rule_accept[i].extend(settings.accept.iter().map(|a| ignores::key(&a.value)));
        }

        let mut overrides = Vec::new();
        for o in &config.overrides {
            let mut scoped = Scoped {
                paths: o.paths.clone(),
                accept: o.accept.iter().map(|a| ignores::key(a)).collect(),
                levels: HashMap::new(),
                rule_accept: HashMap::new(),
            };
            for (id, settings) in &o.rule {
                let Some(i) = find(id, &o.origin)? else {
                    continue;
                };
                if let Some(level) = &settings.level {
                    scoped.levels.insert(i, level.value);
                }
                scoped
                    .rule_accept
                    .entry(i)
                    .or_default()
                    .extend(settings.accept.iter().map(|a| ignores::key(&a.value)));
            }
            overrides.push(scoped);
        }

        let mut accept: HashSet<String> = config
            .accept
            .iter()
            .map(|a| ignores::key(&a.value))
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
        })
    }

    /// A rule's level outside any override, or `None` when it's off.
    pub fn level(&self, rule: RuleIndex) -> Option<Level> {
        match self.levels.get(rule.as_usize()).copied().flatten() {
            Some(set) => set.level(),
            None => self.own.get(rule.as_usize()).copied(),
        }
    }

    /// The level config sets for a rule under the overrides `here`, the
    /// last one that sets it winning; `None` when config sets none.
    fn setting_here(&self, here: &[&Scoped], rule: RuleIndex) -> Option<Setting> {
        here.iter()
            .rev()
            .find_map(|o| o.levels.get(&rule).copied())
            .or_else(|| self.levels.get(rule.as_usize()).copied().flatten())
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
                    && self.setting_here(&here, e.rule) != Some(Setting::Off)
            })
            .collect();
        let mut issues = Vec::with_capacity(linted.issues.len());
        for mut issue in linted.issues {
            let r = issue.rule;
            if !control.allows(r, issue.span.start) {
                continue;
            }
            let level = match self.setting_here(&here, r) {
                Some(set) => match set.level() {
                    Some(level) => level,
                    None => continue,
                },
                None => issue.level,
            };
            let k = ignores::key(&issue.matched);
            let accepted = self.accept.contains(&k)
                || self
                    .rule_accept
                    .get(r.as_usize())
                    .is_some_and(|a| a.contains(&k))
                || here.iter().any(|o| {
                    o.accept.contains(&k) || o.rule_accept.get(&r).is_some_and(|a| a.contains(&k))
                });
            let id = self.ids.ids.get(r.as_usize()).map(String::as_str);
            let line = || {
                lines
                    .as_ref()
                    .map(|l| l.position(source, issue.span.start).line)
            };
            let is_ignored = ignored.iter().any(|g| {
                g.matched == k
                    && g.rule.as_deref().is_none_or(|rule| Some(rule) == id)
                    && g.line.is_none_or(|n| Some(n) == line())
            });
            if accepted || is_ignored {
                continue;
            }
            issue.level = level;
            issues.push(issue);
        }
        Ok(Applied { issues, exhausted })
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
    fn ignores_match_file_rule_and_line() {
        let mut ignores = Ignores::default();
        ignores.accept.insert("i think".into());
        ignores.issues.insert(
            "/p/a.md".into(),
            vec![
                Ignored {
                    rule: Some("default/jargon".into()),
                    matched: "synergy".into(),
                    line: None,
                },
                Ignored {
                    rule: None,
                    matched: "leverage".into(),
                    line: Some(3),
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
