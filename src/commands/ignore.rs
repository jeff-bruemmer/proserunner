//! `ignore`: add, remove, list, clear, audit and clean ignore lists.

use std::io::{self, Write};
use std::path::Path;

use proserunner::config;
use proserunner::ctx::{Ctx, ListScope};
use proserunner::english;
use proserunner::ignores::{self, Entry, IgnoreFile, Store};
use proserunner::prose;
use proserunner::{Error, Result};

use super::{EXIT_ISSUES, EXIT_OK, console, setup};
use crate::cli::IgnoreCommand;

pub(crate) fn ignore_command(
    ctx: &Ctx,
    command: IgnoreCommand,
    config_file: Option<&Path>,
) -> Result<u8> {
    let project = setup::prepare(ctx, ctx.project())?;
    let project = project.as_deref();
    match command {
        IgnoreCommand::Add { words, scope: args } => {
            let scope = setup::write_scope(project, args)?;
            // Only `--config` can leave out the list written by default.
            if let Some(file) = config_file.filter(|_| !args.given()) {
                let config = config::load(ctx, project, Some(file))?;
                if !config.inherited.includes(&scope) {
                    return Err(Error::Usage(format!(
                        "{} sets `inherit = false`, which leaves {} out of runs with it, so ignores there wouldn't apply; pass --{} to add them anyway",
                        file.display(),
                        list_name(&scope),
                        scope.name()
                    )));
                }
            }
            add(ctx, &scope, &words)
        }
        IgnoreCommand::Remove { words, scope } => {
            remove(ctx, &setup::write_scope(project, scope)?, &words)
        }
        IgnoreCommand::List { scope } => {
            let scopes = if scope.given() {
                vec![setup::write_scope(project, scope)?]
            } else {
                let config = config::load(ctx, project, config_file)?;
                setup::applied_scopes(ctx, &config, project)
            };
            list(ctx, &scopes)
        }
        IgnoreCommand::Clear { force, scope } => {
            clear(ctx, &setup::write_scope(project, scope)?, force)
        }
        IgnoreCommand::Audit { scope } => audit(ctx, &setup::write_scope(project, scope)?),
        IgnoreCommand::Clean { scope } => clean(ctx, &setup::write_scope(project, scope)?),
    }
}

fn add(ctx: &Ctx, scope: &ListScope, words: &[String]) -> Result<u8> {
    let (new, old) = change_accept(ctx, scope, words, Store::add_accept)?;
    if !old.is_empty() {
        console::status(
            ctx,
            &format!("Already ignored: {}.", english::and_list(&old)),
        );
    }
    if !new.is_empty() {
        console::status(
            ctx,
            &format!("Added {} to {}.", english::and_list(&new), list_name(scope)),
        );
    }
    Ok(EXIT_OK)
}

fn remove(ctx: &Ctx, scope: &ListScope, words: &[String]) -> Result<u8> {
    let (gone, absent) = change_accept(ctx, scope, words, Store::remove_accept)?;
    if !absent.is_empty() {
        console::warn(&format!(
            "not in {}: {}",
            list_name(scope),
            english::and_list(&absent)
        ));
    }
    if !gone.is_empty() {
        console::status(
            ctx,
            &format!(
                "Removed {} from {}.",
                english::and_list(&gone),
                list_name(scope)
            ),
        );
    }
    Ok(EXIT_OK)
}

/// Applies `change` to `scope`'s accept list once for each word. Returns
/// the words, quoted, that it changed the list for, and the rest.
fn change_accept(
    ctx: &Ctx,
    scope: &ListScope,
    words: &[String],
    change: impl Fn(&mut Store, &str) -> bool,
) -> Result<(Vec<String>, Vec<String>)> {
    let words = normalized(words)?;
    let changed: Vec<bool> = ignores::edit(ctx, scope, |s| {
        Ok(words.iter().map(|w| change(s, w)).collect())
    })?;
    Ok(split_quoted(&words, &changed))
}

fn list(ctx: &Ctx, scopes: &[ListScope]) -> Result<u8> {
    let mut lists = Vec::new();
    for scope in scopes {
        if let Some(list) = ignores::read(ctx, scope)?.filter(|l| !l.is_empty()) {
            lists.push((scope, list));
        }
    }
    console::stdout(|out| {
        lists
            .iter()
            .try_for_each(|(scope, list)| write_list(out, ctx, scope, list))
    })?;
    if lists.is_empty() {
        console::status(ctx, "No ignores.");
    }
    Ok(EXIT_OK)
}

fn clear(ctx: &Ctx, scope: &ListScope, force: bool) -> Result<u8> {
    use std::io::IsTerminal as _;

    let path = shown_path(ctx, scope)?;
    let n = ignores::read(ctx, scope)?.map_or(0, |l| l.len());
    if n == 0 {
        console::status(
            ctx,
            &format!("{} is already empty.", capitalized(&list_name(scope))),
        );
        return Ok(EXIT_OK);
    }
    if !force {
        if !(io::stdin().is_terminal() && io::stderr().is_terminal()) {
            return Err(Error::Usage(
                "ignore clear needs --force when it isn't run in a terminal".into(),
            ));
        }
        if !confirm(&format!(
            "Clear {} from {path}?",
            english::count(n, "ignore")
        ))? {
            console::status(ctx, "Nothing cleared.");
            return Ok(EXIT_OK);
        }
    }
    let cleared = ignores::edit(ctx, scope, |s| Ok(s.clear()))?;
    console::status(
        ctx,
        &format!("Cleared {} from {path}.", english::count(cleared, "ignore")),
    );
    Ok(EXIT_OK)
}

fn audit(ctx: &Ctx, scope: &ListScope) -> Result<u8> {
    let path = shown_path(ctx, scope)?;
    let list = ignores::read(ctx, scope)?.unwrap_or_default();
    let stale = ignores::stale(ctx, scope, &list);
    console::stdout(|out| {
        stale
            .iter()
            .try_for_each(|e| writeln!(out, "{}", entry_line(e)))
    })?;
    if stale.is_empty() {
        console::status(ctx, &format!("No stale ignores in {path}."));
        return Ok(EXIT_OK);
    }
    console::status(
        ctx,
        &format!(
            "{} in {path} name files that no longer exist; 'proserunner ignore clean' removes them.",
            english::count(stale.len(), "ignore")
        ),
    );
    Ok(EXIT_ISSUES)
}

fn clean(ctx: &Ctx, scope: &ListScope) -> Result<u8> {
    let path = shown_path(ctx, scope)?;
    let removed = ignores::edit(ctx, scope, |s| {
        Ok(s.retain_issues(|e| !ignores::is_stale(ctx, scope, e)))
    })?;
    console::status(
        ctx,
        &format!(
            "Removed {} from {path}.",
            english::count(removed, "stale ignore")
        ),
    );
    Ok(EXIT_OK)
}

/// The ignore list's file, as people see it.
fn shown_path(ctx: &Ctx, scope: &ListScope) -> Result<String> {
    Ok(ctx.display(&ignores::path(ctx, scope)?))
}

/// The words, quoted, split by whether an edit changed the list.
fn split_quoted(words: &[String], changed: &[bool]) -> (Vec<String>, Vec<String>) {
    let quoted = |pick: bool| {
        words
            .iter()
            .zip(changed)
            .filter(|(_, c)| **c == pick)
            .map(|(w, _)| format!("“{w}”"))
            .collect()
    };
    (quoted(true), quoted(false))
}

fn normalized(words: &[String]) -> Result<Vec<String>> {
    words
        .iter()
        .map(|w| {
            let w = prose::normalize(w);
            if w.is_empty() {
                Err(Error::Usage("words to ignore can't be empty".into()))
            } else {
                Ok(w)
            }
        })
        .collect()
}

fn write_list(
    out: &mut dyn Write,
    ctx: &Ctx,
    scope: &ListScope,
    list: &IgnoreFile,
) -> io::Result<()> {
    let path =
        ignores::path(ctx, scope).map_or_else(|_| scope.name().to_owned(), |p| ctx.display(&p));
    writeln!(out, "{path} ({})", scope.name())?;
    for a in &list.accept {
        writeln!(out, "  \"{a}\"")?;
    }
    for e in &list.issue {
        writeln!(out, "  {}", entry_line(e))?;
    }
    Ok(())
}

fn entry_line(e: &Entry) -> String {
    match &e.rule {
        Some(rule) => format!("{}: \"{}\" ({rule})", e.place(), e.matched),
        None => format!("{}: \"{}\"", e.place(), e.matched),
    }
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn confirm(question: &str) -> Result<bool> {
    eprint!("{question} [y/N] ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|e| Error::io("<stdin>", e))?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

pub(crate) fn list_name(scope: &ListScope) -> String {
    format!("the {} ignore list", scope.name())
}
