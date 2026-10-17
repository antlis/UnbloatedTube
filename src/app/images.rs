use super::*;

/// How many of a video's storyboard sheets are downloaded when it starts (see `storyboard`).
const MAX_SHEETS: usize = 60;

impl Unbloated {
    /// DeArrow's title and thumbnail for a video, when either is turned on: asked once, the first
    /// time a row or the player shows the video (like thumbnails, only for what is on screen).
    pub(crate) fn dearrow(&mut self, id: &str, cx: &mut Context<Self>) -> Option<dearrow::Branding> {
        if !self.settings.dearrow_titles && !self.settings.dearrow_thumbs {
            return None;
        }
        if let Some(known) = self.dearrow.get(id) {
            return known.clone();
        }
        self.dearrow.insert(id.to_string(), None);
        let id = id.to_string();
        let task = {
            let id = id.clone();
            blocking::unblock(move || dearrow::branding(&id))
        };
        let start = Instant::now();
        cx.spawn(async move |this, cx| {
            // Quietly nothing on a failure: YouTube's own title and thumbnail stay.
            let res = task.await;
            yt::timing(&format!("dearrow {id}: {}", match &res { Ok(b) => format!("{b:?}"), Err(e) => format!("failed ({e})") }), start);
            if let Ok(b) = res {
                this.update(cx, |this, cx| {
                    let changed = b != dearrow::Branding::default();
                    this.dearrow.insert(id, Some(b));
                    if changed {
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
        None
    }

    /// The title to show for `video`, and YouTube's own when DeArrow's replaces it.
    pub(crate) fn shown_title(&mut self, video: &Video, cx: &mut Context<Self>) -> (String, Option<String>) {
        let better = self.dearrow(&video.id, cx).and_then(|b| b.title).filter(|_| self.settings.dearrow_titles);
        match better {
            Some(t) if t != video.title => (t, Some(video.title.clone())),
            _ => (video.title.clone(), None),
        }
    }

    /// The thumbnail to show for `video`: DeArrow's frame when there is one and it loaded, else
    /// YouTube's (also while DeArrow's is still on its way).
    pub(crate) fn video_thumb(&mut self, video: &Video, w: f32, h: f32, radius: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let frame = self.dearrow(&video.id, cx).and_then(|b| b.thumb).filter(|_| self.settings.dearrow_thumbs);
        if let Some(time) = frame {
            let key = format!("{}-dearrow-{}", video.id, (time * 1000.) as u64);
            if !self.thumbs_failed.contains(&key) {
                let url = dearrow::thumb_url(&video.id, time);
                if let Thumb::Ready(_) = self.thumb(&key, Some(url), cx) {
                    return self.thumb_el(&key, None, w, h, radius, cx);
                }
            }
        }
        self.thumb_el(&video.id, Some(video.thumb_url()), w, h, radius, cx)
    }

    /// Cached thumbnail, kicking off a download the first time a key is seen.
    pub(crate) fn thumb(&mut self, key: &str, url: Option<String>, cx: &mut Context<Self>) -> Thumb {
        let path = thumbs::path(key);
        if self.thumbs_ready.contains(key) || path.exists() {
            self.thumbs_ready.insert(key.to_string());
            return Thumb::Ready(path);
        }
        if url.is_none() || self.thumbs_failed.contains(key) {
            return Thumb::Missing;
        }
        if let Some(url) = url.filter(|_| self.thumbs_requested.insert(key.to_string())) {
            let dest = path.clone();
            let key = key.to_string();
            let task = blocking::unblock(move || { thumbs::download(&url, &dest) });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let ok = result.is_ok();
                if let (Err(e), true) = (&result, key.starts_with("sb-")) {
                    yt::timing(&format!("storyboard sheet {key} failed ({e})"), Instant::now());
                }
                this.update(cx, |this, cx| {
                    if !ok {
                        this.thumbs_failed.insert(key);
                    }
                    cx.notify()
                })
                .ok();
            })
            .detach();
        }
        Thumb::Pending
    }

    /// The playing video's storyboard, looked up once per video (again after a few seconds while
    /// there is none, because the lookup that has it may still be running).
    pub(crate) fn storyboard(&mut self, cx: &mut Context<Self>) -> Option<Storyboard> {
        let id = self.current.as_ref()?.id.clone();
        let stale = match &self.storyboard {
            Some((v, sb, at)) => *v != id || (sb.is_none() && at.elapsed() > Duration::from_secs(3)),
            None => true,
        };
        if stale {
            let sb = storyboard::load(&id);
            yt::timing(&format!("storyboard {id}: {}", sb.as_ref().map_or("none".to_string(), |s| format!("{} sheets of {}x{}", s.sheets.len(), s.w, s.h))), Instant::now());
            // All the sheets now, a few KB each (a tooltip already showing doesn't pick up a sheet
            // that arrives later): the first move over the bar has its pictures. A very long video
            // gets the first ones, the rest when the pointer gets there (`want_sheet`).
            for (i, (url, _)) in sb.iter().flat_map(|sb| sb.sheets.iter()).enumerate().take(MAX_SHEETS) {
                self.thumb(&format!("sb-{id}-{i}"), Some(url.clone()), cx);
            }
            // mpv's own bar (the video controls) draws pictures from pixels, not from the files.
            self.player.set_storyboard("");
            self.raw_sheets = sb.clone().map(|sb| {
                let done = Arc::new(Mutex::new(vec![None; sb.sheets.len()]));
                let (id, results) = (id.clone(), done.clone());
                blocking::unblock(move || {
                    let dir = store::cache_dir().join("sb-raw");
                    let _ = std::fs::remove_dir_all(&dir);
                    let _ = std::fs::create_dir_all(&dir);
                    for i in 0..sb.sheets.len().min(MAX_SHEETS) {
                        let source = thumbs::path(&format!("sb-{id}-{i}"));
                        // Waits for the download (it may be on its way, or not start for a while).
                        let start = Instant::now();
                        while !source.exists() && start.elapsed() < Duration::from_secs(90) {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                        if let Ok(raw) = storyboard::to_raw(&source, &dir.join(format!("{i}.bgra"))) {
                            results.lock().unwrap()[i] = Some(raw);
                        }
                    }
                })
                .detach();
                (done, 0)
            });
            self.storyboard = Some((id.clone(), sb, Instant::now()));
        }
        self.storyboard.as_ref().and_then(|(_, sb, _)| sb.clone())
    }

    /// Tell mpv's controls about the sheets converted since last time.
    pub(crate) fn sync_raw_sheets(&mut self) {
        let (Some((results, sent)), Some((_, Some(sb), _))) = (&mut self.raw_sheets, &self.storyboard) else { return };
        let sheets = results.lock().unwrap().clone();
        let ready = sheets.iter().flatten().count();
        if ready != *sent {
            *sent = ready;
            self.player.set_storyboard(&storyboard::for_mpv(sb, &sheets));
        }
    }

    /// Where the sheet of the playing video's storyboard is on disk, once it has been downloaded.
    pub(crate) fn sheet_path(&self, sheet: usize) -> Option<std::path::PathBuf> {
        let (id, ..) = self.storyboard.as_ref()?;
        Some(thumbs::path(&format!("sb-{id}-{sheet}"))).filter(|p| p.exists())
    }

    /// Start downloading a sheet of the playing video's storyboard (when the pointer is over a moment
    /// on it): the bar shows its pictures from then on.
    pub(crate) fn want_sheet(&mut self, sheet: usize, cx: &mut Context<Self>) {
        let Some((id, Some(sb), _)) = &self.storyboard else { return };
        let (key, url) = (format!("sb-{id}-{sheet}"), sb.sheets.get(sheet).map(|s| s.0.clone()));
        self.thumb(&key, url, cx);
    }

    /// A thumbnail box: the image, a pulsing skeleton while it downloads, or a plain box.
    pub(crate) fn thumb_el(&mut self, key: &str, url: Option<String>, w: f32, h: f32, radius: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let frame = div().w(px(w)).h(px(h)).flex_none().overflow_hidden().rounded(radius).bg(themed(HOVER));
        match self.thumb(key, url, cx) {
            // Round the image itself: the frame's overflow clip is rectangular, so a rounded
            // frame alone would leave square corners (e.g. on round avatars).
            Thumb::Ready(path) => frame.child(img(path).size_full().rounded(radius).object_fit(ObjectFit::Cover)).into_any_element(),
            Thumb::Pending => pulse(SharedString::from(format!("thumb-{key}")), frame),
            Thumb::Missing => frame.into_any_element(),
        }
    }
}
