use super::*;

impl Unbloated {
    /// Stream a yt-dlp listing into `slot` in the background. New entries appear as they arrive,
    /// unless the slot already shows a list (cached or a refresh), which stays until the new one
    /// is complete. With `cache` (a file name), the finished list is saved and shown instantly
    /// next time. `key` identifies the slot: a newer fetch for the same key supersedes this one.
    pub(crate) fn fetch<T: Clone + Send + Serialize + DeserializeOwned + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        key: &'static str,
        slot: fn(&mut Self) -> &mut Load<T>,
        cache: Option<String>,
        f: impl FnOnce(&Config, &mut dyn FnMut(T)) -> Result<(), String> + Send + 'static,
    ) {
        let generation = self.generations.get(key).map_or(0, |g| g + 1);
        self.generations.insert(key, generation);
        if let (Some(name), Load::Idle) = (&cache, slot(self)) {
            // The list caches belong to the account; a logged-out session shows the
            // connect hint instead of last time's lists. The anonymous home ("anon")
            // has no account behind it and is kept across sessions.
            if self.cfg.has_auth() || name == "anon" {
                if let Some(items) = store::load_list(name) {
                    *slot(self) = Load::Ready(items);
                    self.after_load(key, cx);
                }
            }
        }
        let keep_old = !slot(self).items().is_empty();
        let old = match std::mem::replace(slot(self), Load::Idle) {
            Load::Ready(v) | Load::Loading(v) if keep_old => v,
            _ => Vec::new(),
        };
        *slot(self) = Load::Loading(old);

        let inbox: Inbox<T> = Default::default();
        let (cfg, sink) = (self.cfg.clone(), inbox.clone());
        blocking::unblock(move || {
                let res = f(&cfg, &mut |item| sink.lock().unwrap().0.push(item));
                sink.lock().unwrap().1 = Some(res);
            })
            .detach();
        cx.spawn(async move |this, cx| {
            let mut fresh = Vec::new();
            loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let (new, done) = {
                    let mut inbox = inbox.lock().unwrap();
                    (std::mem::take(&mut inbox.0), inbox.1.take())
                };
                let finished = done.is_some();
                let current = this.update(cx, |this, cx| {
                    if this.generations.get(key) != Some(&generation) {
                        return false;
                    }
                    if keep_old {
                        fresh.extend(new);
                    } else if let Load::Loading(v) = slot(this) {
                        if !new.is_empty() {
                            v.extend(new);
                            cx.notify();
                        }
                    }
                    if let Some(res) = done {
                        let items = match std::mem::replace(slot(this), Load::Idle) {
                            Load::Loading(v) if !keep_old => v,
                            // A failed refresh keeps showing the previous list.
                            Load::Loading(old) if fresh.is_empty() && res.is_err() => old,
                            _ => std::mem::take(&mut fresh),
                        };
                        let ok = res.is_ok();
                        let failure = res.as_ref().err().filter(|_| items.is_empty()).cloned();
                        *slot(this) = match res {
                            Err(e) if items.is_empty() => Load::Failed(e),
                            _ => Load::Ready(items),
                        };
                        if let Some(e) = failure {
                            this.note_login_failure(key, &e);
                        }
                        this.after_load(key, cx);
                        if let (Some(name), true) = (&cache, ok) {
                            store::save_list(name, slot(this).items());
                        }
                        cx.notify();
                    }
                    true
                });
                if finished || !current.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn after_load(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "subs" => {
                if let Load::Ready(g) = &mut self.subs.groups {
                    g.sort_by_key(|g| g.title.to_lowercase());
                }
            }
            "history" if self.current.is_none() => self.current = self.yt_history.items().iter().find(|v| !v.short).cloned(),
            // Logged out: a random video waits in the player (not playing), like the last
            // watched one does with a login.
            "anon" if self.current.is_none() && !self.cfg.has_auth() => {
                let pool: Vec<&Video> = self.anon.items().iter().filter(|v| !v.short).collect();
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
                self.current = (!pool.is_empty()).then(|| pool[nanos % pool.len()].clone());
                // Only shown: loading it into mpv would be harmless logged out, but a login
                // later makes yt-dlp mark it watched on YouTube.
                self.preloaded = true;
                // Recommended then shows that video's channel instead of repeating the home list.
                self.load_recs(cx);
            }
            "feed" => {
                if !self.seen.baseline && matches!(self.feed, Load::Ready(_)) {
                    // First run: what's in the feed now is old news; count only what comes after.
                    self.seen.ids.extend(self.feed.items().iter().map(|v| v.id.clone()));
                    self.seen.baseline = true;
                    self.seen.save();
                }
                self.notify_uploads();
            }
            _ => {}
        }
    }

    pub(crate) fn load_tab(&mut self, cx: &mut Context<Self>) {
        match self.tab {
            Tab::Settings => {}
            Tab::Downloads => self.prune_library(),
            Tab::Search => self.run_search(cx),
            Tab::History if !self.cfg.has_auth() => self.load_anon(cx),
            Tab::History => {
                if self.cfg.has_auth() {
                    self.load_yt_history(cx);
                }
            }
            tab => match self.browser(tab).open.clone() {
                Some(_) => self.load_group_videos(tab, cx),
                None if !self.cfg.has_auth() => self.load_anon(cx),
                None if tab == Tab::Subscriptions => {
                    self.fetch(cx, "subs", |s| &mut s.subs.groups, Some("subs".into()), |cfg, on| yt::subscriptions(cfg, on));
                    if !matches!(self.feed, Load::Loading(_)) {
                        self.fetch(cx, "feed", |s| &mut s.feed, Some("feed".into()), |cfg, on| yt::feed(cfg, on));
                    }
                }
                None => {
                    self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
                }
            },
        }
    }

    /// YouTube's watch history (cached; the cache shows until the fresh one is complete).
    pub(crate) fn load_yt_history(&mut self, cx: &mut Context<Self>) {
        self.yt_history_asked = Some(Instant::now());
        let account = self.account.clone();
        self.fetch(cx, "history", |s| &mut s.yt_history, Some("history".into()), move |cfg, on| yt::history(cfg, account, on));
    }

    /// The logged-out home: a fresh random mix; the cached one shows until it arrives.
    pub(crate) fn load_anon(&mut self, cx: &mut Context<Self>) {
        self.fetch(cx, "anon", |s| &mut s.anon, Some("anon".into()), |cfg, on| yt::anonymous(cfg, on));
    }

    pub(crate) fn run_search(&mut self, cx: &mut Context<Self>) {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.tab = Tab::Search;
        self.recent_searches.retain(|q| *q != query);
        self.recent_searches.insert(0, query.clone());
        self.recent_searches.truncate(10);
        store::save_data("searches", &self.recent_searches);
        // A new query replaces the old results right away instead of after it finishes.
        self.search = Load::Idle;
        self.fetch(cx, "search", |s| &mut s.search, None, move |cfg, on| yt::search(cfg, &query, on));
    }

    pub(crate) fn search_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match edit_text(&mut self.caret, "search", &mut self.query, ev, cx) {
            Edit::Submit => match yt::parse_link(&self.query) {
                // A pasted YouTube link plays instead of being searched for.
                Some(link) => {
                    self.query.clear();
                    self.close_search(window, cx);
                    self.open_link(link, cx);
                }
                None => {
                    self.run_search(cx);
                    window.blur();
                }
            },
            Edit::Cancel => self.close_search(window, cx),
            Edit::Changed => {}
            Edit::Moved => {}
            // Arrows etc. inside the field mustn't reach the shortcuts.
            Edit::Ignored => cx.stop_propagation(),
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Open a YouTube link: a video plays, a channel or playlist opens in the left column.
    pub(crate) fn open_link(&mut self, link: yt::YtLink, cx: &mut Context<Self>) {
        match link {
            yt::YtLink::Video(video) => self.open_video_link(video, cx),
            yt::YtLink::Channel { url } => {
                let id = url.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string();
                let channel = Group { title: id.clone(), url: format!("{url}/videos"), id, thumb: None, subscribers: None };
                self.show_channel(channel, cx);
            }
            yt::YtLink::Playlist(id) => {
                let url = format!("https://www.youtube.com/playlist?list={id}");
                let playlist = Group { title: "Playlist".into(), id, url, thumb: None, subscribers: None };
                self.tab = Tab::Playlists;
                self.open_group(Tab::Playlists, playlist, cx);
            }
        }
        cx.notify();
    }

    /// Play a video link: yt-dlp looks the video up, then it plays like a picked one.
    pub(crate) fn open_video_link(&mut self, link: yt::VideoLink, cx: &mut Context<Self>) {
        self.notice = Some("Opening the link…".into());
        let (cfg, id) = (self.cfg.clone(), link.id.clone());
        let task = blocking::unblock(move || { yt::video(&cfg, &id) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(video) => {
                        this.notice = None;
                        this.link_start = link.start;
                        this.play(video, None, cx);
                    }
                    Err(e) => this.notice = Some(format!("Couldn't open the link: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Enter search mode (/ or the search icon): full-width field, focused.
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.left_collapsed = false;
        if !self.searching {
            self.prev_tab = if self.tab == Tab::Search { Tab::Subscriptions } else { self.tab };
            self.searching = true;
            self.tab = Tab::Search;
        }
        window.focus(&self.search_focus);
        cx.notify();
    }

    /// Leave search mode, back to the tab it was opened from.
    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.searching = false;
        self.tab = self.prev_tab;
        window.blur();
        cx.notify();
    }

    pub(crate) fn browser(&mut self, tab: Tab) -> &mut Browser {
        if tab == Tab::Subscriptions { &mut self.subs } else { &mut self.playlists }
    }

    pub(crate) fn browser_ref(&self, tab: Tab) -> &Browser {
        if tab == Tab::Subscriptions { &self.subs } else { &self.playlists }
    }

    /// An open channel or playlist's videos; New uploads is limited to the active group.
    pub(crate) fn browser_videos(&self, tab: Tab) -> Vec<Video> {
        let b = self.browser_ref(tab);
        let feed = b.open.as_ref().is_some_and(|g| g.id == FEED_ID);
        b.videos
            .items()
            .iter()
            .filter(|v| !feed || (self.in_active_group(v) && !self.is_muted(v)))
            .filter(|v| self.video_matches(v))
            .cloned()
            .collect()
    }

    /// Whether `text` matches the left list's filter (case-insensitive; empty matches all).
    pub(crate) fn filter_match(&self, text: &str) -> bool {
        let q = self.list_filter.trim().to_lowercase();
        q.is_empty() || text.to_lowercase().contains(&q)
    }

    pub(crate) fn video_matches(&self, v: &Video) -> bool {
        self.filter_match(&format!("{} {}", v.title, v.channel.as_deref().unwrap_or_default()))
    }

    /// Ctrl+F: focus the filter, where the current list has one.
    pub(crate) fn focus_filter(&mut self, window: &mut Window) -> bool {
        let has = matches!(self.tab, Tab::Subscriptions | Tab::Playlists | Tab::History | Tab::Downloads);
        if has {
            window.focus(&self.filter_focus);
        }
        has
    }

    /// Show a channel's videos in the left column (subscribed or not).
    pub(crate) fn show_channel(&mut self, channel: Group, cx: &mut Context<Self>) {
        self.tab = Tab::Subscriptions;
        self.open_group(Tab::Subscriptions, channel, cx);
    }

    pub(crate) fn open_group(&mut self, tab: Tab, group: Group, cx: &mut Context<Self>) {
        if tab == Tab::Subscriptions && group.id != FEED_ID {
            // Opening a channel shows its new videos, so they no longer count as new.
            let ids: Vec<String> = self.feed.items().iter().filter(|v| channel_of(v) == Some(&group.id)).map(|v| v.id.clone()).collect();
            if !ids.is_empty() {
                self.seen.ids.extend(ids);
                self.seen.save();
            }
        }
        self.confirm_unsub = None;
        self.editing_groups = false;
        let b = self.browser(tab);
        b.open = Some(group);
        b.view = ChannelView::Videos;
        self.load_group_videos(tab, cx);
    }

    pub(crate) fn set_channel_view(&mut self, view: ChannelView, cx: &mut Context<Self>) {
        if self.subs.view != view {
            self.subs.view = view;
            self.load_group_videos(Tab::Subscriptions, cx);
        }
    }

    /// (Re)load the open group's list: a channel's videos or Shorts, or a playlist.
    pub(crate) fn load_group_videos(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let b = self.browser(tab);
        let Some(group) = b.open.clone() else { return };
        let (page, suffix) = match b.view {
            ChannelView::Shorts => ("/shorts", "-shorts"),
            ChannelView::Live => ("/streams", "-live"),
            ChannelView::Videos => ("/videos", ""),
        };
        let url = group.url.replace("/videos", page);
        // Revisiting a channel or playlist shows its last list instantly, then refreshes.
        let cache = Some(format!("group-{}{suffix}", group.id));
        b.videos = Load::Idle;
        if tab == Tab::Subscriptions {
            self.fetch(cx, "subs.videos", |s| &mut s.subs.videos, cache, move |cfg, on| yt::group_videos(cfg, &url, on));
        } else {
            self.fetch(cx, "playlists.videos", |s| &mut s.playlists.videos, cache, move |cfg, on| {
                yt::playlist_videos(cfg, &url, on)
            });
        }
    }

    pub(crate) fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.cycle_lower = false;
        self.left_collapsed = false;
        self.searching = false;
        if tab == Tab::Downloads {
            self.prune_library();
        }
        let idle = match tab {
            Tab::Search | Tab::Settings | Tab::Downloads => false,
            // Logged out: only fetch the anonymous home the first time (or after a failure).
            _ if !self.cfg.has_auth() => matches!(self.anon, Load::Idle | Load::Failed(_)),
            // The list shown may be the one saved last time, or from a while ago: ask again
            // (it stays on screen until the new one is complete).
            Tab::History => {
                !matches!(self.yt_history, Load::Loading(_)) && self.yt_history_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(60))
            }
            _ => matches!(self.browser(tab).groups, Load::Idle),
        };
        if idle {
            self.load_tab(cx);
        }
        cx.notify();
    }

    pub(crate) fn tab_loading(&self) -> bool {
        match self.tab {
            Tab::Settings | Tab::Downloads => false,
            Tab::Search => matches!(self.search, Load::Loading(_)),
            Tab::History if self.cfg.has_auth() => matches!(self.yt_history, Load::Loading(_)),
            tab => {
                let b = self.browser_ref(tab);
                if b.open.is_some() {
                    matches!(b.videos, Load::Loading(_))
                } else if !self.cfg.has_auth() {
                    matches!(self.anon, Load::Loading(_))
                } else {
                    matches!(b.groups, Load::Loading(_))
                }
            }
        }
    }

    /// Home feed when logged in; otherwise more uploads from the current video's channel.
    pub(crate) fn load_recs(&mut self, cx: &mut Context<Self>) {
        let channel = self.current.as_ref().and_then(|v| v.channel_url.clone());
        let auth = self.cfg.has_auth();
        if !auth && channel.is_none() {
            // Logged out with nothing playing: `render_recs` falls back to the anonymous home.
            self.recs = Load::Idle;
            return;
        }
        // Only the home feed is the same every time, so only it is cached.
        self.fetch(cx, "recs", |s| &mut s.recs, auth.then(|| "recs".into()), move |cfg, on| {
            if auth {
                match yt::recommendations(cfg, on) {
                    Err(e) if channel.is_none() => return Err(e),
                    Err(_) => {}
                    ok => return ok,
                }
            }
            yt::channel_uploads(cfg, &channel.unwrap(), on)
        });
    }

    /// The streams that are live now from your subscriptions (the feed's live rows, in the active
    /// group and not muted); `filtered`: also by the text in the list filter.
    pub(crate) fn live_videos(&self, filtered: bool) -> Vec<Video> {
        self.feed
            .items()
            .iter()
            .filter(|v| v.live && self.in_active_group(v) && !self.is_muted(v))
            .filter(|v| !filtered || self.video_matches(v))
            .cloned()
            .collect()
    }

    /// The left column's list in display order (same order the views render).
    pub(crate) fn left_items(&self) -> Vec<Item> {
        let videos = |list: Vec<Video>| -> Vec<Item> {
            let list: Arc<[Video]> = self.visible(&list).into();
            list.iter().map(|v| Item::Video(v.clone(), list.clone())).collect()
        };
        match self.tab {
            Tab::Settings => Vec::new(),
            Tab::Search => videos(self.wanted(self.search.items().to_vec())),
            Tab::Downloads => videos(self.library_videos()),
            Tab::History if self.cfg.has_auth() => {
                let (partial, all) = self.history_items();
                let mut items = videos(partial);
                items.extend(videos(all));
                items
            }
            tab => match &self.browser_ref(tab).open {
                Some(_) if tab == Tab::Playlists => videos(self.browser_videos(tab)),
                Some(_) => videos(self.wanted(self.browser_videos(tab))),
                // Logged out: the anonymous home list.
                None if !self.cfg.has_auth() => videos(self.wanted(self.anon.items().to_vec())),
                None if tab == Tab::Subscriptions && self.subs_live => videos(self.wanted(self.live_videos(true))),
                None => self.group_items(tab, &self.unseen_counts()).into_iter().map(Item::Group).collect(),
            },
        }
    }

    /// Settings → Hide videos with words, lower-cased.
    pub(crate) fn hide_words(&self) -> Vec<String> {
        self.settings.hide_words.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect()
    }

    /// `videos` without the ones whose title says a hidden word.
    pub(crate) fn wanted(&self, videos: Vec<Video>) -> Vec<Video> {
        let words = self.hide_words();
        if words.is_empty() {
            return videos;
        }
        videos.into_iter().filter(|v| !says_any(&v.title, &words)).collect()
    }

    /// Videos as lists show them (Shorts hidden when turned off).
    pub(crate) fn visible(&self, videos: &[Video]) -> Vec<Video> {
        videos.iter().filter(|v| self.settings.shorts || !v.short).cloned().collect()
    }

    /// The History tab shows its Shorts list (only while Shorts are on in Settings).
    pub(crate) fn history_shorts(&self) -> bool {
        self.settings.shorts && self.history_view == ChannelView::Shorts
    }

    /// History tab: Continue watching (started, not finished), then local + YouTube history.
    pub(crate) fn history_items(&self) -> (Vec<Video>, Vec<Video>) {
        // YouTube's own order first (what was played here is in it, via mark-watched), then
        // what only this app knows; without YouTube's list, just the local one.
        let mut all: Vec<Video> = self.yt_history.items().to_vec();
        let seen: HashSet<String> = all.iter().map(|v| v.id.clone()).collect();
        all.extend(self.history.items.iter().filter(|w| !seen.contains(&w.video.id)).map(|w| w.video.clone()));
        let shorts = self.history_shorts();
        all.retain(|v| self.video_matches(v) && v.short == shorts);
        let partial = self
            .history
            .items
            .iter()
            .filter(|w| w.position > 30. && !w.finished && self.video_matches(&w.video) && w.video.short == shorts)
            .take(50)
            .map(|w| w.video.clone())
            .collect();
        (partial, all)
    }

    /// A browser tab's groups in display order. Subscriptions: New uploads first, then channels
    /// with new videos, then the rest (each part A–Z).
    pub(crate) fn group_items(&self, tab: Tab, counts: &HashMap<String, usize>) -> Vec<Group> {
        let mut g: Vec<Group> = self.browser_ref(tab).groups.items().to_vec();
        let filtering = !self.list_filter.trim().is_empty();
        g.retain(|c| self.filter_match(&c.title));
        if tab == Tab::Subscriptions {
            if let Some(group) = self.active_group() {
                g.retain(|c| group.channels.contains(&c.id));
            }
            g.sort_by_key(|g| !counts.contains_key(&g.id));
            if !filtering {
                g.insert(0, Group { id: FEED_ID.into(), title: "New uploads".into(), url: ":ytsubs".into(), thumb: None, subscribers: None });
            }
        }
        g
    }

    pub(crate) fn set_subs_live(&mut self, live: bool, cx: &mut Context<Self>) {
        if self.subs_live != live {
            self.subs_live = live;
            self.vim_cursor = 0;
            cx.notify();
        }
    }
}
