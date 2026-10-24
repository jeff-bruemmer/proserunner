//! Command-line arguments. `main` runs them.

use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Args, Parser, Subcommand};
use proserunner::format::Output;
use proserunner::rules::Level;
use proserunner::scan::Format;

const EXAMPLES: &str = "\
Examples:
  proserunner docs/                          Check a directory
  proserunner README.md -o plain             One issue per line, for editors and grep
  pandoc -t markdown a.docx | proserunner -  Check another program's output
  proserunner --stdin-filename docs/a.md     Check unsaved text as docs/a.md
  proserunner README.md --ignore-issues 1,3  Ignore issues by the numbers shown
  proserunner docs/ -o sarif > prose.sarif   Results for code scanning
  proserunner explain hedging                What a rule matches, and what changes it

Exit status: 0 no issues at --fail-on or above, 1 issues, 2 error.";

#[derive(Parser)]
#[command(
    name = "proserunner",
    version,
    about = "A fast, customizable prose linter.",
    after_help = EXAMPLES,
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    // Before `check`, whose help headings would otherwise carry on to it.
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(flatten)]
    pub check: CheckArgs,
}

/// The command line, or clap's error and exit status 2. A word after a
/// path is a path, never a command. A check flag before a command, as in
/// `-o json rules`, is an error, since the command would ignore it;
/// `--config` and `--quiet` go anywhere.
pub(crate) fn parse() -> Cli {
    use clap::error::ErrorKind;
    use clap::parser::ValueSource;
    use clap::{CommandFactory as _, FromArgMatches as _};

    let mut command = Cli::command();
    let matches = command.get_matches_mut();
    if let Some((name, _)) = matches.subcommand() {
        let misplaced = command.get_arguments().find(|a| {
            !a.is_global_set()
                && matches.value_source(a.get_id().as_str()) == Some(ValueSource::CommandLine)
        });
        if let Some(arg) = misplaced {
            let message = format!(
                "'{arg}' can't come before the subcommand '{name}'\n\n  \
                 tip: to check a path named '{name}', use './{name}'"
            );
            command.error(ErrorKind::ArgumentConflict, message).exit();
        }
    }
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

#[derive(Args)]
pub(crate) struct GlobalArgs {
    /// Apply FILE last, over the user and project configs
    #[arg(short, long, value_name = "FILE", global = true)]
    pub config: Option<PathBuf>,

    /// Print only results and errors
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Check files and directories (the default; `check` is optional)
    Check(CheckArgs),
    /// Show what a rule matches, its examples, and the settings that change it
    Explain {
        /// A rule id, such as default/hedging, or its name alone when only
        /// one rule has it
        rule: String,
    },
    /// Ignore words everywhere, or list, audit and clear ignores
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommand,
    },
    /// List rules, test their examples, or export the built-ins
    Rules {
        #[command(subcommand)]
        command: Option<RulesCommand>,
    },
    /// Show the merged config, with the file each value came from
    Config,
    /// Set up .proserunner/ for a project in this directory
    Init,
    /// Convert files from proserunner 0.8 (settings convert on their own)
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
}

#[derive(Args)]
pub(crate) struct CheckArgs {
    /// Files and directories to check; - reads standard input, as do
    /// --stdin-format and --stdin-filename with no paths
    pub paths: Vec<PathBuf>,

    /// Skip paths matching PATTERN (gitignore syntax, relative to each
    /// directory checked; repeat for more)
    #[arg(short, long, value_name = "PATTERN", help_heading = INPUT)]
    pub exclude: Vec<String>,

    /// Skip excluded and ignored files even when they're named, as hooks
    /// such as pre-commit name them
    #[arg(long, help_heading = INPUT)]
    pub force_exclude: bool,

    /// Read standard input as FORMAT: markdown, mdx, org, text, html,
    /// asciidoc or typst [default: from --stdin-filename, else markdown]
    #[arg(long, value_name = "FORMAT", value_parser = format_parser(), hide_possible_values = true, help_heading = INPUT)]
    pub stdin_format: Option<Format>,

    /// Check standard input as the file at PATH: its format, overrides and
    /// ignores apply, and issues name it
    #[arg(long, value_name = "PATH", help_heading = INPUT)]
    pub stdin_filename: Option<PathBuf>,

    /// Check quoted text too
    #[arg(long, overrides_with = "no_quoted_text", help_heading = WHAT)]
    pub quoted_text: bool,

    /// Don't check quoted text, whatever config says
    #[arg(long, overrides_with = "quoted_text", help_heading = WHAT)]
    pub no_quoted_text: bool,

    /// Check code blocks too
    #[arg(long, overrides_with = "no_code_blocks", help_heading = WHAT)]
    pub code_blocks: bool,

    /// Don't check code blocks, whatever config says
    #[arg(long, overrides_with = "code_blocks", help_heading = WHAT)]
    pub no_code_blocks: bool,

    /// Output format [default: group]
    #[arg(short, long, value_enum, help_heading = OUTPUT)]
    pub output: Option<Output>,

    /// Exit 1 only for issues at LEVEL or above [default: warning]
    #[arg(long, value_name = "LEVEL", value_enum, help_heading = OUTPUT)]
    pub fail_on: Option<Level>,

    /// Print how long the run took
    #[arg(long, help_heading = OUTPUT)]
    pub timer: bool,

    /// Don't apply ignore lists for this run
    #[arg(long, help_heading = IGNORING)]
    pub no_ignores: bool,

    /// Ignore issues by the numbers the last run of these paths showed,
    /// such as 1,3,5-7
    #[arg(long, value_name = "NUMBERS", conflicts_with = "ignore_all", help_heading = IGNORING)]
    pub ignore_issues: Option<String>,

    /// Ignore every issue from this run
    #[arg(long, help_heading = IGNORING)]
    pub ignore_all: bool,

    #[command(flatten, next_help_heading = IGNORING)]
    pub scope: ScopeArgs,
}

/// `check`'s help headings.
const INPUT: &str = "Input";
const WHAT: &str = "What to check";
const OUTPUT: &str = "Output";
const IGNORING: &str = "Ignoring";

/// Which ignore list a command writes. By default, the project's inside a
/// project, otherwise the global one.
#[derive(Args, Clone, Copy)]
pub(crate) struct ScopeArgs {
    /// Use the global ignore list, even inside a project
    #[arg(long, conflicts_with = "project")]
    pub global: bool,

    /// Use the project's ignore list (.proserunner/ignores.toml)
    #[arg(long)]
    pub project: bool,
}

/// `--stdin-format`'s values: the format names config takes.
fn format_parser() -> impl TypedValueParser<Value = Format> {
    PossibleValuesParser::new(Format::all().map(Format::name))
        .try_map(|name| Format::named(&name).ok_or("not a format"))
}

/// A flag pair's answer: `--x` or `--no-x`, whichever came last, else
/// `default`.
pub(crate) fn switch(on: bool, off: bool, default: bool) -> bool {
    if off { false } else { on || default }
}

impl ScopeArgs {
    pub(crate) fn given(self) -> bool {
        self.global || self.project
    }
}

#[derive(Subcommand)]
pub(crate) enum IgnoreCommand {
    /// Ignore a word or phrase everywhere (a whole match, any case)
    Add {
        #[arg(required = true)]
        words: Vec<String>,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Stop ignoring a word or phrase
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required = true)]
        words: Vec<String>,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Show the ignore lists that apply here
    #[command(visible_alias = "ls")]
    List {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Remove every ignore from a list
    Clear {
        /// Don't ask first (required when not run in a terminal)
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// List ignores for files that no longer exist
    Audit {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Remove ignores for files that no longer exist
    Clean {
        #[command(flatten)]
        scope: ScopeArgs,
    },
}

#[derive(Subcommand)]
pub(crate) enum RulesCommand {
    /// List rules and their levels (the default)
    List,
    /// Check every rule's examples; with DIRs, only the rules in them
    Test { dirs: Vec<PathBuf> },
    /// Copy the built-in rules into DIR, to edit them
    Export { dir: PathBuf },
}

#[derive(Subcommand)]
pub(crate) enum MigrateCommand {
    /// Convert 0.8 check files (*.edn) in SRC into rule files in DST
    Rules { src: PathBuf, dst: PathBuf },
}
