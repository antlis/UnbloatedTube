//! The operating system's side of the app: the video surface mpv draws into, media keys, local
//! sockets, process control, files only the user can read, opening links, the default browser.
//!
//! All of it is the Linux/Unix implementation today. Nothing outside this module (and the yt-dlp
//! shim in `backend/prefetch.rs`, which is Unix by design) touches X11, D-Bus, `/proc`, Unix sockets
//! or `libc`, so a port starts by giving these modules a version for the other system: the
//! compiler then lists what is still missing.

pub(crate) mod desktop;
pub(crate) mod embed;
pub(crate) mod fs;
pub(crate) mod ipc;
pub(crate) mod mpris;
pub(crate) mod paths;
pub(crate) mod process;
