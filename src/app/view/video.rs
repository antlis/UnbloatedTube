use super::*;

impl Unbloated {
    pub(crate) fn render_player(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let Some(video) = self.current.clone() else {
            return div().p_4().text_sm().text_color(themed(MUTED)).child("Pick a video on the left.");
        };
        let resume = self.history.position(&video.id);
        let screen = self.screen(&video, false, cx);
        let cast_status = self.casting.as_ref().map(|c| c.status.clone());
        let (pos, dur, paused, active) = match (&cast_status, &self.state) {
            (Some(Some(c)), _) => (c.position, c.duration, c.paused, true),
            (Some(None), _) => (resume, video.duration.unwrap_or(0.), true, false),
            (None, Some(s)) => (s.position, s.duration, s.paused, true),
            (None, None) => (resume, video.duration.unwrap_or(0.), true, false),
        };
        let filled = if dur > 0. { ((pos / dur) * SEEK_SEGMENTS as f64) as usize } else { 0 };
        let chapters = self.state.as_ref().filter(|_| self.casting.is_none()).map(|s| s.chapters.clone()).unwrap_or_default();

        let (play_icon, play_tip) = match (active, paused) {
            (true, true) => ("play", "Play (Space)".to_string()),
            (true, false) => ("pause", "Pause (Space)".to_string()),
            (false, _) if resume > 0. => ("play", format!("Resume at {}", fmt_duration(resume))),
            (false, _) => ("play", "Play".to_string()),
        };
        let time = if self.loading {
            String::new()
        } else {
            format!("{} / {}", fmt_duration(pos), fmt_duration(dur))
        };
        let replay = video.clone();
        let info = {
            let st = self.current_status();
            let parts: Vec<String> = [
                st.and_then(|s| s.views.clone()).filter(|_| self.settings.show_views),
                st.and_then(|s| s.date.clone()).filter(|_| self.settings.show_date),
                self.votes
                    .as_ref()
                    .filter(|(id, _)| self.settings.show_votes && *id == video.id)
                    .and_then(|(_, c)| *c)
                    .map(|(likes, dislikes)| format!("{} likes  ·  {} dislikes", fmt_count(likes), fmt_count(dislikes))),
            ]
            .into_iter()
            .flatten()
            .collect();
            div()
                .flex()
                .items_center()
                .text_xs()
                .text_color(themed(MUTED))
                .when(video.live, |d| d.child(div().mr_2().child(live_badge(true))))
                .child(parts.join("  ·  "))
                // A live stream's position and length mean little: the badge says it.
                .child(div().ml_auto().pl_2().flex_none().child(if video.live { String::new() } else { time }))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(screen)
            .child({
                // A DeArrow title, with YouTube's own under it, small.
                let (title, original) = self.shown_title(&video, cx);
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_color(themed(TEXT)).line_clamp(2).child(self.linked_text(format!("title-{}", video.id), title, cx)))
                    .when_some(original, |d, o| d.child(div().text_xs().text_color(themed(MUTED)).truncate().child(format!("Original title: {o}"))))
            })
            .child(info)
            .child({
                let name = video.channel.clone().unwrap_or_default();
                let subs = self.current_status().and_then(|s| s.subscribers.clone()).filter(|_| self.settings.show_subs);
                match channel_group(&video) {
                    // Styled as a chip (avatar, name, chevron) so it reads as clickable.
                    Some(g) => {
                        // Avatar from your subscriptions, else from YouTube's info on this video.
                        let thumb = self
                            .subs
                            .groups
                            .items()
                            .iter()
                            .find(|s| s.id == g.id)
                            .and_then(|s| s.thumb.clone())
                            .or_else(|| self.current_status().and_then(|st| st.avatar.clone()));
                        let avatar = self.thumb_el(&g.id, thumb, 20., 20., px(10.), cx);
                        div().flex().child(
                            div()
                                .id("channel-link")
                                .flex()
                                .items_center()
                                .gap_2()
                                .pl_1()
                                .pr_3()
                                .py_1()
                                .rounded_full()
                                .bg(themed(HOVER))
                                .text_xs()
                                .text_color(themed(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(themed(BORDER)))
                                .child(avatar)
                                .child(name)
                                .children(subs.map(|s| div().text_color(themed(MUTED)).child(s)))
                                .child(div().text_color(themed(MUTED)).child("›"))
                                .tooltip(tip("Show this channel's videos"))
                                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.show_channel(g.clone(), cx)),
                        )
                    }
                    None => div().text_xs().text_color(themed(MUTED)).child(name),
                }
            })
            .child(if self.loading {
                div().h(px(10.)).py(px(3.)).child(loading_bar("video-loading")).into_any_element()
            } else {
                div()
                    .flex()
                    .h(px(10.))
                    .items_center()
                    .children((0..SEEK_SEGMENTS).map(|i| {
                        let step = dur / SEEK_SEGMENTS as f64;
                        let at = step * i as f64;
                        // Hover shows the time there and the chapter it falls in; chapter starts
                        // get a small gap so the bar shows where chapters begin.
                        let chapter = chapters.iter().rev().find(|(t, _)| *t <= at).map(|(_, title)| title.clone());
                        let starts_chapter = i > 0 && chapters.iter().any(|(t, _)| *t > 0. && *t >= at && *t < at + step);
                        let tooltip = match chapter.filter(|c| !c.is_empty()) {
                            Some(c) => format!("{} · {c}", fmt_duration(at)),
                            None => fmt_duration(at),
                        };
                        div()
                            .id(("seek", i))
                            .flex_1()
                            .h_full()
                            .py(px(3.))
                            .when(starts_chapter, |d| d.ml(px(3.)))
                            .cursor_pointer()
                            .child(div().size_full().bg(if i < filled { themed(ACCENT) } else { themed(BORDER) }))
                            .when(dur > 0., |d| d.tooltip(tip(tooltip)))
                            .when(active, |d| {
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    this.seek_to(dur * i as f64 / SEEK_SEGMENTS as f64, cx)
                                }))
                            })
                    }))
                    .into_any_element()
            })
            // With the hover bar on the video, playback controls live there instead.
            .when(!self.settings.video_controls, |d| d.child(
                div()
                    .flex()
                    .items_center()
                    .child(icon_button("play", play_icon, play_tip, true).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        if this.casting.is_some() {
                            this.pause_toggle(cx);
                        } else if this.state.is_some() {
                            this.player.toggle_pause();
                        } else {
                            this.play(replay.clone(), None, cx);
                        }
                    }))
                    .child(self.step_button("prev", "prev", "Previous (P)", -1, cx))
                    .child(self.step_button("next", "next", "Next (N)", 1, cx))
                    .child(
                        icon_button("back10", "back", "Back 10 seconds (J)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(-10., cx)),
                    )
                    .child(
                        icon_button("fwd10", "forward", "Forward 10 seconds (L)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(10., cx)),
                    )
                    .child(
                        icon_button("pip", "pip", if self.pip { "Back into the app" } else { "Picture-in-picture" }, active || self.pip)
                            .when(self.pip, |d| d.bg(themed(ACCENT)))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_pip(cx)),
                    )
                    .child(
                        icon_button("full", "fullscreen", "Fullscreen (F, Esc to leave)", active)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.set_fullscreen(true)),
                    )
                    .child(
                        div()
                            .id("speed")
                            .mr_2()
                            .h(px(30.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(themed(HOVER))
                            .text_xs()
                            .text_color(themed(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(BORDER)))
                            .child(format!("{}×", self.settings.speed))
                            .tooltip(tip("Playback speed (click to change)"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.cycle_speed(cx)),
                    )
                    .when(self.settings.volume_control, |d| d.child(self.volume_bar(cx)))
            ))
            // Second row: what you can do with this video.
            .when(self.account_buttons() || self.cast_available() || self.cc_under_player() || self.settings.share_button || self.settings.share_time_button || self.settings.browser_button || self.settings.download_button || self.settings.history_buttons, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                            .when(self.settings.history_buttons, |d| {
                                let (back, next) = self.nav_targets();
                                let idle = self.casting.is_none();
                                let (can_back, can_next) = (idle && back.is_some(), idle && next.is_some());
                                let title = |i: Option<usize>| i.and_then(|i| self.nav.get(i)).map(|v| v.title.clone());
                                d.child(
                                    icon_button("nav-back", "arrow-left", format!("Back: {} (Alt+←)", title(back).unwrap_or_else(|| "nothing earlier".into())), can_back)
                                        .when(can_back, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.nav_go(false, cx))),
                                )
                                .child(
                                    icon_button("nav-forward", "arrow-right", format!("Forward: {} (Alt+→)", title(next).unwrap_or_else(|| "nothing later".into())), can_next)
                                        .when(can_next, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.nav_go(true, cx))),
                                )
                            })
                            .when(self.account_buttons(), |d| d.children(self.account_buttons_els(cx)))
                            .when(self.settings.download_button, |d| {
                                let saved = self.library.iter().find(|s| s.video.id == video.id).map(|s| s.path.clone());
                                let (tip_text, enabled) = match self.downloads.get(&video.id) {
                                    _ if saved.is_some() => (format!("Downloaded to {}", saved.unwrap_or_default()), false),
                                    Some(Download { result: None, progress, .. }) => (format!("Downloading {progress:.0}%"), false),
                                    Some(Download { result: Some(Ok(f)), .. }) => (format!("Downloaded to {f}"), false),
                                    _ => ("Download".to_string(), true),
                                };
                                let v = video.clone();
                                d.child(
                                    icon_button("download", "download", tip_text, enabled)
                                        .when(enabled, |d| d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.download(v.clone(), cx))),
                                )
                            })
                            .when(self.cast_available(), |d| {
                                let targets = self.cast_targets();
                                let many = targets.len() > 1;
                                d.children(targets.into_keys().enumerate().map(|(i, name)| {
                                    let target = name.clone();
                                    icon_button(("cast", i), "cast", format!("Cast to {name}{}", if i == 0 && !many { " (T)" } else { "" }), true)
                                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.cast_to(Some(target.clone()), cx))
                                }))
                            })
                            .when(self.cc_under_player(), |d| {
                                let on = self.state.as_ref().is_some_and(|s| s.sub_on);
                                let key = if self.settings.vim { "v" } else { "V" };
                                let tip_text = format!("Subtitles {} ({key})", if on { "on" } else { "off" });
                                d.child(
                                    icon_button("subtitles", if on { "cc" } else { "cc-off" }, tip_text, true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_subtitles(cx)),
                                )
                            })
                            .when(self.settings.share_button, |d| {
                                let key = if self.settings.vim { "yy" } else { "C" };
                                d.child(
                                    icon_button("share", "share", format!("Copy link ({key})"), true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.copy_link(cx)),
                                )
                            })
                            .when(self.settings.share_time_button, |d| {
                                let key = if self.settings.vim { "yt" } else { "⇧C" };
                                d.child(
                                    icon_button("share-time", "recent", format!("Copy link at the current time ({key})"), true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.copy_link_at_time(cx)),
                                )
                            })
                            .when(self.settings.browser_button, |d| {
                                d.child(
                                    icon_button("browser", "browser", if self.settings.vim { "Open in browser (o)" } else { "Open in browser (O)" }, true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.open_in_browser(cx)),
                                )
                            })
                )
            })
    }

    /// Mute button and a ten-step volume bar (click a step to set the volume).
    pub(crate) fn volume_bar(&self, cx: &mut Context<Self>) -> gpui::Div {
        let muted = self.state.as_ref().is_some_and(|s| s.muted);
        let volume = self.settings.volume;
        let filled = if muted { 0 } else { (volume / 10.).round() as usize };
        let silent = muted || volume == 0.;
        div()
            .flex()
            .items_center()
            .mr_2()
            .child(
                icon_button("mute", if silent { "volume-off" } else { "volume" }, if muted { "Unmute (M)" } else { "Mute (M)" }, true)
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, _| this.player.toggle_mute()),
            )
            .child(div().flex().items_center().h(px(30.)).children((1..=10usize).map(|i| {
                div()
                    .id(("volume", i))
                    .h_full()
                    .px(px(1.5))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(div().w(px(5.)).h(px(14.)).rounded(px(1.)).bg(if i <= filled { themed(ACCENT) } else { themed(BORDER) }))
                    .tooltip(tip(format!("Volume {}%", i * 10)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_volume(i as f32 * 10., cx)))
            })))
    }

    /// Subscribe / Save / Like icons, as enabled in settings. Greyed until the account's
    /// state for this video is known.
    pub(crate) fn account_buttons_els(&mut self, cx: &mut Context<Self>) -> Vec<Stateful<gpui::Div>> {
        let st = self.current_status().cloned();
        let ready = st.is_some();
        let s = st.unwrap_or_default();
        let set = &self.settings;
        let mut out = Vec::new();
        if set.subscribe_button && (!ready || s.channel_id.is_some()) {
            let (icon, tip) = if s.subscribed { ("subscribed", "Unsubscribe") } else { ("subscribe", "Subscribe") };
            out.push(
                icon_button("subscribe", icon, tip, ready)
                    .when(ready && !s.subscribed, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_subscribe(cx))),
            );
        }
        if set.save_button {
            out.push(icon_button("save", "save", "Save to playlist", ready).when(ready, |d| {
                d.on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                    if this.saving { this.close_save(cx) } else { this.open_save(window, cx) }
                })
            }));
        }
        if set.watch_later_button {
            out.push(icon_button("watch-later", "watch-later", "Watch later (W)", ready).when(ready, |d| {
                d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.watch_later(cx))
            }));
        }
        if set.like_button {
            let (icon, tip) = if s.liked { ("liked", "Remove like") } else { ("like", "Like") };
            out.push(
                icon_button("like", icon, tip, ready)
                    // Red while liked, so the state is obvious.
                    .when(ready && s.liked, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_like(cx))),
            );
        }
        if set.dislike_button {
            let (icon, tip) = if s.disliked { ("disliked", "Remove dislike") } else { ("dislike", "Dislike") };
            out.push(
                icon_button("dislike", icon, tip, ready)
                    .when(ready && s.disliked, |d| d.bg(themed(ACCENT)))
                    .when(ready, |d| d.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_dislike(cx))),
            );
        }
        out
    }

    /// Prev / Next: play the neighbouring video of the queue; greyed out at its ends.
    pub(crate) fn step_button(
        &self,
        id: &'static str,
        icon: &'static str,
        tip: &'static str,
        step: isize,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        // Casting a list: these drive the receiver's queue.
        if self.casting.as_ref().is_some_and(|c| c.listed) {
            return icon_button(id, icon, format!("{tip} (on the receiver)"), true)
                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.next_prev(step > 0, cx));
        }
        let target = if step > 0 { self.next_video() } else { self.neighbor(step) };
        let queued = step > 0 && !self.up_next.is_empty();
        let tip = match &target {
            Some(v) if queued => format!("{tip}, from Up next: {}", v.title),
            Some(v) => format!("{tip}: {}", v.title),
            None => tip.to_string(),
        };
        icon_button(id, icon, tip, target.is_some()).when_some(target, |d, video| {
            d.on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), None, cx))
        })
    }
}
