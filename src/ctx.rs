//! Where a run happens: the working directory, the home and config
//! directories, and the project around the working directory. `main` reads
//! these from the environment once; everything else takes them as an
//! argument, so nothing depends on global state.

use std::env;
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};
use crate::walk;

/// The user config directory's name under `$XDG_CONFIG_HOME`.
pub const APP: &str = "proserunner";

/// A project's settings directory. The directory that holds it is the
/// project root.
pub const PROJECT_DIR: &str = ".proserunner";

/// Where a run happens, and how much it prints.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Ctx {
    /// Absolute.
    pub cwd: PathBuf,
    /// `$HOME`, unless it's unset or empty.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME/proserunner`, else `~/.config/proserunner`; `None`
    /// when neither variable is set.
    pub config_dir: Option<PathBuf>,
    /// Print only results and errors.
    pub quiet: bool,
}

impl Ctx {
    /// Reads the working directory, `HOME` and `XDG_CONFIG_HOME`.
    pub fn from_env(quiet: bool) -> Result<Ctx> {
        let cwd = env::current_dir().map_err(|e| Error::io(".", e))?;
        let home = env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        // The XDG spec says to ignore a relative value.
        let xdg = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        Ok(Ctx::new(cwd, home, xdg, quiet))
    }

    /// A context that reads nothing from the environment. `cwd` must be
    /// absolute, and so must `xdg_config_home` (`$XDG_CONFIG_HOME`), if
    /// given.
    pub fn new(
        cwd: PathBuf,
        home: Option<PathBuf>,
        xdg_config_home: Option<PathBuf>,
        quiet: bool,
    ) -> Ctx {
        let config_dir = xdg_config_home
            .or_else(|| home.as_ref().map(|h| h.join(".config")))
            .map(|d| d.join(APP));
        Ctx {
            cwd,
            home,
            config_dir,
            quiet,
        }
    }

    /// `path` against the working directory, with `.` and `..` resolved.
    pub fn absolute(&self, path: &Path) -> PathBuf {
        absolute(&self.cwd, path)
    }

    /// A path written in a settings file: `~/` is the home directory, and
    /// anything else relative is relative to `base`.
    pub fn resolve(&self, base: &Path, written: &str) -> PathBuf {
        match (written.strip_prefix("~/"), &self.home) {
            (Some(rest), Some(home)) => absolute(home, Path::new(rest)),
            _ => absolute(base, Path::new(written)),
        }
    }

    /// How a path is shown: relative to the working directory when under
    /// it, `~/…` under the home directory, otherwise absolute. Files in the
    /// global config directory always show as `~/…`, the way people know
    /// them.
    pub fn display(&self, path: &Path) -> String {
        if path.is_absolute()
            && self
                .config_dir
                .as_deref()
                .is_some_and(|d| path.starts_with(d))
        {
            return self.portable(path);
        }
        walk::display(path, &self.cwd, self.home.as_deref())
    }

    /// A path to write into a settings file that has no root of its own:
    /// `~/…` under the home directory, otherwise absolute.
    pub fn portable(&self, path: &Path) -> String {
        match self.home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
            Some(rel) => format!("~/{}", slashed(rel)),
            None => path.display().to_string(),
        }
    }

    /// The root of the project around the working directory.
    pub fn project(&self) -> Option<PathBuf> {
        find_project(&self.cwd, self.home.as_deref())
    }

    /// The project the checked paths are in: the one around them, or, when
    /// none is in a project, the one around the working directory. Paths in
    /// two projects are an error, since a run has one config.
    pub fn project_for<'p>(
        &self,
        paths: impl IntoIterator<Item = &'p Path>,
    ) -> Result<Option<PathBuf>> {
        let mut found: Option<(&Path, PathBuf)> = None;
        let files = paths.into_iter().filter(|p| p.as_os_str() != walk::STDIN);
        for path in files {
            let absolute = self.absolute(path);
            let start = if absolute.is_dir() {
                absolute.as_path()
            } else {
                absolute.parent().unwrap_or(&absolute)
            };
            let Some(root) = find_project(start, self.home.as_deref()) else {
                continue;
            };
            match &found {
                Some((first, other)) if *other != root => {
                    return Err(Error::Usage(format!(
                        "{} is in the project at {}, and {} in the one at {}; check each project on its own",
                        first.display(),
                        self.display(other),
                        path.display(),
                        self.display(&root)
                    )));
                }
                Some(_) => {}
                None => found = Some((path, root)),
            }
        }
        Ok(found.map(|(_, root)| root).or_else(|| self.project()))
    }
}

/// Which ignore list: the global one, or a project's. It says where
/// settings that proserunner writes go.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ListScope {
    /// The user config directory.
    Global,
    /// A project, by its root.
    Project(PathBuf),
}

impl ListScope {
    /// The directory that holds this scope's files.
    pub fn dir(&self, ctx: &Ctx) -> Result<PathBuf> {
        match self {
            ListScope::Global => ctx.config_dir.clone().ok_or_else(|| {
                Error::Usage("no global config directory: set HOME or XDG_CONFIG_HOME".into())
            }),
            ListScope::Project(root) => Ok(root.join(PROJECT_DIR)),
        }
    }

    /// What messages call it.
    pub fn name(&self) -> &'static str {
        match self {
            ListScope::Global => "global",
            ListScope::Project(_) => "project",
        }
    }
}

/// The nearest directory at or above `start` that holds `.proserunner/`.
/// The search stops at a directory with `.git` (a repository is a
/// boundary) and at the home directory, which is never a project: 0.7 kept
/// its global config in `~/.proserunner`.
pub fn find_project(start: &Path, home: Option<&Path>) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if Some(dir) == home {
            return None;
        }
        if dir.join(PROJECT_DIR).is_dir() {
            return Some(dir.to_path_buf());
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

/// `path` against `base`, with `.` and `..` resolved without touching the
/// file system, so it works for paths that don't exist.
pub fn absolute(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// The part of absolute `path` below `root`, or, for a path outside
/// `root`, below the deepest directory they share: what a walk from there
/// goes through to reach `path`, and none of the directories it starts
/// in.
pub fn below<'p>(path: &'p Path, root: &Path) -> &'p Path {
    path.ancestors()
        .find(|a| root.starts_with(a))
        .and_then(|a| path.strip_prefix(a).ok())
        .unwrap_or(path)
}

/// A relative path with `/` between components on every platform, as
/// settings files store it.
pub fn slashed(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn ctx() -> Ctx {
        Ctx::new("/work/proj".into(), Some("/home/me".into()), None, false)
    }

    #[test]
    fn config_dir_follows_xdg() {
        assert_eq!(
            ctx().config_dir,
            Some(PathBuf::from("/home/me/.config/proserunner"))
        );
        let c = Ctx::new("/w".into(), Some("/h".into()), Some("/x".into()), false);
        assert_eq!(c.config_dir, Some(PathBuf::from("/x/proserunner")));
        let c = Ctx::new("/w".into(), None, None, false);
        assert_eq!(c.config_dir, None);
    }

    #[test]
    fn paths_resolve_lexically() {
        let c = ctx();
        assert_eq!(
            c.absolute(Path::new("./a/../b.md")),
            Path::new("/work/proj/b.md")
        );
        assert_eq!(
            c.resolve(Path::new("/base"), "~/r"),
            Path::new("/home/me/r")
        );
        assert_eq!(c.resolve(Path::new("/base"), "../r"), Path::new("/r"));
        assert_eq!(c.resolve(Path::new("/base"), "/abs"), Path::new("/abs"));
        assert_eq!(c.portable(Path::new("/home/me/n/a.md")), "~/n/a.md");
        assert_eq!(c.portable(Path::new("/etc/a.md")), "/etc/a.md");
    }

    #[test]
    fn projects_are_found_upward_but_not_past_git_or_home() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(root.join(".proserunner")).unwrap();
        fs::create_dir_all(root.join("docs/deep")).unwrap();
        assert_eq!(
            find_project(&root.join("docs/deep"), None),
            Some(root.clone())
        );

        let repo = root.join("vendor/lib");
        fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(find_project(&repo, None), None);

        let home = dir.path().join("home");
        fs::create_dir_all(home.join(".proserunner")).unwrap();
        fs::create_dir_all(home.join("notes")).unwrap();
        assert_eq!(find_project(&home.join("notes"), Some(&home)), None);
    }

    #[test]
    fn the_project_is_the_one_around_the_checked_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for root in [&a, &b] {
            fs::create_dir_all(root.join(".proserunner")).unwrap();
            fs::create_dir_all(root.join("docs")).unwrap();
            fs::write(root.join("docs/x.md"), "x").unwrap();
        }
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let c = Ctx::new(elsewhere, None, None, false);
        let found = |paths: &[&str]| c.project_for(paths.iter().map(Path::new));

        assert_eq!(found(&["../a/docs/x.md"]).unwrap(), Some(a.clone()));
        assert_eq!(found(&["../a/docs", "-"]).unwrap(), Some(a.clone()));
        assert_eq!(found(&["-"]).unwrap(), None);
        assert!(matches!(
            found(&["../a/docs", "../b/docs/x.md"]),
            Err(Error::Usage(_))
        ));

        let inside = Ctx::new(a.join("docs"), None, None, false);
        let paths = [Path::new("../../b/docs/x.md")];
        assert_eq!(inside.project_for(paths).unwrap(), Some(b));
        assert_eq!(inside.project_for([]).unwrap(), Some(a));
    }

    #[test]
    fn below_starts_where_a_walk_would() {
        let root = Path::new("/x/drafts/novel");
        let below = |p: &str| below(Path::new(p), root).to_owned();
        assert_eq!(below("/x/drafts/novel/ch/a.md"), Path::new("ch/a.md"));
        assert_eq!(below("/x/drafts/other/a.md"), Path::new("other/a.md"));
        assert_eq!(below("/y/a.md"), Path::new("y/a.md"));
        assert_eq!(below("/x/drafts/novel"), Path::new(""));
    }
}
