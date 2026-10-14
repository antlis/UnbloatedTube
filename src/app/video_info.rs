use super::*;

impl Unbloated {
    /// Description tab is on and there is a video to show it for.
    pub(crate) fn description_on(&self) -> bool {
        self.settings.description && self.current.is_some()
    }

    /// A link clicked in the description or a comment: a YouTube link (and an @handle) opens in
    /// the app, any other in the browser; a time seeks there; a #tag is searched for.
    pub(crate) fn open_text_link(&mut self, link: links::Link, cx: &mut Context<Self>) {
        match link {
            links::Link::Url(url) => match yt::parse_link(&url) {
                Some(link) => self.open_link(link, cx),
                None => self.open_url(&url, cx),
            },
            links::Link::Time(secs) => self.seek_link(secs, cx),
            links::Link::Tag(tag) => {
                self.query = tag;
                self.left_collapsed = false;
                self.run_search(cx);
            }
            links::Link::Handle(handle) => self.open_link(yt::YtLink::Channel { url: format!("https://www.youtube.com/{handle}") }, cx),
        }
        cx.notify();
    }

    /// A timestamp clicked in the description or a comment: play from there (starting the
    /// video first when it isn't loaded).
    pub(crate) fn seek_link(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        let loaded = self.state.as_ref().filter(|s| s.path == self.media_path(&video));
        if self.casting.is_some() {
            self.seek_to(secs, cx);
        } else if let Some(paused) = loaded.map(|s| s.paused) {
            self.seek_to(secs, cx);
            if paused {
                self.player.toggle_pause();
            }
        } else {
            self.link_start = Some(secs);
            self.play(video, None, cx);
        }
    }

    /// Transcript tab is on and there is a video to show it for.
    pub(crate) fn transcript_on(&self) -> bool {
        self.settings.transcript && self.current.is_some()
    }

    /// Comments tab is on and there is a video to show them for.
    pub(crate) fn comments_on(&self) -> bool {
        self.settings.comments && self.current.is_some()
    }

    /// Fetch the first `comments_limit` comments of the current video; what is shown stays until they arrive.
    pub(crate) fn fetch_comments(&mut self, cx: &mut Context<Self>) {
        let id = self.current.as_ref().map(|v| v.id.clone()).unwrap_or_default();
        let limit = self.comments_limit;
        self.fetch(cx, "comments", |s| &mut s.comments, None, move |cfg, on| yt::comments(cfg, &id, limit, on));
    }

    /// The playing video's chapters (empty when off in Settings or the video has none).
    pub(crate) fn chapter_list(&self) -> Arc<[(f64, String)]> {
        match &self.state {
            Some(s) if self.settings.chapters && self.casting.is_none() => s.chapters.clone().into(),
            _ => Arc::from(Vec::new()),
        }
    }
}
