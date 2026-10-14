//! Child processes and this process's own identity.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Make the child of `cmd` end when this process does, also when it is killed or crashes, so no
/// mpv or helper is left running.
pub(crate) fn die_with_parent(cmd: &mut Command) {
    // SAFETY: prctl is async-signal-safe, as pre_exec requires.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

/// Put the child of `cmd` in a process group of its own, so what happens to this process's group
/// (a Ctrl-C in the terminal, say) doesn't reach it.
pub(crate) fn own_group(cmd: &mut Command) {
    cmd.process_group(0);
}

/// This program, to run again. On Linux through `/proc/<pid>/exe`, which reaches the running
/// binary even after a rebuild replaced the file (`current_exe()` then names a path that is gone,
/// and every run would fail until a restart). Other programs can use it too: it is this process's
/// pid, not theirs.
pub(crate) fn own_exe() -> Option<PathBuf> {
    let proc = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
    if proc.exists() { Some(proc) } else { std::env::current_exe().ok() }
}

/// A path that writes to this process's standard error, for programs it starts that should log there too.
pub(crate) fn own_stderr_path() -> String {
    format!("/proc/{}/fd/2", std::process::id())
}
