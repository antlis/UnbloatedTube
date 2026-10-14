use super::*;

impl Unbloated {
    /// The subtitle languages to look for: the setting, or the system language ("en" if unknown).
    pub(crate) fn sub_langs(&self) -> String {
        let setting = self.settings.sub_lang.trim();
        if !setting.is_empty() {
            return setting.to_string();
        }
        std::env::var("LANG")
            .ok()
            .and_then(|l| l.split(['_', '.']).next().map(str::to_string))
            .filter(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()))
            .unwrap_or_else(|| "en".into())
    }

    /// The caption languages and kind for video `id`: a pick from the player menu, else
    /// Settings → Subtitles.
    pub(crate) fn video_sub_langs(&self, id: &str) -> (String, bool) {
        match &self.sub_pick {
            Some((v, code, auto)) if v == id => (code.clone(), *auto),
            _ => (self.sub_langs(), self.settings.sub_auto),
        }
    }

    /// Ask yt-dlp which caption tracks the current video has (once per video).
    pub(crate) fn load_caption_langs(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if self.caption_langs.as_ref().is_some_and(|(v, r)| *v == id && !matches!(r, Some(Err(_)))) {
            return;
        }
        self.caption_langs = Some((id.clone(), None));
        let cfg = self.cfg.clone();
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::caption_langs(&cfg, &id) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if let Some((_, slot)) = this.caption_langs.as_mut().filter(|(v, _)| *v == id) {
                    *slot = Some(res);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The player menu's subtitle list: Off, the video's own tracks, its generated one, and
    /// generated translations into the languages of Settings → Subtitles (YouTube offers ~100;
    /// those are the ones you read). Label, choice, ticked.
    pub(crate) fn sub_choices(&self) -> Vec<(String, SubChoice, bool)> {
        let Some(video) = &self.current else { return Vec::new() };
        let on = self.state.as_ref().is_some_and(|s| s.sub_on);
        let picked = self.sub_pick.as_ref().filter(|(v, _, _)| *v == video.id).map(|(_, c, a)| (c.clone(), *a));
        let mut out = vec![("Off".to_string(), SubChoice::Off, !on)];
        match self.caption_langs.as_ref().filter(|(v, _)| *v == video.id).map(|(_, r)| r) {
            Some(Some(Ok(langs))) => {
                let mut push = |label: String, code: &str, auto: bool| {
                    let ticked = on && picked.as_ref().is_some_and(|(c, a)| c == code && *a == auto);
                    out.push((label, SubChoice::Lang(code.to_string(), auto), ticked));
                };
                for l in langs.iter().filter(|l| !l.auto) {
                    push(l.name.clone(), &l.code, false);
                }
                for l in langs.iter().filter(|l| l.auto && l.code.ends_with("-orig")) {
                    push(format!("{} (auto-generated)", l.name.trim_end_matches(" (Original)")), &l.code, true);
                }
                let has_auto = langs.iter().any(|l| l.auto);
                for want in self.sub_langs().split(',').map(str::trim) {
                    let own = langs.iter().any(|l| !l.auto && l.code == want);
                    if let Some(l) = langs.iter().find(|l| has_auto && l.auto && l.code == want).filter(|_| !own) {
                        push(format!("{} (auto-translated)", l.name), &l.code, true);
                    }
                }
                if out.len() == 1 {
                    out.push(("No captions for this video".into(), SubChoice::Info, false));
                }
            }
            Some(Some(Err(e))) => out.push((e.clone(), SubChoice::Info, false)),
            _ => out.push(("Loading languages…".into(), SubChoice::Info, false)),
        }
        out
    }

    /// Show the captions picked in the player menu for the current video (fetched like the
    /// subtitles), or hide them.
    pub(crate) fn pick_sub(&mut self, choice: SubChoice, label: String, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        match choice {
            SubChoice::Off => {
                self.player.show_subtitles(false, None);
                self.notice = Some("Subtitles off".into());
            }
            SubChoice::Lang(code, auto) => {
                self.sub_pick = Some((id.clone(), code, auto));
                self.subs_none = None;
                self.subs_pending = None;
                self.subs_wanted = Some(id);
                // The Transcript tab follows the pick.
                self.transcript_for = None;
                self.notice = Some(format!("Subtitles: {label}"));
            }
            SubChoice::Info => {}
        }
        cx.notify();
    }

    /// Download the captions of video `id` (yt-dlp, cached); they are added to mpv by `poll_subtitles`.
    pub(crate) fn fetch_subtitles(&mut self, id: String, cx: &mut Context<Self>) {
        let (cfg, (langs, auto)) = (self.cfg.clone(), self.video_sub_langs(&id));
        let dir = store::cache_dir().join("captions");
        // The first version cached the captions as YouTube writes them, in a "subs" folder.
        let _ = std::fs::remove_dir_all(store::cache_dir().join("subs"));
        self.subs_loading = Some(id.clone());
        self.subs_none = None;
        let task = {
            let id = id.clone();
            blocking::unblock(move || { yt::subtitles(&cfg, &id, &langs, auto, &dir) })
        };
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |this, cx| {
                if this.subs_loading.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.subs_loading = None;
                let current = this.current.as_ref().is_some_and(|v| v.id == id);
                match res {
                    Ok(Some(file)) if current => this.subs_pending = Some((id, file)),
                    Ok(None) => this.subs_none = Some(id),
                    Err(e) if current => this.notice = Some(format!("Subtitles: {e}")),
                    _ => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Start the caption download a new video asked for, and give downloaded captions to mpv
    /// once it has that video open.
    pub(crate) fn poll_subtitles(&mut self, state: Option<&player::State>, cx: &mut Context<Self>) {
        if let Some(id) = self.subs_wanted.take() {
            self.fetch_subtitles(id, cx);
        }
        let Some((id, file)) = self.subs_pending.clone() else { return };
        let Some(video) = self.current.as_ref().filter(|v| v.id == id) else {
            self.subs_pending = None;
            return;
        };
        if state.is_some_and(|s| s.playing && s.path == self.media_path(video)) {
            self.player.add_subtitle(&file);
            self.subs_pending = None;
        }
    }

    /// The CC button / V: subtitles on or off for the playing video. With none yet it turns the
    /// setting on and fetches them.
    pub(crate) fn toggle_subtitles(&mut self, cx: &mut Context<Self>) {
        let (tracks, on, track) = self.state.as_ref().map_or((0, false, None), |s| (s.sub_tracks, s.sub_on, s.sub_id));
        let Some(id) = self.current.as_ref().map(|v| v.id.clone()) else { return };
        if tracks > 0 {
            self.player.show_subtitles(!on, track);
        } else if self.subs_loading.is_some() || self.subs_pending.is_some() {
            self.notice = Some("The subtitles are still loading…".into());
        } else if !self.settings.subtitles {
            self.settings.subtitles = true;
            self.settings.save();
            self.notice = Some("Subtitles on".into());
            self.subs_wanted = Some(id);
        } else if self.subs_none.as_deref() == Some(id.as_str()) {
            self.notice = Some(format!("No \"{}\" subtitles for this video", self.sub_langs()));
        } else {
            self.subs_wanted = Some(id);
        }
        cx.notify();
    }
}
