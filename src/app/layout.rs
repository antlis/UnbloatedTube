use super::*;

impl Unbloated {
    /// At most one column is hidden: hiding one brings the other back.
    pub(crate) fn toggle_left_collapsed(&mut self, cx: &mut Context<Self>) {
        self.left_collapsed = !self.left_collapsed;
        self.right_collapsed = false;
        self.sync_embed();
        cx.notify();
    }

    pub(crate) fn toggle_right_collapsed(&mut self, cx: &mut Context<Self>) {
        self.right_collapsed = !self.right_collapsed;
        self.left_collapsed = false;
        self.sync_embed();
        cx.notify();
    }

    /// Give the lower pane the whole right column, or take it back. Nothing to do without a lower pane.
    pub(crate) fn toggle_lower_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        if self.lower_full || self.settings.recommendations || !self.up_next.is_empty() || !self.chapter_list().is_empty() {
            self.lower_full = !self.lower_full;
            self.player_full = false;
            self.sync_embed();
            cx.notify();
        }
    }

    /// Hide the lower pane so the player takes the whole right column, or bring it back.
    pub(crate) fn toggle_player_full(&mut self, cx: &mut Context<Self>) {
        self.player_full = !self.player_full;
        self.lower_full = false;
        self.sync_embed();
        cx.notify();
    }

    /// The lower pane's tabs as shown: Recommended (if on), Chapters (if the video has them), Up next.
    pub(crate) fn lower_tabs(&self) -> Vec<Lower> {
        let mut tabs = Vec::new();
        if self.settings.recommendations {
            tabs.push(Lower::Recommended);
        }
        if self.description_on() {
            tabs.push(Lower::Description);
        }
        if self.transcript_on() {
            tabs.push(Lower::Transcript);
        }
        if !self.chapter_list().is_empty() {
            tabs.push(Lower::Chapters);
        }
        if self.watch_later_on() {
            tabs.push(Lower::WatchLater);
        }
        if self.comments_on() {
            tabs.push(Lower::Comments);
        }
        tabs.push(Lower::UpNext);
        tabs
    }

    /// The lower pane tab shown now (see the lower pane in `render`).
    pub(crate) fn shown_lower(&self, tabs: &[Lower]) -> Lower {
        match self.lower {
            Lower::Chapters | Lower::Comments | Lower::WatchLater | Lower::Recommended | Lower::Description | Lower::Transcript if !tabs.contains(&self.lower) => tabs[0],
            tab => tab,
        }
    }

    /// `[` / `]`: go to the previous (-1) or next (1) tab, through the header tabs and then the
    /// lower pane's, wrapping around: the same seven tabs as the number keys.
    pub(crate) fn cycle_tabs(&mut self, step: isize, cx: &mut Context<Self>) {
        let header = self.tab_list();
        let lower = self.lower_tabs();
        let at = if self.cycle_lower {
            header.len() + lower.iter().position(|t| *t == self.shown_lower(&lower)).unwrap_or(0)
        } else {
            header.iter().position(|t| *t == self.tab).unwrap_or(0)
        };
        let next = (at as isize + step).rem_euclid((header.len() + lower.len()) as isize) as usize;
        if next < header.len() {
            self.select_tab(header[next], cx);
        } else {
            self.show_lower(lower[next - header.len()], cx);
        }
    }

    pub(crate) fn show_lower(&mut self, tab: Lower, cx: &mut Context<Self>) {
        self.lower = tab;
        if tab == Lower::WatchLater && !matches!(self.watch_later, Load::Idle) {
            // Pick up changes made elsewhere; the list shown stays until the new one arrives.
            self.fetch(cx, "watch-later", |s| &mut s.watch_later, Some("watch-later".into()), |cfg, on| yt::group_videos(cfg, ":ytwatchlater", on));
        }
        self.cycle_lower = true;
        self.player_full = false;
        cx.notify();
    }

    /// Jump to a tab by number, as shown: 1-4 are the header tabs, 5-9 the lower pane's.
    /// Nothing if there are fewer.
    pub(crate) fn tab_number(&mut self, n: usize, cx: &mut Context<Self>) {
        // The lower pane starts at 5, or after the header's tabs when Downloads makes them five.
        let header = self.tab_list();
        let first_lower = header.len().max(4) + 1;
        if n < first_lower {
            if let Some(tab) = header.get(n.wrapping_sub(1)).copied() {
                self.select_tab(tab, cx);
            }
        } else if let Some(tab) = self.lower_tabs().get(n - first_lower).copied() {
            self.show_lower(tab, cx);
        }
    }

    /// Header tabs in order, as shown.
    pub(crate) fn tab_list(&self) -> Vec<Tab> {
        let st = &self.settings;
        if !self.cfg.has_auth() {
            // No account tabs: the anonymous home (Subscriptions), Downloads and Settings.
            let mut tabs = vec![Tab::Subscriptions];
            tabs.extend(self.downloads_on().then_some(Tab::Downloads));
            tabs.push(Tab::Settings);
            return tabs;
        }
        let mut tabs: Vec<Tab> = [(st.subscriptions, Tab::Subscriptions), (st.playlists, Tab::Playlists), (st.history, Tab::History), (self.downloads_on(), Tab::Downloads)]
            .into_iter()
            .filter_map(|(on, t)| on.then_some(t))
            .collect();
        tabs.push(Tab::Settings);
        tabs
    }
}
