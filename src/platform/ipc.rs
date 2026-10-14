//! Local sockets between this program's own processes: mpv's IPC, the command line's single
//! instance, the yt-dlp helper.

use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;

pub(crate) use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

/// Remove the `<prefix><pid>.sock` files of `dir` whose process is gone: instances that were
/// killed never cleaned up after themselves.
pub(crate) fn remove_stale_sockets(dir: &Path, prefix: &str) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let pid = name.strip_prefix(prefix).and_then(|n| n.strip_suffix(".sock"));
        if pid.is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Send `payload` and the file descriptors over a unix socket in one message.
pub(crate) fn send_with_fds(sock: &Stream, payload: &[u8], fds: &[RawFd]) -> std::io::Result<()> {
    let len = std::mem::size_of_val(fds) as u32;
    // SAFETY: the buffers outlive the call, the control buffer is aligned and sized by CMSG_SPACE.
    unsafe {
        let mut iov = libc::iovec { iov_base: payload.as_ptr() as *mut _, iov_len: payload.len() };
        let space = libc::CMSG_SPACE(len) as usize;
        let mut control = vec![0u64; space.div_ceil(8)];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let header = libc::CMSG_FIRSTHDR(&msg);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(len) as _;
        std::ptr::copy_nonoverlapping(fds.as_ptr(), libc::CMSG_DATA(header).cast::<RawFd>(), fds.len());
        if libc::sendmsg(sock.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
