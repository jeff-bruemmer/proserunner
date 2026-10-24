//! `rules` (list, test, export) and `explain`.

use std::fs;
use std::path::Path;

use proserunner::ctx::Ctx;
use proserunner::english;
use proserunner::explain::{self, List};
use proserunner::ignores;
use proserunner::rules;
use proserunner::{Error, Result, examples};

use super::{EXIT_ISSUES, EXIT_OK, console, setup};
use crate::cli::RulesCommand;

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
        Some(RulesCommand::Export { dir }) => {
            fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            for file in rules::builtin_files() {
                let path = dir.join(format!("{}.toml", file.stem));
                if path.exists() {
                    return Err(Error::Usage(format!(
                        "{} already exists; not overwriting it",
                        path.display()
                    )));
                }
            }
            for file in rules::builtin_files() {
                let path = dir.join(format!("{}.toml", file.stem));
                fs::write(&path, file.text).map_err(|e| Error::io(&path, e))?;
            }
            console::status(
                ctx,
                &format!(
                    "Wrote {} rules to {}.",
                    rules::builtin_files().len(),
                    dir.display()
                ),
            );
            Ok(EXIT_OK)
        }
    }
}
