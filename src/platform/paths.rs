//! Where per-session files go.

use std::path::PathBuf;

/// The session's private runtime folder (`$XDG_RUNTIME_DIR`), when the system has one.
pub(crate) fn session_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

/// `session_dir`, else the temp folder: sockets and exported cookies live here.
pub(crate) fn runtime_dir() -> PathBuf {
    session_dir().unwrap_or_else(std::env::temp_dir)
}
