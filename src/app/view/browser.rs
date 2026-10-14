use super::*;

impl Unbloated {
    /// Search mode with nothing searched yet (or the field cleared): recent queries.
    pub(crate) fn render_recent_searches(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.recent_searches.is_empty() {
            return self.status("Type and press Enter to search YouTube.");
        }
        div()
            .flex()
            .flex_col()
            .py_2()
            .child(div().px_3().pb_1().text_xs().text_color(themed(MUTED)).child("RECENT SEARCHES"))
            .children(self.recent_searches.iter().enumerate().map(|(i, q)| {
                let q = q.clone();
                div()
                    .id(("recent", i))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_sm()
                    .text_color(themed(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(themed(HOVER)))
                    .child(svg().path(icons::path("recent")).size(px(14.)).text_color(themed(MUTED)))
                    .child(q.clone())
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, window, cx| {
                        this.query = q.clone();
                        this.run_search(cx);
                        window.blur();
                    })
            }))
            .child(
                div()
                    .id("clear-recent")
                    .px_3()
                    .pt_2()
                    .text_xs()
                    .text_color(themed(MUTED))
                    .cursor_pointer()
                    .hover(|d| d.text_color(themed(TEXT)))
                    .child("Clear recent searches")
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        this.recent_searches.clear();
                        store::save_data("searches", &this.recent_searches);
                        cx.notify();
                    }),
            )
            .into_any_element()
    }

    /// The left list's filter field (Ctrl+F). Filters as you type; Esc clears.
    pub(crate) fn filter_field(&self, placeholder: &'static str, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let focused = self.filter_focus.is_focused(window);
        let empty = self.list_filter.is_empty();
        div()
            .id("list-filter")
            .track_focus(&self.filter_focus)
            .flex()
            .items_center()
            .gap_2()
            .min_w(px(160.))
            .px_2()
            .py(px(3.))
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
            .text_xs()
            .cursor_text()
            .child(svg().path(icons::path("search")).size(px(12.)).flex_none().text_color(themed(MUTED)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if empty { themed(MUTED) } else { themed(TEXT) })
                    .map(|d| match (empty, focused) {
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("filter", &self.list_filter, placeholder)),
                        (true, false) => d.child(format!("{placeholder} (Ctrl+F)")),
                        (false, false) => d.child(self.list_filter.clone()),
                    }),
            )
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.filter_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "filter", &mut this.list_filter, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.list_filter.clear();
                        window.blur();
                    }
                    Edit::Changed => {
                        // The list changed under the selection.
                        this.vim_cursor = 0;
                    }
                    Edit::Moved => {}
                    Edit::Ignored => {}
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    /// A focused text field's text with its cursor and selection; `placeholder` when empty.
    pub(crate) fn caret_text(&self, id: &'static str, text: &str, placeholder: &str) -> gpui::Div {
        let (pos, anchor) = self.caret.get(id, text);
        let (start, end) = (pos.min(anchor), pos.max(anchor));
        // Stretched to the line's height by the row.
        let bar = || div().flex_none().w(px(1.)).bg(themed(TEXT));
        let part = |t: &str| (!t.is_empty()).then(|| div().flex_none().child(t.to_string()));
        let row = div().flex().min_w_0().overflow_hidden().whitespace_nowrap();
        if text.is_empty() {
            return row.child(bar()).child(div().text_color(themed(MUTED)).child(placeholder.to_string()));
        }
        row.children(part(&text[..start]))
            .when(pos == start, |d| d.child(bar()))
            .children(part(&text[start..end]).map(|d| d.bg(gpui::rgba((ACCENT << 8) | 0x66))))
            .when(pos == end && start != end, |d| d.child(bar()))
            .children(part(&text[end..]))
    }

    /// A full-width bar holding the filter field.
    pub(crate) fn filter_bar(&self, placeholder: &'static str, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .child(self.filter_field(placeholder, window, cx).flex_1())
    }

    /// A round icon button for the channel header; red when `on`.
    pub(crate) fn icon_chip(&self, id: &'static str, icon: &str, on: bool, tooltip: impl Into<SharedString>) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .size(px(28.))
            .ml_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(if on { themed(ACCENT) } else { themed(HOVER) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(svg().path(icons::path(icon)).size(px(14.)).text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) }))
            .tooltip(tip_left(tooltip))
    }

    /// A small rounded chip (filter bar, group editor).
    pub(crate) fn chip(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .flex()
            .items_center()
            .px_3()
            .py(px(3.))
            .rounded_full()
            .text_xs()
            .bg(if on { themed(ACCENT) } else { themed(HOVER) })
            .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(label.into())
    }

    /// The "+ Group" chip, or the field for typing a new group's name.
    pub(crate) fn new_group_chip(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        match &self.new_group {
            Some(name) => {
                let focused = self.new_group_focus.is_focused(window);
                div()
                    .id("new-group-field")
                    .track_focus(&self.new_group_focus)
                    .flex_none()
                    .min_w(px(110.))
                    .px_3()
                    .py(px(3.))
                    .rounded_full()
                    .border_1()
                    .border_color(themed(MUTED))
                    .text_xs()
                    .text_color(if name.is_empty() && !focused { themed(MUTED) } else { themed(TEXT) })
                    .map(|d| if focused { d.child(self.caret_text("group", name, "Group name")) } else { d.child(if name.is_empty() { "Group name".to_string() } else { name.clone() }) })
                    .on_key_down(cx.listener(Self::new_group_key))
            }
            None => self.chip("new-group", "+ Group", false).on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                this.new_group = Some(String::new());
                window.focus(&this.new_group_focus);
                cx.notify();
            }),
        }
    }

    /// Above the channel list: All, each group, + Group, and Delete for the selected group.
    pub(crate) fn render_group_bar(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.active_group.clone();
        let mut bar = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .child(
                self.chip("group-all", "All", active.is_none())
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_group(None, cx)),
            );
        for (i, g) in self.groups.iter().enumerate() {
            let name = g.name.clone();
            bar = bar.child(
                self.chip(("group", i), g.name.clone(), active.as_deref() == Some(g.name.as_str()))
                    // Channel count, dimmer than the name.
                    .child(div().ml(px(6.)).opacity(0.6).child(g.channels.len().to_string()))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_group(Some(name.clone()), cx)),
            );
        }
        bar = bar.child(self.new_group_chip(window, cx));
        if let Some(name) = active {
            let confirming = self.confirm_delete_group.as_deref() == Some(name.as_str());
            bar = bar.child(
                self.chip("delete-group", if confirming { "Delete group?" } else { "Delete" }, confirming)
                    .when(!confirming, |d| d.text_color(themed(MUTED)))
                    .tooltip(tip("Click twice to delete this group (channels stay subscribed)"))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.delete_group(name.clone(), cx)),
            );
        }
        // The filter on top, where the other tabs have it; the group chips under it.
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .child(self.filter_field("Filter channels", window, cx))
            .child(bar)
    }

    /// In a channel's header: which groups the channel belongs to (click to toggle).
    pub(crate) fn render_group_editor(&self, channel: &str, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = div().flex().flex_wrap().gap_1().px_3().py_2().border_b_1().border_color(themed(BORDER));
        if self.groups.is_empty() {
            row = row.child(div().text_xs().text_color(themed(MUTED)).mr_2().child("No groups yet:"));
        }
        for (i, g) in self.groups.iter().enumerate() {
            let member = g.channels.iter().any(|c| c == channel);
            let (name, ch) = (g.name.clone(), channel.to_string());
            row = row.child(
                self.chip(("member", i), g.name.clone(), member)
                    // Channel count, as in the group bar of the list.
                    .child(div().ml(px(6.)).opacity(0.6).child(g.channels.len().to_string()))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.toggle_membership(&name, &ch, cx)),
            );
        }
        row.child(self.new_group_chip(window, cx))
    }

    /// The Downloads tab: videos saved with Download, newest first, played from their files.
    pub(crate) fn render_downloads(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let videos = self.library_videos();
        let body = if videos.is_empty() {
            self.status("No matches.")
        } else {
            self.video_list("downloads", &videos, Some(0), cx)
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.filter_bar("Filter downloads", window, cx))
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The Watch later tab: fetched the first time it is shown.
    pub(crate) fn render_watch_later(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.watch_later, Load::Idle) {
            self.fetch(cx, "watch-later", |s| &mut s.watch_later, Some("watch-later".into()), |cfg, on| yt::group_videos(cfg, ":ytwatchlater", on));
        }
        if let Some(p) = self.placeholder(&self.watch_later, "Watch later is empty.", Rows::Videos) {
            return p;
        }
        let v = self.watch_later.items().to_vec();
        self.video_list("watch-later", &v, None, cx)
    }

    /// `watched`: dim the row. `in_up_next`: the row is in the Up next list (✕ instead of +).
    pub(crate) fn video_row(
        &mut self,
        id: impl Into<ElementId>,
        video: Video,
        queue: Arc<[Video]>,
        watched: bool,
        in_up_next: bool,
        selected: bool,
        playlist: Option<Group>,
        in_history: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let playing = self.current.as_ref().is_some_and(|c| c.id == video.id);
        let progress = match (self.history.position(&video.id), video.duration) {
            _ if self.history.is_finished(&video.id) => Some(1.),
            (p, Some(d)) if p > 0. && d > 0. => Some((p / d).min(1.) as f32),
            _ => None,
        };
        let action = {
            let v = video.clone();
            let (icon, tooltip) = if in_up_next { ("close", "Remove from Up next") } else { ("add", "Add to Up next") };
            div()
                .id("row-action")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path(icon)).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip(tooltip))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if in_up_next { this.dequeue(&v.id, cx) } else { this.enqueue(v.clone(), cx) }
                }))
        };
        // Everywhere but Watch later itself (and a request needs your login): add to Watch later.
        let later = (self.cfg.has_auth() && playlist.as_ref().is_none_or(|g| g.id != "WL")).then(|| {
            let v = video.clone();
            div()
                .id("row-later")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("watch-later")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left("Add to Watch later"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.add_to_watch_later(v.clone(), cx);
                }))
        });
        // In an open playlist: remove the video from it.
        let remove = playlist.map(|g| {
            let v = video.clone();
            div()
                .id("row-remove")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("trash")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left(if g.id == "LL" { "Unlike (remove from Liked videos)".to_string() } else { format!("Remove from {}", g.title) }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.remove_from_playlist(g.clone(), v.clone(), cx);
                }))
        });
        // In History (and Continue watching): forget this video.
        let forget = in_history.then(|| {
            let id = video.id.clone();
            div()
                .id("row-forget")
                .flex_none()
                .p(px(6.))
                .rounded_md()
                .invisible()
                .group_hover("video-row", |s| s.visible())
                .hover(|d| d.bg(themed(BORDER)))
                .child(svg().path(icons::path("trash")).size(px(14.)).text_color(themed(TEXT)))
                .tooltip(tip_left("Remove from history"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.forget_video(&id, cx);
                }))
        });
        let views = video.views.filter(|_| self.settings.show_views).map(|v| format!("{} {}", fmt_count(v), if video.live { "watching" } else { "views" }));
        let meta = [video.channel.clone(), views, video.duration.map(fmt_duration), video.watched.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("  ·  ");
        let thumb = self.video_thumb(&video, 96., 54., px(4.), cx);
        let (title, original) = self.shown_title(&video, cx);
        let title_tip = match &original {
            Some(o) => format!("{title}\n\nOriginal title: {o}"),
            None => title.clone(),
        };
        let menu_video = video.clone();
        let menu_queue = queue.clone();
        let hover_id = video.id.clone();
        div()
            .id(id)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.prefetch_hover(&hover_id, *hovered, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &gpui::MouseDownEvent, window, cx| {
                    this.open_video_menu(menu_video.clone(), Some(menu_queue.clone()), ev.position, window, cx)
                }),
            )
            .w_full()
            .overflow_hidden()
            .h(px(ROW_H))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .group("video-row")
            .when(playing, |d| d.bg(themed(HOVER)))
            .when(selected, |d| d.bg(themed(BORDER)))
            // Watched videos are dimmed, back to full on hover.
            .when(watched && !playing && !selected, |d| d.opacity(0.45))
            .hover(|d| d.bg(themed(HOVER)).opacity(1.))
            .child(
                div()
                    .relative()
                    .child(thumb)
                    .when_some(progress.filter(|_| !video.live), |d, p| {
                        d.child(div().absolute().bottom_0().left_0().h(px(3.)).w(px(96. * p)).bg(themed(ACCENT)))
                    })
                    .when(video.live, |d| d.child(div().absolute().bottom(px(3.)).right(px(3.)).child(live_badge(false)))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // Titles are truncated; hovering shows the whole one (and YouTube's, for a
                    // DeArrow title).
                    .child(
                        div()
                            .id("title")
                            .text_sm()
                            .text_color(themed(TEXT))
                            .truncate()
                            .child(title)
                            .tooltip(tip(title_tip)),
                    )
                    .child(div().text_xs().text_color(themed(MUTED)).truncate().child(meta)),
            )
            .children(remove)
            .children(forget)
            .children(later)
            .child(action)
            .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(video.clone(), Some(queue.clone()), cx))
    }

    /// `left`: this is the left column's list, whose rows start at that index for Vim selection.
    pub(crate) fn video_list(&self, list_id: &'static str, videos: &[Video], left: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        // With Shorts turned off, they're hidden from every list; hidden words from all but
        // your own lists (what you put there stays).
        let own = matches!(list_id, "history" | "up-next" | "watch-later" | "downloads") || (list_id == "group" && self.tab == Tab::Playlists);
        let videos: Arc<[Video]> = if own { self.visible(videos) } else { self.visible(&self.wanted(videos.to_vec())) }.into();
        let in_up_next = list_id == "up-next";
        let in_history = list_id == "history";
        let playlist = if list_id == "group" && self.tab == Tab::Playlists {
            self.playlists.open.clone()
        } else if list_id == "watch-later" {
            Some(Self::watch_later_group())
        } else {
            None
        };
        // Watched: finished here, or in your YouTube history. Not dimmed in History itself.
        let watched: Arc<HashSet<String>> = Arc::new(if list_id == "history" {
            HashSet::new()
        } else {
            self.history
                .items
                .iter()
                .filter(|w| w.finished)
                .map(|w| w.video.id.clone())
                .chain(self.yt_history.items().iter().map(|v| v.id.clone()))
                .collect()
        });
        uniform_list(
            list_id,
            videos.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let seen = watched.contains(&videos[i].id);
                        let selected = left.is_some_and(|o| this.vim_selected(o + i));
                        this.video_row(i, videos[i].clone(), videos.clone(), seen, in_up_next, selected, playlist.clone(), in_history, cx)
                    })
                    .collect()
            }),
        )
        .when(left.is_some(), |l| l.track_scroll(self.vim_scroll.clone()))
        .flex_1()
        .into_any_element()
    }

    pub(crate) fn status(&self, msg: impl Into<SharedString>) -> AnyElement {
        div().p_4().text_sm().text_color(themed(MUTED)).child(msg.into()).into_any_element()
    }

    /// The hint appended to any error while logged out, also shown on its own for a list
    /// that never loaded.
    pub(crate) fn needs_login(&self) -> &'static str {
        "This list needs your YouTube account.\nConnect it in Settings → Connect YouTube."
    }

    pub(crate) fn failed(&self, e: &str) -> AnyElement {
        if self.cfg.has_auth() && looks_like_login_error(e) {
            self.status(format!("{}\n\nYour YouTube login isn't working. Settings → Account lets you try again or connect another browser.", session_hint(e.to_string())))
        } else if self.cfg.has_auth() {
            self.status(e.to_string())
        } else {
            self.status(format!("{e}\n\n{}", self.needs_login()))
        }
    }

    /// What to show instead of a list that has no items (yet), or None if it has some.
    pub(crate) fn placeholder<T>(&self, load: &Load<T>, empty: &str, rows: Rows) -> Option<AnyElement> {
        match load {
            Load::Failed(e) => Some(self.failed(e)),
            // Never fetched (logged out): the connect hint instead of a skeleton.
            Load::Idle if !self.cfg.has_auth() => Some(self.status(self.needs_login().to_string())),
            Load::Ready(v) if v.is_empty() => Some(self.status(empty.to_string())),
            Load::Ready(_) => None,
            Load::Loading(v) if !v.is_empty() => None,
            _ => Some(skeleton(rows)),
        }
    }

    /// The anonymous video list: the left column's home and the Recommendations pane
    /// while logged out.
    pub(crate) fn render_anon(&mut self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        match &self.anon {
            Load::Failed(e) => return self.status(e.clone()),
            Load::Ready(v) if v.is_empty() => return self.status("No videos.".to_string()),
            // A refresh keeps the previous mix on screen until the new one arrives.
            Load::Ready(v) | Load::Loading(v) if !v.is_empty() => {}
            _ => return skeleton(Rows::Videos),
        }
        let videos = self.anon.items().to_vec();
        self.video_list(id, &videos, Some(0), cx)
    }

    pub(crate) fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.tab {
            Tab::History if self.cfg.has_auth() => {
                let body = self.render_history(cx);
                return div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .when(self.settings.shorts, |d| {
                        let shorts = self.history_shorts();
                        d.child(
                            div()
                                .flex()
                                .px_2()
                                .border_b_1()
                                .border_color(themed(BORDER))
                                .child(tab_button("Videos", !shorts).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.history_view = ChannelView::Videos;
                                    cx.notify();
                                }))
                                .child(tab_button("Shorts", shorts).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.history_view = ChannelView::Shorts;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(self.filter_bar("Filter history", window, cx))
                    .child(div().flex().flex_col().flex_1().min_h_0().child(body))
                    .into_any_element();
            }
            Tab::Settings => return self.render_settings(window, cx),
            Tab::Downloads => return self.render_downloads(window, cx),
            Tab::Search if self.query.trim().is_empty() || matches!(self.search, Load::Idle) => {
                return self.render_recent_searches(cx);
            }
            Tab::Search => {
                return match self.placeholder(&self.search, "No results.", Rows::Videos) {
                    Some(p) => p,
                    None => self.video_list("search", &self.search.items().to_vec(), Some(0), cx),
                };
            }
            // Logged out: an open channel still shows through render_browser; anything
            // else shares the anonymous home.
            tab if !self.cfg.has_auth() => match tab {
                Tab::Subscriptions | Tab::Playlists if self.browser_ref(tab).open.is_some() => {
                    self.render_browser(tab, window, cx)
                }
                _ => self.render_anon("home", cx),
            },
            tab => self.render_browser(tab, window, cx),
        }
    }

    pub(crate) fn render_history(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // YouTube's history first, then what only this app has seen.
        let (partial, videos) = self.history_items();
        if videos.is_empty() && !self.list_filter.trim().is_empty() {
            return self.status("No matches.");
        }
        if videos.is_empty() {
            if let Some(p) = self.placeholder(&self.yt_history, "Nothing here.", Rows::Videos) {
                return p;
            }
            if self.history_shorts() {
                return self.status("No Shorts.");
            }
        }
        if partial.is_empty() {
            return self.video_list("history", &videos, Some(0), cx);
        }
        let label = |text: &'static str| div().px_3().pt_3().pb_1().text_xs().text_color(themed(MUTED)).child(text);
        let queue: Arc<[Video]> = partial.into();
        let offset = queue.len();
        let rows = uniform_list(
            "continue",
            queue.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let selected = this.vim_selected(i);
                        this.video_row(("continue", i), queue[i].clone(), queue.clone(), false, false, selected, None, true, cx)
                    })
                    .collect()
            }),
        )
        .track_scroll(self.continue_scroll.clone())
        // Never taller than its rows.
        .h(px(self.settings.continue_height.min(offset as f32 * ROW_H)));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(label("CONTINUE WATCHING"))
            .child(rows)
            .child(div().pt_2().child(divider("split-continue", Split::Continue, cx)))
            .child(label("HISTORY"))
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.video_list("history", &videos, Some(offset), cx)))
            .into_any_element()
    }

    pub(crate) fn render_browser(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (back, list_id) = match tab {
            Tab::Subscriptions => ("← Subscriptions", "subs"),
            _ => ("← Playlists", "playlists"),
        };
        if let Some(open) = self.browser_ref(tab).open.clone() {
            // A channel opened from a video's channel link may not be a subscription.
            let subscribed = open.id == FEED_ID || self.subs.groups.items().iter().any(|g| g.id == open.id);
            let back = if tab == Tab::Subscriptions && !subscribed { "← Back" } else { back };
            // Subscribe / unsubscribe for a real channel (UC… id), when logged in.
            let sub_button = (tab == Tab::Subscriptions && open.id.starts_with("UC") && self.cfg.has_auth()).then(|| {
                let confirming = self.confirm_unsub.as_deref() == Some(open.id.as_str());
                let g = open.clone();
                let button = if confirming {
                    // The confirm step stays spelled out.
                    self.chip("channel-sub", "Unsubscribe?", true).ml_1()
                } else if subscribed {
                    self.icon_chip("channel-sub", "subscribed", false, "Subscribed — click twice to unsubscribe")
                } else {
                    self.icon_chip("channel-sub", "subscribe", true, "Subscribe")
                };
                button
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        // The header row itself means "back"; don't trigger it.
                        cx.stop_propagation();
                        this.toggle_channel_sub(g.clone(), subscribed, cx);
                    })
            });
            let is_channel = tab == Tab::Subscriptions && open.id != FEED_ID;
            let groups_button = is_channel.then(|| {
                let in_groups: Vec<&str> = self.groups.iter().filter(|g| g.channels.contains(&open.id)).map(|g| g.name.as_str()).collect();
                let tooltip =
                    if in_groups.is_empty() { "Add this channel to groups".to_string() } else { format!("In: {} (click to change)", in_groups.join(", ")) };
                let count = in_groups.len();
                self.icon_chip("channel-groups", "folder", self.editing_groups, tooltip)
                    .when(count > 0, |d| {
                        // A count badge, so you can tell the channel is in a group without opening the editor.
                        d.relative().child(
                            div()
                                .absolute()
                                .top(px(-4.))
                                .right(px(-4.))
                                .min_w(px(14.))
                                .h(px(14.))
                                .px(px(3.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(themed(ACCENT))
                                .border_1()
                                .border_color(themed(BORDER))
                                .text_size(px(9.))
                                .text_color(themed(ON_ACCENT))
                                .child(count.to_string()),
                        )
                    })
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        cx.stop_propagation();
                        this.editing_groups = !this.editing_groups;
                        cx.notify();
                    })
            });
            let flag_chips = is_channel.then(|| {
                let (notify, muted) = (self.flags.notify.contains(&open.id), self.flags.muted.contains(&open.id));
                let (a, b) = (open.id.clone(), open.id.clone());
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .child(
                        self.icon_chip(
                            "channel-notify",
                            "bell",
                            notify,
                            if notify { "Notifications on (click to turn off)" } else { "Notify me about new uploads" },
                        )
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_flag(true, &a, cx);
                            }),
                    )
                    .child(
                        self.icon_chip(
                            "channel-mute",
                            "muted",
                            muted,
                            if muted { "Muted: hidden from New uploads (click to unmute)" } else { "Hide this channel's uploads from New uploads" },
                        )
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_flag(false, &b, cx);
                            }),
                    )
            });
            let queue_button = (tab == Tab::Playlists).then(|| {
                self.icon_chip("queue-all", "save", false, "Add all to Up next").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.enqueue_all(tab, cx);
                })
            });
            let play_button = (tab == Tab::Playlists).then(|| {
                self.icon_chip("play-all", "play", false, "Play the whole playlist").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    let videos = this.visible(&this.browser_videos(tab));
                    this.play_all(videos, cx);
                })
            });
            let cast_button = (tab == Tab::Playlists && self.cast_available()).then(|| {
                self.icon_chip("cast-all", "cast", false, "Cast the whole playlist").on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                    cx.stop_propagation();
                    let videos = this.visible(&this.browser_videos(tab));
                    this.cast_list(None, videos, cx);
                })
            });
            let group_editor = (is_channel && self.editing_groups).then(|| self.render_group_editor(&open.id, window, cx));

            let view = self.browser_ref(tab).view;
            // Channels (not the New uploads feed or playlists) get Videos | Shorts | Live tabs.
            let channel_tabs = tab == Tab::Subscriptions && open.id != FEED_ID;
            let shorts_tab = self.settings.shorts;
            let videos = &self.browser_ref(tab).videos;
            let empty = match view {
                ChannelView::Shorts => "No Shorts.",
                ChannelView::Live => "No live streams.",
                ChannelView::Videos => "No videos.",
            };
            let shown = self.browser_videos(tab);
            let body = match self.placeholder(videos, empty, Rows::Videos) {
                Some(p) => p,
                None if shown.is_empty() && !self.list_filter.trim().is_empty() => self.status("No matches."),
                None => self.video_list("group", &shown, Some(0), cx),
            };
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("back")
                        .px_3()
                        .py_2()
                        .flex()
                        .text_sm()
                        .cursor_pointer()
                        .border_b_1()
                        .border_color(themed(BORDER))
                        .hover(|d| d.bg(themed(HOVER)))
                        .child(svg().path(icons::path("arrow-left")).size(px(16.)).mr_3().flex_none().text_color(themed(MUTED)))
                        .tooltip(tip(back.trim_start_matches("← ").to_string()))
                        .items_center()
                        .child(div().flex_1().min_w_0().text_color(themed(TEXT)).truncate().child(open.title.clone()))
                        .children(flag_chips)
                        .children(groups_button)
                        .children(sub_button)
                        .children(queue_button)
                        .children(play_button)
                        .children(cast_button)
                        .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                            this.browser(tab).open = None;
                            cx.notify();
                        }),
                )
                .children(group_editor)
                .when(channel_tabs, |d| {
                    d.child(
                        div()
                            .flex()
                            .px_2()
                            .border_b_1()
                            .border_color(themed(BORDER))
                            .child(
                                tab_button("Videos", view == ChannelView::Videos)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Videos, cx)),
                            )
                            .when(shorts_tab, |d| {
                                d.child(
                                    tab_button("Shorts", view == ChannelView::Shorts)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Shorts, cx)),
                                )
                            })
                            .child(
                                tab_button("Live", view == ChannelView::Live)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_channel_view(ChannelView::Live, cx)),
                            ),
                    )
                })
                .child(self.filter_bar("Filter videos", window, cx))
                .child(body)
                .into_any_element();
        }
        let groups = &self.browser_ref(tab).groups;
        if let Some(p) = self.placeholder(groups, "Nothing here.", Rows::Groups) {
            return p;
        }
        let counts = Arc::new(if tab == Tab::Subscriptions { self.unseen_counts() } else { HashMap::new() });
        let g: Arc<[Group]> = self.group_items(tab, &counts).into();
        let bar = if tab == Tab::Subscriptions {
            self.render_group_bar(window, cx)
        } else {
            self.filter_bar("Filter playlists", window, cx)
        };
        let list = uniform_list(
            list_id,
            g.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let group = g[i].clone();
                        let avatar = if group.id == FEED_ID {
                            div()
                                .size(px(28.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(themed(ACCENT))
                                .child(svg().path(icons::path("new")).size(px(14.)).text_color(themed(ON_ACCENT)))
                                .into_any_element()
                        } else {
                            this.thumb_el(&group.id, group.thumb.clone(), 28., 28., px(14.), cx)
                        };
                        let new = counts.get(&group.id).copied().unwrap_or(0);
                        let selected = this.vim_selected(i);
                        let muted = this.flags.muted.contains(&group.id);
                        let bell = this.flags.notify.contains(&group.id);
                        let in_groups: Vec<String> =
                            this.groups.iter().filter(|g| g.channels.contains(&group.id)).map(|g| g.name.clone()).collect();
                        // Right-click a subscribed channel (not "New uploads") for its menu.
                        let menu_group = (tab == Tab::Subscriptions && group.id.starts_with("UC") && this.cfg.has_auth()).then(|| group.clone());
                        // Right-click a playlist: cast it without opening it.
                        let menu_playlist = (tab == Tab::Playlists).then(|| group.clone());
                        div()
                            .id(i)
                            .when_some(menu_playlist, |d, g| {
                                d.on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                                        this.channel_menu = None;
                                        this.video_menu = None;
                                        this.playlist_menu = Some((g.clone(), ev.position, Instant::now()));
                                        this.menu_hovered = false;
                                        cx.notify();
                                    }),
                                )
                            })
                            .when_some(menu_group, |d, g| {
                                d.on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                                        this.confirm_unsub = None;
                                        this.video_menu = None;
                                        this.channel_menu = Some((g.clone(), ev.position, Instant::now()));
                                        this.menu_hovered = false;
                                        cx.notify();
                                    }),
                                )
                            })
                            .when(selected, |d| d.bg(themed(BORDER)))
                            .w_full()
                            .h(px(44.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .text_color(themed(TEXT))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(avatar)
                            .child(
                                div()
                                    .id("name")
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .when(muted, |d| d.text_color(themed(MUTED)))
                                    .child(group.title.clone())
                                    .tooltip(tip(group.title.clone())),
                            )
                            .when_some(group.subscribers.filter(|_| this.settings.show_subs), |d, n| {
                                d.child(div().flex_none().text_xs().text_color(themed(MUTED)).child(fmt_count(n)))
                            })
                            .when(!in_groups.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id(("in-groups", i))
                                        .flex_none()
                                        .child(svg().path(icons::path("folder")).size(px(12.)).text_color(themed(MUTED)))
                                        .tooltip(tip_left(format!("In: {}", in_groups.join(", ")))),
                                )
                            })
                            .when(bell, |d| d.child(svg().path(icons::path("bell")).size(px(12.)).flex_none().text_color(themed(MUTED))))
                            .when(muted, |d| d.child(svg().path(icons::path("muted")).size(px(13.)).flex_none().text_color(themed(MUTED))))
                            .when(new > 0, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .min_w(px(20.))
                                        .px(px(6.))
                                        .flex()
                                        .justify_center()
                                        .whitespace_nowrap()
                                        .rounded_full()
                                        .bg(themed(ACCENT))
                                        .text_xs()
                                        .text_color(themed(ON_ACCENT))
                                        .child(new.to_string()),
                                )
.tooltip(tip_left(format!("{new} new video{}", if new == 1 { "" } else { "s" })))
                            })
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.open_group(tab, group.clone(), cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.vim_scroll.clone())
        .flex_1();
        // Subscriptions: the channel list, or what is live now (one click from the top).
        if tab == Tab::Subscriptions {
            let live_count = self.live_videos(false).len();
            let tabs = div()
                .flex()
                .px_2()
                .border_b_1()
                .border_color(themed(BORDER))
                .child(
                    tab_button("Channels", !self.subs_live)
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_subs_live(false, cx)),
                )
                .child(
                    tab_button(if live_count > 0 { format!("Live ({live_count})") } else { "Live".to_string() }, self.subs_live)
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.set_subs_live(true, cx)),
                );
            let body = if self.subs_live {
                let live = self.live_videos(true);
                match self.placeholder(&self.feed, "No live streams right now.", Rows::Videos) {
                    Some(p) => p,
                    None if live.is_empty() && !self.live_videos(false).is_empty() => self.status("No matches."),
                    None if live.is_empty() => self.status("No live streams right now."),
                    None => self.video_list("live", &live, Some(0), cx),
                }
            } else {
                list.into_any_element()
            };
            return div().flex().flex_col().flex_1().min_h_0().child(tabs).child(bar).child(body).into_any_element();
        }
        div().flex().flex_col().flex_1().min_h_0().child(bar).child(list).into_any_element()
    }

    pub(crate) fn render_recs(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if !self.cfg.has_auth() && self.recs.items().is_empty() {
            return self.render_anon("recs", cx);
        }
        if let Some(p) = self.placeholder(&self.recs, "No recommendations.", Rows::Videos) {
            return p;
        }
        let current = self.current.as_ref().map(|c| c.id.clone());
        let v: Vec<Video> = self.recs.items().iter().filter(|r| Some(&r.id) != current.as_ref()).cloned().collect();
        self.video_list("recs", &v, None, cx)
    }
}
