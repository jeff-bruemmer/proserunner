//! Rule files to compile: built-in text, or files read from a directory.

use std::borrow::Cow;
use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::fsutil;

/// A rule file to compile: built-in text or a file read from disk.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RuleSource<'text> {
    /// `<source>/<file stem>`, e.g. `default/cliches`.
    pub id: String,
    /// Where it came from, for error messages: a path, or the id for a
    /// built-in.
    pub origin: String,
    /// The file's TOML.
    pub text: Cow<'text, str>,
}

impl<'text> RuleSource<'text> {
    /// A rule file whose origin is its id, as for a built-in. Assign
    /// `origin` when it came from somewhere else.
    pub fn new(id: impl Into<String>, text: impl Into<Cow<'text, str>>) -> Self {
        let id = id.into();
        RuleSource {
            origin: id.clone(),
            id,
            text: text.into(),
        }
    }
}

/// Every `*.toml` file directly in `dir`, in name order, with ids
/// `<label>/<file stem>`.
pub fn load_dir(dir: &Path, label: &str) -> Result<Vec<RuleSource<'static>>> {
    fsutil::files_with_extension(dir, "toml")?
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(RuleSource {
                id: format!("{label}/{stem}"),
                origin: path.display().to_string(),
                text: Cow::Owned(text),
            })
        })
        .collect()
}
