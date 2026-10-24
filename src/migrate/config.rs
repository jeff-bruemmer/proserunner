//! The one-time move from proserunner 0.8's EDN settings to TOML.
//!
//! 0.8 kept `config.edn` (check directories, and which checks in each are
//! on) and `ignore.edn` in the user config directory, and
//! `.proserunner/config.edn` (checks and ignores together) in a project,
//! with each check in its own `*.edn` file. Migration plans everything in
//! memory first, so a file that doesn't convert stops it before anything
//! is written. Then it writes the TOML and renames each file it replaced
//! to `*.edn.bak`. It runs when an EDN file has no TOML file beside it, so
//! it happens once.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use super::checks;
use super::edn::{self, Edn};
use crate::config;
use crate::ctx::{Ctx, ListScope, PROJECT_DIR};
use crate::english;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::ignores::{self, Entry};
use crate::rules::{self, DEFAULT_SOURCE, RuleSet};
use crate::settings;

#[derive(Default)]
struct Plan {
    writes: Vec<(PathBuf, String)>,
    /// EDN files to rename to `.edn.bak` once everything is written.
    replaced: Vec<PathBuf>,
    notes: Vec<String>,
    /// 0.8 check names (see [`name_key`]) to rule ids, for ignore entries
    /// that name a check.
    names: HashMap<String, String>,
}

impl Plan {
    fn write(&mut self, path: PathBuf, text: String) {
        if !self.writes.iter().any(|(p, _)| *p == path) {
            self.writes.push((path, text));
        }
    }

    fn replace(&mut self, path: PathBuf) {
        if !self.replaced.contains(&path) {
            self.replaced.push(path);
        }
    }
}

/// What goes into one migrated `config.toml`.
struct Settings {
    inherit: bool,
    /// TOML values for `rules`.
    sources: Vec<String>,
    prefixes: Vec<String>,
    /// Rule ids that 0.8 left out of `:files`.
    off: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            inherit: true,
            sources: Vec::new(),
            prefixes: Vec::new(),
            off: Vec::new(),
        }
    }
}

/// Migrates the user config directory, and the project at `project`, when
/// either still has 0.8 settings. Returns what happened, for people.
pub fn auto(ctx: &Ctx, project: Option<&Path>) -> Result<Vec<String>> {
    // Every run comes through here, so checking costs a few stats.
    let (user, root) = to_migrate(ctx, project);
    let scope = match (user, root) {
        (None, None) => return Ok(Vec::new()),
        (None, Some(root)) => ListScope::Project(root.to_path_buf()),
        (Some(_), _) => ListScope::Global,
    };

    let mut plan = Plan {
        names: builtin_names(),
        ..Plan::default()
    };
    if let Some(dir) = user {
        plan_user(ctx, dir, &mut plan)?;
    }
    if let Some(root) = root {
        plan_project(ctx, root, &mut plan)?;
    }

    // Runs that start together, such as a hook's parallel batches, would
    // each migrate: the first to get the lock does, and the rest find
    // nothing left to do.
    let _lock = ignores::lock(ctx, &scope)?;
    if to_migrate(ctx, project) != (user, root) {
        return Ok(Vec::new());
    }
    for (path, text) in &plan.writes {
        fsutil::write_atomic(path, text)?;
    }
    for path in &plan.replaced {
        let backup = path.with_extension("edn.bak");
        fs::rename(path, &backup).map_err(|e| Error::io(path, e))?;
    }

    let is_rule = |p: &Path| {
        p.file_name()
            .is_some_and(|n| n != config::FILE && n != ignores::FILE)
    };
    let mut wrote: Vec<String> = plan
        .writes
        .iter()
        .filter(|(p, _)| !is_rule(p))
        .map(|(p, _)| ctx.display(p))
        .collect();
    match plan.writes.iter().filter(|(p, _)| is_rule(p)).count() {
        0 => {}
        1 => wrote.push("1 rule file".into()),
        n => wrote.push(format!("{n} rule files")),
    }
    let mut notes = vec![format!(
        "Migrated proserunner 0.8 settings: wrote {}. The 0.8 files are now *.edn.bak.",
        english::and_list(&wrote)
    )];
    notes.append(&mut plan.notes);
    Ok(notes)
}

/// The user config directory and the project root, each when it still has
/// 0.8 settings.
fn to_migrate<'a>(ctx: &'a Ctx, project: Option<&'a Path>) -> (Option<&'a Path>, Option<&'a Path>) {
    let user = ctx.config_dir.as_deref().filter(|d| {
        pending(d, "config.edn", config::FILE) || pending(d, "ignore.edn", ignores::FILE)
    });
    let project = project.filter(|r| pending(&r.join(PROJECT_DIR), "config.edn", config::FILE));
    (user, project)
}

/// Whether `dir` has a 0.8 file with no replacement beside it.
fn pending(dir: &Path, edn: &str, toml: &str) -> bool {
    dir.join(edn).is_file() && !dir.join(toml).exists()
}

fn plan_user(ctx: &Ctx, dir: &Path, plan: &mut Plan) -> Result<()> {
    let edn_config = dir.join("config.edn");
    if pending(dir, "config.edn", config::FILE) {
        let data = read_edn(ctx, &edn_config)?;
        let entries = data
            .get("checks")
            .and_then(Edn::items)
            .ok_or_else(|| invalid(ctx, &edn_config, ":checks must be a vector"))?;
        let mut settings = Settings::default();
        for entry in entries {
            let directory = text(entry, "directory").ok_or_else(|| {
                invalid(ctx, &edn_config, "every :checks entry needs a :directory")
            })?;
            let files = list(entry, "files");
            if directory == DEFAULT_SOURCE {
                add_default(&mut settings, files.as_deref(), plan);
            } else {
                let path = ctx.resolve(dir, &directory);
                add_dir(
                    ctx,
                    &mut settings,
                    &directory,
                    &path,
                    files.as_deref(),
                    plan,
                )?;
            }
        }
        plan.write(
            dir.join(config::FILE),
            render_config("config.edn", &settings),
        );
        plan.replace(edn_config);
        if dir.join(DEFAULT_SOURCE).is_dir() {
            plan.notes.push(format!(
                "{} holds the checks 0.8 downloaded; the rules are built in now, so you can delete it.",
                ctx.display(&dir.join(DEFAULT_SOURCE))
            ));
        }
    }

    let edn_ignore = dir.join("ignore.edn");
    if pending(dir, "ignore.edn", ignores::FILE) {
        let data = read_edn(ctx, &edn_ignore)?;
        let (accept, issues) =
            ignores_of(&data, &plan.names).map_err(|m| invalid(ctx, &edn_ignore, &m))?;
        plan.write(
            dir.join(ignores::FILE),
            render_ignores("ignore.edn", &accept, &issues),
        );
        plan.replace(edn_ignore);
    }
    Ok(())
}

fn plan_project(ctx: &Ctx, root: &Path, plan: &mut Plan) -> Result<()> {
    let dir = root.join(PROJECT_DIR);
    let edn_config = dir.join("config.edn");
    if !pending(&dir, "config.edn", config::FILE) {
        return Ok(());
    }
    let data = read_edn(ctx, &edn_config)?;
    if !matches!(data, Edn::Map(_)) {
        return Err(invalid(ctx, &edn_config, "expected a map"));
    }

    let mut settings = Settings::default();
    let project_checks = format!("{PROJECT_DIR}/checks");
    for entry in data.get("checks").and_then(Edn::items).unwrap_or(&[]) {
        match entry {
            Edn::Str(s) if s == DEFAULT_SOURCE => add_default(&mut settings, None, plan),
            Edn::Str(s) if s == "checks" => {
                add_dir(
                    ctx,
                    &mut settings,
                    &project_checks,
                    &dir.join("checks"),
                    None,
                    plan,
                )?;
            }
            // Any other name is a directory from the global config.
            Edn::Str(s) => match &ctx.config_dir {
                Some(global) => {
                    let path = ctx.resolve(global, s);
                    add_dir(ctx, &mut settings, &ctx.portable(&path), &path, None, plan)?;
                }
                None => plan.notes.push(format!(
                    "{}: left out the global checks \"{s}\": there's no global config directory.",
                    ctx.display(&edn_config)
                )),
            },
            Edn::Map(_) => {
                let directory = text(entry, "directory").ok_or_else(|| {
                    invalid(ctx, &edn_config, "every :checks map needs a :directory")
                })?;
                let files = list(entry, "files");
                let (written, path) = if directory == "checks" {
                    (project_checks.as_str(), dir.join("checks"))
                } else {
                    (directory.as_str(), ctx.resolve(root, &directory))
                };
                add_dir(ctx, &mut settings, written, &path, files.as_deref(), plan)?;
            }
            _ => {
                return Err(invalid(
                    ctx,
                    &edn_config,
                    ":checks entries must be strings or maps",
                ));
            }
        }
    }
    // Both modes only decided whether the global ignores applied.
    let keyword = |k: &str| match data.get(k) {
        Some(Edn::Keyword(v)) => Some(v.as_str()),
        _ => None,
    };
    settings.inherit =
        keyword("ignore-mode") != Some("replace") && keyword("config-mode") != Some("project-only");
    plan.write(
        dir.join(config::FILE),
        render_config("config.edn", &settings),
    );

    let (accept, issues) =
        ignores_of(&data, &plan.names).map_err(|m| invalid(ctx, &edn_config, &m))?;
    if (!accept.is_empty() || !issues.is_empty()) && !dir.join(ignores::FILE).exists() {
        plan.write(
            dir.join(ignores::FILE),
            render_ignores("config.edn", &accept, &issues),
        );
    }
    plan.replace(edn_config);
    Ok(())
}

/// The built-in rules, with every rule 0.8 didn't list in `:files` off.
fn add_default(settings: &mut Settings, files: Option<&[String]>, plan: &mut Plan) {
    if !settings.prefixes.iter().any(|p| p == DEFAULT_SOURCE) {
        settings.sources.push(settings::quote(DEFAULT_SOURCE));
        settings.prefixes.push(DEFAULT_SOURCE.to_owned());
    }
    let Some(files) = files else {
        return;
    };
    let stems: Vec<&str> = rules::builtin_files().iter().map(|f| f.stem).collect();
    for stem in &stems {
        if !files.iter().any(|f| f == stem) {
            settings.off.push(format!("{DEFAULT_SOURCE}/{stem}"));
        }
    }
    let dropped = files.iter().filter(|f| !stems.contains(&f.as_str()));
    for f in dropped {
        plan.notes.push(format!(
            "The 0.8 default check \"{f}\" isn't a built-in rule now, so it was left out."
        ));
    }
}

/// A directory of 0.8 checks: converts each `*.edn` check that has no
/// `.toml` beside it, and turns off the ones not in `files`.
fn add_dir(
    ctx: &Ctx,
    settings: &mut Settings,
    written: &str,
    dir: &Path,
    files: Option<&[String]>,
    plan: &mut Plan,
) -> Result<()> {
    if !dir.is_dir() {
        plan.notes.push(format!(
            "{}: left out a 0.8 check directory that doesn't exist.",
            ctx.display(dir)
        ));
        return Ok(());
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut prefix = name.clone();
    if prefix.is_empty() || prefix == DEFAULT_SOURCE || settings.prefixes.contains(&prefix) {
        // `custom/style` and `style` would both be `style`.
        let long = written.replace(['/', '~', '.'], "-");
        prefix = long.trim_matches('-').to_owned();
    }
    if prefix.is_empty() {
        prefix = "checks".to_owned();
    }
    let stem = prefix.clone();
    let mut n = 2;
    while settings.prefixes.contains(&prefix) {
        prefix = format!("{stem}-{n}");
        n += 1;
    }
    settings.sources.push(if prefix == name {
        settings::quote(written)
    } else {
        format!(
            "{{ path = {}, prefix = {} }}",
            settings::quote(written),
            settings::quote(&prefix)
        )
    });
    settings.prefixes.push(prefix.clone());

    for path in fsutil::files_with_extension(dir, "edn")? {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let id = format!("{prefix}/{stem}");
        if let Some(n) = edn::parse(&text)
            .ok()
            .as_ref()
            .and_then(|c| c.get("name"))
            .and_then(Edn::as_str)
        {
            plan.names.insert(name_key(n), id.clone());
        }
        let toml_path = dir.join(format!("{stem}.toml"));
        if !toml_path.exists() {
            let toml = checks::convert(&text).map_err(|e| invalid(ctx, &path, &e))?;
            plan.write(toml_path, toml);
            plan.replace(path);
        }
        if files.is_some_and(|f| !f.contains(&stem)) {
            settings.off.push(id);
        }
    }
    Ok(())
}

/// 0.8's ignores: a map with `:ignore` (strings) and `:ignore-issues`
/// (maps), or, from before contextual ignores, a bare set of strings.
fn ignores_of(
    data: &Edn,
    names: &HashMap<String, String>,
) -> Result<(Vec<String>, Vec<Entry>), String> {
    let (simple, contextual): (&[Edn], &[Edn]) = match data {
        Edn::Map(_) => (
            data.get("ignore").and_then(Edn::items).unwrap_or(&[]),
            data.get("ignore-issues")
                .and_then(Edn::items)
                .unwrap_or(&[]),
        ),
        other => (
            other.items().ok_or("expected a map, or a set of strings")?,
            &[],
        ),
    };
    let accept: Vec<String> = simple
        .iter()
        .map(|e| {
            e.as_str()
                .map(str::to_owned)
                .ok_or("ignored specimens must be strings")
        })
        .collect::<Result<_, _>>()?;
    let mut issues = Vec::new();
    for e in contextual {
        let (Some(file), Some(matched)) = (text(e, "file"), text(e, "specimen")) else {
            return Err("every :ignore-issues entry needs :file and :specimen".into());
        };
        let line = match e.get("line-num").or_else(|| e.get("line")) {
            Some(Edn::Int(n)) if *n > 0 => Some(*n as usize),
            _ => None,
        };
        let rule = text(e, "check")
            .or_else(|| text(e, "name"))
            .and_then(|n| names.get(&name_key(&n)).cloned());
        issues.push(Entry {
            file,
            rule,
            matched,
            line,
        });
    }
    Ok((accept, issues))
}

fn render_config(from: &str, s: &Settings) -> String {
    let mut out = format!("# Migrated from proserunner 0.8's {from}.\nschema = 1\n");
    if !s.inherit {
        out.push_str("# The 0.8 project didn't use the global ignores.\ninherit = false\n");
    }
    let _ = writeln!(out, "rules = [{}]", s.sources.join(", "));
    for id in &s.off {
        let _ = writeln!(out, "\n[rule.{}]\nlevel = \"off\"", settings::quote(id));
    }
    out
}

fn render_ignores(from: &str, accept: &[String], issues: &[Entry]) -> String {
    let mut out = format!("# Migrated from proserunner 0.8's {from}.\nschema = 1\n");
    if !accept.is_empty() {
        let quoted: Vec<String> = accept.iter().map(|a| settings::quote(a)).collect();
        let _ = writeln!(out, "accept = [{}]", quoted.join(", "));
    }
    for e in issues {
        let _ = writeln!(out, "\n[[issue]]\nfile = {}", settings::quote(&e.file));
        if let Some(rule) = &e.rule {
            let _ = writeln!(out, "rule = {}", settings::quote(rule));
        }
        let _ = writeln!(out, "match = {}", settings::quote(&e.matched));
        if let Some(line) = e.line {
            let _ = writeln!(out, "line = {line}");
        }
    }
    out
}

/// The built-in rules by name and by file stem.
fn builtin_names() -> HashMap<String, String> {
    let set = RuleSet::compile(rules::builtin()).rules;
    let mut names = HashMap::new();
    for rule in set.rules() {
        names.insert(name_key(&rule.name), rule.id.clone());
        if let Some(stem) = rule.id.strip_prefix("default/") {
            names.insert(name_key(stem), rule.id.clone());
        }
    }
    names
}

/// 0.8 showed `needless-variants` as "Needless variants", so names compare
/// ignoring case, hyphens and underscores.
fn name_key(name: &str) -> String {
    name.to_lowercase()
        .replace(['-', '_'], " ")
        .trim()
        .to_owned()
}

fn read_edn(ctx: &Ctx, path: &Path) -> Result<Edn> {
    let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    edn::parse(&text).map_err(|e| invalid(ctx, path, &e))
}

fn invalid(ctx: &Ctx, path: &Path, message: &str) -> Error {
    Error::Migrate(format!(
        "{}: {message}. proserunner converts 0.8 settings on its first run and changes nothing \
         when one won't convert; fix or remove this file, then run it again.",
        ctx.display(path)
    ))
}

fn text(edn: &Edn, key: &str) -> Option<String> {
    edn.get(key).and_then(Edn::as_str).map(str::to_owned)
}

fn list(edn: &Edn, key: &str) -> Option<Vec<String>> {
    edn.get(key).and_then(Edn::items).map(|items| {
        items
            .iter()
            .filter_map(Edn::as_str)
            .map(str::to_owned)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    fn keys(names: &[(&str, &str)]) -> HashMap<String, String> {
        names
            .iter()
            .map(|(n, id)| (name_key(n), (*id).to_owned()))
            .collect()
    }

    #[test]
    fn ignore_shapes() {
        let names = keys(&[("jargon", "default/jargon")]);
        let (accept, issues) = ignores_of(
            &edn::parse(
                r#"{:ignore #{"hopefully"}
                    :ignore-issues [{:file "a.md" :line-num 4 :specimen "utilize" :check "Jargon"}
                                    {:file "b.md" :specimen "x" :check "gone"}]}"#,
            )
            .unwrap(),
            &names,
        )
        .unwrap();
        assert_eq!(accept, ["hopefully"]);
        assert_eq!(
            issues,
            [
                Entry {
                    file: "a.md".into(),
                    rule: Some("default/jargon".into()),
                    matched: "utilize".into(),
                    line: Some(4)
                },
                Entry {
                    file: "b.md".into(),
                    rule: None,
                    matched: "x".into(),
                    line: None
                }
            ]
        );
        // Before contextual ignores the file was a bare set.
        let (accept, issues) = ignores_of(
            &edn::parse(
                r#"#{"a" ;; "commented"
                                       "b"}"#,
            )
            .unwrap(),
            &names,
        )
        .unwrap();
        assert_eq!(accept, ["a", "b"]);
        assert!(issues.is_empty());
        assert!(
            ignores_of(
                &edn::parse("{:ignore-issues [{:file \"a.md\"}]}").unwrap(),
                &names
            )
            .is_err()
        );
        assert!(ignores_of(&edn::parse("42").unwrap(), &names).is_err());
    }

    #[test]
    fn rendered_files_parse() {
        let settings = Settings {
            inherit: false,
            sources: vec![
                settings::quote("default"),
                "{ path = \"x\", prefix = \"y\" }".into(),
            ],
            prefixes: vec![],
            off: vec!["default/jargon".into()],
        };
        let c = config::parse(&render_config("config.edn", &settings)).unwrap();
        assert_eq!(c.inherit, Some(false));
        assert_eq!(c.rules.unwrap().len(), 2);
        let entry = Entry {
            file: "a \"b\".md".into(),
            rule: None,
            matched: "x".into(),
            line: Some(1),
        };
        let i = ignores::parse(&render_ignores(
            "ignore.edn",
            &["it's".into()],
            slice::from_ref(&entry),
        ))
        .unwrap();
        assert_eq!(i.issue, [entry]);
    }

    #[test]
    fn builtin_names_cover_names_and_stems() {
        let names = builtin_names();
        assert_eq!(
            names.get("needless variants").map(String::as_str),
            Some("default/needless-variants")
        );
        assert_eq!(
            names.get("jargon").map(String::as_str),
            Some("default/jargon")
        );
    }
}
