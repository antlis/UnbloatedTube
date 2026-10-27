use super::*;

impl Unbloated {
    /// Fetch your playlists again (after one was made, renamed or deleted).
    fn reload_playlists(&mut self, cx: &mut Context<Self>) {
        self.fetch(cx, "playlists", |s| &mut s.playlists.groups, Some("playlists".into()), |cfg, on| yt::playlists(cfg, on));
    }

    /// Start typing a playlist's name: `id` renames that playlist, None makes a new one.
    pub(crate) fn edit_playlist_name(&mut self, id: Option<String>, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.playlist_menu = None;
        self.playlist_edit = Some((id, name));
        window.focus(&self.playlist_edit_focus);
        cx.notify();
    }

    /// Typing in the playlist name field: Enter renames or creates, Escape cancels.
    pub(crate) fn playlist_edit_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, name)) = &mut self.playlist_edit else { return };
        match edit_text(&mut self.caret, "playlist", name, ev, cx) {
            Edit::Submit => {
                let (id, title) = (id.clone(), name.trim().to_string());
                self.playlist_edit = None;
                window.blur();
                if !title.is_empty() {
                    self.save_playlist_name(id, title, cx);
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

    fn save_playlist_name(&mut self, id: Option<String>, title: String, cx: &mut Context<Self>) {
        self.notice = Some(if id.is_some() { "Renaming…" } else { "Creating…" }.into());
        let name = title.clone();
        self.with_account(
            cx,
            move |a| match &id {
                Some(id) => a.rename_playlist(id, &title).map(|_| Some(id.clone())),
                None => a.create_playlist(&title).map(|_| None),
            },
            move |this, res, cx| {
                this.notice = Some(match res {
                    Ok(Some(id)) => {
                        this.edit_playlist_list(|v| v.iter_mut().filter(|g| g.id == id).for_each(|g| g.title = name.clone()));
                        format!("Renamed to {name}")
                    }
                    Ok(None) => {
                        this.reload_playlists(cx);
                        format!("Created {name}")
                    }
                    Err(e) => e,
                });
                cx.notify();
            },
        );
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
}
