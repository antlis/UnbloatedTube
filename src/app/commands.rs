use super::*;

impl Unbloated {
    /// A command from the command line (see backend/cli.rs); the answer it prints.
    pub(crate) fn run_command(&mut self, command: cli::Command, window: &mut Window, cx: &mut Context<Self>) -> cli::Reply {
        use cli::Command::*;
        let paused = self.casting.as_ref().map_or_else(
            || self.state.as_ref().filter(|s| s.playing).map(|s| s.paused),
            |c| c.status.as_ref().filter(|s| s.playing).map(|s| s.paused),
        );
        let nothing = || Err("nothing is playing".to_string());
        match command {
            Open { url } => {
                let link = yt::parse_link(&url).ok_or_else(|| format!("not a YouTube link: {url}"))?;
                // The link replaces the last-watched video, so loading that first would only flash it.
                self.preloaded = true;
                self.open_link(link, cx);
                window.activate_window();
                Ok("Opening".into())
            }
            Queue { url } => {
                let link = yt::parse_video_link(&url).ok_or_else(|| format!("not a YouTube video link: {url}"))?;
                let (cfg, id) = (self.cfg.clone(), link.id);
                let task = blocking::unblock(move || { yt::video(&cfg, &id) });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |this, cx| match res {
                        Ok(video) => this.enqueue(video, cx),
                        Err(e) => {
                            this.notice = Some(format!("Couldn't add the link to Up next: {e}"));
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .detach();
                Ok("Adding to Up next".into())
            }
            Pause | Play | Toggle => {
                let Some(now) = paused else { return nothing() };
                if matches!((&command, now), (Pause, false) | (Play, true) | (Toggle, _)) {
                    self.pause_toggle(cx);
                }
                Ok(if matches!((&command, now), (Pause, _) | (Toggle, false)) { "Paused" } else { "Playing" }.into())
            }
            Next | Prev => {
                if paused.is_none() && self.current.is_none() {
                    return nothing();
                }
                self.next_prev(matches!(command, Next), cx);
                Ok("OK".into())
            }
            Seek { secs, relative } => {
                if paused.is_none() {
                    return nothing();
                }
                if relative { self.seek_by(secs, cx) } else { self.seek_to(secs.max(0.), cx) }
                Ok("OK".into())
            }
            Status => Ok(self.status_line()),
            Raise => {
                window.activate_window();
                Ok("OK".into())
            }
            Quit => {
                self.quit_cast(false, window);
                Ok("Closing".into())
            }
        }
    }

    /// A media key or the desktop's player widget (MPRIS): like the command line, except that
    /// play on a video that isn't loaded starts it.
    pub(crate) fn run_mpris(&mut self, command: cli::Command, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(command, cli::Command::Play | cli::Command::Toggle) && self.mpris_snapshot().0.status == "Stopped" {
            if let Some(v) = self.current.clone() {
                return self.play(v, None, cx);
            }
        }
        let _ = self.run_command(command, window, cx);
    }

    /// What plays, for MPRIS (the receiver's video while casting), and the position.
    pub(crate) fn mpris_snapshot(&self) -> (mpris::Snapshot, f64) {
        let Some(v) = &self.current else { return (mpris::Snapshot { status: "Stopped", ..Default::default() }, 0.) };
        let playing = match &self.casting {
            Some(c) => c.status.as_ref().filter(|s| s.playing).map(|s| (s.paused, s.position, s.duration)),
            None => self.state.as_ref().filter(|s| s.playing && s.path == self.media_path(v)).map(|s| (s.paused, s.position, s.duration)),
        };
        let (status, position, duration) = match playing {
            Some((paused, position, duration)) => (if paused { "Paused" } else { "Playing" }, position, duration),
            None => ("Stopped", 0., 0.),
        };
        let now = mpris::Snapshot {
            status,
            id: v.id.clone(),
            title: v.title.clone(),
            channel: v.channel.clone().unwrap_or_default(),
            url: v.url(),
            art: v.thumb_url(),
            duration: if duration > 0. { duration } else { v.duration.unwrap_or(0.) },
        };
        (now, position)
    }

    /// What `unbloatedtube status` prints.
    pub(crate) fn status_line(&self) -> String {
        let (title, state, position, duration, target) = match &self.casting {
            Some(c) => match c.status.as_ref().filter(|s| s.playing) {
                Some(s) => (Some(s.title.clone()), if s.paused { "paused" } else { "playing" }, s.position, s.duration, Some(c.name.clone())),
                None => (None, "idle", 0., 0., Some(c.name.clone())),
            },
            None => match self.state.as_ref().filter(|s| s.playing) {
                Some(s) => (self.current.as_ref().map(|v| v.title.clone()), if s.paused { "paused" } else { "playing" }, s.position, s.duration, None),
                None => (None, "idle", 0., 0., None),
            },
        };
        let mut lines = vec![format!("state: {state}")];
        lines.extend(title.map(|t| format!("title: {t}")));
        if state != "idle" {
            lines.push(format!("position: {} / {}", fmt_duration(position), fmt_duration(duration)));
        }
        lines.extend(target.map(|t| format!("casting to: {t}")));
        lines.push(format!("up next: {}", self.up_next.len()));
        lines.join("\n")
    }
}
