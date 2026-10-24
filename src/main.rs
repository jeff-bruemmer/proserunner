//! The `proserunner` binary: arguments in, an exit status out. The
//! commands live in `commands`; everything else is the library.

mod cli;
mod commands;

use std::process::ExitCode;

use cli::{Cli, Command};
use commands::EXIT_ERROR;
use proserunner::Result;
use proserunner::ctx::Ctx;

fn main() -> ExitCode {
    let cli = cli::parse();
    let result = Ctx::from_env(cli.global.quiet).and_then(|ctx| run(&ctx, cli));
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            for one in e.each() {
                eprintln!("proserunner: {one}");
            }
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn run(ctx: &Ctx, cli: Cli) -> Result<u8> {
    let config = cli.global.config.as_deref();
    match cli.command {
        None => commands::check(ctx, &cli.check, config),
        Some(Command::Check(args)) => commands::check(ctx, &args, config),
        Some(Command::Explain { rule }) => commands::explain_command(ctx, &rule, config),
        Some(Command::Ignore { command }) => commands::ignore_command(ctx, command, config),
        Some(Command::Rules { command }) => commands::rules_command(ctx, command, config),
        Some(Command::Config) => commands::config_command(ctx, config),
        Some(Command::Init) => commands::init(ctx),
        Some(Command::Migrate { command }) => commands::migrate_command(ctx, &command),
        Some(Command::Guide { topic }) => commands::guide_command(topic),
        Some(Command::Schema { kind }) => commands::schema_command(kind),
    }
}
