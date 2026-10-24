//! The harness for black-box tests: argv in; exit status, stdout and
//! stderr out. Each test gets its own directory, home, and config and
//! cache directories, so none touch the real ones. The directory is a git
//! repository root, so project discovery stops there.

#![allow(dead_code, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use assert_cmd::Command;
use assert_cmd::cargo;
use tempfile::TempDir;

pub(crate) struct Env {
    dir: TempDir,
}

impl Env {
    pub(crate) fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        Env { dir }
    }

    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    pub(crate) fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    pub(crate) fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    pub(crate) fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    /// The user config directory, relative to the root.
    pub(crate) const CONFIG: &'static str = "home/.config/proserunner";

    pub(crate) fn cmd(&self) -> Command {
        self.cmd_in("")
    }

    /// A command run from `rel`, a directory under the root.
    pub(crate) fn cmd_in(&self, rel: &str) -> Command {
        let cwd = self.path(rel);
        fs::create_dir_all(&cwd).unwrap();
        let mut cmd = Command::cargo_bin("proserunner").unwrap();
        cmd.current_dir(cwd)
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("home/.config"))
            .env("XDG_CACHE_HOME", self.path("home/.cache"));
        cmd
    }
}

impl Env {
    /// A plain process command, for tests that manage the child's pipes.
    pub(crate) fn process(&self) -> process::Command {
        let mut cmd = process::Command::new(cargo::cargo_bin("proserunner"));
        cmd.current_dir(self.root())
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("home/.config"))
            .env("XDG_CACHE_HOME", self.path("home/.cache"));
        cmd
    }
}

pub(crate) struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn run(cmd: &mut Command) -> Run {
    let out = cmd.output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

/// Every file under `dir`, relative and sorted, for checking that a run
/// wrote nothing.
pub(crate) fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(dir).unwrap().display().to_string());
            }
        }
    }
    out.sort();
    out
}

pub(crate) const CLEAN: &str = "A short, plain sentence.\n";
pub(crate) const DIRTY: &str =
    "We utilize synergy.\n\nIt failed due to the fact\nthat it rained, per your request.\n";
