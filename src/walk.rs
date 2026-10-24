//! Finds the files to check. Directories are walked with gitignore rules:
//! `.proserunnerignore`, `.ignore` and, inside a git repository,
//! `.gitignore` at any level, then git's `info/exclude` and global
//! excludes file, plus `--exclude` globs (relative to each directory
//! checked) and config `exclude` patterns. `--force-exclude` applies the
//! same rules to files named directly.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::{self, Debug, Formatter};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::overrides::OverrideBuilder;
use ignore::{Match, WalkBuilder};

use crate::ctx;
use crate::english;
use crate::error::{Error, Result};
use crate::scan::Format;

/// Files larger than this are skipped.
pub const MAX_FILE_BYTES: u64 = 10_000_000;

/// The path that means standard input.
pub const STDIN: &str = "-";

/// Something to check.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(clippy::exhaustive_enums, reason = "closed: standard input or a file")]
pub enum Input {
    /// Standard input.
    Stdin,
    /// A file, named or found in a directory.
    File {
        /// As named, or under the directory named.
        path: PathBuf,
        /// What to read it as.
        format: Format,
    },
}

/// What [`inputs`] found.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Walk {
    /// Each once, in the order found, however each was named.
    pub inputs: Vec<Input>,
    /// What a directory walk skipped, with the reason: files too large,
    /// directories or ignore files it couldn't read, and a directory named
    /// with nothing to check in it.
    pub skipped: Vec<String>,
    /// The absolute paths of the files named directly, rather than found
    /// in a directory.
    pub named: HashSet<PathBuf>,
}

/// A walk underway, and what's in its inputs, so a repeat is found in
/// constant time.
#[derive(Default)]
struct Walking {
    walk: Walk,
    /// Each input's absolute path, or `None` for standard input, so a
    /// file named two ways (`a.md`, `./a.md`) comes once.
    seen: HashSet<Option<PathBuf>>,
}

impl Walking {
    /// Adds `input`, unless it's there already.
    fn push(&mut self, input: Input, cwd: &Path) {
        let key = match &input {
            Input::Stdin => None,
            Input::File { path, .. } => Some(ctx::absolute(cwd, path)),
        };
        if self.seen.insert(key) {
            self.walk.inputs.push(input);
        }
    }
}

/// Whether config excludes a file.
pub type Excluded<'walk> = &'walk dyn Fn(&Path) -> bool;

/// A file's format, from config and its extension, or `None` when
/// proserunner doesn't read it.
pub type FormatOf<'walk> = &'walk dyn Fn(&Path) -> Option<Format>;

/// What keeps a file from being checked, and what it's checked as.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Filters<'walk> {
    /// `--exclude` patterns.
    pub exclude: &'walk [String],
    /// Config `exclude`.
    pub excluded: Excluded<'walk>,
    /// Apply both, and ignore files, to files named directly too.
    pub force: bool,
    /// What each file is read as. A file it has no format for is skipped
    /// when found, and an error when named.
    pub format: FormatOf<'walk>,
    /// `--stdin-filename`: what `force` checks for standard input.
    pub stdin_name: Option<&'walk Path>,
    /// What relative paths are relative to.
    pub cwd: &'walk Path,
}

impl<'walk> Filters<'walk> {
    /// Filters that exclude nothing and check standard input as no file.
    /// Assign the other fields to exclude more.
    pub fn new(cwd: &'walk Path, format: FormatOf<'walk>) -> Self {
        Filters {
            exclude: &[],
            excluded: &never,
            force: false,
            format,
            stdin_name: None,
            cwd,
        }
    }
}

/// The default [`Filters::excluded`]: nothing is.
fn never(_: &Path) -> bool {
    false
}

impl Debug for Filters<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Filters")
            .field("exclude", &self.exclude)
            .field("force", &self.force)
            .field("stdin_name", &self.stdin_name)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

/// The inputs named by `paths`, in order, without duplicates. A file
/// named directly is checked unless `force` excludes it; it's an error if
/// it doesn't exist, is too large, or isn't a supported type.
pub fn inputs(paths: &[PathBuf], filters: Filters<'_>) -> Result<Walk> {
    let mut walk = Walking::default();
    let mut named = Named::default();
    for path in paths {
        if path.as_os_str() == STDIN {
            let excluded = match filters.stdin_name {
                Some(name) if filters.force => named.excluded(name, filters)?,
                _ => false,
            };
            if !excluded {
                walk.push(Input::Stdin, filters.cwd);
            }
            continue;
        }
        let meta = fs::metadata(path).map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::NotFound(path.clone()),
            _ => Error::io(path, e),
        })?;
        if meta.is_dir() {
            walk_dir(path, filters, &mut walk)?;
            continue;
        }
        if filters.force && named.excluded(path, filters)? {
            continue;
        }
        let format = (filters.format)(path).ok_or_else(|| Error::Unsupported(path.clone()))?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(Error::TooLarge(path.clone()));
        }
        walk.walk.named.insert(ctx::absolute(filters.cwd, path));
        let input = Input::File {
            path: path.clone(),
            format,
        };
        walk.push(input, filters.cwd);
    }
    Ok(walk.walk)
}

/// proserunner's own ignore file, which outranks the others.
const IGNORE_FILE: &str = ".proserunnerignore";

/// `--force-exclude` for files named directly: the `--exclude` patterns,
/// each directory's ignore files and git's global excludes file, each
/// read once.
#[derive(Default)]
struct Named {
    exclude: Option<Gitignore>,
    /// Directory → its ignore files.
    dirs: HashMap<PathBuf, DirIgnores>,
    /// Git's global excludes file, rooted at the working directory.
    global: Option<Gitignore>,
}

impl Named {
    fn excluded(&mut self, path: &Path, filters: Filters<'_>) -> Result<bool> {
        if (filters.excluded)(path) {
            return Ok(true);
        }
        let absolute = ctx::absolute(filters.cwd, path);
        if self.exclude.is_none() {
            self.exclude = Some(exclude_globs(filters.cwd, filters.exclude)?);
        }
        if self
            .exclude
            .as_ref()
            .is_some_and(|g| excluded_by(g, &absolute))
        {
            return Ok(true);
        }
        Ok(self.skipped(&absolute, filters.cwd))
    }

    /// Whether a walk from `root` would skip the file `path`. Going down
    /// through what [`ctx::below`] gives, each directory and then the
    /// file is skipped if an ignore file ignores it, or if it's hidden
    /// and no ignore file names it.
    fn skipped(&mut self, path: &Path, root: &Path) -> bool {
        let depth = ctx::below(path, root).components().count();
        let entries: Vec<&Path> = path.ancestors().take(depth).collect();
        entries.into_iter().rev().any(|entry| {
            self.ignored(entry, /*is_dir=*/ entry != path, root)
                .unwrap_or_else(|| is_hidden(entry))
        })
    }

    /// Whether the ignore files above `entry` ignore it, or `None` when
    /// none has an opinion, by a walk's rules: for each kind of
    /// [`DirIgnores`], the nearest file with an opinion decides, and the
    /// strongest kind with one wins, then git's global excludes file. The
    /// git kinds count only in a repository, and not above its root.
    fn ignored(&mut self, entry: &Path, is_dir: bool, cwd: &Path) -> Option<bool> {
        let dirs: Vec<&Path> = entry.ancestors().skip(1).collect();
        let in_repo = dirs.iter().any(|d| self.dir(d).repo);
        let mut kinds = [None; 4];
        let mut above_repo = false;
        for dir in dirs {
            let ignores = self.dir(dir);
            let counted = if in_repo && !above_repo {
                ignores.kinds.len()
            } else {
                DirIgnores::ANYWHERE
            };
            for (kind, matcher) in kinds.iter_mut().zip(&ignores.kinds).take(counted) {
                if kind.is_none() {
                    *kind = verdict(matcher.matched(entry, is_dir));
                }
            }
            above_repo |= ignores.repo;
        }
        let found = kinds.into_iter().flatten().next();
        if found.is_some() || !in_repo {
            return found;
        }
        let global = self.global.get_or_insert_with(|| global_excludes(cwd));
        verdict(global.matched(entry, is_dir))
    }

    fn dir(&mut self, dir: &Path) -> &DirIgnores {
        self.dirs
            .entry(dir.to_path_buf())
            .or_insert_with(|| DirIgnores::read(dir))
    }
}

/// One directory's ignore files as a walk reads them: a matcher for each
/// kind, strongest first.
struct DirIgnores {
    /// `.proserunnerignore` and `.ignore`, which count anywhere, then
    /// `.gitignore` and git's `info/exclude`, which count only in a
    /// repository.
    kinds: [Gitignore; 4],
    /// Whether the directory is a repository's root: it holds `.git` or
    /// `.jj`.
    repo: bool,
}

impl DirIgnores {
    /// How many [`kinds`](Self::kinds) count outside a repository.
    const ANYWHERE: usize = 2;

    fn read(dir: &Path) -> Self {
        let file = |name: &str| ignore_file(dir, &dir.join(name));
        let exclude = git_dir(dir).map_or_else(Gitignore::empty, |git| {
            ignore_file(dir, &git.join("info/exclude"))
        });
        DirIgnores {
            kinds: [
                file(IGNORE_FILE),
                file(".ignore"),
                file(".gitignore"),
                exclude,
            ],
            repo: dir.join(".git").exists() || dir.join(".jj").exists(),
        }
    }
}

/// Whether a walk treats `path` as hidden: its name starts with `.`.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// A match as whether it ignores, or `None` for no opinion.
fn verdict<T>(found: Match<T>) -> Option<bool> {
    match found {
        Match::Ignore(_) => Some(true),
        Match::Whitelist(_) => Some(false),
        Match::None => None,
    }
}

/// The ignore file `file` as a matcher rooted at `dir`, empty when there's
/// no such file. A line that doesn't parse is skipped, as a walk skips it.
fn ignore_file(dir: &Path, file: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(dir);
    if file.is_file() {
        let _ = builder.add(file);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// The git directory where a walk reads `info/exclude` for `dir`: its
/// `.git`, or in a worktree, where `.git` is a file, the main
/// repository's, through `commondir`. `None` when a `.git` file leads to
/// none. As in a walk, a relative `gitdir:` is read from the working
/// directory.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    if !dot_git.is_file() {
        return Some(dot_git);
    }
    let first_line = |file: &Path| Some(fs::read_to_string(file).ok()?.lines().next()?.to_owned());
    let real = PathBuf::from(first_line(&dot_git)?.strip_prefix("gitdir: ")?);
    let common = first_line(&real.join("commondir"))?;
    Some(if common.starts_with('.') {
        real.join(common)
    } else {
        PathBuf::from(common)
    })
}

/// Git's global excludes file (`core.excludesFile`, else
/// `$XDG_CONFIG_HOME/git/ignore`) as a walk reads it, rooted at `cwd`.
fn global_excludes(cwd: &Path) -> Gitignore {
    GitignoreBuilder::new(cwd).build_global().0
}

/// Whether `globs` ignore `path`, or a directory it's in below their root.
/// Unlike `matched_path_or_any_parents`, it takes a path outside the root,
/// where only patterns without a `/` can match it.
fn excluded_by(globs: &Gitignore, path: &Path) -> bool {
    let root = globs.path();
    for (i, p) in path.ancestors().enumerate() {
        if root.starts_with(p) {
            break;
        }
        if let Some(ignored) = verdict(globs.matched(p, /*is_dir=*/ i > 0)) {
            return ignored;
        }
    }
    false
}

/// `--exclude` patterns as a gitignore rooted at `cwd`.
fn exclude_globs(cwd: &Path, exclude: &[String]) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(cwd);
    let patterns = exclude.iter().filter(|p| !p.trim().is_empty());
    for pattern in patterns {
        builder
            .add_line(None, pattern.trim())
            .map_err(|e| Error::Usage(format!("--exclude {pattern}: {e}")))?;
    }
    builder
        .build()
        .map_err(|e| Error::Usage(format!("--exclude: {e}")))
}

/// A walker with proserunner's ignore files.
fn walker(dir: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(dir);
    builder
        .add_custom_ignore_filename(IGNORE_FILE)
        .sort_by_file_path(Path::cmp);
    builder
}

fn walk_dir(dir: &Path, filters: Filters<'_>, walk: &mut Walking) -> Result<()> {
    let mut overrides = OverrideBuilder::new(dir);
    let patterns = filters.exclude.iter().filter(|p| !p.trim().is_empty());
    for pattern in patterns {
        overrides
            .add(&format!("!{}", pattern.trim()))
            .map_err(|e| Error::Usage(format!("--exclude {pattern}: {e}")))?;
    }
    let overrides = overrides
        .build()
        .map_err(|e| Error::Usage(format!("--exclude: {e}")))?;
    // The directory named must be readable; what's under it is skipped
    // with a warning when it isn't.
    fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;

    let mut found = 0;
    let mut unread = BTreeSet::new();
    for entry in walker(dir).overrides(overrides).build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                // A bad line in an ignore file skips only that line.
                let skipped = if e.is_partial() { "" } else { "; skipped" };
                walk.walk.skipped.push(format!("{e}{skipped}"));
                continue;
            }
        };
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(format) = (filters.format)(path) else {
            if let Some(ext) = path.extension() {
                unread.insert(format!(".{}", ext.to_string_lossy().to_lowercase()));
            }
            continue;
        };
        if (filters.excluded)(path) {
            continue;
        }
        found += 1;
        let too_large = entry.metadata().is_ok_and(|m| m.len() > MAX_FILE_BYTES);
        if too_large {
            walk.walk
                .skipped
                .push(format!("{}: skipped, {}", path.display(), Error::TOO_LARGE));
            continue;
        }
        let input = Input::File {
            path: path.to_path_buf(),
            format,
        };
        walk.push(input, filters.cwd);
    }
    if found == 0 {
        walk.walk.skipped.push(nothing_to_check(dir, &unread));
    }
    Ok(())
}

/// Why a directory named had nothing to check, naming the extensions of
/// the files in it that proserunner doesn't read, when there are some.
fn nothing_to_check(dir: &Path, unread: &BTreeSet<String>) -> String {
    const SHOWN: usize = 5;
    if unread.is_empty() {
        return format!("{}: no files to check", dir.display());
    }
    let mut exts: Vec<String> = unread.iter().take(SHOWN).cloned().collect();
    if unread.len() > SHOWN {
        exts.push(format!("{} more", unread.len() - SHOWN));
    }
    format!(
        "{}: no files to check; proserunner doesn't read {} files (config's [formats] can map an extension to a format it reads)",
        dir.display(),
        english::and_list(&exts)
    )
}

/// How a path is shown: relative to `cwd` when under it, `~/…` when under
/// `home`, otherwise as given. A leading `./` is dropped.
pub fn display(path: &Path, cwd: &Path, home: Option<&Path>) -> String {
    let shown = if path.is_absolute() {
        if let Ok(rel) = path.strip_prefix(cwd) {
            rel.display().to_string()
        } else if let Some(rel) = home.and_then(|h| path.strip_prefix(h).ok()) {
            format!("~/{}", rel.display())
        } else {
            path.display().to_string()
        }
    } else {
        path.display().to_string()
    };
    shown.strip_prefix("./").map(str::to_owned).unwrap_or(shown)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::slice;

    use super::*;

    #[test]
    fn walks_sorted_supported_files_with_ignores() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("drafts")).unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("b.md"), "b").unwrap();
        fs::write(root.join("A.MD"), "a").unwrap();
        fs::write(root.join("code.rs"), "x").unwrap();
        fs::write(root.join("drafts/d.md"), "d").unwrap();
        fs::write(root.join("sub/x.backup.md"), "x").unwrap();
        fs::write(root.join("sub/c.txt"), "c").unwrap();
        fs::write(root.join(".proserunnerignore"), "drafts/\n").unwrap();

        let excluded = |p: &Path| p.ends_with("sub/skip.md");
        fs::write(root.join("sub/skip.md"), "s").unwrap();
        let exclude = ["*.backup.md".to_owned()];
        let filters = Filters {
            exclude: &exclude,
            excluded: &excluded,
            force: false,
            format: &Format::of,
            stdin_name: None,
            cwd: root,
        };
        let walk = inputs(&[root.to_path_buf()], filters).unwrap();
        let names: Vec<_> = walk
            .inputs
            .iter()
            .map(|i| match i {
                Input::File { path, .. } => path.strip_prefix(root).unwrap().display().to_string(),
                Input::Stdin => "-".into(),
            })
            .collect();
        assert_eq!(names, ["A.MD", "b.md", "sub/c.txt"]);
    }

    #[test]
    fn inputs_come_once_each_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "a").unwrap();
        fs::write(root.join("b.md"), "b").unwrap();
        let filters = Filters {
            exclude: &[],
            excluded: &|_| false,
            force: false,
            format: &Format::of,
            stdin_name: None,
            cwd: root,
        };
        let (a, b) = (root.join("a.md"), root.join("b.md"));
        let paths = [
            b.clone(),
            PathBuf::from(STDIN),
            root.to_path_buf(),
            b.clone(),
            PathBuf::from(STDIN),
        ];
        let walk = inputs(&paths, filters).unwrap();
        assert_eq!(
            walk.inputs,
            [
                Input::File {
                    path: b,
                    format: Format::Markdown,
                },
                Input::Stdin,
                Input::File {
                    path: a,
                    format: Format::Markdown,
                },
            ]
        );
    }

    #[test]
    fn named_files_must_be_supported() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.rs");
        fs::write(&file, "x").unwrap();
        let none = |_: &Path| false;
        let filters = Filters {
            exclude: &[],
            excluded: &none,
            force: false,
            format: &Format::of,
            stdin_name: None,
            cwd: dir.path(),
        };
        assert!(matches!(
            inputs(&[file], filters),
            Err(Error::Unsupported(_))
        ));
        let missing = dir.path().join("missing.md");
        assert!(matches!(
            inputs(&[missing], filters),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn forced_excludes_apply_to_named_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("drafts")).unwrap();
        for name in ["a.md", "b.backup.md", "c.md", "drafts/d.md"] {
            fs::write(root.join(name), "x").unwrap();
        }
        fs::write(root.join(".proserunnerignore"), "drafts/\n").unwrap();
        let named: Vec<PathBuf> = ["a.md", "b.backup.md", "c.md", "drafts/d.md"]
            .iter()
            .map(|n| root.join(n))
            .collect();
        let excluded = |p: &Path| p.ends_with("c.md");
        let exclude = ["*.backup.md".to_owned()];
        let names = |force: bool| {
            let filters = Filters {
                exclude: &exclude,
                excluded: &excluded,
                force,
                format: &Format::of,
                stdin_name: None,
                cwd: root,
            };
            inputs(&named, filters)
                .unwrap()
                .inputs
                .iter()
                .map(|i| match i {
                    Input::File { path, .. } => {
                        path.strip_prefix(root).unwrap().display().to_string()
                    }
                    Input::Stdin => "-".into(),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(names(false), ["a.md", "b.backup.md", "c.md", "drafts/d.md"]);
        assert_eq!(names(true), ["a.md"]);
    }

    /// Writes `files` under a new directory, where `{root}` in a text
    /// stands for that directory, and checks that naming each Markdown
    /// file with `force` keeps exactly the ones a walk of it finds.
    /// Returns those, relative and sorted.
    fn forced_keeps_what_a_walk_finds(files: &[(&str, &str)]) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (name, text) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.replace("{root}", &root.display().to_string())).unwrap();
        }
        let found = |filters: Filters<'_>, paths: &[PathBuf]| {
            let walk = inputs(paths, filters).unwrap();
            let mut names: Vec<String> = walk
                .inputs
                .iter()
                .filter_map(|i| match i {
                    Input::File { path, .. } => Some(ctx::slashed(path.strip_prefix(root).ok()?)),
                    Input::Stdin => None,
                })
                .collect();
            names.sort();
            names
        };
        let mut filters = Filters::new(root, &Format::of);
        let walked = found(filters, &[root.to_path_buf()]);
        let named: Vec<PathBuf> = files
            .iter()
            .filter(|(name, _)| name.ends_with(".md"))
            .map(|(name, _)| root.join(name))
            .collect();
        filters.force = true;
        assert_eq!(found(filters, &named), walked);
        walked
    }

    #[test]
    fn forced_excludes_skip_what_a_walk_skips_as_hidden() {
        let walked = forced_keeps_what_a_walk_finds(&[
            (".github/pr.md", "x"),
            (".shown/a.md", "x"),
            ("docs/.b.md", "x"),
            ("docs/c.md", "x"),
            // An ignore file that names a hidden directory lets a walk in.
            (".proserunnerignore", "!.shown/\n"),
        ]);
        assert_eq!(walked, [".shown/a.md", "docs/c.md"]);
    }

    #[test]
    fn forced_excludes_rank_ignore_files_as_a_walk_does() {
        let walked = forced_keeps_what_a_walk_finds(&[
            (".git/info/exclude", "excluded.md\n"),
            (".proserunnerignore", "*.draft.md\n"),
            // Within a kind of ignore file the nearest decides, but a
            // nearer `.gitignore` can't bring back what a
            // `.proserunnerignore` ignores.
            ("sub/.proserunnerignore", "!ok.draft.md\n"),
            ("sub/.gitignore", "!keep.draft.md\n"),
            ("sub/ok.draft.md", "x"),
            ("sub/keep.draft.md", "x"),
            ("a.md", "x"),
            ("excluded.md", "x"),
            // A worktree reads the main repository's `info/exclude`.
            ("wt/.git", "gitdir: {root}/.git/worktrees/wt\n"),
            (".git/worktrees/wt/commondir", "../..\n"),
            ("wt/excluded.md", "x"),
            ("wt/b.md", "x"),
        ]);
        assert_eq!(walked, ["a.md", "sub/ok.draft.md", "wt/b.md"]);
    }

    #[test]
    fn forced_excludes_take_a_jujutsu_repository_for_a_git_one() {
        let walked = forced_keeps_what_a_walk_finds(&[
            (".jj/repo", ""),
            (".gitignore", "skipped.md\n"),
            ("skipped.md", "x"),
            ("kept.md", "x"),
        ]);
        assert_eq!(walked, ["kept.md"]);
    }

    #[test]
    fn a_file_named_two_ways_comes_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.md"), "a").unwrap();
        let paths = [
            root.join("sub/.."),
            root.join("sub/a.md"),
            root.join("sub/../sub/a.md"),
        ];
        let walk = inputs(&paths, Filters::new(root, &Format::of)).unwrap();
        assert_eq!(walk.inputs.len(), 1, "{:?}", walk.inputs);
        assert_eq!(
            walk.named,
            HashSet::from([root.join("sub/a.md")]),
            "named files are keyed by absolute path"
        );
    }

    #[test]
    fn forced_excludes_take_files_outside_the_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cwd = root.join("cwd");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(root.join("other/b")).unwrap();
        let names = ["other/a.md", "other/a.bak.md", "other/b/c.md"];
        for name in names {
            fs::write(root.join(name), "x").unwrap();
        }
        // `/other/a.md` is anchored to the working directory, so it doesn't
        // reach outside it; the others match at any depth.
        let exclude = ["*.bak.md", "b/", "/other/a.md"].map(str::to_owned);
        let mut filters = Filters::new(&cwd, &Format::of);
        filters.exclude = &exclude;
        filters.force = true;
        let named = names.map(|n| cwd.join("..").join(n));
        let walk = inputs(&named, filters).unwrap();
        assert_eq!(
            walk.inputs,
            [Input::File {
                path: cwd.join("../other/a.md"),
                format: Format::Markdown,
            }]
        );

        let stdin_name = cwd.join("../other/a.bak.md");
        filters.stdin_name = Some(&stdin_name);
        let walk = inputs(&[PathBuf::from(STDIN)], filters).unwrap();
        assert!(walk.inputs.is_empty(), "{:?}", walk.inputs);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_directories_under_one_named_are_skipped() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let locked = root.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(root.join("a.md"), "a").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads it anyway, so there's nothing to test.
        let readable = fs::read_dir(&locked).is_ok();
        let walked = inputs(&[root.to_path_buf()], Filters::new(root, &Format::of));
        let named = inputs(slice::from_ref(&locked), Filters::new(root, &Format::of));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if readable {
            return;
        }
        let walked = walked.unwrap();
        assert_eq!(walked.inputs.len(), 1, "{:?}", walked.inputs);
        assert_eq!(walked.skipped.len(), 1, "{:?}", walked.skipped);
        assert!(walked.skipped[0].contains("locked"), "{:?}", walked.skipped);
        assert!(
            walked.skipped[0].ends_with("; skipped"),
            "{:?}",
            walked.skipped
        );
        assert!(matches!(named, Err(Error::Io { .. })), "{named:?}");
    }

    #[test]
    fn the_format_lookup_chooses_what_a_walk_finds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in ["a.mdx", "b.md", "c.rs"] {
            fs::write(root.join(name), "x").unwrap();
        }
        let none = |_: &Path| false;
        let mdx = |p: &Path| match p.extension() {
            Some(e) if e == "mdx" => Some(Format::Markdown),
            _ => Format::of(p),
        };
        let filters = Filters {
            exclude: &[],
            excluded: &none,
            force: false,
            format: &mdx,
            stdin_name: None,
            cwd: root,
        };
        let walk = inputs(&[root.to_path_buf()], filters).unwrap();
        assert_eq!(
            walk.inputs,
            [
                Input::File {
                    path: root.join("a.mdx"),
                    format: Format::Markdown,
                },
                Input::File {
                    path: root.join("b.md"),
                    format: Format::Markdown,
                }
            ]
        );
        let named = inputs(&[root.join("a.mdx")], filters).unwrap();
        assert_eq!(named.inputs.len(), 1);
    }

    #[test]
    fn forced_excludes_apply_to_the_stdin_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".proserunnerignore"), "drafts/\n").unwrap();
        let none = |_: &Path| false;
        let stdin = |name: &str, force: bool| {
            let name = root.join(name);
            let filters = Filters {
                exclude: &[],
                excluded: &none,
                force,
                format: &Format::of,
                stdin_name: Some(&name),
                cwd: root,
            };
            inputs(&[PathBuf::from(STDIN)], filters).unwrap().inputs
        };
        assert_eq!(stdin("drafts/new.md", true), []);
        assert_eq!(stdin("drafts/new.md", false), [Input::Stdin]);
        assert_eq!(stdin("docs/new.md", true), [Input::Stdin]);
    }

    #[test]
    fn display_paths() {
        let cwd = Path::new("/work/proj");
        let home = Some(Path::new("/home/me"));
        assert_eq!(display(Path::new("./docs/a.md"), cwd, home), "docs/a.md");
        assert_eq!(display(Path::new("/work/proj/a.md"), cwd, home), "a.md");
        assert_eq!(
            display(Path::new("/home/me/notes/a.md"), cwd, home),
            "~/notes/a.md"
        );
        assert_eq!(display(Path::new("/etc/a.md"), cwd, home), "/etc/a.md");
    }
}
