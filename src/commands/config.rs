//! `config`: the merged config, and where each value came from.

use std::path::Path;

use proserunner::Result;
use proserunner::config;
use proserunner::ctx::Ctx;

use super::{EXIT_OK, console, setup};

pub(crate) fn config_command(ctx: &Ctx, config_file: Option<&Path>) -> Result<u8> {
    let project = setup::prepare(ctx, ctx.project())?;
    let config = config::load(ctx, project.as_deref(), config_file)?;
    console::stdout(|out| out.write_all(config.render(ctx).as_bytes()))?;
    Ok(EXIT_OK)
}
