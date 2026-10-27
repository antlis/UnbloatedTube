use super::*;

mod browser;
mod info;
mod menus;
mod overlays;
mod settings;
mod video;
mod widgets;

pub(crate) use widgets::*;

impl Render for Unbloated {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let (true, Some(video)) = (self.fullscreen, self.current.clone()) {
            // Only the video; f / Esc (here, or in mpv with its hotkeys on) and a double click leave fullscreen.
            if window.focused(cx).is_none() {
                window.focus(&self.root_focus);
            }
            return div()
                .size_full()
                .bg(gpui::black())
                .track_focus(&self.root_focus)
                .on_key_down(cx.listener(Self::shortcut))
                .child(self.screen(&video, true, cx));
        }
        let st = &self.settings;
        let auth = self.cfg.has_auth();
        let tabs: Vec<_> = [
            (auth && st.subscriptions, "Subscriptions", Tab::Subscriptions),
            (auth && st.playlists, "Playlists", Tab::Playlists),
            (auth && st.history, "History", Tab::History),
            (auth && self.downloads_on(), "Downloads", Tab::Downloads),
        ]
        .into_iter()
        .filter_map(|(on, label, tab)| on.then_some((label, tab)))
        .collect();
        let header_icon = |id: &'static str, icon: &'static str| {
            div()
                .id(id)
                .flex_none()
                .p(px(7.))
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(themed(HOVER)))
                .child(svg().path(icons::path(icon)).size(px(16.)).text_color(themed(MUTED)))
        };
        let header = if self.searching {
            // Search mode: ← back, a full-width field, ✕ to clear.
            let focused = self.search_focus.is_focused(window);
            let field = div()
                .id("search")
                .track_focus(&self.search_focus)
                .flex_1()
                .min_w_0()
                .mx_1()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(themed(HOVER))
                .border_1()
                .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                .text_sm()
                .truncate()
                .cursor_text()
                .map(|d| match (self.query.is_empty(), focused) {
                    (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("search", &self.query, "Search YouTube")),
                    (true, false) => d.text_color(themed(MUTED)).child("Search YouTube, or paste a link"),
                    (false, false) => d.text_color(themed(TEXT)).child(self.query.clone()),
                })
                .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                    window.focus(&this.search_focus);
                    cx.notify();
                })
                .on_key_down(cx.listener(Self::search_key));
            div()
                .flex()
                .items_center()
                .px_2()
                .py(px(5.))
                .border_b_1()
                .border_color(themed(BORDER))
                .child(
                    header_icon("search-back", "arrow-left")
                        .tooltip(tip("Back (Esc)"))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.close_search(window, cx)),
                )
                .child(field)
                .when(!self.query.is_empty(), |d| {
                    d.child(
                        header_icon("search-clear", "close")
                            .tooltip(tip_left("Clear"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                                this.query.clear();
                                window.focus(&this.search_focus);
                                cx.notify();
                            }),
                    )
                })
        } else {
            div()
            .flex()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(themed(BORDER))
            .children(tabs.into_iter().map(|(label, tab)| {
                tab_button(label, self.tab == tab).on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.select_tab(tab, cx))
            }))
            // Logged out: the account tabs are replaced by the home list and one way in.
            .when(!auth, |d| {
                d.child(
                    tab_button("Home", !matches!(self.tab, Tab::Settings | Tab::Downloads))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Subscriptions, cx)),
                )
                .when(self.downloads_on(), |d| {
                    d.child(
                        tab_button("Downloads", self.tab == Tab::Downloads)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Downloads, cx)),
                    )
                })
                .child(
                    tab_button("Sign in", self.tab == Tab::Settings)
                        .tooltip(tip_left("Connect your YouTube account (Settings)"))
                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Settings, cx)),
                )
            })
            .child(div().flex_1())
            .child(
                header_icon("search-open", "search")
                    .tooltip(tip_left("Search (/)"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| this.open_search(window, cx)),
            )
            .child({
                let icon = svg().path(icons::path("refresh")).size(px(16.)).text_color(themed(MUTED));
                let icon = if self.tab_loading() {
                    // Spins while the current list loads.
                    icon.with_animation("refresh-spin", Animation::new(Duration::from_millis(900)).repeat(), |s, t| {
                        s.with_transformation(Transformation::rotate(radians(t * std::f32::consts::TAU)))
                    })
                    .into_any_element()
                } else {
                    icon.into_any_element()
                };
                div()
                    .id("refresh")
                    .ml_2()
                    .p(px(7.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|d| d.bg(themed(HOVER)))
                    .child(icon)
                    .tooltip(tip_left("Refresh"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.load_tab(cx))
            })
            .child(
                div()
                    .id("settings")
                    .ml_1()
                    .px(px(7.))
                    .py(px(9.))
                    .cursor_pointer()
                    .border_b_2()
                    .border_color(if self.tab == Tab::Settings { themed(ACCENT).into() } else { Hsla::transparent_black() })
                    .child(svg().path(icons::path("settings")).size(px(16.)).text_color(if self.tab == Tab::Settings {
                        themed(TEXT)
                    } else {
                        themed(MUTED)
                    }))
                    .tooltip(tip_left("Settings"))
                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.select_tab(Tab::Settings, cx)),
            )
        };

        let header = header.h(px(HEADER_H)).flex_none();
        let header_bar = if self.tab_loading() {
            loading_bar("list-loading").into_any_element()
        } else {
            div().h(px(2.)).into_any_element()
        };
        let left = div()
            .flex()
            .flex_col()
            .when(self.right_collapsed, |d| d.flex_1())
            .when(!self.right_collapsed, |d| d.w(relative(self.settings.split)).flex_none())
            .min_w_0()
            .child(header)
            .child(header_bar)
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.render_list(window, cx)))
;

        let player = div().flex().flex_col().min_h_0().child(self.render_player(cx));
        let right = div().relative().flex().flex_col().flex_1().min_w_0().bg(themed(PANEL));
        let right = if self.settings.window_buttons {
            let control = |id: &'static str, icon: &'static str, tooltip: &'static str, hover: u32, color: u32| {
                div()
                    .id(id)
                    .p(px(7.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |d| d.bg(themed(hover)))
                    .child(svg().path(icons::path(icon)).size(px(14.)).text_color(themed(color)))
                    .tooltip(tip(tooltip))
            };
            right.child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .h(px(HEADER_H))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(control("win-min", "minimize", "Minimize", HOVER, MUTED).on_click(|_, window, _| window.minimize_window()))
                    .child(control("win-max", "maximize", "Maximize", HOVER, MUTED).on_click(|_, window, _| window.zoom_window()))
                    .child(control("win-close", "close", "Close", ACCENT, TEXT).on_click(cx.listener(|this, _, window, cx| this.request_close(window, cx)))),
            )
        } else {
            right
        };
        let show_recs = self.settings.recommendations;
        let chapters = self.chapter_list();
        let show_comments = self.comments_on();
        let show_desc = self.description_on();
        let show_transcript = self.transcript_on();
        let show_later = self.watch_later_on();
        let right = if (show_recs || !self.up_next.is_empty() || !chapters.is_empty() || show_comments || show_desc || show_transcript || show_later) && !self.player_full {
            let lower = match self.lower {
                Lower::Description if show_desc => Lower::Description,
                Lower::Transcript if show_transcript => Lower::Transcript,
                Lower::Comments if show_comments => Lower::Comments,
                Lower::WatchLater if show_later => Lower::WatchLater,
                Lower::Chapters if !chapters.is_empty() => Lower::Chapters,
                Lower::Chapters | Lower::Comments | Lower::WatchLater | Lower::Recommended | Lower::Description | Lower::Transcript if show_recs => Lower::Recommended,
                _ => Lower::UpNext,
            };
            let body = match lower {
                Lower::Chapters => self.render_chapters(chapters.clone(), cx),
                Lower::Description => self.render_description(cx),
                Lower::Transcript => self.render_transcript(window, cx),
                Lower::Comments => self.render_comments(cx),
                Lower::WatchLater => self.render_watch_later(cx),
                Lower::Recommended => self.render_recs(cx),
                Lower::UpNext if self.up_next.is_empty() => self.status("Nothing queued. Hover a video and press + to add it."),
                Lower::UpNext => {
                let list = self.video_list("up-next", &self.up_next.clone(), None, cx);
                let n = self.up_next.len();
                let save = self.cfg.has_auth().then(|| self.playlist_name_chip("Save as playlist", true, window, cx));
                let cast = self.cast_available().then(|| {
                    self.chip("up-next-cast", format!("Cast all {n}"), true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        let videos = this.up_next.clone();
                        this.cast_list(None, videos, cx);
                    })
                });
                if save.is_some() || cast.is_some() {
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .child(div().flex().flex_none().gap_2().px_3().py_1().children(save).children(cast))
                        .child(div().flex().flex_col().flex_1().min_h_0().child(list))
                        .into_any_element()
                } else {
                    list
                }
            }
            };
            right
                .child(player.h(if self.lower_full { px(0.).into() } else { relative(self.settings.player) }).overflow_hidden().flex_none())
                .child(divider("split-player", Split::Player, cx))
                .child(
                    div()
                        .flex()
                        .px_2()
                        .border_b_1()
                        .border_color(themed(BORDER))
                        .when(show_recs, |d| {
                            d.child(
                                tab_button("Recommended", lower == Lower::Recommended).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.show_lower(Lower::Recommended, cx);
                                }),
                            )
                        })
                        .when(show_desc, |d| {
                            d.child(tab_button("Description", lower == Lower::Description).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Description, cx);
                            }))
                        })
                        .when(show_transcript, |d| {
                            d.child(tab_button("Transcript", lower == Lower::Transcript).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Transcript, cx);
                            }))
                        })
                        .when(!chapters.is_empty(), |d| {
                            d.child(
                                tab_button(format!("Chapters ({})", chapters.len()), lower == Lower::Chapters).on_click_hinted(
                                    &self.hint_reg(),
                                    cx,
                                    |this, _, _, cx| {
                                        this.show_lower(Lower::Chapters, cx);
                                    },
                                ),
                            )
                        })
                        .when(show_later, |d| {
                            let label = match &self.watch_later {
                                Load::Ready(v) if !v.is_empty() => format!("Watch later ({})", v.len()),
                                _ => "Watch later".to_string(),
                            };
                            d.child(tab_button(label, lower == Lower::WatchLater).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::WatchLater, cx);
                            }))
                        })
                        .when(show_comments, |d| {
                            d.child(tab_button("Comments", lower == Lower::Comments).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::Comments, cx);
                            }))
                        })
                        .child(
                            tab_button(
                                if self.up_next.is_empty() { "Up next".to_string() } else { format!("Up next ({})", self.up_next.len()) },
                                lower == Lower::UpNext,
                            )
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.show_lower(Lower::UpNext, cx);
                            }),
                        ),
                )
                .child(div().flex().flex_col().flex_1().min_h_0().child(body))
        } else {
            right.child(player.flex_1())
        };
        let right = if self.saving { right.child(self.render_save_overlay(window, cx)) } else { right };

        // Nothing focused (e.g. after leaving a text field): give focus back to the root so
        // keyboard shortcuts keep working.
        if window.focused(cx).is_none() {
            window.focus(&self.root_focus);
        }
        let key = self.left_key();
        if key != self.vim_list {
            // Remember where we were in the old list; come back to the new list's last spot.
            self.vim_positions.insert(std::mem::take(&mut self.vim_list), self.vim_cursor);
            self.vim_cursor = self.vim_positions.get(&key).copied().unwrap_or(0);
            self.vim_list = key;
            self.list_filter.clear();
            self.vim_scroll.scroll_to_item(self.vim_cursor, ScrollStrategy::Center);
        }
        // The ring follows its element if the layout moved it since the last frame.
        if self.kb_focus.is_some() {
            let targets = self.kb_targets(window);
            self.kb_focus = self.kb_current(&targets).map(|i| targets[i].0).or(self.kb_focus);
        }
        let kb_ring = self.kb_focus.map(|b| {
            div()
                .absolute()
                .left(b.origin.x - px(2.))
                .top(b.origin.y - px(2.))
                .w(b.size.width + px(4.))
                .h(b.size.height + px(4.))
                .rounded_md()
                .border_2()
                .border_color(themed(ACCENT))
        });
        // Collected again during this frame's paint (see hint_target).
        self.hint_targets.borrow_mut().clear();
        let hint_overlay = self.hints.as_ref().map(|(targets, typed)| hint_labels(targets, typed));
        let sheet = self.show_keys.then(|| self.render_cheatsheet(cx));
        let welcome = self.welcome_visible().then(|| self.render_welcome(cx));
        div()
            .size_full()
            .relative()
            .flex()
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::shortcut))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.kb_focus.take().is_some() {
                        cx.notify();
                    }
                    // Pressed off any text: the selection goes (a press on text sets its own
                    // first — that block is deeper in the tree, and presses bubble outwards).
                    // Read text_press without consuming it; the root's mouse-up will clear it.
                    let on_text = this.text_press;
                    if !on_text && this.text_sel.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .bg(themed(BG))
            .text_color(themed(TEXT))
            .children((!self.left_collapsed).then_some(left))
            .child(divider("split-columns", Split::Columns, cx))
            .children((!self.right_collapsed).then_some(right))
            .children(kb_ring)
            .children(self.render_channel_menu(window, cx))
            .children(self.render_video_menu(window, cx))
            .children(self.render_playlist_menu(window, cx))
            .children(self.render_quit_prompt(cx))
            .children(self.render_toasts())
            .children(sheet)
            .children(hint_overlay)
            .children(welcome)
            // While dragging, X keeps sending us pointer events even over mpv's window.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                // A text selection grows with the drag wherever the pointer goes — past the
                // block's edges, where the block's own handler can't follow it. Only while
                // the press is down (text_press); a plain move after release must not extend.
                if this.text_press && this.text_sel.as_mut().is_some_and(|sel| sel.drag_to(e.position)) {
                    cx.notify();
                }
                let Some(split) = this.dragging else { return };
                let size = window.viewport_size();
                match split {
                    Split::Columns => {
                        // Dragged to an edge: hide that column, keeping the width for later.
                        let at = e.position.x / size.width;
                        this.left_collapsed = at < 0.08;
                        this.right_collapsed = at > 0.92;
                        if !this.left_collapsed && !this.right_collapsed {
                            this.settings.split = at.clamp(0.2, 0.8);
                        }
                        this.sync_embed();
                    }
                    Split::Player => {
                        this.settings.player = (e.position.y / size.height).clamp(0.02, 0.9);
                        this.lower_full = false;
                        this.player_full = false;
                    }
                    Split::Continue => {
                        let (y, h) = this.drag_from;
                        let max = (f32::from(size.height) - 250.).max(ROW_H);
                        this.settings.continue_height = (h + f32::from(e.position.y) - y).clamp(ROW_H, max);
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    // The press is over: a press that never dragged selects nothing.
                    this.text_press = false;
                    if this.text_sel.as_ref().is_some_and(|sel| sel.range().is_empty()) {
                        this.text_sel = None;
                    }
                    if this.dragging.take().is_some() {
                        this.settings.save();
                    }
                }),
            )
    }
}

impl Unbloated {
    /// The video area: thumbnail underneath, mpv's embedded window placed on top of it.
    pub(crate) fn screen(&mut self, video: &Video, full: bool, cx: &mut Context<Self>) -> gpui::Div {
        let mut screen = div().relative().bg(gpui::black()).overflow_hidden();
        if full {
            screen = screen.size_full();
        } else {
            // Fills the player section; mpv letterboxes the video inside.
            screen = screen.w_full().flex_1().min_h_0();
        }
        match self.thumb(&video.id, Some(video.thumb_url()), cx) {
            Thumb::Ready(path) => screen = screen.child(img(path).size_full().object_fit(ObjectFit::Contain)),
            Thumb::Pending => screen = screen.child(pulse("screen-skeleton", div().size_full().bg(themed(HOVER)))),
            Thumb::Missing => {}
        }
        if self.pip {
            screen = screen.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .bg(gpui::black().opacity(0.7))
                    .child(div().text_sm().text_color(themed(TEXT)).child("Playing in picture-in-picture"))
                    .child(
                        self.chip("pip-back", "Bring it back here", true)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.toggle_pip(cx)),
                    ),
            );
        }
        if let Some(c) = &self.casting {
            let (name, status) = (c.name.clone(), c.status.clone());
            let line = match &status {
                Some(s) if s.playing => format!("{} / {}", fmt_duration(s.position), fmt_duration(s.duration)),
                _ => "Starting…".to_string(),
            };
            let listed = self.casting.as_ref().is_some_and(|c| c.listed);
            let queue_line = status.as_ref().and_then(|s| s.queue).map(|(i, n)| format!("{} of {n}", i + 1));
            let now_title = status.as_ref().map(|s| s.title.clone()).filter(|t| !t.is_empty());
            let paused = status.as_ref().is_some_and(|s| s.paused);
            screen = screen.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .bg(gpui::black().opacity(0.85))
                    .child(div().text_sm().text_color(themed(TEXT)).child(format!("Casting to {name}")))
                    .children(queue_line.map(|q| div().text_xs().text_color(themed(MUTED)).child(q)))
                    .children(now_title.map(|t| div().max_w(px(420.)).px_4().text_sm().text_color(themed(TEXT)).truncate().child(t)))
                    .child(div().text_xs().text_color(themed(MUTED)).child(line))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(listed, |d| {
                                d.child(
                                    self.chip("cast-prev", "Previous", true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.next_prev(false, cx)),
                                )
                            })
                            .child(
                                self.chip("cast-back", "−10s", status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(-10., cx)),
                            )
                            .child(
                                self.chip("cast-pause", if paused { "Play" } else { "Pause" }, status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.pause_toggle(cx)),
                            )
                            .child(
                                self.chip("cast-fwd", "+10s", status.is_some())
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.seek_by(10., cx)),
                            )
                            .when(listed, |d| {
                                d.child(
                                    self.chip("cast-next", "Next", true)
                                        .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.next_prev(true, cx)),
                                )
                            })
                            .child(
                                self.chip("cast-stop", "Stop", true)
                                    .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.stop_cast(cx)),
                            ),
                    )
                    .child(
                        self.chip("cast-here", "Back to this screen", true)
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.end_cast("Cast view closed, it keeps playing there", cx)),
                    ),
            );
        }
        // Nothing playing yet (e.g. after startup): clicking the picture starts it. The playback
        // buttons under the video are replaced by the hover bar, which exists only in mpv's window.
        if !full && !self.pip && !self.loading && self.state.is_none() && self.settings.video_controls {
            let v = video.clone();
            screen = screen.child(
                div()
                    .id("screen-play")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(svg().path(icons::path("play")).size(px(56.)).text_color(gpui::white().opacity(0.85)))
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.play(v.clone(), None, cx)),
            );
        }
        // mpv in this process has no window to get the pointer from: the app passes it on.
        if let (Some(embed), true) = (self.embed.clone(), self.player.needs_pointer() && self.state.is_some() && self.casting.is_none() && !self.pip) {
            let at = {
                let embed = embed.clone();
                move |p: gpui::Point<gpui::Pixels>, window: &Window| {
                    embed.borrow().video_point(f32::from(p.x), f32::from(p.y), window.scale_factor())
                }
            };
            let (at_move, at_down) = (at.clone(), at);
            screen = screen.child(
                div()
                    .id("screen-pointer")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_move(cx.listener(move |this, ev: &gpui::MouseMoveEvent, window, _| {
                        this.player.pointer(Some(at_move(ev.position, window)), None, false);
                    }))
                    .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                        if !*hovered {
                            this.player.pointer(None, None, false);
                        }
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener({
                            let at = at_down.clone();
                            move |this, ev: &gpui::MouseDownEvent, window, _| {
                                this.player.pointer(Some(at(ev.position, window)), Some(0), ev.click_count > 1);
                            }
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, ev: &gpui::MouseDownEvent, window, _| {
                            this.player.pointer(Some(at_down(ev.position, window)), Some(2), false);
                        }),
                    ),
            );
        }
        if let Some(embed) = self.embed.clone() {
            screen = screen.child(
                canvas(|_, _, _| {}, move |b, _, window, _| {
                    let s = window.scale_factor();
                    let px = |v: gpui::Pixels| (f32::from(v) * s).round();
                    embed.borrow_mut().place(
                        px(b.origin.x) as i32,
                        px(b.origin.y) as i32,
                        px(b.size.width) as u32,
                        px(b.size.height) as u32,
                        s,
                    );
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        screen
    }
}
