use super::*;

impl Unbloated {
    /// Keyboard shortcuts; see SHORTCUTS. Text fields stop the keys they use from reaching here.
    pub(crate) fn shortcut(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if k.key == "escape" && self.welcome_visible() {
            self.finish_welcome(cx);
            cx.stop_propagation();
            return;
        }
        if self.fullscreen && k.key == "escape" {
            self.player.set_fullscreen(false);
            cx.stop_propagation();
            return;
        }
        if self.quit_prompt {
            // The dialog is modal: Escape cancels it, other keys do nothing.
            if k.key == "escape" {
                self.quit_prompt = false;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.playlist_menu.is_some() && k.key == "escape" {
            self.playlist_menu = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.video_menu.is_some() && k.key == "escape" {
            self.close_video_menu(cx);
            cx.stop_propagation();
            return;
        }
        if self.channel_menu.is_some() && k.key == "escape" {
            self.channel_menu = None;
            self.confirm_unsub = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.key == "tab" && !k.modifiers.control && !k.modifiers.alt && !k.modifiers.platform {
            self.kb_step(k.modifiers.shift, window);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.kb_focus.is_some() {
            match k.key.as_str() {
                "enter" | "space" if !k.modifiers.control && !k.modifiers.alt && self.kb_activate(window, cx) => {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                "escape" => {
                    self.kb_focus = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                // Any other key goes back to being a shortcut (Space pauses again, and so on).
                _ => self.kb_focus = None,
            }
        }
        // Escape lets go of a selection — unless Escape is spoken for by something else on
        // screen (a dialog, search mode, the key list), which each take it below.
        let escape = k.key == "escape";
        let escape_busy = self.saving
            || self.searching
            || self.show_keys
            || self.hints.is_some()
            || (matches!(self.tab, Tab::Subscriptions | Tab::Playlists) && self.browser(self.tab).open.is_some());
        if escape && !escape_busy && self.text_sel.take().is_some() {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.settings.vim && self.vim_key(ev, window, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control && k.key == "c" && !k.modifiers.shift && !k.modifiers.alt {
            // Ctrl+C over text the mouse selected (a text field's own copy runs first, there).
            if let Some(sel) = self.text_sel.as_ref() && !sel.range().is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(sel.text().to_string()));
                cx.stop_propagation();
                return;
            }
        }
        if k.modifiers.control && k.key == "v" {
            // Ctrl+V outside a text field: play the YouTube link on the clipboard.
            match cx.read_from_clipboard().and_then(|c| c.text()).and_then(|t| yt::parse_link(&t)) {
                Some(link) => self.open_link(link, cx),
                None => self.notice = Some("No YouTube link on the clipboard".into()),
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control && k.key == "f" && self.focus_filter(window) {
            self.left_collapsed = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.alt && !k.modifiers.control && !k.modifiers.platform && matches!(k.key.as_str(), "left" | "right") {
            self.nav_go(k.key == "right", cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return;
        }
        let active = self.state.is_some() || self.casting.is_some();
        // "?" is Shift+/ on most layouts: check the typed character before the "/" key.
        if k.key_char.as_deref() == Some("?") || (self.show_keys && k.key == "escape") {
            self.show_keys = !self.show_keys;
            self.sync_embed();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        match k.key.as_str() {
            "space" | "k" => self.toggle_play(cx),
            "left" if active => self.seek_by(-5., cx),
            "right" if active => self.seek_by(5., cx),
            "j" if active => self.seek_by(-10., cx),
            "l" if active => self.seek_by(10., cx),
            "f" if active && self.casting.is_none() => self.player.set_fullscreen(!self.fullscreen),
            "m" => self.player.toggle_mute(),
            "v" => self.toggle_subtitles(cx),
            "t" => self.cast_to(None, cx),
            "up" | "=" => self.change_volume(5., cx),
            "down" | "-" => self.change_volume(-5., cx),
            "e" if k.modifiers.shift => self.toggle_player_full(cx),
            "e" => self.toggle_lower_full(window, cx),
            "b" if k.modifiers.shift => self.toggle_right_collapsed(cx),
            "b" => self.toggle_left_collapsed(cx),
            "[" => self.cycle_tabs(-1, cx),
            "]" => self.cycle_tabs(1, cx),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" if !k.modifiers.shift => self.tab_number(k.key.parse().unwrap_or(0), cx),
            "c" if k.modifiers.shift => self.copy_link_at_time(cx),
            "c" => self.copy_link(cx),
            "o" => self.open_in_browser(cx),
            "w" => self.watch_later(cx),
            "n" => self.next_prev(true, cx),
            "p" => self.next_prev(false, cx),
            "/" => self.open_search(window, cx),
            "escape" if self.saving => self.close_save(cx),
            "escape" if self.searching => self.close_search(window, cx),
            "escape" if matches!(self.tab, Tab::Subscriptions | Tab::Playlists) => self.browser(self.tab).open = None,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Vim mode keys; returns whether the key was used. See VIM_SHORTCUTS.
    pub(crate) fn vim_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let k = &ev.keystroke;
        if k.modifiers.alt || k.modifiers.platform {
            return false;
        }
        // The typed character when there is one ("G" for Shift+g), else the key's name.
        let token = match (&k.key_char, k.modifiers.control) {
            (_, true) => format!("C-{}", k.key),
            (Some(c), false) if c != " " => c.clone(),
            _ => k.key.clone(),
        };
        if self.hints.is_some() {
            self.hint_key(&token, window, cx);
            return true;
        }
        let pending_g = std::mem::take(&mut self.vim_g);
        let pending_y = std::mem::take(&mut self.vim_y);
        let active = self.state.is_some() || self.casting.is_some();
        let len = self.left_items().len();
        let page = 10;
        match token.as_str() {
            "?" => {
                self.show_keys = !self.show_keys;
                self.sync_embed();
            }
            "escape" if self.show_keys => {
                self.show_keys = false;
                self.sync_embed();
            }
            "j" | "down" => self.vim_move(1, len, window),
            "k" | "up" => self.vim_move(-1, len, window),
            "C-d" => self.vim_move(page, len, window),
            "C-u" => self.vim_move(-page, len, window),
            "C-f" => {
                self.focus_filter(window);
            }
            "g" if pending_g => self.vim_move(isize::MIN, len, window),
            "y" if pending_y => self.copy_link(cx),
            "t" if pending_y => self.copy_link_at_time(cx),
            "o" => self.open_in_browser(cx),
            "w" => self.watch_later(cx),
            "E" => self.toggle_player_full(cx),
            "e" => self.toggle_lower_full(window, cx),
            "B" => self.toggle_right_collapsed(cx),
            "b" => self.toggle_left_collapsed(cx),
            "[" => self.cycle_tabs(-1, cx),
            "]" => self.cycle_tabs(1, cx),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => self.tab_number(token.parse().unwrap_or(0), cx),
            "y" => self.vim_y = true,
            "g" => self.vim_g = true,
            "G" => self.vim_move(isize::MAX, len, window),
            "enter" | "l" => self.vim_activate(cx),
            "h" | "backspace" | "escape" => {
                if self.saving {
                    self.close_save(cx);
                } else if self.searching {
                    self.close_search(window, cx);
                } else if matches!(self.tab, Tab::Subscriptions | Tab::Playlists) {
                    self.browser(self.tab).open = None;
                }
            }
            "H" => self.vim_tab(-1, cx),
            "L" => self.vim_tab(1, cx),
            "F" => {
                let window_size = window.viewport_size();
                // Only what's on screen (the list may have laid out rows below the fold).
                let targets: Vec<_> = self
                    .hint_targets
                    .borrow()
                    .iter()
                    .filter(|(b, _)| {
                        let c = b.center();
                        c.y > px(0.) && c.y < window_size.height && c.x > px(0.) && c.x < window_size.width
                    })
                    .cloned()
                    .collect();
                if !targets.is_empty() {
                    self.hints = Some((targets, String::new()));
                }
            }
            "x" => {
                if let Some(Item::Video(v, _)) = self.left_items().get(self.vim_cursor).cloned() {
                    self.enqueue(v, cx);
                }
            }
            "space" => self.toggle_play(cx),
            "left" if active => self.seek_by(-5., cx),
            "right" if active => self.seek_by(5., cx),
            "," if active => self.seek_by(-10., cx),
            "." if active => self.seek_by(10., cx),
            "m" => self.player.toggle_mute(),
            "v" => self.toggle_subtitles(cx),
            "t" => self.cast_to(None, cx),
            "+" | "=" => self.change_volume(5., cx),
            "-" => self.change_volume(-5., cx),
            "n" => self.next_prev(true, cx),
            "p" => self.next_prev(false, cx),
            "/" => self.open_search(window, cx),
            _ => return false,
        }
        self.prefetch_cursor(cx);
        true
    }

    /// A key in hint mode: narrow down by label, click the target once a label is complete.
    pub(crate) fn hint_key(&mut self, token: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((targets, typed)) = &mut self.hints else { return };
        match token {
            "escape" => self.hints = None,
            "backspace" => {
                typed.pop();
            }
            t if t.len() == 1 && HINT_CHARS.contains(t) => {
                typed.push_str(t);
                let labels = hint_label_list(targets.len());
                let matching: Vec<usize> = (0..targets.len()).filter(|&i| labels[i].starts_with(typed.as_str())).collect();
                match matching.as_slice() {
                    [] => self.hints = None,
                    [i] if labels[*i] == *typed => {
                        // Run the element's own click handler.
                        let action = targets[*i].1.clone();
                        self.hints = None;
                        action(self, window, cx);
                    }
                    _ => {}
                }
            }
            _ => self.hints = None,
        }
    }

    /// Clickable things on screen in Tab order: the left column first, then the right one, each
    /// from top to bottom.
    pub(crate) fn kb_targets(&self, window: &Window) -> Vec<HintTarget> {
        let size = window.viewport_size();
        let divider_x = if self.left_collapsed {
            px(0.)
        } else if self.right_collapsed {
            size.width
        } else {
            size.width * self.settings.split
        };
        let mut targets: Vec<HintTarget> = self
            .hint_targets
            .borrow()
            .iter()
            .filter(|(b, _)| {
                let c = b.center();
                b.size.width > px(1.) && b.size.height > px(1.) && c.x > px(0.) && c.x < size.width && c.y > px(0.) && c.y < size.height
            })
            .cloned()
            .collect();
        targets.sort_by_key(|(b, _)| ((b.center().x >= divider_x) as u8, (f32::from(b.origin.y) / 12.).round() as i32, f32::from(b.origin.x) as i32));
        targets
    }

    /// The target the ring is on: the one closest to where it was last seen.
    pub(crate) fn kb_current(&self, targets: &[HintTarget]) -> Option<usize> {
        let at = self.kb_focus?;
        let gap = |b: &GBounds<GPixels>| {
            [b.origin.x - at.origin.x, b.origin.y - at.origin.y, b.size.width - at.size.width, b.size.height - at.size.height]
                .iter()
                .map(|d| f32::from(*d).abs())
                .sum::<f32>()
        };
        (0..targets.len()).min_by(|&i, &j| gap(&targets[i].0).total_cmp(&gap(&targets[j].0)))
    }

    /// Tab / Shift+Tab: move the focus ring to the next / previous clickable thing.
    pub(crate) fn kb_step(&mut self, back: bool, window: &mut Window) {
        if self.show_keys || self.saving || self.channel_menu.is_some() || self.video_menu.is_some() || self.playlist_menu.is_some() || self.hints.is_some() {
            return;
        }
        // Leave a text field, so typing doesn't go into it any more.
        window.focus(&self.root_focus);
        let targets = self.kb_targets(window);
        let n = targets.len();
        if n == 0 {
            return;
        }
        let next = match (self.kb_current(&targets), back) {
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
            (None, false) => 0,
            (None, true) => n - 1,
        };
        self.kb_focus = Some(targets[next].0);
    }

    /// Enter / Space on the focus ring: do what clicking that thing does.
    pub(crate) fn kb_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let targets = self.kb_targets(window);
        let Some(i) = self.kb_current(&targets) else { return false };
        let action = targets[i].1.clone();
        action(self, window, cx);
        true
    }

    /// Where `on_click_hinted` registers click targets this frame.
    pub(crate) fn hint_reg(&self) -> HintReg {
        HintReg { targets: self.hint_targets.clone() }
    }

    /// Move the selection, scrolling only when it would leave the visible rows.
    pub(crate) fn vim_move(&mut self, by: isize, len: usize, window: &Window) {
        if len == 0 {
            return;
        }
        let cursor = (self.vim_cursor as isize).saturating_add(by).clamp(0, len as isize - 1) as usize;
        self.vim_cursor = cursor;
        // History shows Continue watching rows above its scrolling list.
        let (offset, row_h) = match self.tab {
            Tab::History => (self.history_items().0.len(), ROW_H),
            Tab::Subscriptions | Tab::Playlists if self.browser_ref(self.tab).open.is_none() && !(self.tab == Tab::Subscriptions && self.subs_live) => (0, 44.),
            _ => (0, ROW_H),
        };
        let Some(ix) = cursor.checked_sub(offset) else {
            // In Continue watching, which scrolls on its own.
            let (top, visible) = scroll_window(&self.continue_scroll, ROW_H, self.settings.continue_height);
            if cursor < top {
                self.continue_scroll.scroll_to_item(cursor, ScrollStrategy::Top);
            } else if cursor >= top + visible {
                self.continue_scroll.scroll_to_item(cursor, ScrollStrategy::Bottom);
            }
            return;
        };
        let (top, visible) = scroll_window(&self.vim_scroll, row_h, f32::from(window.viewport_size().height) - 160.);
        if ix < top {
            self.vim_scroll.scroll_to_item(ix, ScrollStrategy::Top);
        } else if ix >= top + visible {
            self.vim_scroll.scroll_to_item(ix, ScrollStrategy::Bottom);
        }
    }

    pub(crate) fn vim_activate(&mut self, cx: &mut Context<Self>) {
        match self.left_items().get(self.vim_cursor).cloned() {
            Some(Item::Group(g)) => self.open_group(self.tab, g, cx),
            Some(Item::Video(v, queue)) => self.play(v, Some(queue), cx),
            None => {}
        }
    }

    pub(crate) fn vim_tab(&mut self, step: isize, cx: &mut Context<Self>) {
        let tabs = self.tab_list();
        let i = tabs.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let next = (i + step).rem_euclid(tabs.len() as isize) as usize;
        self.select_tab(tabs[next], cx);
    }

    /// Identifies the list in the left column; the Vim selection resets when it changes.
    pub(crate) fn left_key(&self) -> String {
        match self.tab {
            Tab::Subscriptions | Tab::Playlists => {
                let b = self.browser_ref(self.tab);
                format!("{:?}-{:?}-{:?}", self.tab, b.open.as_ref().map(|g| &g.id), b.view)
            }
            Tab::History => format!("History-{}", self.history_shorts()),
            tab => format!("{tab:?}"),
        }
    }

    pub(crate) fn shortcuts(&self) -> &'static [(&'static str, &'static str)] {
        if self.settings.vim { &VIM_SHORTCUTS } else { &SHORTCUTS }
    }

    /// Whether row `i` of the left list is the Vim selection.
    pub(crate) fn vim_selected(&self, i: usize) -> bool {
        self.settings.vim && self.vim_cursor == i
    }
}
