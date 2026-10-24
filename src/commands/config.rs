//! `config`: the merged config, and where each value came from.

use std::path::Path;

use proserunner::Result;
use proserunner::ctx::Ctx;

use super::{EXIT_OK, console, setup};

/// Prints the merged config. The rules load too, so every mistake a check
/// would stop at, such as an id that names no rule, stops this first.
pub(crate) fn config_command(ctx: &Ctx, config_file: Option<&Path>) -> Result<u8> {
    let setup = setup::setup(ctx, ctx.project(), config_file, /*use_ignores=*/ false)?;
    console::stdout(|out| out.write_all(setup.config.render(ctx).as_bytes()))?;
    Ok(EXIT_OK)
}
