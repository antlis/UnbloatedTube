//! On-disk thumbnail cache (~/.cache/unbloatedtube/thumbs/<id>.jpg).

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};

/// Downloads at once: enough to fill a screen quickly, few enough that a long list doesn't
/// queue dozens of requests ahead of the rows you are looking at.
const PARALLEL: usize = 8;
static RUNNING: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

pub fn path(id: &str) -> PathBuf {
    crate::store::cache_dir().join("thumbs").join(format!("{id}.jpg"))
}

pub fn download(url: &str, dest: &PathBuf) -> Result<(), String> {
    let mut running = FREED.wait_while(RUNNING.lock().unwrap(), |n| *n >= PARALLEL).unwrap();
    *running += 1;
    drop(running);
    let res = fetch(url, dest);
    *RUNNING.lock().unwrap() -= 1;
    FREED.notify_one();
    res
}

fn fetch(url: &str, dest: &PathBuf) -> Result<(), String> {
    let resp = crate::http::agent().get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader().take(2 << 20).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    // Write via a temp file so a half-written jpg is never picked up by the UI.
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dest).map_err(|e| e.to_string())
}
