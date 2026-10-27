use super::*;

/// What the playlist name field is for.
#[derive(Clone, PartialEq)]
pub(crate) enum PlaylistEdit {
    /// A new, empty playlist.
    New,
    /// A new playlist holding the Up next queue.
    Queue,
    /// The playlist with this id gets a new name.
    Rename(String),
    /// The playlist with this id gets a new description.
    Describe(String),
}

/// Whose playlists the Playlists list shows.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Owner {
    /// Made by you (the ones YouTube tells the privacy of), with Watch later and Liked videos.
    Mine,
    /// Made by someone else, saved to your library.
    Saved,
}

impl Unbloated {
    /// Fetch your playlists again (after one was made).
    fn reload_playlists(&mut self, cx: &mut Context<Self>) {
        self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
    }

    /// Start typing in the playlist field, with `text` already in it.
    pub(crate) fn edit_playlist_name(&mut self, target: PlaylistEdit, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        self.playlist_edit = Some((target, text));
        window.focus(&self.playlist_edit_focus);
        cx.notify();
    }

    /// Typing in the playlist field: Enter saves, Escape cancels.
    pub(crate) fn playlist_edit_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, text)) = &mut self.playlist_edit else { return };
        match edit_text(&mut self.caret, "playlist", text, ev, cx) {
            Edit::Submit => {
                let (target, text) = (target.clone(), text.trim().to_string());
                self.playlist_edit = None;
                window.blur();
                // An empty description clears it; an empty name is no name.
                if !text.is_empty() || matches!(target, PlaylistEdit::Describe(_)) {
                    self.save_playlist_edit(target, text, cx);
                }
            }
            Edit::Cancel => {
                self.playlist_edit = None;
                window.blur();
            }
            Edit::Changed | Edit::Moved | Edit::Ignored => {}
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Change the list shown in Playlists by hand: YouTube's own list lags behind for a while
    /// after a change, so reloading it right away would still show the old state.
    fn edit_playlist_list(&mut self, f: impl FnOnce(&mut Vec<Group>)) {
        if let Load::Ready(v) | Load::Loading(v) = &mut self.playlists.groups {
            f(v);
            store::save_list("playlists", v);
        }
    }

    fn save_playlist_edit(&mut self, target: PlaylistEdit, text: String, cx: &mut Context<Self>) {
        self.notice = Some("Saving…".into());
        let ids: Vec<String> = if target == PlaylistEdit::Queue { self.up_next.iter().map(|v| v.id.clone()).collect() } else { Vec::new() };
        let (job, name) = (target.clone(), text.clone());
        self.with_account(
            cx,
            move |a| match &job {
                PlaylistEdit::New | PlaylistEdit::Queue => a.create_playlist(&text, &ids).map(|_| ()),
                PlaylistEdit::Rename(id) => a.rename_playlist(id, &text),
                PlaylistEdit::Describe(id) => a.set_playlist_description(id, &text),
            },
            move |this, res, cx| {
                this.notice = Some(match res {
                    Err(e) => e,
                    Ok(()) => match target {
                        PlaylistEdit::New => {
                            this.reload_playlists(cx);
                            format!("Created {name}")
                        }
                        PlaylistEdit::Queue => {
                            this.reload_playlists(cx);
                            format!("Saved the queue as {name}")
                        }
                        PlaylistEdit::Rename(id) => {
                            this.edit_playlist_list(|v| v.iter_mut().filter(|g| g.id == id).for_each(|g| g.title = name.clone()));
                            format!("Renamed to {name}")
                        }
                        PlaylistEdit::Describe(_) => "Description saved".into(),
                    },
                });
                cx.notify();
            },
        );
    }

    /// Make a playlist of yours "PUBLIC", "UNLISTED" or "PRIVATE".
    pub(crate) fn set_playlist_privacy(&mut self, playlist: Group, privacy: &'static str, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        self.notice = Some(format!("Updating {}…", playlist.title));
        let id = playlist.id.clone();
        self.with_account(cx, move |a| a.set_playlist_privacy(&id, privacy), move |this, res, cx| {
            this.notice = Some(match res {
                Ok(()) => {
                    let label = format!("{}{}", &privacy[..1], privacy[1..].to_lowercase());
                    this.edit_playlist_list(|v| v.iter_mut().filter(|g| g.id == playlist.id).for_each(|g| g.privacy = Some(label.clone())));
                    format!("{} is now {}", playlist.title, label.to_lowercase())
                }
                Err(e) => e,
            });
            cx.notify();
        });
        cx.notify();
    }

    /// Delete a playlist of yours; asks for a second click first.
    pub(crate) fn delete_playlist(&mut self, playlist: Group, cx: &mut Context<Self>) {
        if self.confirm_delete_playlist.as_deref() != Some(playlist.id.as_str()) {
            self.confirm_delete_playlist = Some(playlist.id);
            cx.notify();
            return;
        }
        self.confirm_delete_playlist = None;
        self.playlist_menu = None;
        self.notice = Some(format!("Deleting {}…", playlist.title));
        let id = playlist.id.clone();
        self.with_account(cx, move |a| a.delete_playlist(&id), move |this, res, cx| {
            this.notice = Some(match res {
                Ok(()) => {
                    this.edit_playlist_list(|v| v.retain(|g| g.id != playlist.id));
                    if this.playlists.open.as_ref().is_some_and(|g| g.id == playlist.id) {
                        this.playlists.open = None;
                    }
                    format!("Deleted {}", playlist.title)
                }
                Err(e) => e,
            });
            cx.notify();
        });
        cx.notify();
    }

    /// The videos you have watched: finished here, or in your YouTube history.
    pub(crate) fn watched_ids(&self) -> HashSet<String> {
        let finished = self.history.items.iter().filter(|w| w.finished).map(|w| w.video.id.clone());
        finished.chain(self.yt_history.items().iter().map(|v| v.id.clone())).collect()
    }

    /// Take the watched videos out of Watch later; asks for a second click first.
    pub(crate) fn remove_watched_from_watch_later(&mut self, cx: &mut Context<Self>) {
        let watched = self.watched_ids();
        let gone: HashSet<String> = self.watch_later.items().iter().filter(|v| watched.contains(&v.id)).map(|v| v.id.clone()).collect();
        if gone.is_empty() {
            self.notice = Some("No watched videos in Watch later".into());
        } else if !self.confirm_clear_watched {
            self.confirm_clear_watched = true;
        } else {
            self.confirm_clear_watched = false;
            self.notice = Some(format!("Removing {} watched…", gone.len()));
            let ids = gone.clone();
            self.with_account(cx, move |a| a.remove_videos("WL", &ids), move |this, res, cx| {
                this.notice = Some(match res {
                    Ok(_) => {
                        if let Load::Ready(v) | Load::Loading(v) = &mut this.watch_later {
                            v.retain(|x| !gone.contains(&x.id));
                            store::save_list("watch-later", v);
                        }
                        format!("Removed {} watched from Watch later", gone.len())
                    }
                    Err(e) => e,
                });
                cx.notify();
            });
        }
        cx.notify();
    }
}
