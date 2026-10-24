//! Where a file is, for tools that open it from the working directory.

use std::path::Path;

use crate::ctx;

pub(super) enum Located<'path> {
    Relative(String),
    Absolute(&'path Path),
}

impl<'path> Located<'path> {
    /// Where a file is for tools that open it from the working directory,
    /// such as a code-scanning upload: relative with `/` when it's under
    /// `cwd`, otherwise absolute. `None` for standard input.
    pub(super) fn of(path: Option<&'path Path>, cwd: &Path) -> Option<Located<'path>> {
        let path = path?;
        Some(match path.strip_prefix(cwd) {
            Ok(rel) => Located::Relative(ctx::slashed(rel)),
            Err(_) => Located::Absolute(path),
        })
    }
}
