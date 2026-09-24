//! A running record of what the app did in the background, in a file anyone
//! can open.
//!
//! Release builds have no console, so before this every `eprintln!` went
//! nowhere and "it did nothing" had no evidence either way. Now each line also
//! lands, timestamped, in `<app data>/logs/agentarcade.log`: hooks received and
//! from which agent, windows shown and hidden, hook installs, errors.
//!
//! What it never records: prompts, file contents, or anything else from a hook
//! payload beyond the event and the session id.
//!
//! The file is capped: past `MAX_BYTES` it is renamed to `agentarcade.old.log`
//! (replacing the previous one) and a fresh file starts, so it can never grow
//! without bound.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;

static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Start writing to `<dir>/agentarcade.log`. Lines logged before this only
/// reach stderr.
pub fn init(dir: &Path) {
    let path = dir.join("agentarcade.log");
    if fs::create_dir_all(dir).is_ok() {
        *PATH.lock().unwrap() = Some(path);
    }
}

pub fn path() -> Option<PathBuf> {
    PATH.lock().unwrap().clone()
}

/// Write one line. Never fails loudly: a log that cannot be written must not
/// take the app down with it.
pub fn write(message: &str) {
    eprintln!("[agentarcade] {message}");
    // Held for the whole append so two threads cannot interleave a rotation.
    let guard = PATH.lock().unwrap();
    let Some(path) = guard.as_ref() else {
        return;
    };
    rotate_if_full(path);
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{stamp}  {message}");
    }
}

fn rotate_if_full(path: &Path) {
    if fs::metadata(path).is_ok_and(|m| m.len() >= MAX_BYTES) {
        let _ = fs::rename(path, path.with_file_name("agentarcade.old.log"));
    }
}

/// `alog!("...")` — format and log a line, like `eprintln!`.
macro_rules! alog {
    ($($arg:tt)*) => {
        $crate::applog::write(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_log_rotates_instead_of_growing() {
        let dir = std::env::temp_dir().join("agentarcade-applog-rotate");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agentarcade.log");
        fs::write(&path, vec![b'x'; MAX_BYTES as usize]).unwrap();

        rotate_if_full(&path);

        assert!(!path.exists(), "the full file moved aside");
        assert!(dir.join("agentarcade.old.log").exists());
    }

    #[test]
    fn a_small_log_is_left_alone() {
        let dir = std::env::temp_dir().join("agentarcade-applog-small");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agentarcade.log");
        fs::write(&path, "one line\n").unwrap();
        rotate_if_full(&path);
        assert!(path.exists());
    }
}
