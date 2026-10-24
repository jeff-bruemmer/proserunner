//! The issues a run numbered, kept so `--ignore-issues` means the issues
//! a person saw, even after they edit the files and the numbers move.
//!
//! The most recent runs are kept in the cache directory, each in a file
//! of its own, with the working directory and the paths it checked; a run
//! of the same paths from the same directory reads it back. So a check of
//! other paths, such as a commit hook's, doesn't lose the numbers a person
//! is working from. Keeping them is best effort: a run that can't write
//! still succeeds, and without its paths' run `--ignore-issues` numbers
//! the issues it finds itself.

use std::cmp::Reverse;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::time::SystemTime;

use proserunner::ctx::Ctx;
use proserunner::report::Report;
use proserunner::walk;
use serde::{Deserialize, Serialize};

/// The directory in the cache that holds the runs.
const DIR: &str = "runs";

/// How many runs are kept: the most recently written.
const KEPT: usize = 20;

/// What a kept run is written in. Another is read as no run.
const VERSION: u32 = 1;

/// Which run: the working directory, and the paths checked (absolute,
/// sorted, `-` for standard input) with `--stdin-filename`'s.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The name of the file its run is kept in: a hash of it. The file
    /// holds the key too, so keys that share a hash read as no run, and a
    /// build that hashes differently only loses the runs kept before it.
    fn file(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        format!("{:016x}.json", hasher.finish())
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

/// Keeps `report`'s numbers as the last run of `key`, if it can, and
/// drops all but the [`KEPT`] most recent runs.
pub(crate) fn save(ctx: &Ctx, key: &Key, report: &Report<'_>) {
    let Some(cache) = &ctx.cache_dir else {
        return;
    };
    let kept = Kept {
        version: VERSION,
        key: key.clone(),
        files: kept(shown(report)),
    };
    let dir = cache.join(DIR);
    if let Ok(json) = serde_json::to_string(&kept)
        && let Ok(path) = replace(&dir, &key.file(), &json)
    {
        prune(&dir, &path);
    }
}

/// Writes `contents` as `name` in `dir` through a temporary file, so a
/// reader never sees half of it, and returns its path. Unlike a settings
/// file, it doesn't wait for the disk: a crash costs at most one run's
/// numbers.
fn replace(dir: &Path, name: &str, contents: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(name);
    let tmp = dir.join(format!(".{name}.{}.tmp", process::id()));
    let written = fs::write(&tmp, contents).and_then(|()| fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map(|()| path)
}

/// Removes the runs in `dir` past the [`KEPT`] most recently written,
/// never `newest`, the one just written.
fn prune(dir: &Path, newest: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut older: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let written = fs::metadata(&path).ok()?.modified().ok()?;
            let run = path.extension()? == "json" && path != newest;
            run.then_some((written, path))
        })
        .collect();
    if older.len() < KEPT {
        return;
    }
    older.sort_by_key(|&(written, _)| Reverse(written));
    for (_, path) in &older[KEPT - 1..] {
        let _ = fs::remove_file(path);
    }
}

/// The issues the last run of `key` numbered, or `None` when none was
/// kept.
pub(crate) fn load(ctx: &Ctx, key: &Key) -> Option<Vec<Shown>> {
    let path = ctx.cache_dir.as_ref()?.join(DIR).join(key.file());
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

    #[test]
    fn only_the_most_recent_runs_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for i in 0..KEPT + 5 {
            paths.push(replace(dir.path(), &format!("{i}.json"), "{}").unwrap());
        }
        let newest = &paths[0];
        prune(dir.path(), newest);
        let left = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, KEPT);
        assert!(newest.exists());
    }
}
