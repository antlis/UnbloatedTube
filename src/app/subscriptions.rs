use super::*;

/// The video's channel as a browsable group, if its channel URL is known.
pub(crate) fn channel_group(v: &Video) -> Option<Group> {
    let url = v.channel_url.clone()?;
    // Used for the cache file name too, so keep it to the id or handle.
    let id = channel_of(v)
        .map(String::from)
        .unwrap_or_else(|| url.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string());
    let title = v.channel.clone().unwrap_or_else(|| id.clone());
    Some(Group { id, title, url: format!("{}/videos", url.trim_end_matches('/')), thumb: None, subscribers: None, privacy: None })
}

/// Whether `title` says one of `words` (lower-case) as a word of its own: "art" hides "Art
/// school" but not "start".
pub(crate) fn says_any(title: &str, words: &[String]) -> bool {
    let title = title.to_lowercase();
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    words.iter().any(|w| {
        title.match_indices(w.as_str()).any(|(i, _)| {
            let (before, after) = (title[..i].chars().next_back(), title[i + w.len()..].chars().next());
            // A word that itself starts or ends with a symbol (#shorts) needs no edge there.
            (edge(before) || !w.starts_with(char::is_alphanumeric)) && (edge(after) || !w.ends_with(char::is_alphanumeric))
        })
    })
}

/// Channel id (UC…) of a video, from its channel URL.
pub(crate) fn channel_of(v: &Video) -> Option<&str> {
    v.channel_url.as_deref()?.split("/channel/").nth(1).map(|id| id.trim_end_matches('/'))
}

impl Unbloated {
    /// Per channel id, feed videos you haven't seen in its list or played; total under FEED_ID.
    /// One pass over the feed, so it's cheap enough to compute on every render.
    pub(crate) fn unseen_counts(&self) -> HashMap<String, usize> {
        let played: HashSet<&str> = self.history.items.iter().map(|w| w.video.id.as_str()).collect();
        let words = self.hide_words();
        let mut counts = HashMap::new();
        for v in self.feed.items() {
            if self.seen.ids.contains(&v.id) || played.contains(v.id.as_str()) || !self.in_active_group(v) || self.is_muted(v) || says_any(&v.title, &words) {
                continue;
            }
            *counts.entry(FEED_ID.to_string()).or_default() += 1;
            if let Some(ch) = channel_of(v) {
                *counts.entry(ch.to_string()).or_default() += 1;
            }
        }
        counts
    }

    /// Desktop notifications for feed videos of channels with the bell on, not notified yet.
    pub(crate) fn notify_uploads(&mut self) {
        let words = self.hide_words();
        let fresh: Vec<Video> = self
            .feed
            .items()
            .iter()
            .filter(|v| channel_of(v).is_some_and(|c| self.flags.notify.contains(c)))
            .filter(|v| !self.flags.notified.contains(&v.id) && !self.seen.ids.contains(&v.id))
            .filter(|v| !says_any(&v.title, &words))
            .cloned()
            .collect();
        // Forget ids that left the feed, so the set stays small.
        let in_feed: HashSet<&str> = self.feed.items().iter().map(|v| v.id.as_str()).collect();
        self.flags.notified.retain(|id| in_feed.contains(id.as_str()));
        if fresh.is_empty() {
            store::save_data("channel_flags", &self.flags);
            return;
        }
        self.flags.notified.extend(fresh.iter().map(|v| v.id.clone()));
        store::save_data("channel_flags", &self.flags);
        if !self.settings.notifications {
            return;
        }
        // A few: one each; many (e.g. after a long time closed): one summary.
        let messages: Vec<(String, String)> = if fresh.len() <= 3 {
            fresh.iter().map(|v| (v.channel.clone().unwrap_or_default(), v.title.clone())).collect()
        } else {
            let mut names: Vec<String> = fresh.iter().filter_map(|v| v.channel.clone()).collect();
            names.dedup();
            vec![(format!("{} new videos", fresh.len()), names.join(", "))]
        };
        std::thread::spawn(move || {
            for (summary, body) in messages {
                let _ = notify_rust::Notification::new().appname("UnbloatedTube").summary(&summary).body(&body).show();
            }
        });
    }

    /// Turn mute / notify on or off for a channel.
    pub(crate) fn toggle_flag(&mut self, notify: bool, channel: &str, cx: &mut Context<Self>) {
        let set = if notify { &mut self.flags.notify } else { &mut self.flags.muted };
        let on = set.insert(channel.to_string());
        if !on {
            set.remove(channel);
        }
        if notify && on {
            // Only uploads from now on: what's in the feed already isn't news.
            let ids: Vec<String> = self.feed.items().iter().filter(|v| channel_of(v) == Some(channel)).map(|v| v.id.clone()).collect();
            self.flags.notified.extend(ids);
        }
        store::save_data("channel_flags", &self.flags);
        self.notice = Some(match (notify, on) {
            (true, true) => "Notifications on for this channel".into(),
            (true, false) => "Notifications off for this channel".into(),
            (false, true) => "Muted: hidden from New uploads".into(),
            (false, false) => "Unmuted".into(),
        });
        cx.notify();
    }

    pub(crate) fn is_muted(&self, v: &Video) -> bool {
        channel_of(v).is_some_and(|c| self.flags.muted.contains(c))
    }

    /// The group Subscriptions is filtered to, if any.
    pub(crate) fn active_group(&self) -> Option<&ChannelGroup> {
        let name = self.active_group.as_deref()?;
        self.groups.iter().find(|g| g.name == name)
    }

    /// Whether a feed video's channel is in the active group (always true without one).
    pub(crate) fn in_active_group(&self, v: &Video) -> bool {
        self.active_group().is_none_or(|g| channel_of(v).is_some_and(|c| g.channels.iter().any(|m| m == c)))
    }

    pub(crate) fn save_groups(&self) {
        store::save_data("groups", &self.groups);
    }

    pub(crate) fn select_group(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        self.active_group = if self.active_group == name { None } else { name };
        self.confirm_delete_group = None;
        cx.notify();
    }

    pub(crate) fn toggle_membership(&mut self, group: &str, channel: &str, cx: &mut Context<Self>) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.name == group) {
            match g.channels.iter().position(|c| c == channel) {
                Some(i) => {
                    g.channels.remove(i);
                }
                None => g.channels.push(channel.to_string()),
            }
            self.save_groups();
        }
        cx.notify();
    }

    pub(crate) fn delete_group(&mut self, name: String, cx: &mut Context<Self>) {
        if self.confirm_delete_group.as_deref() != Some(name.as_str()) {
            self.confirm_delete_group = Some(name);
        } else {
            self.groups.retain(|g| g.name != name);
            self.save_groups();
            self.active_group = None;
            self.confirm_delete_group = None;
        }
        cx.notify();
    }

    /// Typing a new group's name: Enter creates it (and, in a channel, adds that channel).
    pub(crate) fn new_group_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = &mut self.new_group else { return };
        match edit_text(&mut self.caret, "group", name, ev, cx) {
            Edit::Submit => {
                let name = name.trim().to_string();
                self.new_group = None;
                window.blur();
                if !name.is_empty() && !self.groups.iter().any(|g| g.name == name) {
                    let channel = self.subs.open.as_ref().filter(|g| g.id != FEED_ID).map(|g| g.id.clone());
                    self.groups.push(ChannelGroup { name: name.clone(), channels: channel.into_iter().collect() });
                    self.save_groups();
                    if self.subs.open.is_none() {
                        self.active_group = Some(name);
                    }
                }
            }
            Edit::Cancel => {
                self.new_group = None;
                window.blur();
            }
            Edit::Changed => {}
            Edit::Moved => {}
            Edit::Ignored => {}
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Subscribe to `channel`, or unsubscribe after a confirming second click.
    pub(crate) fn toggle_channel_sub(&mut self, channel: Group, subscribed: bool, cx: &mut Context<Self>) {
        if subscribed && self.confirm_unsub.as_deref() != Some(channel.id.as_str()) {
            self.confirm_unsub = Some(channel.id.clone());
            cx.notify();
            return;
        }
        self.confirm_unsub = None;
        let (id, on) = (channel.id.clone(), !subscribed);
        self.with_account(cx, move |a| a.subscribe(&id, on), move |this, res, _| match res {
            Ok(()) => {
                if let Load::Ready(groups) | Load::Loading(groups) = &mut this.subs.groups {
                    groups.retain(|g| g.id != channel.id);
                    if on {
                        groups.push(channel.clone());
                        groups.sort_by_key(|g| g.title.to_lowercase());
                    }
                }
                store::save_list("subs", this.subs.groups.items());
                // Keep the player's subscribe button in sync if it's the same channel.
                if let Some((_, st)) = &mut this.status {
                    if st.channel_id.as_deref() == Some(channel.id.as_str()) {
                        st.subscribed = on;
                    }
                }
                this.notice = Some(format!("{} {}", if on { "Subscribed to" } else { "Unsubscribed from" }, channel.title));
            }
            Err(e) => this.notice = Some(e),
        });
        cx.notify();
    }

    /// A video without its channel (a Short from YouTube's history) gets it from its watch page:
    /// the playing one (so the channel button shows), and its rows in History.
    pub(crate) fn fill_channel(&mut self, id: &str, st: &account::VideoStatus) {
        let (Some(name), Some(channel)) = (&st.channel, &st.channel_id) else { return };
        let url = format!("https://www.youtube.com/channel/{channel}");
        let fill = |v: &mut Video| {
            if v.id == id && v.channel_url.is_none() {
                v.channel = Some(name.clone());
                v.channel_url = Some(url.clone());
            }
        };
        if let Some(v) = self.current.as_mut() {
            fill(v);
        }
        if let Load::Ready(list) | Load::Loading(list) = &mut self.yt_history {
            if list.iter().any(|v| v.id == id && v.channel_url.is_none()) {
                list.iter_mut().for_each(fill);
                store::save_list("history", list);
            }
        }
        self.history.set_channel(id, name, &url);
        self.history.save();
    }
}
