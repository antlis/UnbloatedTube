use super::*;

impl Unbloated {
    /// Save `video` with yt-dlp in the background, using the player's quality settings.
    pub(crate) fn download(&mut self, video: Video, cx: &mut Context<Self>) {
        let home = dirs::home_dir().unwrap_or_default();
        let dir = match self.settings.download_dir.trim() {
            "" => dirs::download_dir().unwrap_or_else(|| home.join("Downloads")),
            d => d.strip_prefix("~/").map_or_else(|| PathBuf::from(d), |rest| home.join(rest)),
        };
        let (cfg, format, url, id) = (self.cfg.clone(), player::format(&self.settings), video.url(), video.id.clone());
        let saved = video.clone();
        self.downloads.insert(id.clone(), Download { progress: 0., result: None, done_at: None });
        let shared: Arc<Mutex<Download>> = Arc::new(Mutex::new(Download { progress: 0., result: None, done_at: None }));
        let sink = shared.clone();
        blocking::unblock(move || {
                let res = yt::download(&cfg, &url, &format, &dir, |p| sink.lock().unwrap().progress = p);
                sink.lock().unwrap().result = Some(res);
            })
            .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let now = shared.lock().unwrap().clone();
                let done = now.result.is_some();
                if this.update(cx, |this, cx| {
                    let mut now = now;
                    if done {
                        now.done_at = Some(std::time::Instant::now());
                        if let Some(Ok(path)) = &now.result {
                            this.add_to_library(saved.clone(), path.clone());
                        }
                    }
                    this.downloads.insert(id.clone(), now);
                    cx.notify();
                })
                .is_err()
                    || done
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The Downloads tab is on and has something in it.
    pub(crate) fn downloads_on(&self) -> bool {
        self.settings.downloads_tab && !self.library.is_empty()
    }

    /// A finished download goes to the top of the library (replacing an older copy).
    pub(crate) fn add_to_library(&mut self, video: Video, path: String) {
        self.library.retain(|s| s.video.id != video.id);
        self.library.insert(0, Saved { video, path });
        store::save_data("downloads", &self.library);
    }

    /// Forget downloads whose file is gone (deleted or moved outside the app).
    pub(crate) fn prune_library(&mut self) {
        let before = self.library.len();
        self.library.retain(|s| std::path::Path::new(&s.path).exists());
        if self.library.len() != before {
            store::save_data("downloads", &self.library);
        }
    }

    /// Delete a downloaded file and forget it; a file already gone is only forgotten.
    pub(crate) fn delete_download(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(i) = self.library.iter().position(|s| s.video.id == id) else { return };
        let path = self.library[i].path.clone();
        match std::fs::remove_file(&path) {
            Ok(()) => self.notice = Some(format!("Deleted {path}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.notice = Some("Removed from Downloads".into()),
            Err(e) => {
                self.notice = Some(format!("Couldn't delete {path}: {e}"));
                cx.notify();
                return;
            }
        }
        self.library.remove(i);
        self.downloads.remove(id);
        store::save_data("downloads", &self.library);
        // The tab goes away with its last video.
        if self.tab == Tab::Downloads && !self.downloads_on() {
            self.select_tab(self.tab_list()[0], cx);
        }
        cx.notify();
    }

    /// The Downloads tab's videos, as the list filter narrows them.
    pub(crate) fn library_videos(&self) -> Vec<Video> {
        self.library.iter().map(|s| s.video.clone()).filter(|v| self.video_matches(v)).collect()
    }

    /// What mpv plays for `video`: its downloaded file when there is one, else its page.
    pub(crate) fn media_path(&self, video: &Video) -> String {
        self.library
            .iter()
            .find(|s| s.video.id == video.id && std::path::Path::new(&s.path).exists())
            .map_or_else(|| video.url(), |s| s.path.clone())
    }
}
