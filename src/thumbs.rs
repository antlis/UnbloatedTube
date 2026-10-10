//! On-disk thumbnail cache (~/.cache/unbloatedtube/thumbs/<id>.jpg).

use std::io::Read;
use std::path::PathBuf;

pub fn path(id: &str) -> PathBuf {
    crate::store::cache_dir().join("thumbs").join(format!("{id}.jpg"))
}

pub fn download(url: &str, dest: &PathBuf) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader().take(2 << 20).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    // Write via a temp file so a half-written jpg is never picked up by the UI.
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dest).map_err(|e| e.to_string())
}
