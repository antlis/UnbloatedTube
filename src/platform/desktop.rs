//! The desktop around the app: opening links, the user's default browser.

use std::process::{Command, Stdio};

/// Open `url` in the default browser. The opener is reaped in the background, so it doesn't
/// linger as a zombie.
pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    let mut child = Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    drop(std::thread::spawn(move || child.wait()));
    Ok(())
}

/// Open `url` with the first of `programs` that starts (a browser's executable, which opens it in
/// that browser, in its running window if it has one), else in the default browser.
pub(crate) fn open_url_in(programs: &[&str], url: &str) -> std::io::Result<()> {
    for program in programs {
        if let Ok(mut child) = Command::new(program).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            drop(std::thread::spawn(move || child.wait()));
            return Ok(());
        }
    }
    open_url(url)
}

/// The default browser's desktop file id ("firefox.desktop"): `$XDG_DEFAULT_BROWSER`, then
/// `xdg-settings`.
pub(crate) fn default_browser_id() -> Option<String> {
    std::env::var("XDG_DEFAULT_BROWSER").ok().filter(|s| !s.is_empty()).or_else(|| {
        let out = Command::new("xdg-settings").args(["get", "default-web-browser"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
    })
}
