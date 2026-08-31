//! Write a file without ever leaving a half-written one behind.
//!
//! Used for every file this app owns (`stats.json`, `prefs.json`) and — more
//! importantly — for the user's own `~/.claude/settings.json`, which we edit on
//! their behalf when hooks are toggled. A crash or power loss partway through a
//! plain `fs::write` truncates the destination; that is survivable for our own
//! stats and is emphatically not survivable for their editor config.

use std::fs;
use std::path::{Path, PathBuf};

/// Write `bytes` to `path` via a temp file + rename.
///
/// `fs::rename` replaces the destination atomically on Windows, macOS and
/// Linux, so a reader either sees the whole old file or the whole new one.
/// The temp file sits beside the destination so the rename stays on one volume.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp: PathBuf = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            // Do not strand the temp file next to the user's config if the
            // rename failed (permissions, a lock, a vanished parent).
            let _ = fs::remove_file(&tmp);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentarcade-atomic-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_a_new_file_and_creates_parents() {
        let path = temp_dir("new").join("nested").join("x.json");
        write(&path, b"{}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn replaces_an_existing_file() {
        let path = temp_dir("replace").join("x.json");
        write(&path, b"old").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn leaves_no_temp_file_behind() {
        let path = temp_dir("clean").join("x.json");
        write(&path, b"hello").unwrap();
        let tmp = path.with_extension("tmp");
        assert!(!tmp.exists(), "temp file survived a successful write");
    }
}
