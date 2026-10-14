use super::*;

impl Unbloated {
    /// Which SponsorBlock categories to skip (multi-select).
    pub(crate) fn segment_chips(&self, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .px_4()
            .pb_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xs().text_color(themed(MUTED)).child("Skip these segments:"))
            .child(div().flex().flex_wrap().gap_1().children(SEGMENTS.iter().enumerate().map(|(i, &(label, name))| {
                let on = self.settings.skip_segments.iter().any(|s| s == name);
                div()
                    .id(("segment", i))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { themed(ACCENT) } else { themed(HOVER) })
                    .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .child(label)
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        let list = &mut this.settings.skip_segments;
                        match list.iter().position(|s| s == name) {
                            Some(i) => {
                                list.remove(i);
                            }
                            None => list.push(name.to_string()),
                        }
                        this.settings.save();
                        this.apply_player_settings(cx);
                    })
            })))
    }

    /// A label with a row of mutually exclusive options; `set` applies the picked index.
    pub(crate) fn choice_row(
        &self,
        label: &'static str,
        options: Vec<(String, bool)>,
        set: fn(&mut Settings, usize),
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .child(div().flex_1().text_sm().text_color(themed(TEXT)).child(label))
            .children(options.into_iter().enumerate().map(|(i, (text, on))| {
                div()
                    .id((label, i))
                    .ml_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .bg(if on { themed(ACCENT) } else { themed(HOVER) })
                    .text_color(if on { themed(ON_ACCENT) } else { themed(TEXT) })
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .child(text)
                    .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                        set(&mut this.settings, i);
                        this.settings.save();
                        this.apply_player_settings(cx);
                    })
            }))
    }

    /// Settings → Network: the connection, then the bypass method or the proxy's address, presets
    /// and a test.
    pub(crate) fn network_rows(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let matches = |label: &str, hint: &str| query.is_empty() || format!("{label} {hint}").to_lowercase().contains(query);
        let mut rows = Vec::new();
        if !matches("Connection", "network proxy bypass blocked slowdown russia tor byedpi vpn") {
            return rows;
        }
        let conn = self.settings.connection.clone();
        rows.push(
            self.choice_row("Connection", CONNECTIONS.iter().map(|(l, v)| (l.to_string(), *v == conn)).collect(), |s, i| s.connection = CONNECTIONS[i].1.into(), cx)
                .into_any_element(),
        );
        let hint = |text: &'static str| div().px_4().pb_2().text_xs().text_color(themed(MUTED)).child(text).into_any_element();
        match conn.as_str() {
            "bypass" => {
                rows.push(hint(
                    "For where YouTube is slowed down or filtered (Russia, for one): the first packet of each connection goes out split, so the filter doesn't see it's YouTube. Nothing to install; no other server involved. Doesn't help where YouTube's addresses are blocked outright: use a proxy then. If videos stay slow, try another method.",
                ));
                let method = self.settings.bypass_method.clone();
                rows.push(
                    self.choice_row("Method", BYPASS_METHODS.iter().map(|(l, v)| (l.to_string(), *v == method)).collect(), |s, i| s.bypass_method = BYPASS_METHODS[i].1.into(), cx)
                        .into_any_element(),
                );
            }
            "proxy" => {
                rows.push(hint(
                    "Everything goes through a proxy running on your computer or elsewhere: Tor, ByeDPI, a V2Ray/Xray/sing-box or VPN client's local port, a server of your own. The program itself has to be running.",
                ));
                let current = self.settings.proxy.trim().to_string();
                rows.push(
                    self.choice_row("Preset", PROXY_PRESETS.iter().map(|(l, v)| (l.to_string(), *v == current)).collect(), |s, i| s.proxy = PROXY_PRESETS[i].1.into(), cx)
                        .into_any_element(),
                );
                let field = TEXT_FIELDS.iter().position(|(l, _, _)| *l == "Proxy address").unwrap();
                rows.push(self.text_field(field, window, cx).into_any_element());
            }
            _ => {}
        }
        if conn != "direct" {
            let result = self.net_test.clone();
            rows.push(
                div()
                    .px_4()
                    .pb_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.chip("net-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.test_connection(cx)))
                    .when_some(result, |d, r| d.child(div().text_xs().text_color(themed(MUTED)).child(r)))
                    .into_any_element(),
            );
        }
        rows
    }

    /// One of TEXT_FIELDS: click to focus, type, Enter/Esc to finish. Saved on every change.
    pub(crate) fn text_field(&self, i: usize, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let (label, hint, field) = TEXT_FIELDS[i];
        let focus = self.field_focus[i].clone();
        let focused = focus.is_focused(window);
        let value = field(&mut self.settings.clone()).clone();
        div()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().text_color(themed(TEXT)).child(label))
            .child(
                div()
                    .id(("field", i))
                    .track_focus(&focus)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(themed(HOVER))
                    .border_1()
                    .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                    .text_sm()
                    .cursor_text()
                    .map(|d| match (value.is_empty(), focused) {
                        (true, false) => d.text_color(themed(MUTED)).child(hint),
                        (_, true) => d.text_color(themed(TEXT)).child(self.caret_text(label, &value, hint)),
                        _ => d.text_color(themed(TEXT)).child(value.clone()),
                    })
                    .on_click_hinted(&self.hint_reg(), cx, move |_, _, window, cx| {
                        window.focus(&focus);
                        cx.notify();
                    })
                    .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                        match edit_text(&mut this.caret, label, field(&mut this.settings), ev, cx) {
                            Edit::Submit | Edit::Cancel => {
                                window.blur();
                                // Apply once editing is done, not on every keystroke.
                                this.apply_player_settings(cx);
                            }
                            Edit::Changed => this.settings.save(),
                            Edit::Moved => {}
                            Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .when(label == "Cast command", |d| {
                d.child(div().text_xs().text_color(themed(MUTED)).child(
                    "Casts the video with this command (T or the Cast button) and replaces the [cast] targets in config.toml; empty uses those. \
                     {url} is the video's link; {start} (seconds), {id} and {title} work too. Quote arguments that contain spaces. \
                     It is not run through a shell, and it only sends the video: control of the receiver needs a target in config.toml.",
                ))
            })
    }

    /// A button in the Connect panel: accent when `primary`, neutral otherwise.
    pub(crate) fn cta(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_none()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if primary { themed(ACCENT) } else { themed(BORDER) })
            .bg(if primary { themed(ACCENT) } else { themed(HOVER) })
            .text_sm()
            .text_color(if primary { themed(ON_ACCENT) } else { themed(TEXT) })
            .cursor_pointer()
            .hover(|d| d.opacity(0.85))
            .child(label.into())
    }

    /// The ACCOUNT section: the Connect YouTube panel (browser, cookies.txt, config.toml),
    /// the three-step check, and cookies.txt import under Advanced.
    pub(crate) fn connect_section(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let from_toml = self.cfg.cookies_file.is_some() || self.cfg.cookies_from_browser.is_some();
        let source = self.cfg.auth.source_label().unwrap_or_default();
        let config_toml = store::config_dir().join("config.toml");
        let connect = self.connect.clone();
        let detected: Vec<&str> = BROWSERS.iter().filter(|(_, spec, _)| Auth::on_path(spec)).map(|(name, ..)| *name).collect();
        let dot_line = |glyph: &'static str, text: String, color: u32| {
            div()
                .px_4()
                .pb_2()
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .child(div().text_color(themed(ACCENT)).child(glyph))
                .child(div().text_color(themed(color)).child(text))
        };
        let mut d = div().flex().flex_col().child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child("ACCOUNT"));
        match &connect {
            Connect::Idle if !self.cfg.has_auth() => {
                d = d
                    .child(div().px_4().text_sm().text_color(themed(TEXT)).child("Connect YouTube"))
                    .child(div().px_4().pt_1().text_xs().text_color(themed(MUTED)).child(
                        "See your subscriptions, playlists and history. No password is ever asked: the app uses your browser's own YouTube session.",
                    ))
                    .child(
                        div().px_4().py_3().flex().flex_wrap().gap_1().children(BROWSERS.iter().enumerate().map(|(i, (name, spec, _))| {
                            self.chip(("browser", i), *name, self.connect_browser == *spec)
                                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| {
                                    this.connect_browser = (*spec).to_string();
                                    cx.notify();
                                })
                        })),
                    )
                    .child(div().px_4().text_xs().text_color(themed(MUTED)).child(if detected.is_empty() {
                        "No supported browser found in PATH — importing a cookies.txt below still works.".to_string()
                    } else {
                        format!("Detected: {}", detected.join(", "))
                    }))
                    .child(div().px_4().pt_3().child(self.cta("connect-go", "Connect", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                        this.start_connect(cx);
                    })))
                    .child(div().px_4().pt_4().pb_1().text_xs().text_color(themed(MUTED)).child("ADVANCED"));
                // Path field for a Netscape cookies.txt (own field, not a Settings value).
                let focused = self.import_focus.is_focused(window);
                d = d.child(
                    div()
                        .px_4()
                        .py_2()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id("import-field")
                                .track_focus(&self.import_focus)
                                .flex_1()
                                .min_w_0()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(themed(HOVER))
                                .border_1()
                                .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
                                .text_sm()
                                .cursor_text()
                                .map(|d| match (self.import_path.is_empty(), focused) {
                                    (true, false) => d.text_color(themed(MUTED)).child("~/cookies.txt"),
                                    (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("import", &self.import_path, "~/cookies.txt")),
                                    _ => d.text_color(themed(TEXT)).child(self.import_path.clone()),
                                })
                                .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                                    window.focus(&this.import_focus);
                                    cx.notify();
                                })
                                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                                    match edit_text(&mut this.caret, "import", &mut this.import_path, ev, cx) {
                                        Edit::Submit => {
                                            this.import_cookies(cx);
                                            window.blur();
                                        }
                                        Edit::Cancel => window.blur(),
                                        Edit::Changed | Edit::Moved => {}
                                        Edit::Ignored => {
                                            cx.stop_propagation();
                                            return;
                                        }
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                        .child(self.cta("import-go", "Import", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.import_cookies(cx);
                        })),
                );
                d = d.child(div().px_4().pt_1().text_xs().text_color(themed(MUTED)).child(
                    "A Netscape cookies.txt exported from a browser. It contains your YouTube session, so keep it private.",
                ));
            }
            Connect::Idle => {
                d = d.child(dot_line("●", format!("Connected via {source}"), TEXT));
                d = d.child(
                    div().px_4().py_2().flex().gap_2()
                        .child(self.cta("connect-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.connect = Connect::Running(1);
                            this.run_probe(false, cx);
                        }))
                        .child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.logout(cx);
                        })),
                );
            }
            Connect::Running(_) => {
                d = d
                    .child(dot_line("●", "Checking your connection…".into(), TEXT))
                    .children(connect.steps())
                    .child(div().px_4().pt_2().text_xs().text_color(themed(MUTED)).child("This can take up to half a minute."));
            }
            Connect::Failed { error, .. } => {
                d = d
                    .child(dot_line("✕", "Connection problem".into(), ACCENT))
                    .children(connect.steps())
                    .child(div().px_4().pt_2().text_sm().text_color(themed(MUTED)).child(error.clone()))
                    .child(
                        div().px_4().py_2().flex().gap_2()
                            .child(self.cta("connect-retry", "Try again", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                // A failed first connect dropped its login again: save it anew.
                                if this.cfg.has_auth() {
                                    this.connect = Connect::Running(1);
                                    this.run_probe(false, cx);
                                } else {
                                    this.start_connect(cx);
                                }
                            }))
                            .child(self.cta("connect-signin", "Sign in again", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.sign_in_again(cx);
                            }))
                            .when(!from_toml, |d| {
                                d.child(self.cta("connect-other", "Choose another browser", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.choose_another_browser(cx);
                                }))
                            })
                            .when(from_toml, |d| {
                                d.child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                    this.logout(cx);
                                }))
                            }),
                    )
                    .child(
                        div().px_4().pb_2().text_xs().text_color(themed(MUTED)).child(
                            "Signing in keeps failing, or the browser says you are signed out? Clear the cookies of youtube.com and google.com in the browser (its Settings → Privacy → Cookies and site data), then sign in again. The app can't clear a browser's cookies for you.",
                        ),
                    );
            }
            Connect::Done { me } => {
                d = d.child(dot_line("●", format!("Connected via {source}"), TEXT));
                let avatar = me.avatar.clone().map(|url| self.thumb_el("connect-avatar", Some(url), 20., 20., px(10.), cx));
                let word = match &me.handle {
                    Some(handle) => format!("Signed in as {handle}"),
                    None => "Everything checks out.".to_string(),
                };
                d = d.child(div().px_4().pb_1().flex().items_center().gap_2().text_sm().text_color(themed(MUTED)).children(avatar).child(word));
                d = d.children(connect.steps());
                d = d.child(
                    div().px_4().py_2().flex().gap_2()
                        .child(self.cta("connect-test", "Test connection", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.connect = Connect::Running(1);
                            this.run_probe(false, cx);
                        }))
                        .child(self.cta("connect-drop", "Log out", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.logout(cx);
                        })),
                );
            }
        }
        if from_toml {
            d = d.child(div().px_4().pb_2().text_xs().text_color(themed(MUTED)).child(format!("Edit {} to change", config_toml.display())));
        }
        if let Some((ok, msg)) = &self.import_msg {
            d = d.child(
                div()
                    .px_4()
                    .pb_2()
                    .text_xs()
                    .text_color(if *ok { themed(MUTED) } else { themed(ACCENT) })
                    .child(format!("{} {msg}", if *ok { "✓" } else { "✕" })),
            );
        }
        d
    }

    /// The first-run screen: offer to connect, or continue without an account (Esc does the same).
    pub(crate) fn render_welcome(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        div()
            .id("welcome")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(themed(BG))
            .child(
                div()
                    .w(px(560.))
                    .max_w(relative(0.9))
                    .p_8()
                    .rounded_xl()
                    .bg(themed(PANEL))
                    .border_1()
                    .border_color(themed(BORDER))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_2xl().text_color(themed(TEXT)).child("UnbloatedTube"))
                    .child(div().text_sm().text_color(themed(MUTED)).child(
                        "Connect your YouTube account to see your subscriptions, playlists and history. The app never asks for a password: it uses your browser's own YouTube session.",
                    ))
                    .child(
                        div().pt_2().flex().gap_2().child(self.cta("welcome-connect", "Connect YouTube", true).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                            this.welcome_connect(cx);
                        })).child(
                            self.cta("welcome-skip", "Continue without account", false).on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| {
                                this.finish_welcome(cx);
                            }),
                        ),
                    )
                    .child(div().pt_1().text_xs().text_color(themed(MUTED)).child("Esc continues without an account.")),
            )
    }

    pub(crate) fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.settings_filter.to_lowercase();
        let matches = |label: &str, hint: &str| {
            query.is_empty() || label.to_lowercase().contains(&query) || hint.to_lowercase().contains(&query)
        };
        let toggle_row = |i: usize, &(label, hint, field): &Toggle, cx: &mut Context<Self>| {
            let on = *field(&mut self.settings.clone());
            let switch = div()
                .w(px(34.))
                .h(px(18.))
                .p(px(2.))
                .rounded_full()
                .flex()
                .when(on, |d| d.justify_end())
                .bg(if on { themed(ACCENT) } else { themed(BORDER) })
                .child(div().size(px(14.)).rounded_full().bg(themed(ON_ACCENT)));
            div()
                .id(("toggle", i))
                .w_full()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|d| d.bg(themed(HOVER)))
                .child(
                    // min_w_0 lets a long hint wrap instead of pushing the switch out of view.
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().text_color(themed(TEXT)).child(label))
                        .child(div().text_xs().text_color(themed(MUTED)).child(hint)),
                )
                .child(switch.flex_none().ml_3())
                .on_click_hinted(&self.hint_reg(), cx, move |this, _, _, cx| this.toggle(field, cx))
                .into_any_element()
        };
        // Each section: header and its matching rows (hidden when none match).
        let mut sections: Vec<(&str, Vec<AnyElement>)> = Vec::new();
        let mut offset = 0;
        for (title, toggles) in [
            ("SHOW", &TOGGLES[..]),
            ("PLAYER BUTTONS", &BUTTON_TOGGLES[..]),
            ("PLAYER", &PLAYER_TOGGLES[..]),
            ("SUBTITLES", &SUB_TOGGLES[..]),
            ("NOTIFICATIONS", &NOTIFY_TOGGLES[..]),
            ("VIDEO INFO", &INFO_TOGGLES[..]),
            ("DEARROW", &DEARROW_TOGGLES[..]),
        ] {
            let rows = toggles
                .iter()
                .enumerate()
                .filter(|(_, t)| matches(t.0, t.1) && (self.cfg.has_auth() || !ACCOUNT_ONLY.contains(&t.0)))
                .map(|(i, t)| toggle_row(offset + i, t, cx))
                .collect();
            offset += toggles.len();
            sections.push((title, rows));
        }
        let player = &mut sections[2].1;
        if self.settings.sponsorblock && (matches("SponsorBlock segments", "sponsor ads skip")) {
            player.push(self.segment_chips(cx).into_any_element());
        }
        if matches("Max quality", "resolution 1080p") {
            player.push(
                self.choice_row(
                    "Max quality",
                    QUALITIES.iter().map(|q| (format!("{q}p"), *q == self.settings.max_quality)).collect(),
                    |s, i| s.max_quality = QUALITIES[i],
                    cx,
                )
                .into_any_element(),
            );
        }
        if matches("Default speed", "playback speed") {
            player.push(
                self.choice_row(
                    "Default speed",
                    SPEEDS.iter().map(|v| (format!("{v}×"), *v == self.settings.speed)).collect(),
                    |s, i| s.speed = SPEEDS[i],
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut sub_rows: Vec<AnyElement> = Vec::new();
        for (i, (label, hint, _)) in TEXT_FIELDS.iter().enumerate() {
            // The proxy's address goes with the rest of Network.
            if matches(label, hint) && *label != "Proxy address" {
                let row = self.text_field(i, window, cx).into_any_element();
                if *label == "Subtitle language" { sub_rows.push(row) } else { player.push(row) }
            }
        }
        if matches("Subtitle size", "subtitles captions text size") {
            sub_rows.push(
                self.choice_row(
                    "Subtitle size",
                    SUB_SCALES.iter().map(|(name, v)| (name.to_string(), *v == self.settings.sub_scale)).collect(),
                    |s, i| s.sub_scale = SUB_SCALES[i].1,
                    cx,
                )
                .into_any_element(),
            );
        }
        sections[3].1.extend(sub_rows);
        if self.settings.notifications && matches("Check every", "notifications minutes interval") {
            let row = self.choice_row(
                "Check every",
                NOTIFY_MINUTES.iter().map(|m| (format!("{m} min"), *m == self.settings.notify_minutes)).collect(),
                |s, i| s.notify_minutes = NOTIFY_MINUTES[i],
                cx,
            );
            sections[4].1.push(row.into_any_element());
        }
        let net = self.network_rows(&query, window, cx);
        sections.push(("NETWORK", net));
        let show_account = matches("Connect YouTube", "account login logged in sign in browser connect cookies import");
        let nothing = sections.iter().all(|(_, rows)| rows.is_empty())
            && !show_account
            && !self.shortcuts().iter().any(|(k, what)| matches(k, what))
            && !matches("Keyboard shortcuts", "keys");
        let focused = self.settings_focus.is_focused(window);
        let search = div()
            .id("settings-search")
            .track_focus(&self.settings_focus)
            .mx_4()
            .my_2()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(if focused { themed(MUTED) } else { themed(BORDER) })
            .text_sm()
            .cursor_text()
            .map(|d| match (self.settings_filter.is_empty(), focused) {
                (true, false) => d.text_color(themed(MUTED)).child("Search settings"),
                (_, true) => d.text_color(themed(TEXT)).child(self.caret_text("settings", &self.settings_filter, "Search settings")),
                _ => d.text_color(themed(TEXT)).child(self.settings_filter.clone()),
            })
            .on_click_hinted(&self.hint_reg(), cx, |this, _, window, cx| {
                window.focus(&this.settings_focus);
                cx.notify();
            })
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match edit_text(&mut this.caret, "settings", &mut this.settings_filter, ev, cx) {
                    Edit::Submit => window.blur(),
                    Edit::Cancel => {
                        this.settings_filter.clear();
                        window.blur();
                    }
                    Edit::Changed => {}
                    Edit::Moved => {}
                    Edit::Ignored => {
                cx.stop_propagation();
                return;
            }
                }
                cx.stop_propagation();
                cx.notify();
            }));
        div()
            .id("settings-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_2()
            .child(search)
            .when(show_account, |d| d.child(self.connect_section(window, cx)))
            .when(nothing, |d| d.child(self.status("No settings match.")))
            .children(sections.into_iter().filter(|(_, rows)| !rows.is_empty()).map(|(title, rows)| {
                div()
                    .flex()
                    .flex_col()
                    .child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child(title))
                    .children(rows)
            }))
            .when(query.is_empty() || matches("Changes apply", "player"), |d| {
                d.child(
                    div()
                        .px_4()
                        .pt_2()
                        .text_xs()
                        .text_color(themed(MUTED))
                        .child("Changes apply right away (text fields: after Enter)."),
                )
            })
            .when(self.settings.sponsorblock && player::sponsorblock_script().is_none(), |d| {
                d.child(div().px_4().pt_1().text_xs().text_color(themed(ACCENT)).child(
                    "SponsorBlock script not found: start the app from its nix-shell (sets UNBLOATED_SPONSORBLOCK).",
                ))
            })
            .when(self.shortcuts().iter().any(|(k, what)| matches(k, what)) || matches("Keyboard shortcuts", "keys"), |d| {
                d.child(div().px_4().pt_4().pb_2().text_xs().text_color(themed(MUTED)).child("KEYBOARD SHORTCUTS")).children(
                    self.shortcuts().iter().filter(|(k, what)| matches(k, what) || matches("Keyboard shortcuts", "keys")).map(|(k, what)| {
                        div()
                            .px_4()
                            .py_1()
                            .flex()
                            .text_sm()
                            .child(div().w(px(190.)).flex_none().text_color(themed(TEXT)).child(*k))
                            .child(div().text_color(themed(MUTED)).child(*what))
                    }),
                )
            })
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_4()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("about-github")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py(px(7.))
                            .rounded_md()
                            .bg(themed(HOVER))
                            .cursor_pointer()
                            .hover(|d| d.bg(themed(BORDER)))
                            .tooltip(tip(REPO_URL))
                            .child(svg().path(icons::path("github")).size(px(16.)).text_color(themed(TEXT)))
                            .child(div().text_sm().text_color(themed(TEXT)).child("GitHub"))
                            .on_click_hinted(&self.hint_reg(), cx, |this, _, _, cx| this.open_url(REPO_URL, cx)),
                    )
                    .child(div().text_xs().text_color(themed(MUTED)).child(format!("UnbloatedTube {}", env!("CARGO_PKG_VERSION")))),
            )
            .into_any_element()
    }
}
