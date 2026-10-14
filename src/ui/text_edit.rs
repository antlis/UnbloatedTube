use gpui::{App, ClipboardItem, KeyDownEvent};

pub(crate) enum Edit {
    Changed,
    /// Only the cursor or selection moved.
    Moved,
    Submit,
    Cancel,
    Ignored,
}

/// Minimal one-line text editing: type (any layout), Backspace, Ctrl+Backspace, Ctrl+V, Enter, Esc.
/// Where the text cursor is, in the one text field being edited (`id`), and where a
/// selection started (`anchor`; equal to `pos` when nothing is selected). Byte offsets.
#[derive(Default)]
pub(crate) struct Caret {
    pub(crate) id: &'static str,
    pub(crate) pos: usize,
    pub(crate) anchor: usize,
}

impl Caret {
    /// (pos, anchor) in field `id`; a field the caret wasn't in starts with it at the end.
    pub(crate) fn get(&self, id: &'static str, text: &str) -> (usize, usize) {
        let fix = |p: usize| if p <= text.len() && text.is_char_boundary(p) { p } else { text.len() };
        if self.id == id { (fix(self.pos), fix(self.anchor)) } else { (text.len(), text.len()) }
    }
}

pub(crate) fn prev_char(t: &str, p: usize) -> usize {
    t[..p].char_indices().next_back().map_or(0, |(i, _)| i)
}

pub(crate) fn next_char(t: &str, p: usize) -> usize {
    t[p..].chars().next().map_or(p, |c| p + c.len_utf8())
}

pub(crate) fn prev_word(t: &str, p: usize) -> usize {
    t[..p].trim_end_matches(' ').rfind(' ').map_or(0, |i| i + 1)
}

pub(crate) fn next_word(t: &str, p: usize) -> usize {
    let rest = &t[p..];
    let word = rest.trim_start_matches(' ');
    p + (rest.len() - word.len()) + word.find(' ').unwrap_or(word.len())
}

/// Shared key handling for text fields: typing, Home/End, arrows (Ctrl: by word), Shift to
/// select, Ctrl+A/C/X/V, Backspace/Delete.
pub(crate) fn edit_text(caret: &mut Caret, id: &'static str, text: &mut String, ev: &KeyDownEvent, cx: &App) -> Edit {
    let k = &ev.keystroke;
    let m = &k.modifiers;
    let (mut pos, mut anchor) = caret.get(id, text);
    let (start, end) = (pos.min(anchor), pos.max(anchor));
    let selected = start != end;
    let mut edit = Edit::Changed;
    // Move the cursor; with Shift, the selection's other end stays.
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
        "left" => move_to(if m.control { prev_word(text, pos) } else { prev_char(text, pos) }, &mut pos, &mut anchor),
        "right" => move_to(if m.control { next_word(text, pos) } else { next_char(text, pos) }, &mut pos, &mut anchor),
        "a" if m.control => {
            (anchor, pos) = (0, text.len());
            edit = Edit::Moved;
        }
        "c" if m.control => {
            if selected {
                cx.write_to_clipboard(ClipboardItem::new_string(text[start..end].to_string()));
            }
            edit = Edit::Moved;
        }
        "x" if m.control && selected => {
            cx.write_to_clipboard(ClipboardItem::new_string(text[start..end].to_string()));
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" | "delete" if selected => {
            text.replace_range(start..end, "");
            (pos, anchor) = (start, start);
        }
        "backspace" => {
            let from = if m.control { prev_word(text, pos) } else { prev_char(text, pos) };
            text.replace_range(from..pos, "");
            (pos, anchor) = (from, from);
        }
        "delete" => {
            let to = if m.control { next_word(text, pos) } else { next_char(text, pos) };
            text.replace_range(pos..to, "");
        }
        key => {
            let typed = if key == "v" && m.control {
                cx.read_from_clipboard().and_then(|c| c.text()).map(|c| c.lines().next().unwrap_or_default().to_string())
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
