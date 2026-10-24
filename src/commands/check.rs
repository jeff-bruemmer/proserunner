//! `check`: walk, read, lint, apply policy, report. Also
//! `--ignore-issues` and `--ignore-all`, which ignore what a check finds.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;

use proserunner::ctx::Ctx;
use proserunner::ignores::{self, Entry};
use proserunner::lint::Exhausted;
use proserunner::report::{self, Document, LineIndex, Position, Report};
use proserunner::rules::RuleSet;
use proserunner::scan::{Format, Mask, Options};
use proserunner::walk::{self, Filters, FormatOf, Input};
use proserunner::{Error, Result, lint};
use proserunner::{english, format};

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
    let setup = setup::setup(
        ctx,
        ctx.project_for(checked)?,
        config_file,
        !args.no_ignores,
    )?;
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

    let read: Vec<Read> = walk
        .inputs
        .par_iter()
        .map(|input| read_and_lint(ctx, &setup, input, &stdin, options, &walk.named))
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
    let report = report::build(&setup.rules, &documents);

    if ignoring {
        let code = ignore_reported(ctx, &setup, args.scope, &report, numbers.as_deref())?;
        return Ok(if broken_rules(&setup) {
            EXIT_ERROR
        } else {
            code
        });
    }

    let output = args.output.unwrap_or(config.output.value);
    console::stdout(|out| format::write(out, &report, output, &ctx.cwd))?;
    if output.has_summary() {
        console::status(ctx, &english::summary(report.issues.len()));
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

fn read_and_lint(
    ctx: &Ctx,
    setup: &Setup,
    input: &Input,
    stdin: &Stdin,
    options: Options<'_>,
    named: &HashSet<PathBuf>,
) -> Result<Read> {
    use std::io::Read as _;

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
            let source = String::from_utf8(bytes).map_err(|_| Error::NotText(STDIN_NAME.into()))?;
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
    let applied = setup
        .policy
        .apply(path.as_deref(), &source, linted)
        .map_err(|e| {
            let at = LineIndex::new(&source).position(&source, e.at);
            Error::Control {
                origin: format!("{display}:{}:{}", at.line, at.column),
                message: e.message,
            }
        })?;
    let warnings = exhausted_warnings(&setup.rules, &display, &source, &applied.exhausted);
    Ok(Read::Linted(
        Document::new(display, path, source, applied.issues),
        warnings,
    ))
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

/// `--ignore-issues` and `--ignore-all`: adds the chosen issues of this
/// run, numbered as a plain run would show them, to an ignore list.
fn ignore_reported(
    ctx: &Ctx,
    setup: &Setup,
    scope: ScopeArgs,
    report: &Report<'_>,
    numbers: Option<&[RangeInclusive<usize>]>,
) -> Result<u8> {
    let total = report.issues.len();
    let chosen: Vec<_> = match numbers {
        None => report.issues.iter().collect(),
        Some(numbers) => {
            let past_end = numbers
                .iter()
                .find(|r| *r.end() > total)
                .map(|r| (*r.start()).max(total + 1));
            if let Some(n) = past_end {
                return Err(Error::Usage(match total {
                    0 => "--ignore-issues: this run found no issues".into(),
                    1 => format!("--ignore-issues: there's no issue {n}; this run found 1"),
                    _ => format!(
                        "--ignore-issues: there's no issue {n}; this run found {total} (1-{total})"
                    ),
                }));
            }
            report
                .issues
                .iter()
                .filter(|i| numbers.iter().any(|r| r.contains(&i.number)))
                .collect()
        }
    };
    if chosen.is_empty() {
        console::status(ctx, "No issues to ignore.");
        return Ok(EXIT_OK);
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
        .filter_map(|i| {
            Some(Entry::new(
                ignores::file_for(ctx, &scope, i.path?),
                Some(i.rule.id.clone()),
                i.shown().into_owned(),
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
    console::status(
        ctx,
        &format!(
            "Ignored {} in {} ({}).",
            english::count(chosen.len(), "issue"),
            ignore::list_name(&scope),
            ctx.display(&ignores::path(ctx, &scope)?)
        ),
    );
    if added < chosen.len() {
        console::status(
            ctx,
            &format!(
                "{} already had an ignore.",
                english::count(chosen.len() - added, "issue")
            ),
        );
    }
    Ok(EXIT_OK)
}
