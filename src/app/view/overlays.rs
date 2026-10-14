use super::*;

impl Unbloated {
    pub(crate) fn render_quit_prompt(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let name = self.casting.as_ref().filter(|_| self.quit_prompt)?.name.clone();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(div().id("quit-backdrop").absolute().top_0().left_0().size_full().occlude().bg(gpui::black().opacity(0.5)))
                .child(
                    div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().child(
                        div()
                            .id("quit-prompt")
                            .occlude()
                            .w(px(340.))
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .rounded_md()
                            .bg(themed(HOVER))
                            .border_1()
                            .border_color(themed(BORDER))
                            .shadow_lg()
                            .child(div().text_sm().text_color(themed(TEXT)).child(format!("Still casting to {name}")))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        self.chip("quit-stop", format!("Stop {name} and quit"), true)
                                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, _| this.quit_cast(true, window)),
                                    )
                                    .child(
                                        self.chip("quit-keep", "Keep playing and quit", true)
                                            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, _| this.quit_cast(false, window)),
                                    )
                                    .child(self.chip("quit-cancel", "Cancel", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                        this.quit_prompt = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
                ),
        )
    }

    /// Notices and the current video's download status, in the bottom-right corner (under the
    /// controls they'd be covered by the lower pane when the player is short).
    pub(crate) fn render_toasts(&self) -> Option<gpui::Div> {
        let download = self.current.as_ref().and_then(|v| self.downloads.get(&v.id)).and_then(Download::visible_status);
        let lines: Vec<String> = self.notice.iter().cloned().chain(download).chain(self.sleep_status()).collect();
        (!lines.is_empty()).then(|| {
            div()
                .absolute()
                .bottom(px(16.))
                .right(px(16.))
                .flex()
                .flex_col()
                .items_end()
                .gap_2()
                .children(lines.into_iter().map(|n| {
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(themed(HOVER))
                        .border_1()
                        .border_l_4()
                        .border_color(themed(ACCENT))
                        .shadow_lg()
                        .text_sm()
                        .text_color(themed(TEXT))
                        .max_w(px(480.))
                        .line_clamp(2)
                        .child(n)
                }))
        })
    }

    /// Full-height "Save to playlist" list over the right column.
    pub(crate) fn render_save_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let title = self.current.as_ref().map(|v| v.title.clone()).unwrap_or_default();
        let focused = self.save_focus.is_focused(window);
        let lists: Arc<[Group]> = self.save_targets().into();
        let body = if lists.is_empty() {
            if self.playlists.groups.items().is_empty() || self.save_editable.is_none() {
                skeleton(Rows::Groups)
            } else {
                self.status("No matching playlists.")
            }
        } else {
            let lists = lists.clone();
            uniform_list(
                "save-list",
                lists.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| {
                            let g = lists[i].clone();
                            let cover = this.thumb_el(&g.id, g.thumb.clone(), 64., 36., px(4.), cx);
                            div()
                                .id(i)
                                .w_full()
                                .h(px(52.))
                                .px_4()
                                .flex()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .text_color(themed(TEXT))
                                .cursor_pointer()
                                .hover(|d| d.bg(themed(HOVER)))
                                .child(cover)
                                .child(div().truncate().child(g.title.clone()))
                                .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.save_to(g.clone(), cx))
                        })
                        .collect()
                }),
            )
            .flex_1()
            .into_any_element()
        };
        div()
            .id("save-overlay")
            // Block hover/clicks from reaching the player controls underneath.
            .occlude()
            .track_focus(&self.save_focus)
            .on_key_down(cx.listener(Self::save_key))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .bg(themed(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_color(themed(TEXT)).child("Save to playlist"))
                            .child(div().text_xs().text_color(themed(MUTED)).truncate().child(title)),
                    )
                    .child(icon_button("save-close", "close", "Close (Esc)", true).mr_0().on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.close_save(cx))),
            )
            .child(
                div()
                    .mx_4()
                    .mb_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(themed(HOVER))
                    .border_1()
                    .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                    .text_sm()
                    .map(|d| match (self.save_filter.is_empty(), focused) {
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("save", &self.save_filter, "Type to filter…")),
                        (true, false) => d.text_color(themed(MUTED)).child("Click here, then type to filter"),
                        _ => d.text_color(themed(TEXT)).child(self.save_filter.clone()),
                    }),
            )
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
    }

    /// Full-window keyboard cheatsheet (?): keys as keycaps, grouped in columns.
    pub(crate) fn render_cheatsheet(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let keycap = |k: &str| {
            div()
                .min_w(px(28.))
                .px_2()
                .py(px(2.))
                .flex()
                .justify_center()
                .rounded_md()
                .bg(themed(HOVER))
                .border_1()
                .border_b_2()
                .border_color(themed(BORDER))
                .text_sm()
                .text_color(themed(TEXT))
                .child(k.to_string())
        };
        let (mut rest, groups) = if self.settings.vim { (&VIM_SHORTCUTS[..], &VIM_SHEET_GROUPS[..]) } else { (&SHORTCUTS[..], &SHEET_GROUPS[..]) };
        let mut columns = [div().flex_1().min_w_0().flex().flex_col().gap_6(), div().flex_1().min_w_0().flex().flex_col().gap_6()];
        for &(title, n, col) in groups {
            let (items, tail) = rest.split_at(n);
            rest = tail;
            let group = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().pb_1().text_xs().text_color(themed(ACCENT)).child(title.to_uppercase()))
                .children(items.iter().map(|(keys, what)| {
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .w(px(170.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap_1()
                                .children(keys.split(" / ").map(keycap)),
                        )
                        // min_w_0 so long descriptions wrap inside the card.
                        .child(div().flex_1().min_w_0().text_sm().text_color(themed(MUTED)).child(*what))
                }));
            let c = std::mem::replace(&mut columns[col], div());
            columns[col] = c.child(group);
        }
        div()
            .id("cheatsheet")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(themed(BG))
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_keys = false;
                this.sync_embed();
                cx.notify();
            }))
            .child(
                div()
                    .w(px(980.))
                    .max_w(relative(0.92))
                    .max_h(relative(0.92))
                    .p_8()
                    .rounded_xl()
                    .bg(themed(PANEL))
                    .border_1()
                    .border_color(themed(BORDER))
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .child(div().flex_1().text_2xl().text_color(themed(TEXT)).child(if self.settings.vim {
                                "Keyboard shortcuts · Vim mode"
                            } else {
                                "Keyboard shortcuts"
                            }))
                            .child(div().text_xs().text_color(themed(MUTED)).child("? or Esc to close")),
                    )
                    // Scrolls when the window is too short for it.
                    .child(div().id("sheet-body").flex_1().min_h_0().overflow_y_scroll().flex().items_start().gap_10().children(columns)),
            )
    }
}
