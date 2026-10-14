use super::*;

impl Unbloated {
    /// An account list failed like a login that stopped working (expired cookies, a browser
    /// profile that is gone): show it in Settings → Account, where Try again / Choose another
    /// browser are, instead of leaving only failed lists.
    pub(crate) fn note_login_failure(&mut self, key: &str, error: &str) {
        if ["subs", "feed", "history", "playlists", "recs"].contains(&key) {
            self.login_failed(error);
        }
    }

    pub(crate) fn login_failed(&mut self, error: &str) {
        if self.cfg.has_auth() && looks_like_login_error(error) && matches!(self.connect, Connect::Idle) {
            // The browser's cookies couldn't be read (step 1), or YouTube didn't accept them (2).
            let step = if error.to_lowercase().contains("cookie") { 1 } else { 2 };
            self.connect = Connect::Failed { step, error: session_hint(error.to_string()) };
        }
    }

    pub(crate) fn open_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.saving = true;
        self.save_filter.clear();
        self.save_editable = None;
        if let Some(video) = self.current.as_ref().map(|v| v.id.clone()) {
            let id = video.clone();
            self.with_account(cx, move |a| a.save_options(&id), move |this, res, cx| {
                // Ignore an answer for a video the list has moved on from.
                if this.saving && this.current.as_ref().is_some_and(|c| c.id == video) {
                    this.save_editable = Some(res.ok().map(|o| o.into_iter().map(|o| o.id).collect()));
                    cx.notify();
                }
            });
        } else {
            self.save_editable = Some(None);
        }
        window.focus(&self.save_focus);
        if matches!(self.playlists.groups, Load::Idle) {
            self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
        }
        self.sync_embed();
        cx.notify();
    }

    pub(crate) fn close_save(&mut self, cx: &mut Context<Self>) {
        self.saving = false;
        self.sync_embed();
        cx.notify();
    }

    /// Playlists you can save to, narrowed by the typed filter.
    pub(crate) fn save_targets(&self) -> Vec<Group> {
        let filter = self.save_filter.to_lowercase();
        self.playlists
            .groups
            .items()
            .iter()
            // "Liked videos" isn't a playlist you can add to, nor are ones you only saved from others
            // (Watch later is not in YouTube's list, and always works).
            .filter(|g| g.id != "LL" && g.title.to_lowercase().contains(&filter))
            .filter(|g| match &self.save_editable {
                Some(Some(editable)) => g.id == "WL" || editable.contains(&g.id),
                Some(None) => true,
                None => false,
            })
            .cloned()
            .collect()
    }

    /// Type to filter, Enter saves to the first match, Esc closes.
    pub(crate) fn save_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match edit_text(&mut self.caret, "save", &mut self.save_filter, ev, cx) {
            Edit::Submit => {
                if let Some(g) = self.save_targets().into_iter().next() {
                    self.save_to(g, cx);
                }
            }
            Edit::Cancel => self.close_save(cx),
            Edit::Changed => {}
            Edit::Moved => {}
            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The CC button under the player: only where the hover bar's can't be used (hover controls
    /// off, picture-in-picture, casting), since that one does the same.
    pub(crate) fn cc_under_player(&self) -> bool {
        self.settings.subtitles_button && (!self.settings.video_controls || self.pip || self.casting.is_some())
    }

    pub(crate) fn account_buttons(&self) -> bool {
        let st = &self.settings;
        self.cfg.has_auth() && (st.subscribe_button || st.save_button || st.watch_later_button || st.like_button || st.dislike_button)
    }

    /// Some of the playing video's info (views, date, subscribers) is switched on and available.
    pub(crate) fn info_wanted(&self) -> bool {
        let st = &self.settings;
        self.cfg.has_auth() && (st.show_views || st.show_date || st.show_subs)
    }

    /// Run `f` with the account on a background thread (loading it first if needed),
    /// then `done` on the UI thread.
    pub(crate) fn with_account<R: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl Fn(&Account) -> Result<R, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<R, String>, &mut Context<Self>) + 'static,
    ) {
        let (cfg, account) = (self.cfg.clone(), self.account.clone());
        let task = blocking::unblock(move || {
            let cached = account.is_some();
            let mut account = match account {
                Some(a) => a,
                None => account::shared(&cfg)?,
            };
            // Cookies kept from an earlier start may have been rotated since, too.
            let cached = cached || account.from_kept;
            let mut res = f(&account);
            // A login kept for long enough stops being accepted (YouTube rotates its cookies):
            // try once more with the cookies as the browser has them now.
            if res.is_err() && cached {
                if let Ok(fresh) = Account::load_fresh(&cfg) {
                    account = Arc::new(fresh);
                    res = f(&account);
                }
            }
            Ok::<_, String>((account, res))
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                let res = match res {
                    Ok((account, res)) => {
                        account::remember(&account);
                        this.account = Some(account);
                        res
                    }
                    Err(e) => Err(e),
                };
                if let Err(e) = &res {
                    this.login_failed(e);
                }
                done(this, res, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The first-run screen is up: not dismissed yet, and no login configured.
    pub(crate) fn welcome_visible(&self) -> bool {
        !self.settings.welcome_seen && !self.cfg.has_auth()
    }

    /// Dismiss the first-run screen (Esc, or "Continue without account").
    pub(crate) fn finish_welcome(&mut self, cx: &mut Context<Self>) {
        self.settings.welcome_seen = true;
        self.settings.save();
        self.sync_embed();
        cx.notify();
    }

    /// "Connect YouTube" on the first-run screen: go to Settings, where the Connect panel lives.
    pub(crate) fn welcome_connect(&mut self, cx: &mut Context<Self>) {
        self.finish_welcome(cx);
        self.select_tab(Tab::Settings, cx);
    }

    /// Pick up the login from disk again (config.toml + auth.json) after it changed, and let
    /// lists that failed without one reload. mpv gets the new cookie options; a playing video
    /// restarts at its position, like after any player-setting change.
    pub(crate) fn reload_auth(&mut self, cx: &mut Context<Self>) {
        self.cfg = Arc::new(Config::load());
        self.account = None;
        account::forget();
        clear_failed(&mut self.subs.groups);
        clear_failed(&mut self.feed);
        clear_failed(&mut self.yt_history);
        clear_failed(&mut self.playlists.groups);
        clear_failed(&mut self.recs);
        clear_failed(&mut self.watch_later);
        if !matches!(self.tab, Tab::Settings | Tab::Search) {
            self.load_tab(cx);
        }
        if self.welcome_visible() {
            // The login vanished from disk: the connect screen takes over. Stopped first
            // so apply_player_settings doesn't restart the video with the new options.
            self.player.stop();
            self.current = None;
            self.state = None;
        }
        self.apply_player_settings(cx);
        self.sync_embed();
    }

    /// Save the selected browser as the login and run the three-step check.
    pub(crate) fn start_connect(&mut self, cx: &mut Context<Self>) {
        if self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some() {
            self.import_msg = Some((false, "config.toml sets a login, which wins over this one: remove it there first.".into()));
            cx.notify();
            return;
        }
        Auth::Browser(self.connect_browser.clone()).save();
        self.reload_auth(cx);
        self.connect = Connect::Running(1);
        self.run_probe(true, cx);
    }

    /// The check failed. A login that never worked (`rollback`: the first attempt) is dropped
    /// again, so the account tabs don't show up empty. Ignored after a log out meanwhile.
    pub(crate) fn fail_probe(&mut self, step: u8, error: String, rollback: bool, cx: &mut Context<Self>) {
        if !self.cfg.has_auth() {
            return;
        }
        self.connect = Connect::Failed { step, error: session_hint(error) };
        if rollback && self.cfg.cookies_file.is_none() && self.cfg.cookies_from_browser.is_none() {
            Auth::clear();
            self.reload_auth(cx);
        }
        cx.notify();
    }

    /// The check, step by step, off the UI thread: profile readable → session valid →
    /// feed reachable. Each step reports back with `cx.notify()`.
    pub(crate) fn run_probe(&mut self, rollback: bool, cx: &mut Context<Self>) {
        self.import_msg = None;
        let cfg = self.cfg.clone();
        cx.spawn(async move |this, cx| {
            let loaded = blocking::unblock({ let cfg = cfg.clone(); move || Account::load_fresh(&cfg) }).await;
            let account = match loaded {
                Ok(account) => account,
                Err(error) => {
                    this.update(cx, |this, cx| this.fail_probe(1, error, rollback, cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                // The export may have persisted a keyring-qualified spec ("brave+gnomekeyring");
                // pick it up so lists and player restart with the credentials that work.
                if this.cfg.cookies_file.is_none() && this.cfg.cookies_from_browser.is_none()
                    && Auth::load() != this.cfg.auth
                {
                    this.reload_auth(cx);
                }
                this.connect = Connect::Running(2);
                cx.notify();
            })
            .ok();
            let loaded = { let account = account.clone(); blocking::unblock(move || { account.me() }).await };
            let me = match loaded {
                Ok(me) => me,
                Err(error) => {
                    this.update(cx, |this, cx| this.fail_probe(2, error, rollback, cx)).ok();
                    return;
                }
            };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                this.connect = Connect::Running(3);
                cx.notify();
            })
            .ok();
            // Read the config again — the export may have learned the keyring spec.
            let cfg = Arc::new(Config::load());
            let probe = { let cfg = cfg.clone(); blocking::unblock(move || { yt::auth_probe(&cfg) }).await };
            this.update(cx, |this, cx| {
                if !this.cfg.has_auth() {
                    return;
                }
                match probe {
                    Ok(()) => {
                        let account = Arc::new(account);
                        account::remember(&account);
                        this.account = Some(account);
                        this.connect = Connect::Done { me };
                        // Re-fetch the playing video's like/subscribe state under the new login.
                        this.status = None;
                        this.status_requested = None;
                        if rollback {
                            // First connect: like after a launch with a login, the latest
                            // YouTube history video waits in the player (not playing; set by
                            // after_load once the list is there), and the home feed loads.
                            // A video that isn't playing is only the logged-out placeholder: replace it.
                            // Neither is preloaded: mpv's `mark-watched` would add it to YouTube's
                            // history, though it was only shown.
                            if this.state.is_none() {
                                this.current = None;
                            }
                            this.preloaded = true;
                            this.load_yt_history(cx);
                            if this.settings.recommendations {
                                this.load_recs(cx);
                            }
                        }
                    }
                    Err(error) => {
                        this.fail_probe(3, error, rollback, cx);
                        return;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Log out: forget the app-managed login; for a config.toml login, comment its two
    /// lines out instead (the rest of the file is untouched, uncomment to return).
    pub(crate) fn logout(&mut self, cx: &mut Context<Self>) {
        let from_toml = self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some();
        let word = if from_toml {
            match store::disable_login_in_config() {
                Ok(true) => "Logged out — the login in config.toml was commented out, uncomment it to return.",
                Ok(false) => "Logged out.",
                Err(e) => {
                    self.import_msg = Some((false, e));
                    cx.notify();
                    return;
                }
            }
        } else {
            "Logged out."
        };
        // The session's video goes with the account: stop it and clear the pane before
        // reload_auth, so it can't restart the video with the new (empty) cookie options.
        // The local history on disk stays for the next login.
        self.player.stop();
        self.current = None;
        self.state = None;
        self.ended = None;
        self.lower = Lower::Recommended;
        self.comments = Load::Idle;
        self.comments_for = None;
        Auth::clear();
        self.reload_auth(cx);
        // Account fetches still running must not land in the slots cleared below.
        for key in ["subs", "feed", "history", "playlists", "recs"] {
            *self.generations.entry(key).or_insert(0) += 1;
        }
        // Account lists are dropped from memory (the disk caches return with the next
        // login), and the connect screen comes back, now and on the next launch.
        self.subs.groups = Load::Idle;
        self.subs.videos = Load::Idle;
        self.playlists.groups = Load::Idle;
        self.playlists.videos = Load::Idle;
        self.yt_history = Load::Idle;
        self.recs = Load::Idle;
        self.feed = Load::Idle;
        self.watch_later = Load::Idle;
        // No open channels behind the tabs that just disappeared.
        self.subs.open = None;
        self.playlists.open = None;
        self.tab = match self.tab {
            Tab::Playlists | Tab::History => Tab::Subscriptions,
            t => t,
        };
        // The logged-out home, so the left list is ready behind the connect screen.
        self.load_anon(cx);
        self.settings.welcome_seen = false;
        self.settings.save();
        self.connect = Connect::Idle;
        self.import_msg = Some((true, word.to_string()));
        self.sync_embed();
        cx.notify();
    }

    /// Back to browser selection after a failed attempt (app-managed login only).
    pub(crate) fn choose_another_browser(&mut self, cx: &mut Context<Self>) {
        Auth::clear();
        self.reload_auth(cx);
        self.connect = Connect::Idle;
        cx.notify();
    }

    /// Validate a cookies.txt and keep a private 0600 copy of it as the login.
    pub(crate) fn import_cookies(&mut self, cx: &mut Context<Self>) {
        let given = self.import_path.trim();
        if given.is_empty() {
            return;
        }
        if self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some() {
            self.import_msg = Some((false, "config.toml sets a login, which wins over this one: remove it there first.".into()));
            cx.notify();
            return;
        }
        let path = match given.strip_prefix("~/") {
            Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(given),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                self.import_msg = Some((false, format!("{}: {e}", path.display())));
                cx.notify();
                return;
            }
        };
        if let Err(e) = Account::from_cookies(&text) {
            self.import_msg = Some((false, format!("{e} — expected a Netscape cookies.txt from a browser logged in to YouTube")));
            cx.notify();
            return;
        }
        let dest = Auth::imported_cookies_path();
        let _ = std::fs::create_dir_all(store::config_dir());
        // Created 0600 from the start, so the session is never readable by others.
        let copy = platform::fs::private_file()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest)
            .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
        if let Err(e) = copy {
            self.import_msg = Some((false, format!("{}: {e}", dest.display())));
            cx.notify();
            return;
        }
        Auth::CookiesFile(dest.clone()).save();
        self.reload_auth(cx);
        self.connect = Connect::Idle;
        self.import_path.clear();
        self.import_msg = Some((true, format!("YouTube cookies imported to {}", dest.display())));
        cx.notify();
    }

    /// Fetch whether the current video is liked and its channel subscribed.
    pub(crate) fn load_status(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        // Also for a video that came without its channel (a Short from YouTube's history): the
        // watch page names it.
        let no_channel = self.cfg.has_auth() && self.current.as_ref().is_some_and(|v| v.channel_url.is_none());
        if !(self.account_buttons() || self.info_wanted() || no_channel) || self.status_requested.as_ref() == Some(&id) {
            return;
        }
        self.status_requested = Some(id.clone());
        let vid = id.clone();
        self.with_account(cx, move |a| a.status(&vid), move |this, res, _| match res {
            Ok(st) if this.current.as_ref().is_some_and(|v| v.id == id) => {
                this.fill_channel(&id, &st);
                this.status = Some((id, st));
            }
            Ok(_) => {}
            Err(e) => this.notice = Some(e),
        });
    }

    /// Fetch the playing video's like and dislike counts once (Return YouTube Dislike).
    pub(crate) fn load_votes(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if !self.settings.show_votes || self.votes.as_ref().is_some_and(|(v, _)| *v == id) {
            return;
        }
        self.votes = Some((id.clone(), None));
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::votes(&id) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                // Quietly nothing on a failure: the counts are a nicety.
                if let (Ok(counts), Some((v, slot))) = (res, this.votes.as_mut()) {
                    if *v == id {
                        *slot = Some(counts);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn current_status(&self) -> Option<&VideoStatus> {
        let (id, st) = self.status.as_ref()?;
        (self.current.as_ref()?.id == *id).then_some(st)
    }

    pub(crate) fn toggle_subscribe(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(channel) = st.channel_id.clone() else { return };
        let on = !st.subscribed;
        if let Some((_, s)) = &mut self.status {
            s.subscribed = on;
        }
        self.with_account(cx, move |a| a.subscribe(&channel, on), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) if on => "Subscribed".into(),
                Ok(()) => "Unsubscribed".into(),
                Err(e) => {
                    if let Some((_, s)) = &mut this.status {
                        s.subscribed = !on;
                    }
                    e
                }
            });
        });
    }

    pub(crate) fn toggle_like(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let on = !st.liked;
        if let Some((_, s)) = &mut self.status {
            s.liked = on;
            // YouTube keeps one rating: liking replaces a dislike.
            s.disliked &= !on;
        }
        self.with_account(cx, move |a| a.like(&id, on), move |this, res, _| match res {
            Ok(()) => this.notice = Some(if on { "Liked".into() } else { "Like removed".into() }),
            Err(e) => {
                if let Some((_, s)) = &mut this.status {
                    s.liked = !on;
                }
                this.notice = Some(e);
            }
        });
    }

    pub(crate) fn toggle_dislike(&mut self, cx: &mut Context<Self>) {
        let Some(st) = self.current_status().cloned() else { return };
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let on = !st.disliked;
        if let Some((_, s)) = &mut self.status {
            s.disliked = on;
            s.liked &= !on;
        }
        self.with_account(cx, move |a| a.dislike(&id, on), move |this, res, _| match res {
            Ok(()) => this.notice = Some(if on { "Disliked".into() } else { "Dislike removed".into() }),
            Err(e) => {
                if let Some((_, s)) = &mut this.status {
                    s.disliked = !on;
                }
                this.notice = Some(e);
            }
        });
    }

    pub(crate) fn save_to(&mut self, playlist: Group, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.close_save(cx);
        self.save_video_to(video, playlist, cx);
    }

    /// Add any video to a playlist (Watch later too, which also updates its tab's list).
    pub(crate) fn save_video_to(&mut self, video: Video, playlist: Group, cx: &mut Context<Self>) {
        if playlist.id == "WL" {
            self.add_to_watch_later(video, cx);
            return;
        }
        let id = video.id;
        self.notice = Some(format!("Saving to {}…", playlist.title));
        let pid = playlist.id.clone();
        self.with_account(cx, move |a| a.save_to_playlist(&pid, &id), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) => format!("Saved to {}", playlist.title),
                Err(e) => e,
            });
        });
    }

    /// Remove `video` from `playlist` on YouTube (Liked videos: unlike it), then from the open list.
    pub(crate) fn remove_from_playlist(&mut self, playlist: Group, video: Video, cx: &mut Context<Self>) {
        self.notice = Some(format!("Removing from {}…", playlist.title));
        let (pid, vid) = (playlist.id.clone(), video.id.clone());
        self.with_account(
            cx,
            move |a| if pid == "LL" { a.like(&vid, false) } else { a.remove_from_playlist(&pid, &vid) },
            move |this, res, _| {
                this.notice = Some(match res {
                    Ok(()) => {
                        if playlist.id == "WL" {
                            if let Load::Ready(v) | Load::Loading(v) = &mut this.watch_later {
                                v.retain(|x| x.id != video.id);
                                store::save_list("watch-later", v);
                            }
                        }
                        if let Load::Ready(v) | Load::Loading(v) = &mut this.playlists.videos {
                            v.retain(|x| x.id != video.id);
                            if this.playlists.open.as_ref().is_some_and(|g| g.id == playlist.id) {
                                store::save_list(&format!("group-{}", playlist.id), v);
                            }
                        }
                        format!("Removed from {}", playlist.title)
                    }
                    Err(e) => e,
                });
            },
        );
    }

    /// Copy the current video's link (Share button, C, or yy in Vim mode).
    pub(crate) fn copy_link(&mut self, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.copy_video_link(&video, cx);
    }

    pub(crate) fn copy_video_link(&mut self, video: &Video, cx: &mut Context<Self>) {
        let link = format!("https://youtu.be/{}", video.id);
        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
        self.notice = Some(format!("Link copied: {link}"));
        cx.notify();
    }

    /// Drop `id` from the history list here. YouTube's own history can't be changed from the app,
    /// so an entry that came from it is hidden until the next refresh brings it back.
    pub(crate) fn forget_video(&mut self, id: &str, cx: &mut Context<Self>) {
        self.history.items.retain(|w| w.video.id != id);
        self.history.save();
        let from_youtube = self.yt_history.items().iter().any(|v| v.id == id);
        if let Load::Ready(v) | Load::Loading(v) = &mut self.yt_history {
            v.retain(|x| x.id != id);
        }
        store::save_list("history", self.yt_history.items());
        self.notice = Some(if from_youtube {
            "Removed here. It stays in your YouTube history and may come back on refresh.".into()
        } else {
            "Removed from history".into()
        });
        cx.notify();
    }

    /// Add the playing video to Watch later (the button and the `w` key).
    pub(crate) fn watch_later(&mut self, cx: &mut Context<Self>) {
        if let Some(v) = self.current.clone() {
            self.add_to_watch_later(v, cx);
        }
    }

    /// Add any video to Watch later (the row buttons, the player button and the key), and to the
    /// Watch later tab's list if that is loaded.
    pub(crate) fn add_to_watch_later(&mut self, video: Video, cx: &mut Context<Self>) {
        self.notice = Some("Adding to Watch later…".into());
        let vid = video.id.clone();
        self.with_account(cx, move |a| a.save_to_playlist("WL", &vid), move |this, res, _| {
            this.notice = Some(match res {
                Ok(()) => {
                    if let Load::Ready(v) | Load::Loading(v) = &mut this.watch_later {
                        if !v.iter().any(|x| x.id == video.id) {
                            v.insert(0, video.clone());
                        }
                        store::save_list("watch-later", v);
                    }
                    "Added to Watch later".to_string()
                }
                Err(e) => e,
            });
        });
    }

    /// Watch later as a group, for the remove button on its rows.
    pub(crate) fn watch_later_group() -> Group {
        Group { id: "WL".into(), title: "Watch later".into(), url: ":ytwatchlater".into(), thumb: None, subscribers: None }
    }

    /// The Watch later tab is on and you are logged in.
    pub(crate) fn watch_later_on(&self) -> bool {
        self.settings.watch_later_tab && self.cfg.has_auth()
    }

    pub(crate) fn open_in_browser(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.current.as_ref().map(|v| v.url()) else { return };
        self.open_url(&url, cx);
    }

    /// Open a link in the default browser.
    pub(crate) fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        if let Err(e) = platform::desktop::open_url(url) {
            self.notice = Some(format!("Cannot open the browser: {e}"));
        }
        cx.notify();
    }

    /// Copy a link that opens at the current playback position (the resume position when not playing).
    pub(crate) fn copy_link_at_time(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        let secs = self.state.as_ref().map_or_else(|| self.history.position(&id), |s| s.position) as u64;
        let link = if secs > 0 { format!("https://youtu.be/{id}?t={secs}") } else { format!("https://youtu.be/{id}") };
        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
        self.notice = Some(format!("Link copied at {}: {link}", fmt_duration(secs as f64)));
        cx.notify();
    }
}
