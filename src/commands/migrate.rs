//! `migrate`: converts proserunner 0.8's files on request.

use proserunner::ctx::Ctx;
use proserunner::english;
use proserunner::{Result, migrate};

use super::{EXIT_OK, console};
use crate::cli::MigrateCommand;

pub(crate) fn migrate_command(ctx: &Ctx, command: &MigrateCommand) -> Result<u8> {
    match command {
        MigrateCommand::Rules { src, dst } => {
            let written = migrate::checks::convert_dir(src, dst)?;
            console::stdout(|out| written.iter().try_for_each(|path| writeln!(out, "{path}")))?;
            console::status(
                ctx,
                &format!("Converted {}.", english::count(written.len(), "check")),
            );
            Ok(EXIT_OK)
        }
    }
}
