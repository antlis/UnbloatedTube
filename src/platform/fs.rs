//! Files with an owner and a lock.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

/// `OpenOptions` for a file only the user may read (mode 0600), set when it is created so it is
/// never readable by others, not even briefly.
pub(crate) fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(0o600);
    options
}

/// Try to take the exclusive lock of `file` without waiting; whether it was taken. The lock goes
/// when the file is closed.
pub(crate) fn try_lock_exclusive(file: &File) -> bool {
    // SAFETY: flock on a descriptor the file owns.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ubt-fs-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_private_file_is_only_the_users() {
        let path = temp("private");
        let _ = std::fs::remove_file(&path);
        private_file().write(true).create_new(true).open(&path).unwrap().write_all(b"x").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        // create_new refuses to reuse a file that is there
        assert!(private_file().write(true).create_new(true).open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_lock_is_held_until_its_file_is_closed() {
        let path = temp("lock");
        let first = OpenOptions::new().create(true).truncate(false).write(true).open(&path).unwrap();
        let second = OpenOptions::new().write(true).open(&path).unwrap();
        assert!(try_lock_exclusive(&first));
        assert!(!try_lock_exclusive(&second));
        drop(first);
        assert!(try_lock_exclusive(&second));
        let _ = std::fs::remove_file(&path);
    }
}
