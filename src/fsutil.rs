//! Listing settings directories, and writing settings files safely: whole
//! or not at all, and one writer at a time.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process;

use crate::error::{Error, Result};

/// The files directly in `dir` with extension `ext`, in name order.
pub fn files_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == ext))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Writes `contents` to `path` through a temporary file in the same
/// directory, so a crash or a full disk never leaves half a file. The file
/// keeps its permissions, and a symlink stays one: the file it points to
/// is replaced, so a link into a dotfiles repository keeps working.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write as _;

    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let permissions = fs::metadata(&target).ok().map(|m| m.permissions());
    let dir = target.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", process::id()));

    let written = File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(contents.as_bytes())?;
            if let Some(p) = permissions {
                f.set_permissions(p)?;
            }
            f.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, &target));
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }
    Ok(())
}

/// An exclusive lock from [`lock`], released when it's dropped.
#[derive(Debug)]
#[must_use = "dropping it releases the lock"]
pub struct Lock {
    _file: File,
}

/// Waits for an exclusive lock on the file `path`, so two runs (an editor
/// and a terminal, say) can't interleave a read-modify-write and lose
/// one's changes. The lock file is created if needed and left in place:
/// deleting it would let a waiting process lock a file nobody else can
/// see.
pub fn lock(path: &Path) -> Result<Lock> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| Error::io(path, e))?;
    file.lock().map_err(|e| Error::io(path, e))?;
    Ok(Lock { _file: file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_writes_replace_and_leave_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/a.toml");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(dir.path().join("sub")).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_writes_keep_symlinks_and_permissions() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles/ignores.toml");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "one").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("ignores.toml");
        symlink(&real, &link).unwrap();

        write_atomic(&link, "two").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "two");
        let mode = fs::metadata(&real).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn locks_are_released_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".lock");
        drop(lock(&path).unwrap());
        let _again = lock(&path).unwrap();
    }
}
