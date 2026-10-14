use gpui::{
    div, px, prelude::*, FontWeight, KeyDownEvent, SharedString, Styled,
    svg,
};
use std::time::{Duration, Instant};

use crate::{Unbloated, themed, ClipboardItem, icons};

/// Text field caret position and selection anchor.
#[derive(Clone, Default)]
pub struct Caret {
    pub id: &'static str,
    pub pos: usize,
    pub anchor: usize,
}

impl Caret {
    /// (pos, anchor) in field `id`; a field the caret wasn't in starts with it at the end.
    pub fn get(&self, id: &'static str, text: &str) -> (usize, usize) {
        let fix = |p: usize| {
            if p <= text.len() && text.is_char_boundary(p) {
                p
            } else {
                text.len()
            }
        };
        if self.id == id {
            (fix(self.pos), fix(self.anchor))
        } else {
            (text.len(), text.len())
        }
    }
}

fn prev_char(t: &str, p: usize) -> usize {
    t[..p].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_char(t: &str, p: usize) -> usize {
    t[p..].chars().next().map_or(p, |c| p + c.len_utf8())
}

fn prev_word(t: &str, p: usize) -> usize {
    t[..p].trim_end_matches(' ').rfind(' ').map_or(0, |i| i + 1)
}

fn next_word(t: &str, p: usize) -> usize {
    let rest = &t[p..];
    let word = rest.trim_start_matches(' ');
    p + (rest.len() - word.len()) + word.find(' ').unwrap_or(word.len())
}

/// Edit result from a text field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Submit,
    Cancel,
    Moved,
    Changed,
    Ignored,
}

/// Shared key handling for text fields: typing, Home/End, arrows (Ctrl: by word), Shift to
/// select, Ctrl+A/C/X/V, Backspace/Delete.
pub fn edit_text(
    caret: &mut Caret,
    id: &'static str,
    text: &mut String,
    ev: &KeyDownEvent,
    cx: &mut Context<Unbloated>,
) -> Edit {
    let k = &ev.keystroke;
    let m = &k.modifiers;
    let (mut pos, mut anchor) = caret.get(id, text);
    let (start, end) = (pos.min(anchor), pos.max(anchor));
    let selected = start != end;
    let mut edit = Edit::Changed;

    let mut move_to = |to: usize, pos: &mut usize, anchor: &mut usize| {
        *pos = to;
        if !m.shift {
            *anchor = to;
        }
        edit = Edit::Moved;
    };

    match k.key.as_str() {
        "enter" => return Edit::Submit,
        "escape" => return Edit::Cancel,
        "home" => move_to(0, &mut pos, &mut anchor),
        "end" => move_to(text.len(), &mut pos, &mut anchor),
        "left" if selected && !m.shift => move_to(start, &mut pos, &mut anchor),
        "right" if selected && !m.shift => move_to(end, &mut pos, &mut anchor),
        "left" => move_to(
            if m.control {
                prev_word(text, pos)
            } else {
                prev_char(text, pos)
            },
            &mut pos,
            &mut anchor,
        ),
        "right" => move_to(
            if m.control {
                next_word(text, pos)
            } else {
                next_char(text, pos)
            },
            &mut pos,
            &mut anchor,
        ),
        "a" if m.control => {
            (anchor, pos) = (0, text.len());
            edit = Edit::Moved;
        }
        "c" if m.control => {
            if selected {
                cx.write_to_clipboard(crate::ClipboardItem::new_string(text[start..end].to_string()));
            }
            edit = Edit::Moved;
        }
        "x" if m.control && selected => {
            cx.write_to_clipboard(crate::ClipboardItem::new_string(text[start..end].to_string()));
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" | "delete" if selected => {
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" => {
            let from = if m.control {
                prev_word(text, pos)
            } else {
                prev_char(text, pos)
            };
            text.replace_range(from..pos, "");
            (pos, anchor) = (from, from);
        }
        "delete" => {
            let to = if m.control {
                next_word(text, pos)
            } else {
                next_char(text, pos)
            };
            text.replace_range(pos..to, "");
        }
        key => {
            let typed = if key == "v" && m.control {
                cx.read_from_clipboard()
                    .and_then(|c| c.text())
                    .map(|c| c.lines().next().unwrap_or_default().to_string())
            } else if !(m.control || m.alt || m.platform) {
                k.key_char.clone()
            } else {
                None
            };
            let Some(typed) = typed else { return Edit::Ignored };
            text.replace_range(start..end, &typed);
            pos = start + typed.len();
            anchor = pos;
        }
    }
    *caret = Caret { id, pos, anchor };
    edit
}

/// The red "LIVE" tag on live streams (small on thumbnails, `big` beside the playing video).
pub fn live_badge(big: bool) -> gpui::Div {
    crate::div()
        .flex_none()
        .px(crate::px(if big { 6. } else { 4. }))
        .py(crate::px(1.))
        .rounded(crate::px(3.))
        .bg(themed(crate::ACCENT))
        .text_color(themed(crate::ON_ACCENT))
        .text_size(crate::px(if big { 11. } else { 9. }))
        .font_weight(FontWeight::BOLD)
        .child("LIVE")
}

/// Tab button: active shows accent underline, inactive muted.
pub fn tab_button(label: impl Into<SharedString>, active: bool) -> gpui::Stateful<gpui::Div> {
    let label: SharedString = label.into();
    // Id from the label's first word, so "Up next (3)" keeps one id as its count changes.
    let id = SharedString::from(label.split(' ').next().unwrap_or_default().to_string());
    crate::div()
        .id(gpui::ElementId::Name(id))
        .px_3()
        .py_2()
        .text_sm()
        .cursor_pointer()
        .border_b_2()
        .border_color(if active { themed(crate::ACCENT).into() } else { crate::Hsla::transparent_black() })
        .text_color(if active { themed(crate::TEXT) } else { themed(crate::MUTED) })
        .hover(|d| d.text_color(themed(crate::TEXT)))
        .child(label)
}

/// A small rounded chip (filter bar, group editor).
pub fn chip(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let label: SharedString = label.into();
    crate::div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_sm()
        .text_xs()
        .cursor_pointer()
        .bg(if on { themed(crate::ACCENT) } else { themed(crate::HOVER) })
        .text_color(if on { themed(crate::ON_ACCENT) } else { themed(crate::TEXT) })
        .hover(|d| d.bg(themed(crate::BORDER)))
        .flex()
        .items_center()
        .gap_1()
        .child(label)
}

/// Clear a failed load state to Idle so it can be retried.
pub fn clear_failed<T>(load: &mut crate::Load<T>) {
    if matches!(load, crate::Load::Failed(_)) {
        *load = crate::Load::Idle;
    }
}

/// Heuristic: does the error look like a login/cookie problem?
pub fn looks_like_login_error(error: &str) -> bool {
    ["sign in", "log in", "login", "cookie", "unauthorized", "403"]
        .iter()
        .any(|p| error.contains(p))
}

/// Friendly session hint from a raw error string.
pub fn session_hint(error: String) -> String {
    if error.contains("cookies database") {
        "Couldn't find that browser's profile: is it installed, and has it been opened once?".into()
    } else if error.contains("no YouTube login cookies") {
        "That browser isn't signed in to YouTube. Sign in there, then try again (Chromium-based browsers also need an unlocked keyring).".into()
    } else if error.contains("refused") || error.to_lowercase().contains("sign in") {
        format!("{error} — your YouTube session may have expired; sign in to YouTube in your browser, then try again")
    } else {
        error
    }
}