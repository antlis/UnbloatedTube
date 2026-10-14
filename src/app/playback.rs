use super::*;

impl Unbloated {
    /// Move the video into / out of its own always-on-top window (restarts mpv in place).
    /// Right click → Play in separate window: the video in an mpv window of its own, from where
    /// you left it; the app's player keeps what it plays.
    pub(crate) fn play_separate(&mut self, video: &Video, cx: &mut Context<Self>) {
        let start = self.history.position(&video.id);
        self.notice = Some(match player::play_separate(&self.cfg, &self.settings, &self.media_path(video), start) {
            Ok(()) => format!("Playing \"{}\" in a separate window", video.title),
            Err(e) => e,
        });
        cx.notify();
    }

    pub(crate) fn toggle_pip(&mut self, cx: &mut Context<Self>) {
        self.pip = !self.pip;
        if let (true, Some(video)) = (self.player.alive(), self.current.clone()) {
            let paused = self.state.as_ref().is_some_and(|s| s.paused);
            self.start(video, paused);
        }
        self.sync_embed();
        cx.notify();
    }

    /// Show mpv's window only when it has something current to show and nothing covers it.
    pub(crate) fn sync_embed(&mut self) {
        let visible = self.player.alive()
            && !(self.loading && self.hide_while_loading)
            && !self.saving
            && !self.show_keys
            && !self.pip
            && !self.video_menu.as_ref().is_some_and(|m| m.2)
            && self.casting.is_none()
            && !self.lower_full
            && !self.right_collapsed
            // The connect screen is a GPUI overlay; the X11 child window would float above it.
            && !self.welcome_visible()
            // Audio only: keep showing the thumbnail.
            && !self.settings.audio_only
            && self.picked_quality() != Some(0);
        if let Some(e) = &self.embed {
            e.borrow_mut().set_visible(visible);
        }
    }

    /// Play `video`; `queue` is the list it was picked from (None keeps the current one).
    pub(crate) fn play(&mut self, video: Video, queue: Option<Arc<[Video]>>, cx: &mut Context<Self>) {
        // The old video's text is gone; a selection over it means nothing now.
        self.text_sel = None;
        if let Some(queue) = queue {
            self.queue = queue;
        }
        if !self.nav_moving {
            // Playing something new drops what "forward" would have gone to, as in a browser.
            let keep = self.nav_here().map_or(self.nav.len(), |p| p + 1);
            self.nav.truncate(keep);
            if self.nav.last().is_none_or(|v| v.id != video.id) {
                self.nav.push(video.clone());
            }
            self.nav.drain(..self.nav.len().saturating_sub(100));
            self.nav_pos = self.nav.len().checked_sub(1);
        }
        if let Some(i) = self.up_next.iter().position(|v| v.id == video.id) {
            self.up_next.remove(i);
            store::save_data("up_next", &self.up_next);
        }
        self.history.touch(&video);
        self.history.save();
        let channel_changed = self.current.as_ref().map(|c| &c.channel_url) != Some(&video.channel_url);
        if let Some(c) = self.casting.as_mut() {
            // While casting, a picked video goes to the receiver (which resolves it with its own
            // login) and the cast view stays. After "Back to this screen" videos play here again.
            // The cast starts over now: until the receiver plays the new video, the old one (or
            // nothing, while it loads) must neither be shown nor end the cast.
            // No polls (`polling`) until the receiver has taken it; see cast_list.
            let name = c.name.clone();
            *c = Casting { name: name.clone(), remote: c.remote.clone(), status: None, seen: false, started: Instant::now(), polling: true, listed: false, items: vec![video.clone()], marked: Default::default(), sent: 1 };
            self.cast_list(Some(name), vec![video.clone()], cx);
            self.current = Some(video);
        } else {
            self.start(video, false);
        }
        if !self.cfg.has_auth() && channel_changed {
            self.load_recs(cx);
        }
        // What Next would play, so it starts at once.
        if let Some(next) = self.next_video() {
            self.prefetch(&next.id, cx);
        }
        cx.notify();
    }

    /// Resolve a video in the background so playing it starts sooner (see backend/prefetch.rs).
    pub(crate) fn prefetch(&mut self, id: &str, cx: &mut Context<Self>) {
        let recent = |t: &Instant| t.elapsed() < prefetch::TTL - Duration::from_secs(600);
        if !self.settings.prefetch || self.current.as_ref().is_some_and(|c| c.id == id) || self.prefetching >= 2 || self.prefetched.get(id).is_some_and(recent) {
            return;
        }
        self.prefetched.insert(id.to_string(), Instant::now());
        self.prefetching += 1;
        let (cfg, format, id) = (self.cfg.clone(), player::format(&self.settings), id.to_string());
        let task = blocking::unblock(move || prefetch::fetch(&cfg, &id, &format));
        cx.spawn(async move |this, cx| {
            // A video whose lookup failed (members only, upcoming, removed…) isn't asked again
            // until the entry expires: it would fail again, seconds each time, over and over.
            let _ = task.await;
            this.update(cx, |this, _| this.prefetching -= 1).ok();
        })
        .detach();
    }

    /// The pointer entered or left a video row: resolve it once it has rested there a moment.
    pub(crate) fn prefetch_hover(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            if self.hover_video.as_deref() == Some(id) {
                self.hover_video = None;
            }
            return;
        }
        self.hover_video = Some(id.to_string());
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            this.update(cx, |this, cx| {
                if this.hover_video.as_deref() == Some(id.as_str()) {
                    this.prefetch(&id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The first rows of the list on screen, and the head of Up next: what is most likely played.
    pub(crate) fn prefetch_top(&mut self, cx: &mut Context<Self>) {
        if !self.settings.prefetch {
            return;
        }
        let mut ids: Vec<String> = self.up_next.first().map(|v| v.id.clone()).into_iter().collect();
        ids.extend(self.left_items().into_iter().filter_map(|i| if let Item::Video(v, _) = i { Some(v.id) } else { None }).take(3));
        for id in ids {
            self.prefetch(&id, cx);
        }
    }

    /// Vim mode: the same for the video the cursor rests on.
    pub(crate) fn prefetch_cursor(&mut self, cx: &mut Context<Self>) {
        let cursor = self.vim_cursor;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            this.update(cx, |this, cx| {
                if this.vim_cursor == cursor && this.settings.vim {
                    if let Some(Item::Video(v, _)) = this.left_items().get(cursor) {
                        let id = v.id.clone();
                        this.prefetch(&id, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Load `video` into mpv at its resume position.
    pub(crate) fn start(&mut self, video: Video, paused: bool) {
        if self.embed.is_none() {
            self.embed = Embed::new().map(|e| Rc::new(RefCell::new(e)));
            if self.embed.is_none() {
                eprintln!("unbloatedtube: can't embed video (not X11?), using a separate mpv window");
            }
        }
        let wid = self.embed.as_ref().filter(|_| !self.pip).map(|e| {
            // mpv keeps its window unmapped if ours is hidden when it starts.
            e.borrow_mut().set_visible(true);
            e.borrow().id()
        });
        self.casting = None;
        let start = self.link_start.take().unwrap_or_else(|| self.history.position(&video.id));
        self.subs_pending = None;
        self.subs_wanted = self.settings.subtitles.then(|| video.id.clone());
        let options = player::options(&self.cfg, &self.settings, self.pip);
        // Hide the old video's last frame only when mpv keeps running: a freshly started mpv
        // (first video, or changed options) never shows its picture if ours is hidden then.
        let restarts = self.player.options() != options.as_slice();
        self.hide_while_loading = !restarts;
        if let (true, Some(e)) = (self.hide_while_loading, &self.embed) {
            // Show the new thumbnail right away instead of the old video's last frame.
            e.borrow_mut().set_visible(false);
        }
        // A quality picked for another video is over; the running mpv gets this one's format (a
        // restarted mpv starts with the settings' one from `options`).
        if self.quality.as_ref().is_some_and(|(id, _)| *id != video.id) {
            self.quality = None;
        }
        // Load persisted per-video quality if no manual override is active.
        if self.quality.is_none() {
            if let Some(h) = self.settings.quality_override.get(&video.id) {
                if *h > 0 || self.settings.quality_override.contains_key(&video.id) {
                    self.quality = Some((video.id.clone(), *h));
                }
            }
        }
        if !restarts {
            self.player.set_format(&player::format_at(&self.settings, self.quality.as_ref().map(|(_, h)| *h)));
        }
        if let Err(e) = self.player.play(&options, &self.media_path(&video), start, self.settings.speed, wid, paused) {
            eprintln!("{e}");
        }
        self.saving = false;
        self.notice = None;
        self.current = Some(video);
        self.loading = true;
        self.load_started = Instant::now();
    }

    /// A video that plays without moving: mpv shows a black picture at 0:00 when YouTube's video
    /// server drops the connection (or stops mid-video when it stops sending). After STALL, its
    /// kept lookup is dropped and it is loaded once more at the same place, which usually gets
    /// another server; if that one doesn't move either, the notice says so.
    pub(crate) fn watch_stall(&mut self, state: Option<&player::State>, url: Option<&str>, cx: &mut Context<Self>) {
        const STALL: Duration = Duration::from_secs(10);
        let moving = state.filter(|s| s.playing && !s.paused && !s.ended && !s.idle && Some(s.path.as_str()) == url);
        let (Some(s), Some(video), None, false) = (moving, self.current.clone(), &self.casting, self.loading) else {
            self.stall = None;
            // If mpv went idle while we were loading (or just after), the streams likely failed
            // (e.g. 403 on cached URLs). Drop the prefetch cache and retry once.
            if let (Some(_), Some(video), None, false) = (
                state.filter(|s| s.idle && !s.ended),
                self.current.clone(),
                &self.casting,
                self.loading,
            ) {
                if self.stall_retried.as_ref().is_none_or(|(id, _)| id != &video.id) {
                    self.stall_retried = Some((video.id.clone(), true));
                    prefetch::forget(&video.id);
                    self.link_start = Some(0.);
                    self.start(video, false);
                    cx.notify();
                    return;
                }
            }
            // Also retry on mpv error (e.g. HTTP 403 on stale cached URLs).
            if let (Some(_), Some(video), None, false) = (
                state.filter(|s| s.error.as_ref().is_some_and(|e| e.contains("403"))),
                self.current.clone(),
                &self.casting,
                self.loading,
            ) {
                if self.stall_retried.as_ref().is_none_or(|(id, _)| id != &video.id) {
                    self.stall_retried = Some((video.id.clone(), true));
                    prefetch::forget(&video.id);
                    self.link_start = Some(0.);
                    self.start(video, false);
                    cx.notify();
                    return;
                }
            }
            return;
        };
        match self.stall {
            Some((since, at)) if (s.position - at).abs() < 0.25 => {
                if since.elapsed() < STALL {
                    return;
                }
                self.stall = None;
                match &mut self.stall_retried {
                    Some((id, told)) if *id == video.id => {
                        if !*told {
                            *told = true;
                            self.notice = Some("YouTube's video server isn't sending this video; try again in a moment".into());
                            cx.notify();
                        }
                    }
                    _ => {
                        self.stall_retried = Some((video.id.clone(), false));
                        prefetch::forget(&video.id);
                        self.link_start = Some(s.position);
                        self.start(video, false);
                        cx.notify();
                    }
                }
            }
            _ => self.stall = Some((Instant::now(), s.position)),
        }
    }

    /// What the sleep timer waits for, for the notice corner and the menu.
    pub(crate) fn sleep_status(&self) -> Option<String> {
        Some(match self.sleep? {
            Sleep::EndOfVideo => "Sleep after this video".to_string(),
            Sleep::At(t) => {
                let left = t.saturating_duration_since(Instant::now()).as_secs();
                if left < 60 { format!("Sleep in {left} s") } else { format!("Sleep in {} min", left.div_ceil(60)) }
            }
        })
    }

    /// Set the sleep timer (None: off).
    pub(crate) fn set_sleep(&mut self, sleep: Option<Sleep>, cx: &mut Context<Self>) {
        self.sleep = sleep;
        self.notice = Some(self.sleep_status().map_or("Sleep timer off".to_string(), |s| format!("Sleep timer: {}", s.trim_start_matches("Sleep "))));
        cx.notify();
    }

    /// The quality picked for the current video (a height, or 0 for audio only), if any.
    pub(crate) fn picked_quality(&self) -> Option<u32> {
        let id = &self.current.as_ref()?.id;
        self.quality
            .as_ref()
            .filter(|(q, _)| q == id)
            .map(|(_, h)| *h)
            .or_else(|| self.settings.quality_override.get(id).copied())
    }

    /// Play the current video again at another quality (None: the settings' maximum again), from
    /// where it is and paused or not as it was. Only for a video streaming here.
    pub(crate) fn pick_quality(&mut self, pick: Option<u32>, cx: &mut Context<Self>) {
        let Some(video) = self.current.clone() else { return };
        let Some((position, paused)) = self.state.as_ref().filter(|s| s.playing).map(|s| (s.position, s.paused)) else { return };
        // Persist the per-video quality choice.
        if let Some(h) = pick {
            self.settings.quality_override.insert(video.id.clone(), h);
        } else {
            self.settings.quality_override.remove(&video.id);
        }
        self.settings.save();
        self.quality = pick.map(|h| (video.id.clone(), h));
        self.link_start = Some(position);
        self.start(video, paused);
        // After `start`, which clears notices.
        self.notice = Some(match pick {
            None => format!("Quality: up to {}p (Settings)", self.settings.max_quality),
            Some(0) => "Quality: audio only".into(),
            Some(h) => format!("Quality: up to {h}p"),
        });
        self.sync_embed();
        cx.notify();
    }

    /// Next playback speed in SPEEDS, wrapping around.
    pub(crate) fn cycle_speed(&mut self, cx: &mut Context<Self>) {
        let i = SPEEDS.iter().position(|s| *s == self.settings.speed).map_or(1, |i| (i + 1) % SPEEDS.len());
        self.settings.speed = SPEEDS[i];
        self.settings.save();
        self.player.set_speed(self.settings.speed);
        cx.notify();
    }

    /// Play or pause: the receiver while casting, else the local player.
    pub(crate) fn pause_toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(st) = self.casting.as_mut().and_then(|c| c.status.as_mut()) {
            st.paused = !st.paused;
            self.cast_ctl(serde_json::json!({ "action": "toggle" }), cx);
            cx.notify();
        } else if self.casting.is_none() {
            self.player.toggle_pause();
            if let Some(st) = self.state.as_mut() {
                st.paused = !st.paused;
            }
        }
    }

    /// Seek by `secs`: the receiver while casting, else the local player.
    pub(crate) fn seek_by(&mut self, secs: f64, cx: &mut Context<Self>) {
        match self.casting.as_ref().and_then(|c| c.status.as_ref()) {
            Some(st) => self.seek_to((st.position + secs).max(0.), cx),
            None if self.casting.is_none() => self.player.seek_relative(secs),
            None => {}
        }
    }

    /// Seek to `secs`: the receiver while casting, else the local player.
    pub(crate) fn seek_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        if let Some(st) = self.casting.as_mut().and_then(|c| c.status.as_mut()) {
            let secs = if st.duration > 0. { secs.min(st.duration) } else { secs };
            st.position = secs;
            self.cast_ctl(serde_json::json!({ "action": "seek", "position": secs }), cx);
            cx.notify();
        } else if self.casting.is_none() {
            self.player.seek_absolute(secs);
        }
    }

    /// Change the volume by `delta` percent (unmuting when raising it).
    pub(crate) fn change_volume(&mut self, delta: f32, cx: &mut Context<Self>) {
        if delta > 0. && self.state.as_ref().is_some_and(|s| s.muted) {
            self.player.toggle_mute();
        }
        self.set_volume(self.settings.volume + delta, cx);
    }

    pub(crate) fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        let volume = volume.clamp(0., 100.).round();
        self.settings.volume = volume;
        self.settings.save();
        self.volume_set = Instant::now();
        self.player.set_volume(volume);
        if !self.settings.volume_control {
            self.notice = Some(format!("Volume {volume}%"));
        }
        cx.notify();
    }

    pub(crate) fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.casting.is_some() {
            self.pause_toggle(cx);
        } else if self.state.is_some() {
            self.player.toggle_pause();
        } else if let Some(v) = self.current.clone() {
            self.play(v, None, cx);
        }
    }

    /// Load the last-watched video paused, so Resume starts instantly. Only when it can play
    /// inside unbloatedtube; our X11 window may need a few ticks to show up in the WM's client list.
    pub(crate) fn preload(&mut self, cx: &mut Context<Self>) {
        if self.preloaded || self.player.alive() {
            self.preloaded = true;
            return;
        }
        let Some(video) = self.current.clone() else { return };
        if self.embed.is_none() {
            self.embed = Embed::new().map(|e| Rc::new(RefCell::new(e)));
        }
        if self.embed.is_some() {
            self.start(video, true);
            self.preloaded = true;
            cx.notify();
        } else if self.ticks > 20 {
            self.preloaded = true;
        }
    }
}
