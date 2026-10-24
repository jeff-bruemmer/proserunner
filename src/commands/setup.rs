//! What a command loads before it runs: the project, its config, the
//! rules, and the policy that applies them.

use std::path::{Path, PathBuf};

use proserunner::config::{self, Config, RuleDir};
use proserunner::ctx::{Ctx, ListScope};
use proserunner::ignores::{self, Ignores};
use proserunner::policy::Policy;
use proserunner::rules::{self, Compiled, RuleSet, RuleSource};
use proserunner::{Error, Result, migrate};

use super::console;
use crate::cli::ScopeArgs;

/// Converts any 0.8 settings in the user config directory and `project`,
/// before anything reads or writes the new ones.
pub(crate) fn prepare(ctx: &Ctx, project: Option<PathBuf>) -> Result<Option<PathBuf>> {
    for note in migrate::config::auto(ctx, project.as_deref())? {
        console::status(ctx, &note);
    }
    Ok(project)
}

/// What a command that checks needs.
pub(crate) struct Setup {
    pub project: Option<PathBuf>,
    pub config: Config,
    pub rules: RuleSet,
    /// Rule files that didn't load. A check still runs, and exits 2.
    pub broken_rules: usize,
    pub policy: Policy,
}

pub(crate) fn setup(
    ctx: &Ctx,
    project: Option<PathBuf>,
    config_file: Option<&Path>,
    use_ignores: bool,
) -> Result<Setup> {
    let project = prepare(ctx, project)?;
    let config = config::load(ctx, project.as_deref(), config_file)?;
    let (rules, broken_rules) = load_rules(ctx, &config)?;
    let ignores = if use_ignores {
        ignores::load(ctx, &applied_scopes(ctx, &config, project.as_deref()))?
    } else {
        Ignores::default()
    };
    let policy = Policy::try_new(&config, &rules, ignores)?;
    Ok(Setup {
        project,
        config,
        rules,
        broken_rules,
        policy,
    })
}

/// The scopes whose ignore lists apply, after `inherit = false`.
pub(crate) fn applied_scopes(ctx: &Ctx, config: &Config, project: Option<&Path>) -> Vec<ListScope> {
    let mut scopes = Vec::new();
    if config.inherited.global && ctx.config_dir.is_some() {
        scopes.push(ListScope::Global);
    }
    if let Some(root) = project.filter(|_| config.inherited.project) {
        scopes.push(ListScope::Project(root.to_path_buf()));
    }
    scopes
}

/// The list a command writes: `--global` or `--project` if given, else
/// the project's inside a project, else the global one. The project is
/// the one found above the checked files or the working directory, never
/// the working directory itself.
pub(crate) fn write_scope(project: Option<&Path>, args: ScopeArgs) -> Result<ListScope> {
    match (args.global, args.project, project) {
        (true, _, _) => Ok(ListScope::Global),
        (_, true, None) => Err(Error::Usage(
            "--project: no .proserunner/ here or above; run 'proserunner init' to start a project"
                .into(),
        )),
        (_, _, Some(root)) => Ok(ListScope::Project(root.to_path_buf())),
        (_, _, None) => Ok(ListScope::Global),
    }
}

/// The rules config names, and how many rule files didn't load.
fn load_rules(ctx: &Ctx, config: &Config) -> Result<(RuleSet, usize)> {
    let mut sources = Vec::new();
    for dir in &config.rules.value {
        match dir {
            RuleDir::Builtin => sources.extend(rules::builtin()),
            RuleDir::Dir { path, prefix } => {
                sources.extend(dir_rules(ctx, path, prefix, &config.rules.origin)?)
            }
        }
    }
    compile(sources)
}

pub(crate) fn dir_rules(
    ctx: &Ctx,
    path: &Path,
    prefix: &str,
    origin: &str,
) -> Result<Vec<RuleSource<'static>>> {
    if !path.is_dir() {
        return Err(Error::config(
            origin,
            format!("the rule directory {} doesn't exist", ctx.display(path)),
        ));
    }
    let mut sources = rules::load_dir(path, prefix)?;
    for s in &mut sources {
        s.origin = ctx.display(Path::new(&s.origin));
    }
    Ok(sources)
}

/// Compiles rules, warning about each file that doesn't load and
/// counting them, and about anything else compiling found.
pub(crate) fn compile(sources: Vec<RuleSource<'_>>) -> Result<(RuleSet, usize)> {
    let Compiled {
        rules,
        errors,
        warnings,
        ..
    } = RuleSet::compile(sources);
    for e in &errors {
        console::warn(&e.to_string());
    }
    for w in &warnings {
        console::warn(w);
    }
    if rules.is_empty() {
        return Err(Error::Usage("no rules to check with".into()));
    }
    Ok((rules, errors.len()))
}
