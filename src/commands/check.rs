//! `check`: walk, read, lint, apply policy, report. Also
//! `--ignore-issues` and `--ignore-all`, which ignore what a check finds.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;

use proserunner::config;
use proserunner::ctx::Ctx;
use proserunner::format::Output;
use proserunner::ignores::{self, Entry};
use proserunner::lint::Exhausted;
use proserunner::report::{self, Document, LineIndex, Position, Report};
use proserunner::rules::{Level, RuleIndex, RuleSet};
use proserunner::scan::{Format, Mask, Options};
use proserunner::walk::{self, Filters, FormatOf, Input};
use proserunner::{Error, Result, lint};
use proserunner::{english, format};

use super::last_run::{self, Shown};
use super::setup::{self, Setup};
use super::{EXIT_ERROR, EXIT_ISSUES, EXIT_OK, console, ignore};
use crate::cli::{self, CheckArgs, ScopeArgs};

const STDIN_NAME: &str = "<stdin>";

pub(crate) fn check(ctx: &Ctx, args: &CheckArgs, config_file: Option<&Path>) -> Result<u8> {
    use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
    use std::io::IsTerminal as _;

    let start = Instant::now();
    let paths = paths(args)?;
    let ignoring = args.ignore_all || args.ignore_issues.is_some();
    if args.scope.given() && !ignoring {
        return Err(Error::Usage(
            "--global and --project choose where --ignore-issues and --ignore-all write".into(),
        ));
    }
    let reads_stdin = paths.iter().any(|p| p.as_os_str() == walk::STDIN);
    if ignoring && reads_stdin && args.stdin_filename.is_none() {
        return Err(Error::Usage(
            "--ignore-issues and --ignore-all need files: issues in standard input have no file to belong to unless --stdin-filename names one"
                .into(),
        ));
    }
    let numbers = args
        .ignore_issues
        .as_deref()
        .map(ignores::parse_numbers)
        .transpose()
        .map_err(|m| Error::Usage(format!("--ignore-issues: {m}")))?;

    let checked = paths
        .iter()
        .chain(&args.stdin_filename)
        .map(PathBuf::as_path);
    let mut setup = setup::setup(
        ctx,
        ctx.project_for(checked)?,
        config_file,
        !args.no_ignores,
    )?;
    let output = args.output.unwrap_or(setup.config.output.value);
    if args.show_suppressed && !SHOWS_SUPPRESSED.contains(&output) {
        return Err(Error::Usage(format!(
            "--show-suppressed works with -o group, plain and json, not -o {output}"
        )));
    }
    setup.policy.set_keep_suppressed(args.show_suppressed);
    let only = only_rules(&setup.rules, &args.rules)?;
    let config = &setup.config;
    let mask = Mask::try_new(config.mask.iter().map(|m| &m.value)).map_err(Error::Usage)?;
    let mut options = Options::default();
    options.quoted_text = cli::switch(
        args.quoted_text,
        args.no_quoted_text,
        config.quoted_text.value,
    );
    options.code_blocks = cli::switch(
        args.code_blocks,
        args.no_code_blocks,
        config.code_blocks.value,
    );
    options.mask = Some(&mask);

    let excluded = |p: &Path| config.exclude.is_match(&ctx.absolute(p));
    let format_of = |p: &Path| config.format_of(&ctx.absolute(p));
    let stdin = stdin_as(ctx, args, &format_of)?;
    let mut filters = Filters::new(&ctx.cwd, &format_of);
    filters.exclude = &args.exclude;
    filters.excluded = &excluded;
    filters.force = args.force_exclude;
    filters.stdin_name = args.stdin_filename.as_deref();
    let walk = walk::inputs(&paths, filters)?;
    for skipped in &walk.skipped {
        console::warn(skipped);
    }
    if walk.inputs.contains(&Input::Stdin) && io::stdin().is_terminal() && !ctx.quiet {
        eprintln!("Reading from standard input; press Ctrl-D to finish.");
    }

    let lint = Lint {
        ctx,
        setup: &setup,
        stdin: &stdin,
        options,
        named: &walk.named,
        only: only.as_ref(),
    };
    let read: Vec<Read> = walk
        .inputs
        .par_iter()
        .map(|input| lint.read(input))
        .collect::<Result<_>>()?;
    let mut documents = Vec::with_capacity(read.len());
    for r in read {
        match r {
            Read::Linted(document, warnings) => {
                for w in &warnings {
                    console::warn(w);
                }
                documents.push(document);
            }
            Read::Skipped(why) => console::warn(&why),
        }
    }
    let key = last_run::Key::new(ctx, &paths, args.stdin_filename.as_deref());

    if ignoring {
        let report = report::build(&setup.rules, &documents);
        let last = numbers.as_ref().and_then(|_| last_run::load(ctx, &key));
        let chosen = Chosen {
            numbers: numbers.as_deref(),
            last,
            only: only.as_ref(),
        };
        let ignored = ignore_reported(ctx, &setup, args.scope, &report, chosen)?;
        drop(report);
        drop_ignored(&setup.rules, &mut documents, &ignored);
    }

    let report = report::build(&setup.rules, &documents);
    last_run::save(ctx, &key, &report);
    if output.has_styles() && console::wants_styles() {
        console::styled_stdout(|out| format::write_styled(out, &report, output, &ctx.cwd))?;
    } else {
        console::stdout(|out| format::write(out, &report, output, &ctx.cwd))?;
    }
    if output.has_summary() {
        for line in summary(&report) {
            console::status(ctx, &line);
        }
    }
    if args.timer {
        console::status(
            ctx,
            &format!("Completed in {} ms.", start.elapsed().as_millis()),
        );
    }

    if broken_rules(&setup) {
        return Ok(EXIT_ERROR);
    }
    let fail_on = args.fail_on.unwrap_or(config.fail_on.value);
    let fails = report.issues.iter().any(|i| i.level >= fail_on);
    Ok(if fails { EXIT_ISSUES } else { EXIT_OK })
}

/// The output formats that show hidden issues.
const SHOWS_SUPPRESSED: [Output; 3] = [Output::Group, Output::Plain, Output::Json];

/// The rules whose issues `--rule` keeps, or `None` to keep every rule's.
/// Each is an id, a name only one rule has, or `source/*`.
fn only_rules(rules: &RuleSet, queries: &[String]) -> Result<Option<HashSet<RuleIndex>>> {
    if queries.is_empty() {
        return Ok(None);
    }
    let mut only = HashSet::new();
    for query in queries {
        let chosen = match config::pattern_source(query) {
            Some(source) => rules.find_source(source),
            None => rules.find(query).map(|rule| vec![rule]),
        };
        let chosen = chosen.map_err(|m| Error::Usage(format!("--rule: {m}")))?;
        only.extend(chosen.into_iter().map(|rule| rule.index));
    }
    Ok(Some(only))
}

/// Whether rule files didn't load, which makes the run an error, saying
/// so when they didn't.
fn broken_rules(setup: &Setup) -> bool {
    if setup.broken_rules > 0 {
        console::warn(&format!(
            "{} didn't load (see above)",
            english::count(setup.broken_rules, "rule file")
        ));
    }
    setup.broken_rules > 0
}

/// From this many issues, the summary names the rules and files that
/// found the most, so a first run on a large project shows where to
/// start: a rule to tune, or a file that isn't prose.
const BUSIEST_FROM: usize = 20;

/// How many rules, and files, it names.
const BUSIEST: usize = 5;

/// What goes to standard error after the report: how many files and
/// issues, by level, and in a long report, the rules and files that found
/// the most.
fn summary(report: &Report<'_>) -> Vec<String> {
    let levels: Vec<String> = [Level::Error, Level::Warning, Level::Hint]
        .into_iter()
        .filter_map(|level| {
            let n = report.issues.iter().filter(|i| i.level == level).count();
            (n > 0).then(|| english::count(n, level.as_str()))
        })
        .collect();
    let mut lines = vec![english::summary(
        report.files.len(),
        report.issues.len(),
        &levels,
    )];
    let hidden = report.suppressed.len();
    if hidden > 0 {
        let are = if hidden == 1 { "is" } else { "are" };
        lines.push(format!(
            "{} that config, ignore lists or inline control hide {are} shown last.",
            english::count(hidden, "issue")
        ));
    }
    if report.issues.len() < BUSIEST_FROM {
        return lines;
    }
    let rules = busiest(report.issues.iter().map(|i| i.rule.id.as_str()));
    let files = busiest(report.issues.iter().map(|i| i.file));
    // Counts line up across both lists.
    let width = rules
        .iter()
        .chain(&files)
        .map(|&(_, n)| english::number(n).len())
        .max()
        .unwrap_or(0);
    let counted = |(name, n): (&str, usize)| format!("  {:>width$}  {name}", english::number(n));
    lines.push("The rules that found the most:".into());
    lines.extend(rules.into_iter().map(counted));
    // One file holding them all says nothing a reader doesn't know.
    let several_files = files.len() > 1;
    if several_files {
        lines.push("The files with the most:".into());
        lines.extend(files.into_iter().map(counted));
    }
    lines.push(
        "'proserunner explain RULE' shows what a rule matches and how to turn it off.".into(),
    );
    if several_files {
        lines.push(
            "A file that isn't prose can go in .proserunnerignore, or in check.exclude in config."
                .into(),
        );
    }
    lines
}

/// The [`BUSIEST`] names by how many issues each has, most first, and in
/// name order on a tie.
fn busiest<'report>(names: impl Iterator<Item = &'report str>) -> Vec<(&'report str, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    let mut counts: Vec<(&str, usize)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    counts.truncate(BUSIEST);
    counts
}

/// The paths to check: as given, or `-` when only `--stdin-format` or
/// `--stdin-filename` says what to read.
fn paths(args: &CheckArgs) -> Result<Vec<PathBuf>> {
    let stdin_flag = match (&args.stdin_filename, args.stdin_format) {
        (Some(_), _) => Some("--stdin-filename"),
        (None, Some(_)) => Some("--stdin-format"),
        (None, None) => None,
    };
    let reads_stdin = args.paths.iter().any(|p| p.as_os_str() == walk::STDIN);
    match stdin_flag {
        Some(_) if args.paths.is_empty() => Ok(vec![PathBuf::from(walk::STDIN)]),
        Some(flag) if !reads_stdin => Err(Error::Usage(format!(
            "{flag} is for standard input; add - to the paths to check it with them"
        ))),
        _ if args.paths.is_empty() => Err(Error::Usage(
            "nothing to check: give a file, a directory, or - for standard input".into(),
        )),
        _ => Ok(args.paths.clone()),
    }
}

/// How standard input is checked and shown.
struct Stdin {
    display: String,
    /// `--stdin-filename`, absolute, so config and ignores apply.
    path: Option<PathBuf>,
    format: Format,
}

/// Standard input as `--stdin-filename` and `--stdin-format` say: the
/// format given, else the file name's, else Markdown.
fn stdin_as(ctx: &Ctx, args: &CheckArgs, format_of: FormatOf<'_>) -> Result<Stdin> {
    let Some(name) = &args.stdin_filename else {
        return Ok(Stdin {
            display: STDIN_NAME.into(),
            path: None,
            format: args.stdin_format.unwrap_or(Format::Markdown),
        });
    };
    let format = args
        .stdin_format
        .or_else(|| format_of(name))
        .ok_or_else(|| {
            Error::Usage(format!(
                "--stdin-filename: {}; --stdin-format chooses one",
                Error::Unsupported(name.clone())
            ))
        })?;
    Ok(Stdin {
        display: ctx.display(name),
        path: Some(ctx.absolute(name)),
        format,
    })
}

/// A file read and linted, with any warnings about it, or skipped with
/// the reason.
enum Read {
    Linted(Document, Vec<String>),
    Skipped(String),
}

/// A file's text. One that isn't UTF-8 is an error when it was named, and
/// skipped when it was found in a directory, like one that's too large.
fn read_text(path: &Path, named: bool) -> Result<Result<String, String>> {
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Ok(text)),
        Err(_) if named => Err(Error::NotText(path.to_path_buf())),
        Err(_) => Ok(Err(format!(
            "{}: skipped, {}",
            path.display(),
            Error::NOT_TEXT
        ))),
    }
}

/// What reading and linting each input needs.
struct Lint<'run> {
    ctx: &'run Ctx,
    setup: &'run Setup,
    stdin: &'run Stdin,
    options: Options<'run>,
    /// The files named on the command line, absolute.
    named: &'run HashSet<PathBuf>,
    /// The rules `--rule` keeps, if it was given.
    only: Option<&'run HashSet<RuleIndex>>,
}

impl Lint<'_> {
    /// Whether `--rule` keeps `rule`'s issues.
    fn keeps(&self, rule: RuleIndex) -> bool {
        self.only.is_none_or(|only| only.contains(&rule))
    }

    /// Reads, lints and applies the policy to one input.
    fn read(&self, input: &Input) -> Result<Read> {
        use std::io::Read as _;

        let Lint {
            ctx,
            setup,
            stdin,
            options,
            named,
            ..
        } = *self;
        let (display, path, source, format) = match input {
            Input::Stdin => {
                // A byte past the limit is enough to know it's too large.
                let mut bytes = Vec::new();
                io::stdin()
                    .take(walk::MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| Error::io(STDIN_NAME, e))?;
                if bytes.len() as u64 > walk::MAX_FILE_BYTES {
                    return Err(Error::TooLarge(STDIN_NAME.into()));
                }
                let source =
                    String::from_utf8(bytes).map_err(|_| Error::NotText(STDIN_NAME.into()))?;
                (
                    stdin.display.clone(),
                    stdin.path.clone(),
                    source,
                    stdin.format,
                )
            }
            Input::File { path, format } => {
                let absolute = ctx.absolute(path);
                let source = match read_text(path, named.contains(&absolute))? {
                    Ok(source) => source,
                    Err(why) => return Ok(Read::Skipped(why)),
                };
                (ctx.display(path), Some(absolute), source, *format)
            }
        };
        let linted = lint::document(&setup.rules, format, &source, options);
        let mut applied = setup
            .policy
            .apply(path.as_deref(), &source, linted)
            .map_err(|e| {
                let at = LineIndex::new(&source).position(&source, e.at);
                Error::Control {
                    origin: format!("{display}:{}:{}", at.line, at.column),
                    message: e.message,
                }
            })?;
        if self.only.is_some() {
            applied.issues.retain(|i| self.keeps(i.rule));
            applied.exhausted.retain(|e| self.keeps(e.rule));
            applied.suppressed.retain(|s| self.keeps(s.issue.rule));
        }
        let warnings = exhausted_warnings(&setup.rules, &display, &source, &applied.exhausted);
        let mut document = Document::new(display, path, source, applied.issues);
        document.suppressed = applied.suppressed;
        Ok(Read::Linted(document, warnings))
    }
}

/// One warning for each pattern that gave up in a file, at the first block
/// where it did.
fn exhausted_warnings(
    rules: &RuleSet,
    display: &str,
    source: &str,
    exhausted: &[Exhausted],
) -> Vec<String> {
    if exhausted.is_empty() {
        return Vec::new();
    }
    // Each pattern's first block, and how many more.
    let mut patterns: Vec<(&Exhausted, usize)> = Vec::new();
    for e in exhausted {
        match patterns
            .iter_mut()
            .find(|(p, _)| (p.rule, p.entry, p.tag) == (e.rule, e.entry, e.tag))
        {
            Some((first, more)) => {
                *more += 1;
                if e.at < first.at {
                    *first = e;
                }
            }
            None => patterns.push((e, 0)),
        }
    }
    let lines = LineIndex::new(source);
    let mut warnings = Vec::with_capacity(patterns.len());
    for (e, more) in patterns {
        let Some(rule) = rules.get(e.rule) else {
            continue;
        };
        let Some(pattern) = rule.pattern(e.entry, e.tag) else {
            continue;
        };
        let Position { line, column, .. } = lines.position(source, e.at);
        let others = match more {
            0 => String::new(),
            n => format!(" and {}", english::count(n, "other block")),
        };
        warnings.push(format!(
            "{display}:{line}:{column}: `{}` gave up on the pattern `{pattern}` in this block{others} (too much backtracking), so it may have missed issues there",
            rule.id
        ));
    }
    warnings
}

/// Which issues `--ignore-issues` and `--ignore-all` ignore.
struct Chosen<'run> {
    /// `--ignore-issues`' numbers; `None` for `--ignore-all`.
    numbers: Option<&'run [RangeInclusive<usize>]>,
    /// The issues the last run of these paths numbered, if one was kept.
    last: Option<Vec<Shown>>,
    /// The rules `--rule` keeps, if it was given.
    only: Option<&'run HashSet<RuleIndex>>,
}

/// `--ignore-issues` and `--ignore-all`: adds the chosen issues to an
/// ignore list, and returns them. `--ignore-issues` takes the numbers of
/// the last run of these paths when one was kept, so they name what that
/// run showed, even after the files changed; otherwise, this run's.
fn ignore_reported(
    ctx: &Ctx,
    setup: &Setup,
    scope: ScopeArgs,
    report: &Report<'_>,
    chosen: Chosen<'_>,
) -> Result<Vec<Shown>> {
    let current = last_run::shown(report);
    let numbers = chosen.numbers;
    let chosen = match numbers {
        None => current,
        Some(numbers) => {
            let (numbered, run) = match chosen.last {
                Some(last) => (last, "the last run"),
                None => {
                    console::status(
                        ctx,
                        "No earlier run of these paths from here was kept, so the numbers are this run's.",
                    );
                    (current.clone(), "this run")
                }
            };
            in_range(numbers, numbered.len(), run)?;
            let picked = numbered
                .into_iter()
                .filter(|s| numbers.iter().any(|r| r.contains(&s.number)));
            // An issue the last run showed may be gone: fixed since. Or
            // `--rule` may leave out its rule, so this run can't tell.
            let kept: Option<HashSet<&str>> = chosen.only.map(|only| {
                only.iter()
                    .filter_map(|&rule| setup.rules.get(rule))
                    .map(|rule| rule.id.as_str())
                    .collect()
            });
            let (here, missing): (Vec<Shown>, Vec<Shown>) =
                picked.partition(|s| current.iter().any(|c| same_issue(c, s)));
            for s in &missing {
                let why = if kept.as_ref().is_some_and(|k| !k.contains(s.rule.as_str())) {
                    "isn't ignored: --rule leaves its rule out of this run"
                } else {
                    "isn't there any more"
                };
                console::status(ctx, &format!("[{}] {} {why}.", s.number, described(s)));
            }
            here
        }
    };
    if chosen.is_empty() {
        console::status(ctx, "No issues to ignore.");
        return Ok(chosen);
    }

    let scope = setup::write_scope(setup.project.as_deref(), scope)?;
    if !setup.config.inherited.includes(&scope) {
        let flag = if numbers.is_some() {
            "--ignore-issues"
        } else {
            "--ignore-all"
        };
        return Err(Error::Usage(format!(
            "{flag}: config sets `inherit = false`, which leaves {} out of this run, so ignores there would never apply",
            ignore::list_name(&scope)
        )));
    }
    let entries: Vec<Entry> = chosen
        .iter()
        .filter_map(|s| {
            Some(Entry::new(
                ignores::file_for(ctx, &scope, s.path.as_deref()?),
                Some(s.rule.clone()),
                s.matched.clone(),
            ))
        })
        .collect();
    let added = ignores::edit(ctx, &scope, |store| {
        let mut added = 0;
        for entry in entries {
            if store.add_issue(entry) {
                added += 1;
            }
        }
        Ok(added)
    })?;
    let list = format!(
        "{} ({})",
        ignore::list_name(&scope),
        ctx.display(&ignores::path(ctx, &scope)?)
    );
    let ignored = english::count(chosen.len(), "issue");
    if numbers.is_some() {
        console::status(
            ctx,
            &format!("Ignored {ignored} in {list}, each wherever its match is in its file:"),
        );
        for s in chosen.iter().take(LISTED) {
            console::status(ctx, &format!("  [{}] {}", s.number, described(s)));
        }
        if chosen.len() > LISTED {
            console::status(
                ctx,
                &format!("  and {} more.", english::number(chosen.len() - LISTED)),
            );
        }
    } else {
        console::status(ctx, &format!("Ignored {ignored} in {list}."));
    }
    if added < chosen.len() {
        console::status(
            ctx,
            &format!(
                "{} already had an ignore.",
                english::count(chosen.len() - added, "issue")
            ),
        );
    }
    Ok(chosen)
}

/// How many issues `--ignore-issues` lists by name.
const LISTED: usize = 20;

/// An error unless every number is one of the `total` that `run` found.
fn in_range(numbers: &[RangeInclusive<usize>], total: usize, run: &str) -> Result<()> {
    let past_end = numbers
        .iter()
        .find(|r| *r.end() > total)
        .map(|r| (*r.start()).max(total + 1));
    match past_end {
        None => Ok(()),
        Some(n) => Err(Error::Usage(match total {
            0 => format!("--ignore-issues: {run} found no issues"),
            1 => format!("--ignore-issues: there's no issue {n}; {run} found 1"),
            _ => format!("--ignore-issues: there's no issue {n}; {run} found {total} (1-{total})"),
        })),
    }
}

/// An issue as `--ignore-issues` names it: `"utilize" in a.md (rule)`.
fn described(s: &Shown) -> String {
    format!("\"{}\" in {} ({})", s.matched, s.file, s.rule)
}

/// Whether two issues are the same to an ignore list: one rule's match,
/// compared as ignores compare it, in one file.
fn same_issue(a: &Shown, b: &Shown) -> bool {
    a.path == b.path && a.rule == b.rule && ignores::key(&a.matched) == ignores::key(&b.matched)
}

/// Takes the issues just ignored out of `documents`, as the next run's
/// ignore list will.
fn drop_ignored(rules: &RuleSet, documents: &mut [Document], ignored: &[Shown]) {
    for doc in documents {
        let here: Vec<&Shown> = ignored
            .iter()
            .filter(|s| s.path.is_some() && s.path == doc.path)
            .collect();
        if here.is_empty() {
            continue;
        }
        doc.issues.retain(|issue| {
            let Some(rule) = rules.get(issue.rule) else {
                return true;
            };
            let matched = ignores::key(&issue.matched);
            !here
                .iter()
                .any(|s| s.rule == rule.id && ignores::key(&s.matched) == matched)
        });
    }
}
