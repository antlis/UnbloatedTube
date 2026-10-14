use super::*;

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
                let ok = task.await.is_ok();
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
