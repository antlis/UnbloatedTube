//! The small preview pictures YouTube makes of a video, for the seek bar.
//!
//! They come as storyboards: sheets, each a grid of frames, taken at an even interval. yt-dlp
//! lists them (`sb0`..`sb3`, from largest to smallest) in the answer the shim keeps of a lookup
//! (see `prefetch::kept_answer`), so no request is made to find them.

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// The frame size looked for: big enough to see what is on it, small enough to download in a blink.
const WIDTH: f32 = 160.;

#[derive(Clone, Debug, PartialEq)]
pub struct Storyboard {
    /// One frame's size.
    pub w: f32,
    pub h: f32,
    /// Frames per row and rows per full sheet.
    pub cols: usize,
    pub rows: usize,
    /// Each sheet's address and the seconds of video it covers.
    pub sheets: Vec<(String, f64)>,
}

/// Where a moment is: which sheet, and the frame's corner in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub sheet: usize,
    pub x: f32,
    pub y: f32,
}

/// One sheet as pixels mpv can draw (`overlay-add`, BGRA): where they are and how big the sheet is.
#[derive(Clone, Debug, Serialize)]
pub struct RawSheet {
    pub file: String,
    pub w: u32,
    pub h: u32,
}

/// Decode a downloaded sheet into raw BGRA pixels in `dest`, for mpv to draw frames of.
pub fn to_raw(image: &Path, dest: &Path) -> Result<RawSheet, String> {
    let img = image::ImageReader::open(image).and_then(|r| r.with_guessed_format()).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let mut px = img.to_rgba8();
    let (w, h) = px.dimensions();
    px.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
    std::fs::write(dest, px.as_raw()).map_err(|e| e.to_string())?;
    Ok(RawSheet { file: dest.display().to_string(), w, h })
}

/// What the controls script (`controls.lua`) needs to draw a moment's frame, as JSON: the frame
/// size, the grid, the seconds per frame, how many frames the last sheet has, and the sheets
/// converted so far (`null` for one not yet).
pub fn for_mpv(sb: &Storyboard, sheets: &[Option<RawSheet>]) -> String {
    let per_sheet = sb.cols * sb.rows;
    let interval = sb.sheets[0].1 / per_sheet as f64;
    let last_frames = ((sb.sheets[sb.sheets.len() - 1].1 / interval).round() as usize).clamp(1, per_sheet);
    let durations: Vec<f64> = sb.sheets.iter().map(|s| s.1).collect();
    serde_json::json!({
        "w": sb.w, "h": sb.h, "cols": sb.cols, "rows": sb.rows, "n": sb.sheets.len(),
        "interval": interval, "last_frames": last_frames, "durations": durations, "sheets": sheets,
    })
    .to_string()
}

/// The storyboard of the video `id`, if a lookup of it is kept and has one.
pub fn load(id: &str) -> Option<Storyboard> {
    parse(&crate::backend::prefetch::kept_answer(id)?)
}

/// The storyboard nearest `WIDTH` among a lookup's formats. One of a single sheet is left out:
/// how many frames are on it isn't said, and the moments could not be told apart.
fn parse(json: &[u8]) -> Option<Storyboard> {
    let info: Value = serde_json::from_slice(json).ok()?;
    let usable = |f: &&Value| f["format_id"].as_str().is_some_and(|id| id.starts_with("sb")) && f["fragments"].as_array().is_some_and(|s| s.len() > 1);
    let width = |f: &Value| f["width"].as_f64().unwrap_or(0.) as f32;
    let best = info["formats"].as_array()?.iter().filter(usable).min_by(|a, b| {
        let (da, db) = ((width(a) - WIDTH).abs(), (width(b) - WIDTH).abs());
        da.total_cmp(&db).then(width(b).total_cmp(&width(a)))
    })?;
    let sheets: Vec<(String, f64)> =
        best["fragments"].as_array()?.iter().filter_map(|s| Some((s["url"].as_str()?.to_string(), s["duration"].as_f64()?))).collect();
    let (cols, rows) = (best["columns"].as_u64()? as usize, best["rows"].as_u64()? as usize);
    let h = best["height"].as_f64()? as f32;
    (cols > 0 && rows > 0 && sheets.len() > 1 && sheets[0].1 > 0.).then(|| Storyboard { w: width(best), h, cols, rows, sheets })
}

impl Storyboard {
    /// The frame showing second `t`.
    pub fn frame(&self, t: f64) -> Frame {
        let per_sheet = self.cols * self.rows;
        // Every sheet but the last is full, so the first one tells the interval.
        let interval = self.sheets[0].1 / per_sheet as f64;
        let last = self.sheets.len() - 1;
        let index = (t.max(0.) / interval) as usize;
        let sheet = (index / per_sheet).min(last);
        // The last sheet has only the frames the video needed.
        let on_sheet = if sheet == last { ((self.sheets[last].1 / interval).round() as usize).clamp(1, per_sheet) } else { per_sheet };
        let cell = if index / per_sheet > last { on_sheet - 1 } else { (index % per_sheet).min(on_sheet - 1) };
        Frame { sheet, x: (cell % self.cols) as f32 * self.w, y: (cell / self.cols) as f32 * self.h }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1115 s video: 5x5 frames of 160x90 on four full sheets and a last of 13 frames.
    fn video() -> Storyboard {
        let mut sheets: Vec<(String, f64)> = (0..4).map(|i| (format!("s{i}"), 246.7)).collect();
        sheets.push(("s4".into(), 128.3));
        Storyboard { w: 160., h: 90., cols: 5, rows: 5, sheets }
    }

    #[test]
    fn a_moment_has_its_frame() {
        let sb = video();
        assert_eq!(sb.frame(0.), Frame { sheet: 0, x: 0., y: 0. });
        // 9.868 s per frame: second 50 is the sixth frame, the first of the second row.
        assert_eq!(sb.frame(50.), Frame { sheet: 0, x: 0., y: 90. });
        assert_eq!(sb.frame(250.), Frame { sheet: 1, x: 0., y: 0. });
    }

    #[test]
    fn the_end_stays_on_the_last_frame() {
        let sb = video();
        // 13 frames on the last sheet: the third row's third frame is the last.
        assert_eq!(sb.frame(1115.), Frame { sheet: 4, x: 2. * 160., y: 2. * 90. });
        assert_eq!(sb.frame(5000.), sb.frame(1115.));
        assert_eq!(sb.frame(-3.), sb.frame(0.));
    }

    #[test]
    fn the_size_nearest_the_wanted_one_is_taken() {
        let sheet = |n: usize| (0..n).map(|i| serde_json::json!({"url": format!("u{i}"), "duration": 10.0})).collect::<Vec<_>>();
        let json = serde_json::json!({"formats": [
            {"format_id": "251"},
            {"format_id": "sb3", "width": 48, "height": 27, "rows": 10, "columns": 10, "fragments": sheet(2)},
            {"format_id": "sb1", "width": 160, "height": 90, "rows": 5, "columns": 5, "fragments": sheet(3)},
            {"format_id": "sb0", "width": 320, "height": 180, "rows": 3, "columns": 3, "fragments": sheet(9)},
        ]});
        let sb = parse(json.to_string().as_bytes()).unwrap();
        assert_eq!((sb.w, sb.sheets.len()), (160., 3));
    }

    #[test]
    fn nothing_usable_is_none() {
        assert!(parse(b"not json").is_none());
        assert!(parse(br#"{"formats": [{"format_id": "251"}]}"#).is_none());
        // One sheet: the frames on it are not known.
        let one = br#"{"formats": [{"format_id": "sb1", "width": 160, "height": 90, "rows": 5, "columns": 5, "fragments": [{"url": "u", "duration": 20.0}]}]}"#;
        assert!(parse(one).is_none());
    }
}
