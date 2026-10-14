use crate::{store, yt::Video, Tab, Lower};
use gpui::Context;

impl crate::Unbloated {
    /// Next video in the nav stack (forward).
    pub fn next_video(&self) -> Option<Video> {
        self.nav_pos.and_then(|p| self.nav.get(p + 1)).cloned()
            .or_else(|| self.up_next.first().cloned())
    }

    /// Previous/next video in the nav stack (back/forward).
    #[allow(dead_code)]
    pub fn nav_neighbor(&self, step: isize) -> Option<Video> {
        self.nav_pos.and_then(|p| {
            let i = p as isize + step;
            self.nav.get(i as usize).cloned()
        })
    }

    /// Push a video onto the nav stack (truncating forward history).
    #[allow(dead_code)]
    fn nav_push(&mut self, video: Video) {
        if self.nav.last().is_none_or(|v| v.id != video.id) {
            self.nav.push(video);
        }
    }

    /// Start position for the next video (resume from history, else 0).
    #[allow(dead_code)]
    fn resume_pos(&self, video: &Video) -> f64 {
        self.history.position(&video.id)
    }

    /// Prefetch the next video in the nav stack or queue.
    #[allow(dead_code)]
    pub fn prefetch_next(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.next_video() {
            self.prefetch(&next.id, cx);
        }
    }

    /// Play the next video (from queue, up_next, or nav).
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
}