//! Where a command's output goes: results to standard output, warnings
//! and status lines to standard error.

use std::io::{self, BufWriter, ErrorKind, Write};

use anstream::{AutoStream, ColorChoice};
use proserunner::ctx::Ctx;
use proserunner::{Error, Result};

/// Writes a command's results to stdout. A reader that goes away early
/// (`| head`) isn't an error.
pub(crate) fn stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> Result<()> {
    flushed(BufWriter::new(io::stdout().lock()), write)
}

/// Whether results on stdout should carry ANSI styles: it's a terminal
/// that shows color, unless `NO_COLOR` or `TERM=dumb` says not, or
/// `CLICOLOR_FORCE` asks for them anyway.
pub(crate) fn wants_styles() -> bool {
    AutoStream::choice(&io::stdout()) != ColorChoice::Never
}

/// Like [`stdout`], for results with ANSI styles, which reach the
/// terminal in the form it takes. For when [`wants_styles`].
pub(crate) fn styled_stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> Result<()> {
    flushed(BufWriter::new(AutoStream::auto(io::stdout().lock())), write)
}

fn flushed(
    mut out: impl Write,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<()> {
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
