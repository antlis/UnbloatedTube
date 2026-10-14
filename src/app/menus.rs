use super::*;

impl Unbloated {
    /// Open the right-click menu of a video at `pos`. Playlists (for the list at the bottom) are
    /// fetched when you're logged in and they aren't loaded yet.
    pub(crate) fn open_video_menu(&mut self, video: Video, queue: Option<Arc<[Video]>>, pos: gpui::Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        self.channel_menu = None;
        self.menu_queue = queue;
        // The left column's menu stays inside it; anywhere else it may cover the video.
        let left_only = !self.left_collapsed
            && !self.right_collapsed
            && f32::from(pos.x) < self.settings.split * f32::from(window.viewport_size().width);
        self.video_menu = Some((video, pos, !left_only, Instant::now()));
        self.player_menu = false;
        self.menu_hovered = false;
        // Your editable playlists, and which hold this video, as YouTube's Save menu has them.
        self.menu_lists = None;
        if self.cfg.has_auth() {
            let id = self.video_menu.as_ref().map(|m| m.0.id.clone()).unwrap_or_default();
            self.menu_lists = Some((id.clone(), None));
            let vid = id.clone();
            self.with_account(cx, move |a| a.save_options(&vid), move |this, res, cx| {
                if let Some((cur, slot)) = &mut this.menu_lists {
                    if *cur == id {
                        *slot = Some(res.unwrap_or_default());
                        cx.notify();
                    }
                }
            });
        }
        self.sync_embed();
        cx.notify();
    }

    /// Right click on the playing video (mpv's window, so the position comes from mpv, in its
    /// pixels): the same menu with the player rows. mpv's window hides while it is open, which
    /// shows the thumbnail behind it.
    pub(crate) fn open_player_menu(&mut self, at: (f64, f64), window: &Window, cx: &mut Context<Self>) {
        let (Some(video), Some(embed)) = (self.current.clone(), self.embed.clone()) else { return };
        let s = window.scale_factor() as f64;
        let (ox, oy) = embed.borrow().origin();
        let pos = gpui::point(px(((ox as f64 + at.0) / s) as f32), px(((oy as f64 + at.1) / s) as f32));
        self.open_video_menu(video, None, pos, window, cx);
        if let Some(m) = &mut self.video_menu {
            m.2 = true;
        }
        self.player_menu = true;
        self.sync_embed();
        cx.notify();
    }

    pub(crate) fn close_video_menu(&mut self, cx: &mut Context<Self>) {
        self.video_menu = None;
        self.player_menu = false;
        self.quality_menu = false;
        self.sleep_menu = false;
        self.sub_menu = false;
        self.sync_embed();
        cx.notify();
    }
}
