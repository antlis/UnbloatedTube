use super::*;

impl Unbloated {
    /// Cast a playlist without opening it: its first 2000 videos are listed, then sent in chunks.
    pub(crate) fn cast_playlist(&mut self, group: Group, target: String, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        // The one that is open is already loaded.
        if self.playlists.open.as_ref().is_some_and(|g| g.id == group.id) {
            if let Load::Ready(v) = &self.playlists.videos {
                let videos = self.visible(v);
                self.cast_list(Some(target), videos, cx);
                return;
            }
        }
        self.notice = Some(format!("Loading {}…", group.title));
        let (cfg, url) = (self.cfg.clone(), group.url.clone());
        let task = blocking::unblock(move || {
            let mut out = Vec::new();
            yt::playlist_head(&cfg, &url, cast::MAX_LIST, &mut |v| out.push(v)).map(|_| out)
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| match res {
                Ok(videos) if !videos.is_empty() => {
                    let videos = this.visible(&videos);
                    this.cast_list(Some(target), videos, cx);
                }
                Ok(_) => {
                    this.notice = Some(format!("{} has no videos", group.title));
                    cx.notify();
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

    /// Whether the Cast button shows: a target in config.toml, and the button on in Settings.
    pub(crate) fn cast_available(&self) -> bool {
        self.settings.cast_button && !self.cast_targets().is_empty()
    }

    /// Where the video can be cast: the command from Settings, which wins, else config.toml's targets.
    pub(crate) fn cast_targets(&self) -> std::collections::BTreeMap<String, store::CastTarget> {
        let command = cast::split_command(&self.settings.cast_command);
        if command.is_empty() {
            return self.cfg.cast.clone();
        }
        [("device".to_string(), store::CastTarget { command, ..Default::default() })].into()
    }

    /// Send the playing video to a cast target (the first one for the T key).
    pub(crate) fn cast_to(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        self.cast_list(name, vec![video], cx);
    }

    /// Send `videos` to a cast target, the first now and the rest after it. A target with a `url`
    /// is asked over the remote API (a list becomes the receiver's queue, 200 videos at most) and
    /// then controlled from here; a command target gets the first video only. Either way happens
    /// on a background thread, then the local video pauses.
    pub(crate) fn cast_list(&mut self, name: Option<String>, videos: Vec<Video>, cx: &mut Context<Self>) {
        let Some(video) = videos.first().cloned() else { return };
        let targets = self.cast_targets();
        let Some((name, target)) = name
            .and_then(|n| targets.get(&n).map(|t| (n, t.clone())))
            .or_else(|| targets.iter().next().map(|(n, t)| (n.clone(), t.clone())))
        else {
            self.notice = Some("No cast target: add [cast.<name>] to config.toml".into());
            cx.notify();
            return;
        };
        // Where the first one is now (the saved position unless it is the one playing here).
        let live = self.current.as_ref().is_some_and(|c| c.id == video.id).then(|| self.state.as_ref().map(|s| s.position)).flatten();
        let start = live.unwrap_or_else(|| self.history.position(&video.id)).max(0.) as u64;
        let title = video.title.clone();
        let remote = target.url.as_deref().map(|u| cast::Remote::new(u, target.token.as_deref().unwrap_or_default()));
        let command = cast::expand(&target.command, &cast::Playing { url: &video.url(), id: &video.id, title: &title, start });
        // The receiver holds 2000 at most, and takes 200 per request.
        let items: Vec<Video> = videos.iter().take(cast::MAX_LIST).cloned().collect();
        let urls: Vec<String> = items.iter().take(cast::CHUNK).map(|v| v.url()).collect();
        let sent = urls.len();
        let listed = remote.is_some() && items.len() > 1;
        self.notice = Some(format!("Casting to {name}…"));
        let task_remote = remote.clone();
        let items_len = items.len();
        let task = blocking::unblock(move || {
            match task_remote {
                Some(r) if items_len > 1 => r.play_list(&urls, 0, start),
                Some(r) => r.play(&urls[0], start),
                None => cast::run(&command, Duration::from_secs(60)),
            }
        });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => {
                        this.notice = Some(format!("Sent to {name}"));
                        // Otherwise it plays here too.
                        this.player.pause();
                        this.casting = remote.map(|remote| Casting {
                            name,
                            remote,
                            status: None,
                            seen: false,
                            started: Instant::now(),
                            polling: false,
                            listed,
                            items: items.clone(),
                            marked: Default::default(),
                            sent,
                        });
                        // A command target can't be asked what plays: its one video counts now.
                        if this.casting.is_none() {
                            this.mark_watched(&items[0].id, cx);
                        }
                        this.sync_embed();
                    }
                    // A video picked in the cast view didn't go out: the receiver still plays
                    // the old one, which the view no longer describes.
                    Err(e) if this.casting.as_ref().is_some_and(|c| c.polling && c.status.is_none()) => {
                        this.end_cast(&format!("Cast to {name} failed: {e}"), cx)
                    }
                    Err(e) => this.notice = Some(format!("Cast to {name} failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Send the next chunk of a long list when the receiver's queue is within 50 of its end.
    pub(crate) fn refill_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.casting.as_mut().filter(|c| c.listed) else { return };
        let Some((pos, _)) = c.status.as_ref().and_then(|s| s.queue) else { return };
        let Some(range) = cast::next_chunk(pos, c.sent, c.items.len()) else { return };
        let chunk: Vec<String> = c.items[range].iter().map(|v| v.url()).collect();
        let (remote, before) = (c.remote.clone(), c.sent);
        c.sent += chunk.len();
        let task = blocking::unblock(move || { remote.queue_add(&chunk) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                this.update(cx, |this, cx| {
                    // The rest isn't sent (an older bot has no /queue): the list ends where it was.
                    if let Some(c) = this.casting.as_mut() {
                        c.items.truncate(before);
                        c.sent = before;
                    }
                    this.notice = Some(format!("Couldn't add more of the list: {e}"));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Record `id` in the account's YouTube history (a background yt-dlp call); only when logged in.
    pub(crate) fn mark_watched(&mut self, id: &str, _cx: &mut Context<Self>) {
        if !self.cfg.has_auth() {
            return;
        }
        let (cfg, id) = (self.cfg.clone(), id.to_string());
        blocking::unblock(move || {
                if let Err(e) = yt::mark_watched(&cfg, &id) {
                    eprintln!("unbloatedtube: couldn't add {id} to the history: {e}");
                }
            })
            .detach();
    }

    /// Play what Next plays here (Up next first, then the list), and say so when it came from Up
    /// next: that is the part that looks like a bug when you were going through a playlist.
    pub(crate) fn poll_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.casting.as_mut().filter(|c| !c.polling) else { return };
        c.polling = true;
        let (remote, started) = (c.remote.clone(), c.started);
        let task = blocking::unblock(move || { remote.status() });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                // Gone, or another video was cast meanwhile: this answer is about the old one.
                let Some(c) = this.casting.as_mut().filter(|c| c.started == started) else { return };
                c.polling = false;
                let alive = matches!(&res, Ok(s) if s.playing || s.queue.is_some());
                if let Ok(s) = res {
                    c.seen |= s.playing || s.queue.is_some();
                    c.status = Some(s);
                }
                // The video that just started counts as watched: it goes in the YouTube history.
                let now = c.status.as_ref().filter(|s| s.playing).map(|s| s.queue.map_or(0, |(i, _)| i));
                let fresh = now.filter(|i| *i < c.items.len() && c.marked.insert(*i)).map(|i| c.items[i].id.clone());
                if let Some(id) = fresh {
                    this.mark_watched(&id, cx);
                }
                // The main video area follows the receiver: title, picture and details of the
                // video that plays there. (Like playing it here, it leaves Up next.)
                let shown = now.and_then(|i| this.casting.as_ref().and_then(|c| c.items.get(i).cloned()));
                if let Some(v) = shown.filter(|v| this.current.as_ref().is_none_or(|c| c.id != v.id)) {
                    if let Some(i) = this.up_next.iter().position(|q| q.id == v.id) {
                        this.up_next.remove(i);
                        store::save_data("up_next", &this.up_next);
                    }
                    this.history.touch(&v);
                    this.current = Some(v);
                    cx.notify();
                }
                this.refill_cast(cx);
                let Some(c) = this.casting.as_mut() else { return };
                if (c.seen && !alive) || (!c.seen && c.started.elapsed() > Duration::from_secs(90)) {
                    this.end_cast("Cast ended", cx);
                } else {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Leave the cast view and show the local (still paused) video again.
    pub(crate) fn end_cast(&mut self, notice: &str, cx: &mut Context<Self>) {
        self.casting = None;
        // The main area followed the receiver; an mpv holding another video is dropped, so
        // pressing play starts the one shown (a paused leftover would show the wrong picture).
        if let (Some(s), Some(v)) = (&self.state, &self.current) {
            if s.path != self.media_path(v) {
                self.player.stop();
                self.state = None;
            }
        }
        self.notice = Some(notice.to_string());
        self.sync_embed();
        cx.notify();
    }

    /// Close the window; while casting, ask first whether the receiver should stop too.
    pub(crate) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.casting.is_some() {
            self.quit_prompt = true;
            cx.notify();
        } else {
            window.remove_window();
        }
    }

    /// The close dialog's answer: stop the receiver (briefly waiting for it) or leave it playing.
    pub(crate) fn quit_cast(&mut self, stop: bool, window: &mut Window) {
        if let (true, Some(c)) = (stop, &self.casting) {
            let _ = c.remote.stop_within(Duration::from_secs(2));
        }
        self.casting = None;
        window.remove_window();
    }

    /// Stop the receiver and leave the cast view.
    pub(crate) fn stop_cast(&mut self, cx: &mut Context<Self>) {
        let Some(c) = &self.casting else { return };
        let remote = c.remote.clone();
        blocking::unblock(move || { remote.ctl(serde_json::json!({ "action": "stop" })) }).detach();
        self.end_cast("Stopped casting", cx);
    }

    /// Send a control to the receiver and show its effect at once; the next poll corrects it.
    pub(crate) fn cast_ctl(&mut self, body: serde_json::Value, cx: &mut Context<Self>) {
        let Some(c) = &self.casting else { return };
        let remote = c.remote.clone();
        let task = blocking::unblock(move || { remote.ctl(body) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                this.update(cx, |this, cx| {
                    this.notice = Some(format!("Cast: {e}"));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
