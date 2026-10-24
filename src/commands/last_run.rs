//! The issues a run numbered, kept so `--ignore-issues` means the issues
//! a person saw, even after they edit the files and the numbers move.
//!
//! One run is kept, in the cache directory, with the working directory
//! and the paths it checked; a run of the same paths from the same
//! directory reads it back. Keeping it is best effort: a run that can't
//! write it still succeeds, and without it `--ignore-issues` numbers the
//! issues it finds itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use proserunner::ctx::Ctx;
use proserunner::report::Report;
use proserunner::walk;
use serde::{Deserialize, Serialize};

const FILE: &str = "last-run.json";

/// What a kept run is written in. Another is read as no run.
const VERSION: u32 = 1;

/// Which run: the working directory, and the paths checked (absolute,
/// sorted, `-` for standard input) with `--stdin-filename`'s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Key {
    cwd: PathBuf,
    paths: Vec<PathBuf>,
}

impl Key {
    pub(crate) fn new(ctx: &Ctx, paths: &[PathBuf], stdin_filename: Option<&Path>) -> Key {
        let mut absolute: Vec<PathBuf> = paths
            .iter()
            .map(PathBuf::as_path)
            .chain(stdin_filename)
            .map(|p| {
                if p.as_os_str() == walk::STDIN {
                    p.to_path_buf()
                } else {
                    ctx.absolute(p)
                }
            })
            .collect();
        absolute.sort();
        absolute.dedup();
        Key {
            cwd: ctx.cwd.clone(),
            paths: absolute,
        }
    }
}

/// An issue as a run numbered and showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    pub number: usize,
    /// Absolute; `None` for standard input with no `--stdin-filename`.
    pub path: Option<PathBuf>,
    /// The path as shown.
    pub file: String,
    pub rule: String,
    /// As shown, with "…" for hidden text.
    pub matched: String,
}

/// A run as kept: its issues under their files, in the order it numbered
/// them, so the numbers and paths aren't written once per issue.
#[derive(Serialize, Deserialize)]
struct Kept {
    version: u32,
    key: Key,
    files: Vec<KeptFile>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct KeptFile {
    path: Option<PathBuf>,
    file: String,
    /// Each issue's rule and match.
    issues: Vec<(String, String)>,
}

/// The issues of `report`, as it numbers them.
pub(crate) fn shown(report: &Report<'_>) -> Vec<Shown> {
    report
        .issues
        .iter()
        .map(|i| Shown {
            number: i.number,
            path: i.path.map(Path::to_path_buf),
            file: i.file.to_owned(),
            rule: i.rule.id.clone(),
            matched: i.shown().into_owned(),
        })
        .collect()
}

/// Issues in number order, under their files. A report numbers each
/// file's issues in a row, so this keeps every number.
fn kept(shown: Vec<Shown>) -> Vec<KeptFile> {
    let mut files: Vec<KeptFile> = Vec::new();
    for s in shown {
        match files.last_mut() {
            Some(f) if f.file == s.file => f.issues.push((s.rule, s.matched)),
            _ => files.push(KeptFile {
                path: s.path,
                file: s.file,
                issues: vec![(s.rule, s.matched)],
            }),
        }
    }
    files
}

/// Kept issues, numbered again from 1.
fn numbered(files: Vec<KeptFile>) -> Vec<Shown> {
    let mut shown = Vec::new();
    for f in files {
        for (rule, matched) in f.issues {
            shown.push(Shown {
                number: shown.len() + 1,
                path: f.path.clone(),
                file: f.file.clone(),
                rule,
                matched,
            });
        }
    }
    shown
}

/// Keeps `report`'s numbers as the last run of `key`, if it can.
pub(crate) fn save(ctx: &Ctx, key: &Key, report: &Report<'_>) {
    let Some(dir) = &ctx.cache_dir else {
        return;
    };
    let kept = Kept {
        version: VERSION,
        key: key.clone(),
        files: kept(shown(report)),
    };
    if let Ok(json) = serde_json::to_string(&kept) {
        let _ = replace(dir, &json);
    }
}

/// Writes `contents` as [`FILE`] in `dir` through a temporary file, so a
/// reader never sees half of it. Unlike a settings file, it doesn't wait
/// for the disk: a crash costs at most the last run's numbers.
fn replace(dir: &Path, contents: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{FILE}.{}.tmp", process::id()));
    let written = fs::write(&tmp, contents).and_then(|()| fs::rename(&tmp, dir.join(FILE)));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// The issues the last run of `key` numbered, or `None` when the last run
/// kept was of other paths, or none was kept.
pub(crate) fn load(ctx: &Ctx, key: &Key) -> Option<Vec<Shown>> {
    let path = ctx.cache_dir.as_ref()?.join(FILE);
    let kept: Kept = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    (kept.version == VERSION && kept.key == *key).then(|| numbered(kept.files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_issues_number_again_as_they_were() {
        let issue = |number, file: &str, matched: &str| Shown {
            number,
            path: Some(PathBuf::from(format!("/w/{file}"))),
            file: file.to_owned(),
            rule: "default/jargon".to_owned(),
            matched: matched.to_owned(),
        };
        let shown = vec![
            issue(1, "a.md", "x"),
            issue(2, "a.md", "y"),
            issue(3, "b.md", "z"),
        ];
        let files = kept(shown.clone());
        assert_eq!(files.len(), 2);
        assert_eq!(numbered(files), shown);
    }
}
