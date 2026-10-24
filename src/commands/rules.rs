//! `rules` (list, test, export) and `explain`.

use std::fs;
use std::path::Path;

use proserunner::config::{self, Setting};
use proserunner::ctx::Ctx;
use proserunner::english;
use proserunner::explain::{self, List};
use proserunner::ignores;
use proserunner::rules::{self, DEFAULT_SOURCE, RuleSet};
use proserunner::{Error, Result, examples};

use super::{EXIT_ISSUES, EXIT_OK, console, setup};
use crate::cli::{RulesCommand, ScopeArgs};

pub(crate) fn explain_command(ctx: &Ctx, query: &str, config_file: Option<&Path>) -> Result<u8> {
    let setup = setup::setup(ctx, ctx.project(), config_file, /*use_ignores=*/ false)?;
    let rule = setup.rules.find(query).map_err(Error::Usage)?;
    let mut read = Vec::new();
    for scope in setup::applied_scopes(ctx, &setup.config, setup.project.as_deref()) {
        if let Some(file) = ignores::read(ctx, &scope)? {
            read.push((ctx.display(&ignores::path(ctx, &scope)?), file));
        }
    }
    let lists: Vec<List<'_>> = read
        .iter()
        .map(|(path, file)| List::new(path, file))
        .collect();
    let text = explain::render(rule, &setup.config, &lists);
    console::stdout(|out| out.write_all(text.as_bytes()))?;
    Ok(EXIT_OK)
}

pub(crate) fn rules_command(
    ctx: &Ctx,
    command: Option<RulesCommand>,
    config_file: Option<&Path>,
) -> Result<u8> {
    match command {
        None | Some(RulesCommand::List) => {
            let setup = setup::setup(ctx, ctx.project(), config_file, /*use_ignores=*/ false)?;
            let rules = setup.rules.rules();
            let width = rules.iter().map(|r| r.id.len()).max().unwrap_or(0);
            console::stdout(|out| {
                rules.iter().try_for_each(|r| {
                    let level = setup.policy.level(r.index).map_or("off", |l| l.as_str());
                    writeln!(out, "{:<width$}  {level:<7}  {}", r.id, r.name)
                })
            })?;
            Ok(EXIT_OK)
        }
        Some(RulesCommand::Test { dirs }) => {
            let (rules, broken) = if dirs.is_empty() {
                let setup =
                    setup::setup(ctx, ctx.project(), config_file, /*use_ignores=*/ false)?;
                (setup.rules, setup.broken_rules)
            } else {
                let mut sources = Vec::new();
                for dir in &dirs {
                    let path = ctx.absolute(dir);
                    let prefix = path
                        .file_name()
                        .map_or_else(|| "rules".to_owned(), |n| n.to_string_lossy().into_owned());
                    sources.extend(setup::dir_rules(ctx, &path, &prefix, "rules test")?);
                }
                setup::compile(sources)?
            };
            let failures = examples::run(&rules);
            console::stdout(|out| {
                failures
                    .iter()
                    .try_for_each(|f| writeln!(out, "{}: {}: {}", f.origin, f.rule, f.problem))
            })?;
            let n = english::count(rules.len(), "rule");
            let problems = failures.len() + broken;
            if problems == 0 {
                console::status(ctx, &format!("{n}; every example passes."));
                Ok(EXIT_OK)
            } else {
                console::status(
                    ctx,
                    &format!("{n}; {}.", english::count(problems, "problem")),
                );
                Ok(EXIT_ISSUES)
            }
        }
        Some(RulesCommand::Set { rule, level, scope }) => {
            set(ctx, &rule, level, scope.into(), config_file)
        }
        Some(RulesCommand::Export { dir, rules }) => export(ctx, &dir, &rules),
    }
}

/// `rules set`: writes a rule's level into the user or project config,
/// with the full id when given a name, then says if another layer's
/// setting wins over it.
fn set(
    ctx: &Ctx,
    query: &str,
    level: Setting,
    scope: ScopeArgs,
    config_file: Option<&Path>,
) -> Result<u8> {
    let setup = setup::setup(ctx, ctx.project(), config_file, /*use_ignores=*/ false)?;
    let id = rule_key(&setup.rules, query)?;
    let target = setup::write_scope(setup.project.as_deref(), scope)?;
    let changed = config::set_rule_level(ctx, &target, &id, level)?;
    let path = ctx.display(&target.dir(ctx)?.join(config::FILE));
    if changed {
        console::status(ctx, &format!("Set {id} to {level} in {path}."));
    } else {
        console::status(ctx, &format!("{id} is already {level} in {path}."));
    }
    let config = config::load(ctx, setup.project.as_deref(), config_file)?;
    match config.rule_settings(&id).level {
        Some(set) if set.value == level => {}
        Some(set) => console::warn(&format!(
            "{} sets {id} to {}, and wins over {path}",
            set.origin, set.value
        )),
        None => console::warn(&format!(
            "config here sets `inherit = false`, which leaves {path} out"
        )),
    }
    Ok(EXIT_OK)
}

/// The `[rule."…"]` key `query` stands for: a rule's whole id, or a
/// `source/*` pattern for a source this run loads rules from.
fn rule_key(rules: &RuleSet, query: &str) -> Result<String> {
    let key = match config::pattern_source(query) {
        Some(source) => rules.find_source(source).map(|_| query.to_owned()),
        None => rules.find(query).map(|rule| rule.id.clone()),
    };
    key.map_err(Error::Usage)
}

/// `rules export`: copies built-in rules into `dir`, overwriting none,
/// and says how to use them without each issue reported twice.
fn export(ctx: &Ctx, dir: &Path, names: &[String]) -> Result<u8> {
    let files = chosen_builtins(names).map_err(Error::Usage)?;
    for file in &files {
        let path = dir.join(format!("{}.toml", file.stem));
        if path.exists() {
            return Err(Error::Usage(format!(
                "{} already exists; not overwriting it",
                path.display()
            )));
        }
    }
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    for file in &files {
        let path = dir.join(format!("{}.toml", file.stem));
        fs::write(&path, file.text).map_err(|e| Error::io(&path, e))?;
    }
    let shown = settings_path(&ctx.display(dir));
    if names.is_empty() {
        console::status(
            ctx,
            &format!(
                "Wrote {} rules to {shown}/.\n\
                 To use them in place of the built-ins, set `rules = [\"{shown}\"]` in config.\n\
                 With \"default\" there too, each issue would be reported twice.",
                files.len()
            ),
        );
    } else {
        let names: Vec<String> = files.iter().map(|f| format!("{}.toml", f.stem)).collect();
        let (them, builtins) = if files.len() == 1 {
            ("it", "built-in")
        } else {
            ("them", "built-ins")
        };
        let off: String = files
            .iter()
            .map(|f| {
                format!(
                    "\n  [rule.\"{DEFAULT_SOURCE}/{}\"]\n  level = \"off\"",
                    f.stem
                )
            })
            .collect();
        console::status(
            ctx,
            &format!(
                "Wrote {} to {shown}/.\n\
                 To use {them} in place of the {builtins}, add the directory to `rules` in config, and turn the {builtins} off:\n  \
                 rules = [\"default\", \"{shown}\"]{off}",
                english::and_list(&names),
            ),
        );
    }
    Ok(EXIT_OK)
}

/// A path as config writes one: forward slashes, no trailing slash.
fn settings_path(shown: &str) -> String {
    shown.replace('\\', "/").trim_end_matches('/').to_owned()
}

/// The built-in rule files `names` choose, by name or id, or all of them
/// when there are none. An unknown name is an error with the closest.
fn chosen_builtins(names: &[String]) -> Result<Vec<&'static rules::BuiltinFile>, String> {
    let files = rules::builtin_files();
    if names.is_empty() {
        return Ok(files.iter().collect());
    }
    let builtin = RuleSet::compile(rules::builtin()).rules;
    let mut chosen: Vec<&rules::BuiltinFile> = Vec::new();
    for name in names {
        let rule = builtin.find(name)?;
        let file = files
            .iter()
            .find(|f| rule.id == format!("{DEFAULT_SOURCE}/{}", f.stem));
        if let Some(file) = file.filter(|f| !chosen.iter().any(|c| c.stem == f.stem)) {
            chosen.push(file);
        }
    }
    Ok(chosen)
}
