//! Where a command's output goes: results to standard output, warnings
//! and status lines to standard error.

use std::io::{self, BufWriter, ErrorKind, Write};

use proserunner::ctx::Ctx;
use proserunner::{Error, Result};

/// Writes a command's results to stdout. A reader that goes away early
/// (`| head`) isn't an error.
pub(crate) fn stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    match write(&mut out).and_then(|()| out.flush()) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(Error::io("<stdout>", e)),
        _ => Ok(()),
    }
}

pub(crate) fn warn(message: &str) {
    eprintln!("proserunner: {message}");
}

pub(crate) fn status(ctx: &Ctx, message: &str) {
    if !ctx.quiet {
        eprintln!("{message}");
    }
}
