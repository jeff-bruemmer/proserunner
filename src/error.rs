//! The one error type: every failure a run reports is an [`Error`], and
//! [`Result`] defaults to it.

use std::io;
use std::path::PathBuf;
use std::result;

use crate::english;
use crate::scan::Format;

/// Every failure a run can report. `main` alone turns these into messages
/// and exit status 2.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[must_use]
pub enum Error {
    /// A file or directory that couldn't be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// Why it couldn't be.
        #[source]
        source: io::Error,
    },

    /// A path to check, or `--config`'s, that doesn't exist.
    #[error("{}: no such file or directory", .0.display())]
    NotFound(PathBuf),

    /// A file named to check whose format proserunner doesn't read.
    #[error("{}: {}", .0.display(), unsupported())]
    Unsupported(PathBuf),

    /// A file named to check, or standard input, over
    /// [`MAX_FILE_BYTES`](crate::walk::MAX_FILE_BYTES).
    #[error("{}: {}", .0.display(), Error::TOO_LARGE)]
    TooLarge(PathBuf),

    /// A file named to check that isn't UTF-8.
    #[error("{}: {}", .0.display(), Error::NOT_TEXT)]
    NotText(PathBuf),

    /// A rule file that doesn't parse or validate.
    #[error("{origin}: {message}")]
    Rule {
        /// A path, or `default/<name>` for a built-in.
        origin: String,
        /// What's wrong.
        message: String,
    },

    /// A config or ignore file that doesn't parse or validate.
    #[error("{origin}: {message}")]
    Config {
        /// How its path is shown.
        origin: String,
        /// What's wrong.
        message: String,
    },

    /// An inline control comment that doesn't parse, or names a rule that
    /// doesn't exist.
    #[error("{origin}: {message}")]
    Control {
        /// `file:line:column`.
        origin: String,
        /// What's wrong.
        message: String,
    },

    /// A proserunner 0.8 file that doesn't convert, or whose conversion
    /// would overwrite a file.
    #[error("{0}")]
    Migrate(String),

    /// A command line or environment proserunner can't run with.
    #[error("{0}")]
    Usage(String),
}

impl Error {
    /// Why a file over [`MAX_FILE_BYTES`](crate::walk::MAX_FILE_BYTES)
    /// isn't checked.
    pub const TOO_LARGE: &str = "files must be under 10 MB";

    /// Why a file that isn't UTF-8 isn't checked.
    pub const NOT_TEXT: &str = "not UTF-8 text";

    /// An [`Error::Io`].
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    /// An [`Error::Rule`].
    pub fn rule(origin: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Rule {
            origin: origin.into(),
            message: message.into(),
        }
    }

    /// An [`Error::Config`].
    pub fn config(origin: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Config {
            origin: origin.into(),
            message: message.into(),
        }
    }
}

/// Why proserunner doesn't read a file.
fn unsupported() -> String {
    let dotted: Vec<String> = Format::extensions().map(|e| format!(".{e}")).collect();
    format!(
        "unsupported file type (proserunner reads {}, and what config's [formats] maps)",
        english::and_list(&dotted)
    )
}

/// A `Result` whose error is, unless given, an [`Error`].
pub type Result<T, E = Error> = result::Result<T, E>;
