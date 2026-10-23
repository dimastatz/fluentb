//! Making sure only one fluentb runs at a time, with an OS file lock that is released
//! automatically when the process exits, even if it crashes.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

const LOCK_FILE: &str = "fluentb.lock";

/// The outcome of trying to become the running instance.
#[derive(Debug)]
pub enum Instance {
    /// This process holds the lock; keep the file open until exit.
    First(File),
    /// Another fluentb holds the lock.
    AlreadyRunning,
}

/// The lock file, kept next to the config file.
#[must_use]
pub fn lock_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(LOCK_FILE)
}

/// Takes the single-instance lock at `path`, creating the file if needed.
pub fn acquire(path: &Path) -> io::Result<Instance> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Instance::First(file)),
        Err(TryLockError::WouldBlock) => Ok(Instance::AlreadyRunning),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_sits_next_to_the_config() {
        assert_eq!(
            lock_path(Path::new("/home/me/.config/fluentb/config.toml")),
            Path::new("/home/me/.config/fluentb/fluentb.lock")
        );
    }

    #[test]
    fn a_second_instance_is_refused_until_the_first_exits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(LOCK_FILE);

        let first = acquire(&path).unwrap();
        assert!(matches!(first, Instance::First(_)));
        assert!(matches!(acquire(&path).unwrap(), Instance::AlreadyRunning));

        drop(first);
        assert!(matches!(acquire(&path).unwrap(), Instance::First(_)));
    }

    #[test]
    fn reports_a_lock_file_that_cannot_be_created() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        fs::write(&blocker, "").unwrap();

        assert!(acquire(&blocker.join(LOCK_FILE)).is_err());
    }
}
