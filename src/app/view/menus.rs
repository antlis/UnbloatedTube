use super::*;

impl Unbloated {
    /// The right-click menu of a playlist: Play here, and Cast (one row per target). A backdrop closes it.
    pub(crate) fn render_playlist_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (group, pos, _) = self.playlist_menu.clone()?;
        let targets = self.cast_targets();
        let many = targets.len() > 1;
        const MENU_W: f32 = 220.;
        let win = window.viewport_size();
        let win_w = f32::from(win.width);
        let column_right = if self.left_collapsed || self.right_collapsed { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        let top = f32::from(pos.y).min(f32::from(win.height) - 48. - 28. - 36. * (targets.len() + 9) as f32).max(8.);
        let play_group = group.clone();
        let (rename, describe, delete) = (group.clone(), group.clone(), group.clone());
        // Watch later and Liked videos can't be renamed, deleted or changed in this way.
        let editable = !matches!(group.id.as_str(), "WL" | "LL");
        let row = |id: &'static str| div().id(id).px_3().py_2().text_sm().cursor_pointer().text_color(themed(TEXT)).hover(|d| d.bg(themed(BORDER)));
        let privacy_rows: Vec<_> = [("Make public", "PUBLIC"), ("Make unlisted", "UNLISTED"), ("Make private", "PRIVATE")]
            .into_iter()
            .filter(|(_, privacy)| group.privacy.as_deref().map(str::to_uppercase).as_deref() != Some(*privacy))
            .map(|(label, privacy)| {
                let g = group.clone();
                row(privacy).child(label).on_click(cx.listener(move |this, _, _, cx| this.set_playlist_privacy(g.clone(), privacy, cx)))
            })
            .collect();
        let confirming = self.confirm_delete_playlist.as_deref() == Some(group.id.as_str());
        let rows: Vec<_> = targets
            .into_keys()
            .enumerate()
            .map(|(i, name)| {
                let (g, target) = (group.clone(), name.clone());
                div()
                    .id(("playlist-menu-cast", i))
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(if many { format!("Cast to {name}") } else { "Cast".to_string() })
                    .on_click(cx.listener(move |this, _, _, cx| this.cast_playlist(g.clone(), target.clone(), cx)))
            })
            .collect();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("playlist-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| { this.playlist_menu = None; cx.notify(); }))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| { this.playlist_menu = None; cx.notify(); })),
                )
                .child(
                    div()
                        .id("playlist-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, at))) = (*hovered, &mut this.playlist_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(MENU_W))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(group.title.clone()))
                        .child(
                            div()
                                .id("playlist-menu-play")
                                .px_3()
                                .py_2()
                                .text_sm()
                                .cursor_pointer()
                                .text_color(themed(TEXT))
                                .hover(|d| d.bg(themed(BORDER)))
                                .child("Play")
                                .on_click(cx.listener(move |this, _, _, cx| this.play_playlist(play_group.clone(), cx))),
                        )
                        .children(rows)
                        .when(editable, |d| {
                            d.child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                                .child(row("playlist-menu-rename").child("Rename").on_click(cx.listener(move |this, _, window, cx| {
                                    this.edit_playlist_name(PlaylistEdit::Rename(rename.id.clone()), rename.title.clone(), window, cx)
                                })))
                                .child(row("playlist-menu-describe").child("Edit description").on_click(cx.listener(move |this, _, window, cx| {
                                    this.edit_playlist_name(PlaylistEdit::Describe(describe.id.clone()), String::new(), window, cx)
                                })))
                                .children(privacy_rows)
                                .child(
                                    row("playlist-menu-delete")
                                        .text_color(if confirming { themed(ACCENT) } else { themed(TEXT) })
                                        .child(if confirming { "Click again to delete" } else { "Delete" })
                                        .on_click(cx.listener(move |this, _, _, cx| this.delete_playlist(delete.clone(), cx))),
                                )
                        }),
                ),
        )
    }

    /// The right-click menu of a video: copy link, Up next, Watch later, then the playlists in a
    /// scrolling list. A backdrop closes it.
    pub(crate) fn render_video_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (video, pos, over_player, _) = self.video_menu.clone()?;
        let auth = self.cfg.has_auth();
        let queued = self.up_next.iter().any(|v| v.id == video.id);
        let downloaded = self.library.iter().any(|s| s.video.id == video.id);
        let row = |id: &'static str| div().id(id).px_3().py_2().text_sm().cursor_pointer().text_color(themed(TEXT)).hover(|d| d.bg(themed(BORDER)));
        // Ticked when the video is already in the playlist; clicking toggles, like the groups menu.
        let lists = self.menu_lists.as_ref().filter(|(id, _)| *id == video.id).and_then(|(_, l)| l.clone());
        let loading = lists.is_none();
        let list_rows: Vec<_> = lists
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, o)| {
                let (v, member) = (video.clone(), o.contains);
                let pl = Group { id: o.id.clone(), title: o.title.clone(), url: String::new(), thumb: None, subscribers: None, privacy: None };
                div()
                    .id(("video-menu-playlist", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(member, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().truncate().child(o.title.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        if member {
                            this.remove_from_playlist(pl.clone(), v.clone(), cx);
                        } else {
                            this.save_video_to(v.clone(), pl.clone(), cx);
                        }
                    }))
            })
            .collect();
        let n_lists = list_rows.len();
        // Fixed rows: title, Copy link, Up next, [Delete download], [Watch later, divider, "Save to playlist"]. The
        // list shows at most 5½ rows (the cut-off one says it scrolls), less if the window is short.
        const MENU_W: f32 = 240.;
        const ROW_H: f32 = 36.;
        const EDGE: f32 = 48.;
        let fixed_h = 28. + ROW_H * if downloaded { 3. } else { 2. } + if self.menu_queue.is_some() && self.cast_available() { ROW_H * self.cast_targets().len() as f32 } else { 0. } + if auth { ROW_H + 9. + 24. } else { 0. } + if self.player_menu { ROW_H * (8. + if self.quality_menu { 10. } else { 0. } + if self.sleep_menu { 7. } else { 0. } + if self.sub_menu { self.sub_choices().len() as f32 } else { 0. }) + 9. } else { 0. };
        let win = window.viewport_size();
        let (win_w, mut win_h) = (f32::from(win.width), f32::from(win.height));
        if let Some(display) = window.display(cx) {
            let top_on_screen = f32::from(window.window_bounds().get_bounds().origin.y);
            win_h = win_h.min(f32::from(display.bounds().size.height) - top_on_screen);
        }
        let list_max = (ROW_H * 5.5).min(win_h - fixed_h - EDGE).max(ROW_H);
        let list_h = if auth { (n_lists as f32 * ROW_H).max(ROW_H).min(list_max) } else { 0. };
        let column_right = if over_player { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        // Too short a window for every row: the menu itself scrolls (the playlists already
        // shrank to one row above).
        let avail = (win_h - EDGE - 8.).max(ROW_H * 4.);
        let top = f32::from(pos.y).min(win_h - (fixed_h + list_h).min(avail) - EDGE).max(8.);
        let (v_copy, v_queue, v_later, v_separate) = (video.clone(), video.clone(), video.clone(), video.clone());
        let player_menu = self.player_menu;
        let speed = self.settings.speed;
        // Quality: only for a video streaming here (not a downloaded file, not while casting).
        let streaming = self.casting.is_none() && self.current.as_ref().is_some_and(|v| self.media_path(v) == v.url());
        let picked = self.picked_quality();
        let quality_label = match (picked, self.state.as_ref().and_then(|s| s.height)) {
            (Some(0), _) => "audio only".to_string(),
            (_, Some(h)) => format!("{h}p"),
            _ => "…".to_string(),
        };
        // Subtitles: the language list (see `sub_choices`); the row says what shows now.
        let sub_choices = if self.sub_menu { self.sub_choices() } else { Vec::new() };
        let sub_label = match &self.sub_pick {
            _ if !self.state.as_ref().is_some_and(|s| s.sub_on) => "off".to_string(),
            Some((v, code, _)) if self.current.as_ref().is_some_and(|c| c.id == *v) => code.clone(),
            _ => "on".to_string(),
        };
        let sub_rows: Vec<_> = sub_choices
            .into_iter()
            .enumerate()
            .map(|(i, (label, choice, ticked))| {
                let info = choice == SubChoice::Info;
                div()
                    .id(("video-menu-sub-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(if info { themed(MUTED) } else { themed(TEXT) })
                    .when(!info, |d| d.cursor_pointer().hover(|d| d.bg(themed(BORDER))))
                    .child(div().w(px(14.)).flex_none().when(ticked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().min_w_0().truncate().child(label.clone()))
                    .when(!info, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.pick_sub(choice.clone(), label.clone(), cx);
                        }))
                    })
            })
            .collect();
        // Sleep timer: off, end of this video (not while casting: the receiver goes on by
        // itself), or minutes; ticked: the end-of-video choice when set.
        let sleep_label = self.sleep_status().map_or("off".to_string(), |s| s.trim_start_matches("Sleep ").to_string());
        let sleep_choices: [(Option<u64>, &str); 7] =
            [(None, "Off"), (Some(0), "End of this video"), (Some(15), "15 min"), (Some(30), "30 min"), (Some(45), "45 min"), (Some(60), "60 min"), (Some(90), "90 min")];
        let sleep_rows: Vec<_> = sleep_choices
            .into_iter()
            .filter(|_| self.sleep_menu)
            .filter(|(m, _)| *m != Some(0) || self.casting.is_none())
            .enumerate()
            .map(|(i, (minutes, label))| {
                let ticked = match minutes {
                    None => self.sleep.is_none(),
                    Some(0) => self.sleep == Some(Sleep::EndOfVideo),
                    Some(_) => false,
                };
                div()
                    .id(("video-menu-sleep-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(ticked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        let sleep = minutes.map(|m| if m == 0 { Sleep::EndOfVideo } else { Sleep::At(Instant::now() + Duration::from_secs(m * 60)) });
                        this.set_sleep(sleep, cx);
                    }))
            })
            .collect();
        // The list: the settings' maximum, the usual heights, audio only; ticked: in use.
        let quality_rows: Vec<_> = [None, Some(2160), Some(1440), Some(1080), Some(720), Some(480), Some(360), Some(240), Some(144), Some(0)]
            .into_iter()
            .filter(|_| self.quality_menu)
            .enumerate()
            .map(|(i, q)| {
                let label = match q {
                    None => format!("Auto (up to {}p)", self.settings.max_quality),
                    Some(0) => "Audio only".to_string(),
                    Some(h) => format!("{h}p"),
                };
                div()
                    .id(("video-menu-quality-option", i))
                    .pl_6()
                    .pr_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(q == picked, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_video_menu(cx);
                        this.pick_quality(q, cx);
                    }))
            })
            .collect();
        // One row per cast target: this video and the ones after it in its list.
        let rest: Option<Vec<Video>> = self
            .menu_queue
            .as_ref()
            .filter(|_| self.cast_available())
            .map(|q| q.iter().skip_while(|v| v.id != video.id).cloned().collect::<Vec<_>>())
            .filter(|r| !r.is_empty());
        let cast_rows: Vec<_> = rest
            .map(|rest| {
                let targets = self.cast_targets();
                let many = targets.len() > 1;
                let label = if rest.len() > 1 { "from here" } else { "this video" };
                targets
                    .into_keys()
                    .enumerate()
                    .map(|(i, name)| {
                        let (rest, target) = (rest.clone(), name.clone());
                        div()
                            .id(("video-menu-cast", i))
                            .px_3()
                            .py_2()
                            .text_sm()
                            .cursor_pointer()
                            .text_color(themed(TEXT))
                            .hover(|d| d.bg(themed(BORDER)))
                            .child(if many { format!("Cast {label} to {name}") } else { format!("Cast {label}") })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.cast_list(Some(target.clone()), rest.clone(), cx);
                            }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("video-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_video_menu(cx)))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| this.close_video_menu(cx))),
                )
                .child(
                    div()
                        .id("video-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .max_h(px(avail))
                        .overflow_y_scroll()
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, _, at))) = (*hovered, &mut this.video_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(MENU_W))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(video.title.clone()))
                        .when(player_menu, |d| {
                            d.child(row("video-menu-time").child("Copy link at current time").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.copy_link_at_time(cx);
                            })))
                            .child(row("video-menu-loop").child("Loop").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.player.toggle_loop();
                            })))
                            .child(row("video-menu-speed").child(format!("Speed: {speed}×, next")).on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.cycle_speed(cx);
                            })))
                            .when(streaming, |d| {
                                d.child(row("video-menu-quality").child(format!("Quality: {quality_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                    this.quality_menu = !this.quality_menu;
                                    cx.notify();
                                })))
                                .children(quality_rows)
                            })
                            .child(row("video-menu-sleep").child(format!("Sleep: {sleep_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                this.sleep_menu = !this.sleep_menu;
                                cx.notify();
                            })))
                            .children(sleep_rows)
                            .child(row("video-menu-subs").child(format!("Subtitles: {sub_label} ›")).on_click(cx.listener(|this, _, _, cx| {
                                this.sub_menu = !this.sub_menu;
                                if this.sub_menu {
                                    this.load_caption_langs(cx);
                                }
                                cx.notify();
                            })))
                            .children(sub_rows)
                            .child(row("video-menu-stats").child("Stats for nerds").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.player.toggle_stats();
                            })))
                            .child(row("video-menu-browser").child("Open in browser").on_click(cx.listener(|this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.open_in_browser(cx);
                            })))
                            .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                        })
                        .children(cast_rows)
                        .child(row("video-menu-separate").child("Play in separate window").on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.play_separate(&v_separate, cx);
                        })))
                        .child(row("video-menu-copy").child("Copy link").on_click(cx.listener(move |this, _, _, cx| {
                            this.close_video_menu(cx);
                            this.copy_video_link(&v_copy, cx);
                        })))
                        .child(
                            row("video-menu-queue").child(if queued { "Remove from Up next" } else { "Add to Up next" }).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.close_video_menu(cx);
                                    if queued {
                                        this.dequeue(&v_queue.id, cx);
                                    } else {
                                        this.enqueue(v_queue.clone(), cx);
                                    }
                                },
                            )),
                        )
                        .when(downloaded, |d| {
                            let id = video.id.clone();
                            d.child(row("video-menu-delete-download").child("Delete downloaded file").on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.delete_download(&id, cx);
                            })))
                        })
                        .when(auth, |d| {
                            d.child(row("video-menu-later").child("Save to Watch later").on_click(cx.listener(move |this, _, _, cx| {
                                this.close_video_menu(cx);
                                this.add_to_watch_later(v_later.clone(), cx);
                            })))
                            .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                            .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("Save to playlist"))
                            .child(div().id("video-menu-playlists").max_h(px(list_max)).overflow_y_scroll().children(list_rows))
                            .when(n_lists == 0, |d| {
                                d.child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).child(if loading { "Loading playlists…" } else { "No playlists" }))
                            })
                        }),
                ),
        )
    }

    /// The right-click menu of a subscription: a backdrop that closes it and the menu at the click.
    pub(crate) fn render_channel_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (g, pos, _) = self.channel_menu.clone()?;
        let confirming = self.confirm_unsub.as_deref() == Some(g.id.as_str());
        // One row per group, ticked when the channel is in it; clicking toggles.
        let mut group_rows = Vec::new();
        for (i, grp) in self.groups.iter().enumerate() {
            let member = grp.channels.iter().any(|c| *c == g.id);
            let (name, ch) = (grp.name.clone(), g.id.clone());
            group_rows.push(
                div()
                    .id(("channel-menu-group", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .cursor_pointer()
                    .text_color(themed(TEXT))
                    .hover(|d| d.bg(themed(BORDER)))
                    .child(div().w(px(14.)).flex_none().when(member, |d| d.child(svg().path(icons::path("check")).size(px(14.)).text_color(themed(ACCENT)))))
                    .child(div().truncate().child(grp.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_membership(&name, &ch, cx))),
            );
        }
        let no_groups = group_rows.is_empty();
        // Fixed rows: channel name, Unsubscribe, divider, "Add to group". The group list shows at
        // most 5½ rows (the cut-off one says it scrolls), less if the window is short, and the menu
        // is moved up/left to stay inside the window, with a margin: windows can run past the screen.
        const MENU_W: f32 = 220.;
        const FIXED_H: f32 = 28. + 36. + 9. + 24.;
        const ROW_H: f32 = 36.;
        const EDGE: f32 = 48.;
        let win = window.viewport_size();
        let (win_w, mut win_h) = (f32::from(win.width), f32::from(win.height));
        // The window can be taller than the screen (minimum size, display scaling): use the part
        // of it that is visible. The bounds' origin is the window's position on the screen.
        if let Some(display) = window.display(cx) {
            let top_on_screen = f32::from(window.window_bounds().get_bounds().origin.y);
            win_h = win_h.min(f32::from(display.bounds().size.height) - top_on_screen);
        }
        let list_max = (ROW_H * 5.5).min(win_h - FIXED_H - EDGE).max(ROW_H);
        let list_h = (group_rows.len() as f32 * ROW_H).max(ROW_H).min(list_max);
        // Stay inside the left column: the video is a native window drawn over everything on its side.
        let column_right = if self.left_collapsed || self.right_collapsed { win_w } else { self.settings.split * win_w };
        let left = f32::from(pos.x).min(column_right - MENU_W - 8.).max(8.);
        // Too short a window for every row: the menu itself scrolls too.
        let avail = (win_h - EDGE - 8.).max(ROW_H * 4.);
        let top = f32::from(pos.y).min(win_h - (FIXED_H + list_h).min(avail) - EDGE).max(8.);
        fn close(this: &mut Unbloated, cx: &mut Context<Unbloated>) {
            this.channel_menu = None;
            this.confirm_unsub = None;
            cx.notify();
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    div()
                        .id("channel-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| close(this, cx)))
                        .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| close(this, cx))),
                )
                .child(
                    div()
                        .id("channel-menu")
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .max_h(px(avail))
                        .overflow_y_scroll()
                        .occlude()
                        .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                            this.menu_hovered = *hovered;
                            if let (false, Some((_, _, at))) = (*hovered, &mut this.channel_menu) {
                                *at = Instant::now();
                            }
                        }))
                        .w(px(220.))
                        .py_1()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_color(themed(BORDER))
                        .shadow_lg()
                        .child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).truncate().child(g.title.clone()))
                        .child(
                            div()
                                .id("channel-menu-unsub")
                                .px_3()
                                .py_2()
                                .text_sm()
                                .cursor_pointer()
                                .text_color(if confirming { themed(ACCENT) } else { themed(TEXT) })
                                .hover(|d| d.bg(themed(BORDER)))
                                .child(if confirming { "Click again to unsubscribe" } else { "Unsubscribe" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_channel_sub(g.clone(), true, cx);
                                    // The second click did it; the first only asked for confirmation.
                                    if this.confirm_unsub.is_none() {
                                        this.channel_menu = None;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(div().my_1().h(px(1.)).bg(themed(BORDER)))
                        .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("Add to group"))
                        .child(
                            div()
                                .id("channel-menu-groups")
                                .max_h(px(list_max))
                                .overflow_y_scroll()
                                .children(group_rows),
                        )
                        .when(no_groups, |d| d.child(div().px_3().py_1().text_xs().text_color(themed(MUTED)).child("No groups yet: make one with + Group"))),
                ),
        )
    }
}
