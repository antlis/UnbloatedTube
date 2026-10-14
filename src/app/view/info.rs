use super::*;

impl Unbloated {
    /// Description of the current video, fetched on first view (and again after the video changes).
    pub(crate) fn render_description(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.description_for != id {
            self.description_for = id.clone();
            // Not the previous video's description while this one loads.
            self.description = Load::Idle;
            let id = id.unwrap_or_default();
            self.fetch(cx, "description", |s| &mut s.description, None, move |cfg, on| yt::description(cfg, &id, on));
        }
        let text = match &self.description {
            Load::Failed(e) => return self.failed(e),
            Load::Ready(v) => v.first().cloned().unwrap_or_default(),
            _ => return skeleton(Rows::Comments),
        };
        if text.is_empty() {
            return self.status("No description.".to_string());
        }
        div()
            .id("description")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_2()
            // One element per paragraph: GPUI keeps single line breaks, blank lines separate them.
            .children(text.split("\n\n").enumerate().map(|(i, p)| {
                div().text_sm().text_color(themed(TEXT)).child(self.linked_text(format!("description-text-{i}"), p.trim().to_string(), cx))
            }))
            .into_any_element()
    }

    /// `text` with its links, timestamps, #tags and @handles clickable (see `open_text_link`)
    /// and with the mouse's selection over it drawn (see `textsel`).
    pub(crate) fn linked_text(&self, key: String, text: String, cx: &mut Context<Self>) -> gpui::Div {
        let (ranges, targets): (Vec<_>, Vec<_>) = links::find(&text).into_iter().unzip();
        let this = cx.entity().downgrade();
        textsel::text(
            key,
            text,
            ranges,
            Vec::new(),
            self.text_sel.as_ref(),
            move |i, cx| {
                let target = targets[i].clone();
                this.update(cx, |this, cx| this.open_text_link(target, cx)).ok();
            },
            cx,
        )
        // An I-beam over prose: it can be dragged over (a transcript row points instead, its
        // press seeks).
        .cursor_text()
    }

    /// The current video's captions as a list of lines, fetched on first view (and again after
    /// the video changes): a search field narrows them, a click jumps there.
    pub(crate) fn render_transcript(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.transcript_for != id {
            self.transcript_for = id.clone();
            // Not the previous video's lines while these load.
            self.transcript = Load::Idle;
            self.transcript_shown = None;
            self.transcript_query.clear();
            let id = id.unwrap_or_default();
            let ((langs, auto), dir) = (self.video_sub_langs(&id), store::cache_dir().join("captions"));
            self.fetch(cx, "transcript", |s| &mut s.transcript, None, move |cfg, on| yt::transcript(cfg, &id, &langs, auto, &dir, on));
        }
        let q = self.transcript_query.trim().to_lowercase();
        let all = self.transcript.items();
        let lines: Arc<[(f64, String)]> = all.iter().filter(|(_, t)| q.is_empty() || t.to_lowercase().contains(&q)).cloned().collect();
        let count = (!q.is_empty() && !all.is_empty()).then(|| match lines.len() {
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        });
        let body = match &self.transcript {
            Load::Failed(e) => self.failed(e),
            Load::Ready(v) if v.is_empty() => self.status("No captions in your subtitle languages (Settings → Subtitles)."),
            Load::Ready(_) if lines.is_empty() => self.status("Not said in this video."),
            Load::Ready(_) => self.transcript_list(lines, &q, cx),
            _ => skeleton(Rows::Comments),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(self.transcript_field(window, cx).flex_1())
                    .children(count.map(|c| div().flex_none().text_xs().text_color(themed(MUTED)).child(c))),
            )
            .child(div().flex().flex_col().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The lines, the playing one highlighted, matches of `q` marked; a click jumps there.
    pub(crate) fn transcript_list(&mut self, lines: Arc<[(f64, String)]>, q: &str, cx: &mut Context<Self>) -> AnyElement {
        let position = match &self.casting {
            Some(c) => c.status.as_ref().map_or(0., |s| s.position),
            None => self.state.as_ref().map_or(0., |s| s.position),
        };
        let current = lines.iter().rposition(|(t, _)| *t <= position);
        // Scrolled to the playing line when the list appears, then left where you read.
        if self.transcript_shown.is_none() {
            self.transcript_shown = Some(current.unwrap_or(0));
            self.transcript_scroll.scroll_to_item(current.unwrap_or(0), ScrollStrategy::Center);
        }
        let q = q.to_string();
        uniform_list(
            "transcript",
            lines.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let (start, text) = &lines[i];
                        let (start, is_current) = (*start, Some(i) == current);
                        // The match, where lower-casing kept the text's byte offsets.
                        let lower = text.to_lowercase();
                        let found = (!q.is_empty() && lower.len() == text.len()).then(|| lower.find(&q)).flatten().map(|at| at..at + q.len());
                        let mark = HighlightStyle { background_color: Some(gpui::rgba((ACCENT << 8) | 0x66).into()), ..Default::default() };
                        div()
                            .id(i)
                            .w_full()
                            .h(px(32.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .cursor_pointer()
                            .when(is_current, |d| d.bg(themed(HOVER)))
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(
                                div()
                                    .w(px(64.))
                                    .flex_none()
                                    .text_xs()
                                    .text_color(if is_current { themed(ACCENT) } else { themed(MUTED) })
                                    .child(fmt_duration(start)),
                            )
                            .child(div().flex_1().min_w_0().truncate().text_sm().text_color(themed(TEXT)).child(
                                textsel::text(
                                    format!("transcript-line-{i}"),
                                    text.clone(),
                                    Vec::new(),
                                    found.into_iter().map(|r| (r, mark)).collect(),
                                    this.text_sel.as_ref(),
                                    |_, _| {},
                                    cx,
                                ),
                            ))
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| {
                                // Pressed to drag-select rather than to seek (the selection is
                                // still up: the root's press handler runs after this one).
                                if this.text_sel.as_ref().is_some_and(|sel| !sel.range().is_empty()) {
                                    return;
                                }
                                this.seek_link(start, cx)
                            })
                    })
                    .collect()
            }),
        )
        .track_scroll(self.transcript_scroll.clone())
        .flex_1()
        .into_any_element()
    }

    /// The Transcript tab's search field: filters the lines as you type; Esc clears.
    pub(crate) fn transcript_field(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let focused = self.transcript_focus.is_focused(window);
        let empty = self.transcript_query.is_empty();
        div()
            .id("transcript-search")
            .track_focus(&self.transcript_focus)
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
            .child(div().flex_1().min_w_0().truncate().text_color(if empty { themed(MUTED) } else { themed(TEXT) }).map(|d| match (empty, focused) {
                (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("transcript", &self.transcript_query, "Search in this video")),
                (true, false) => d.child("Search in this video"),
                (false, false) => d.child(self.transcript_query.clone()),
            }))
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.transcript_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "transcript", &mut this.transcript_query, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.transcript_query.clear();
                        this.transcript_shown = None;
                        window.blur();
                    }
                    // A new list: start at its top (or, emptied, at the playing line again).
                    Edit::Changed => {
                        this.transcript_shown = if this.transcript_query.trim().is_empty() { None } else { Some(0) };
                        this.transcript_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Edit::Moved | Edit::Ignored => {}
                }
                cx.stop_propagation();
                cx.notify();
            }))
    }

    /// Comments of the current video, fetched on first view (and again after the video changes).
    pub(crate) fn render_comments(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.current.as_ref().map(|v| v.id.clone());
        if self.comments_for != id {
            self.comments_for = id;
            self.comments_limit = COMMENTS_PAGE;
            // Not the previous video's comments while these load.
            self.comments = Load::Idle;
            self.fetch_comments(cx);
        }
        if let Some(p) = self.placeholder(&self.comments, "No comments.", Rows::Comments) {
            return p;
        }
        // A full page suggests there are more (yt-dlp can't continue, so more means refetching).
        let loading = matches!(self.comments, Load::Loading(_));
        let more = loading || self.comments.items().len() >= self.comments_limit;
        div()
            .id("comments")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(self.comments.items().iter().enumerate().map(|(i, c)| {
                let meta = [Some(c.author.clone()), (!c.age.is_empty()).then(|| c.age.clone()), c.likes.filter(|n| *n > 0).map(|n| format!("{} likes", fmt_count(n)))]
                    .into_iter()
                    .flatten()
                    .chain(c.pinned.then(|| "pinned".to_string()))
                    .collect::<Vec<_>>()
                    .join(" · ");
                div()
                    .id(i)
                    .w_full()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_b_1()
                    .border_color(themed(BORDER))
                    .child(div().text_xs().text_color(themed(MUTED)).child(meta))
                    .child(div().text_sm().text_color(themed(TEXT)).child(self.linked_text(format!("comment-text-{}-{i}", self.comments_for.clone().unwrap_or_default()), c.text.clone(), cx)))
            }))
            .when(more, |d| {
                d.child(
                    div().p_3().flex().justify_center().child(
                        div()
                            .id("comments-more")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .text_color(themed(MUTED))
                            .when(!loading, |b| b.cursor_pointer().hover(|b| b.bg(themed(HOVER))))
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(loading, |b| {
                                b.child(svg().path(icons::path("refresh")).size(px(14.)).text_color(themed(MUTED)).with_animation(
                                    "comments-spin",
                                    Animation::new(Duration::from_millis(900)).repeat(),
                                    |s, t| s.with_transformation(Transformation::rotate(radians(t * std::f32::consts::TAU))),
                                ))
                            })
                            .child(if loading { "Loading more comments…" } else { "Load more comments" })
                            .when(!loading, |b| {
                                b.on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.comments_limit += COMMENTS_PAGE;
                                    this.fetch_comments(cx);
                                })
                            }),
                    ),
                )
            })
            .when(!more, |d| {
                let n = self.comments.items().len();
                d.child(div().p_3().flex().justify_center().text_xs().text_color(themed(MUTED)).child(format!("All {n} comments shown")))
            })
            .into_any_element()
    }

    /// Clickable chapters; the current one is highlighted and scrolled into view when it changes.
    pub(crate) fn render_chapters(&mut self, chapters: Arc<[(f64, String)]>, cx: &mut Context<Self>) -> AnyElement {
        let position = self.state.as_ref().map_or(0., |s| s.position);
        let current = chapters.iter().rposition(|(t, _)| *t <= position).unwrap_or(0);
        if self.chapter_shown != Some(current) {
            self.chapter_shown = Some(current);
            self.chapters_scroll.scroll_to_item(current, ScrollStrategy::Center);
        }
        uniform_list(
            "chapters",
            chapters.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let (start, title) = &chapters[i];
                        let (start, is_current) = (*start, i == current);
                        let title = if title.is_empty() { format!("Chapter {}", i + 1) } else { title.clone() };
                        div()
                            .id(i)
                            .w_full()
                            .h(px(40.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .cursor_pointer()
                            .when(is_current, |d| d.bg(themed(HOVER)))
                            .hover(|d| d.bg(themed(HOVER)))
                            .child(
                                div()
                                    .w(px(64.))
                                    .flex_none()
                                    .text_xs()
                                    .text_color(if is_current { themed(ACCENT) } else { themed(MUTED) })
                                    .child(fmt_duration(start)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .text_color(if is_current { themed(TEXT) } else { themed(MUTED) })
                                    .child(title),
                            )
                            .on_click_hinted(&this.hint_reg(), cx, move |this, _, _, cx| this.seek_to(start, cx))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.chapters_scroll.clone())
        .flex_1()
        .into_any_element()
    }
}
