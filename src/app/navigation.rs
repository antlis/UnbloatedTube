use super::*;

impl Unbloated {
    /// What Next / autoplay plays: the Up next queue first, then the list the video came from.
    pub fn next_video(&self) -> Option<Video> {
        self.up_next.first().cloned().or_else(|| self.neighbor(1))
    }

    /// Play the next video (from Up next, or the list).
    pub fn play_next(&mut self, cx: &mut Context<Self>) {
        let from_queue = !self.up_next.is_empty();
        let Some(v) = self.next_video() else { return };
        let title = v.title.clone();
        self.play(v, None, cx);
        // After `play`, which clears notices.
        if from_queue {
            self.notice = Some(format!("Next from Up next: {title}"));
        }
    }

    /// Add the open playlist's videos to Up next (skipping ones already queued).
    pub fn enqueue_all(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let videos = self.visible(&self.browser_videos(tab));
        let before = self.up_next.len();
        for v in videos {
            if !self.up_next.iter().any(|q| q.id == v.id) {
                self.up_next.push(v);
            }
        }
        store::save_data("up_next", &self.up_next);
        let added = self.up_next.len() - before;
        self.lower = Lower::UpNext;
        if self.current.is_none() && !self.up_next.is_empty() {
            let v = self.up_next.remove(0);
            store::save_data("up_next", &self.up_next);
            self.start(v, false);
        }
        // After `start`, which clears notices.
        self.notice = Some(format!("Added {added} videos to Up next"));
        cx.notify();
    }

    pub fn enqueue(&mut self, video: Video, cx: &mut Context<Self>) {
        if !self.up_next.iter().any(|v| v.id == video.id) {
            self.notice = Some(format!("Added to Up next: {}", video.title));
            self.up_next.push(video);
            store::save_data("up_next", &self.up_next);
        }
        cx.notify();
    }

    pub fn dequeue(&mut self, id: &str, cx: &mut Context<Self>) {
        self.up_next.retain(|v| v.id != id);
        store::save_data("up_next", &self.up_next);
        cx.notify();
    }

    /// Play `videos` here as the queue, from the first. Up next is emptied first: it would otherwise
    /// take over at the first Next, and the point of playing a playlist is to hear it through.
    pub(crate) fn play_all(&mut self, videos: Vec<Video>, cx: &mut Context<Self>) {
        let Some(first) = videos.first().cloned() else { return };
        if !self.up_next.is_empty() {
            self.up_next.clear();
            store::save_data("up_next", &self.up_next);
        }
        self.play(first, Some(videos.into()), cx);
    }

    /// Play a playlist here without opening it: all its videos are listed, then the first plays
    /// with the rest as the queue (Next and autoplay go through them).
    pub(crate) fn play_playlist(&mut self, group: Group, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        if self.playlists.open.as_ref().is_some_and(|g| g.id == group.id) {
            if let Load::Ready(v) = &self.playlists.videos {
                let videos = self.visible(v);
                if !videos.is_empty() {
                    self.play_all(videos, cx);
                    return;
                }
            }
        }
        self.notice = Some(format!("Loading {}…", group.title));
        let (cfg, url) = (self.cfg.clone(), group.url.clone());
        let task = blocking::unblock(move || {
            let mut out = Vec::new();
            yt::playlist_videos(&cfg, &url, &mut |v| out.push(v)).map(|_| out)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(videos) => {
                    let videos = this.visible(&videos);
                    if videos.is_empty() {
                        this.notice = Some(format!("{} has no videos", group.title));
                        cx.notify();
                    } else {
                        this.play_all(videos, cx);
                    }
                }
                Err(e) => {
                    this.notice = Some(format!("Couldn't list {}: {e}", group.title));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Next or previous: the receiver's queue while casting a list, else the list here.
    pub(crate) fn next_prev(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.casting.as_ref().is_some_and(|c| c.listed) {
            self.cast_ctl(serde_json::json!({ "action": if forward { "next" } else { "prev" } }), cx);
        } else if forward {
            self.play_next(cx);
        } else if let Some(v) = self.neighbor(-1) {
            self.play(v, None, cx);
        }
    }

    /// The video `step` places away from the current one in the queue.
    /// Where the playing video is in `nav` (the entry we moved to, else its latest one).
    pub(crate) fn nav_here(&self) -> Option<usize> {
        let id = &self.current.as_ref()?.id;
        self.nav_pos.filter(|&p| self.nav.get(p).is_some_and(|v| &v.id == id)).or_else(|| self.nav.iter().rposition(|v| &v.id == id))
    }

    /// The `nav` entries back and forward from the playing video.
    pub(crate) fn nav_targets(&self) -> (Option<usize>, Option<usize>) {
        match self.nav_here() {
            Some(p) => (p.checked_sub(1), Some(p + 1).filter(|&n| n < self.nav.len())),
            // The playing video isn't in the list (a link, say): back goes to the last one played.
            None => (self.nav.len().checked_sub(1), None),
        }
    }

    /// Back (`forward` false) or forward through the videos played.
    pub(crate) fn nav_go(&mut self, forward: bool, cx: &mut Context<Self>) {
        let (back, next) = self.nav_targets();
        let Some(i) = (if forward { next } else { back }) else { return };
        let video = self.nav[i].clone();
        self.nav_moving = true;
        self.nav_pos = Some(i);
        self.play(video, None, cx);
        self.nav_moving = false;
    }

    pub(crate) fn neighbor(&self, step: isize) -> Option<Video> {
        let current = self.current.as_ref()?;
        let i = self.queue.iter().position(|v| v.id == current.id)?;
        self.queue.get(i.checked_add_signed(step)?).cloned()
    }
}
