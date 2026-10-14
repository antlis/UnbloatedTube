use super::*;

/// Route the app's connections as Settings → Network says (see backend/proxy.rs).
pub(crate) fn apply_network(s: &Settings) -> Result<(), String> {
    let route = match s.connection.as_str() {
        "bypass" => proxy::Route::Bypass(match s.bypass_method.as_str() {
            "both" => proxy::Split::Both,
            "tcp" => proxy::Split::Tcp,
            _ => proxy::Split::Tls,
        }),
        "proxy" if s.proxy.trim().is_empty() => return Ok(proxy::set_route(proxy::Route::Direct)),
        "proxy" => match proxy::parse_upstream(&s.proxy) {
            Ok(up) => proxy::Route::Upstream(up),
            Err(e) => {
                proxy::set_route(proxy::Route::Direct);
                return Err(format!("Proxy address: {e}"));
            }
        },
        _ => proxy::Route::Direct,
    };
    proxy::set_route(route);
    Ok(())
}

impl Unbloated {
    pub(crate) fn toggle(&mut self, field: fn(&mut Settings) -> &mut bool, cx: &mut Context<Self>) {
        let on = field(&mut self.settings);
        *on = !*on;
        self.settings.save();
        LIGHT.store(self.settings.light_theme, std::sync::atomic::Ordering::Relaxed);
        if self.settings.recommendations && matches!(self.recs, Load::Idle) {
            self.load_recs(cx);
        }
        if self.tab == Tab::Downloads && !self.downloads_on() {
            self.select_tab(self.tab_list()[0], cx);
        }
        self.apply_player_settings(cx);
    }

    /// Apply player settings to the running video now: speed directly; anything else that
    /// changes mpv's options reloads the current video at the same position.
    pub(crate) fn apply_player_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = apply_network(&self.settings) {
            self.notice = Some(e);
        }
        self.player.set_speed(self.settings.speed);
        let options = player::options(&self.cfg, &self.settings, self.pip);
        let changed = !self.player.options().is_empty() && self.player.options() != options.as_slice();
        if let (true, Some(video)) = (changed, self.current.clone()) {
            let paused = self.state.as_ref().is_none_or(|s| s.paused);
            self.start(video, paused);
        }
        self.sync_embed();
        cx.notify();
    }

    /// Settings → Network → Test connection: one request to YouTube the way the app sends them.
    pub(crate) fn test_connection(&mut self, cx: &mut Context<Self>) {
        self.net_test = Some("Testing…".into());
        let through_proxy = proxy::url().is_some();
        let task = blocking::unblock(move || {
            let start = Instant::now();
            // First whether the proxy gets through at all, with its reason when it doesn't (the
            // HTTP client only knows "Proxy failed to connect").
            if through_proxy {
                if let Err(e) = proxy::check("www.youtube.com") {
                    return (Err(e), start.elapsed());
                }
            }
            let res = http::agent_with_timeout(Duration::from_secs(12)).get("https://www.youtube.com/generate_204").call();
            // The proxy says why in its answer's status line ("502 1.2.3.4:443: timed out").
            let res = res.map(drop).map_err(|e| match e {
                ureq::Error::Status(code, resp) => format!("{code} {}", resp.status_text()),
                e if e.to_string().contains("timed out") => "no answer within 12 s".to_string(),
                e => e.to_string(),
            });
            (res, start.elapsed())
        });
        cx.spawn(async move |this, cx| {
            let (res, took) = task.await;
            this.update(cx, |this, cx| {
                this.net_test = Some(match res {
                    Ok(()) => format!("YouTube answers: {} ms", took.as_millis()),
                    Err(e) => format!("Doesn't work: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}
