//! The AppImage's yt-dlp is a fallback, and YouTube breaks old yt-dlp within weeks, but the
//! AppImage itself is read-only. So when it is the only yt-dlp, a copy goes to the data folder
//! (`AppRun` puts that folder on `PATH` after yours and before the bundled one) and updates itself
//! there with `yt-dlp -U`, which checks what it downloads.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::backend::store;

const EVERY: Duration = Duration::from_secs(24 * 3600);

/// Whether `path` (a `PATH` value) holds a yt-dlp outside `ours`: the user's own, which they keep up to date.
fn user_has_ytdlp(path: &OsStr, ours: &[&Path]) -> bool {
    std::env::split_paths(path).any(|d| !ours.contains(&d.as_path()) && d.join("yt-dlp").is_file())
}

/// One round: make the copy if there is none, update it unless that was done within `EVERY`.
fn update_once(bundled: &Path, dir: &Path) -> std::io::Result<()> {
    let (copy, stamp) = (dir.join("yt-dlp"), dir.join("yt-dlp.checked"));
    if !copy.is_file() {
        std::fs::create_dir_all(dir)?;
        // Renamed into place, so a yt-dlp that is half copied is never on PATH.
        let part = dir.join("yt-dlp.new");
        std::fs::copy(bundled, &part)?;
        std::fs::rename(part, &copy)?;
    }
    let fresh = std::fs::metadata(&stamp).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < EVERY);
    if !fresh {
        // Stamped first: offline, it tries again tomorrow, not on every start.
        std::fs::write(&stamp, "")?;
        Command::new(&copy).arg("-U").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    }
    Ok(())
}

/// In the AppImage (`AppRun` names its yt-dlp in `UNBLOATED_BUNDLED_YTDLP`) with no yt-dlp of
/// yours: keep the copy in the data folder current, for the life of the app. Otherwise does nothing.
pub fn keep_fresh() {
    let Some(bundled) = std::env::var_os("UNBLOATED_BUNDLED_YTDLP").map(PathBuf::from) else { return };
    let dir = store::data_dir().join("bin");
    let Some(helpers) = bundled.parent() else { return };
    if user_has_ytdlp(&std::env::var_os("PATH").unwrap_or_default(), &[&dir, helpers]) {
        return;
    }
    loop {
        let _ = update_once(&bundled, &dir);
        std::thread::sleep(EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_yt_dlp_outside_ours_counts() {
        let root = std::env::temp_dir().join(format!("ytdlp-update-{}", std::process::id()));
        let (mine, bundled, users) = (root.join("mine"), root.join("bundled"), root.join("users"));
        for d in [&mine, &bundled, &users] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(mine.join("yt-dlp"), "").unwrap();
        std::fs::write(bundled.join("yt-dlp"), "").unwrap();
        let ours = [mine.as_path(), bundled.as_path()];
        let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
        assert!(!user_has_ytdlp(&path(&[&users, &mine, &bundled]), &ours));
        std::fs::write(users.join("yt-dlp"), "").unwrap();
        assert!(user_has_ytdlp(&path(&[&users, &mine, &bundled]), &ours));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copies_once_and_updates_once_a_day() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("ytdlp-copy-{}", std::process::id()));
        let (bundled, dir) = (root.join("yt-dlp"), root.join("bin"));
        std::fs::create_dir_all(&root).unwrap();
        // Stands in for yt-dlp: every call writes a line to `calls`.
        std::fs::write(&bundled, format!("#!/bin/sh\necho \"$1\" >> {}/calls\n", root.display())).unwrap();
        std::fs::set_permissions(&bundled, std::fs::Permissions::from_mode(0o755)).unwrap();
        update_once(&bundled, &dir).unwrap();
        update_once(&bundled, &dir).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("calls")).unwrap(), "-U\n");
        assert!(dir.join("yt-dlp").is_file() && !dir.join("yt-dlp.new").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
