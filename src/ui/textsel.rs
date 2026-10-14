//! Mouse text selection over the app's static text. GPUI draws none of its own — a `div`
//! holding text is pixels you can drag over and nothing happens, and `InteractiveText` only
//! reports clicks on fixed ranges — so the selection lives here: a press starts it at the
//! character under the pointer, a drag extends it past the block's own edges, and Ctrl+C
//! copies what it covers.
//!
//! The selection is app state (`Unbloated::text_sel`), not element state: the block draws it
//! when it renders, the root window's mouse handlers follow the drag (they fire wherever the
//! pointer goes, unlike a hitbox's), and Ctrl+C can reach it from the keyboard.

use crate::Unbloated;
use gpui::{
    div, prelude::*, App, Context, Div, HighlightStyle, InteractiveText, IntoElement, MouseDownEvent,
    MouseButton, Pixels, Point, SharedString, StyledText, TextLayout, UnderlineStyle,
};
use std::ops::Range;

/// A selection over one block of text: the block it's in (`key`, so only that block draws it),
/// the block's whole text (what Ctrl+C takes, kept so the copy doesn't depend on the block
/// still being laid out), and the byte offsets the press (`anchor`) and the pointer (`focus`)
/// are at in it.
pub struct TextSel {
    key: String,
    text: String,
    anchor: usize,
    focus: usize,
    /// The block's text layout, to map the pointer's position to a character offset while the
    /// drag goes past the block's edges (its handlers can't see that; the window's can).
    layout: TextLayout,
}

impl TextSel {
    /// A new, empty selection in `key`'s block, starting (and ending) at the character under
    /// `position` — the press that begins a drag. The text comes from the layout, not the
    /// caller: a `.truncate()`ed or `.line_clamp()`ed block lays out less than it was given,
    /// and the offsets the layout reports are into what it actually laid out.
    pub fn pressed(key: String, layout: TextLayout, position: Point<Pixels>) -> Self {
        let sel = TextSel { key, text: layout.text(), anchor: 0, focus: 0, layout };
        let at = sel.at(position);
        TextSel { anchor: at, focus: at, ..sel }
    }

    /// Move the drag's end to the character under `position`; whether that changed anything.
    pub fn drag_to(&mut self, position: Point<Pixels>) -> bool {
        let at = self.at(position);
        let moved = at != self.focus;
        self.focus = at;
        moved
    }

    /// What the selection covers, smaller offset first.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    /// The selected text, empty when nothing is selected.
    pub fn text(&self) -> &str {
        &self.text[self.range()]
    }

    /// The byte offset in this block's text nearest `position` — clamped to the text, and to
    /// a character boundary (`index_for_position` can land between two).
    fn at(&self, position: Point<Pixels>) -> usize {
        // Err carries the nearest offset, for a position past either end of the text.
        let at = self.layout.index_for_position(position).unwrap_or_else(|nearest| nearest);
        let at = at.min(self.text.len());
        let mut ix = at;
        while ix > 0 && !self.text.is_char_boundary(ix) {
            ix -= 1;
        }
        ix
    }
}

/// `text` with `links` in it clickable (each calling `on_link` with its index, as
/// `InteractiveText` did), `marks` of the app's own drawn over it (a search match), and the
/// selection drawn when it's `key`'s. Pressing on it starts a selection at the character under
/// the pointer (see `main::Unbloated::text_sel`).
pub fn text(
    key: String,
    text: String,
    links: Vec<Range<usize>>,
    marks: Vec<(Range<usize>, HighlightStyle)>,
    sel: Option<&TextSel>,
    on_link: impl Fn(usize, &mut App) + 'static,
    cx: &mut Context<Unbloated>,
) -> Div {
    let selected = sel.filter(|s| s.key == key).map(TextSel::range);
    // The selection's offsets come from the block's layout, which may hold less than `text`
    // (see `TextSel::pressed`), so they're floored to a character of the text they'll paint.
    let selected = selected.map(|range| floor(range, &text));
    let highlights = runs(&text, &links, &marks, selected);
    let styled = StyledText::new(text).with_highlights(highlights);
    // Cloned out of the StyledText before it moves into the tree: the same Rc the text lays
    // itself out into, so the handlers below can map the pointer through it.
    let layout = styled.layout().clone();
    let inner = if links.is_empty() {
        styled.into_any_element()
    } else {
        let this = cx.entity().downgrade();
        InteractiveText::new(SharedString::from(key.clone()), styled)
            .on_click(links, move |ix, _, cx| {
                // A drag over the block was selecting, not clicking the link in it. Read the
                // selection rather than stepping into the app: `on_link` updates the app itself,
                // and one update inside another doesn't end well.
                let selecting = this
                    .read_with(cx, |this, _| this.text_sel.as_ref().is_some_and(|sel| !sel.range().is_empty()))
                    .unwrap_or(false);
                if selecting {
                    return;
                }
                on_link(ix, cx);
            })
            .into_any_element()
    };
    div()
        .child(inner)
        // The press starts the selection; the root window's move/up handlers extend and end
        // it, so a drag keeps going past this block's edges (see `main::Unbloated::render`).
        // The cursor is the caller's call: a transcript row would rather point than select.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                // Clicking prose leaves a text field, the way clicking anywhere else outside
                // one does not: the field would otherwise keep the keyboard — and Ctrl+C —
                // while the press over here is what you're actually working with.
                if window.focused(cx).is_some() && !this.root_focus.within_focused(window, cx) {
                    window.blur();
                }
                this.text_sel = Some(TextSel::pressed(key.clone(), layout.clone(), ev.position));
                // The root's own press handler runs after this one (it's the ancestor, and
                // mouse events bubble outwards), so it needs telling to leave the selection.
                this.text_press = true;
                cx.notify();
            }),
        )
}

/// `range` with both ends pulled onto a character boundary of `text` they fall inside (and
/// inside `text`): the offsets come from a layout that may hold less than the text painted.
fn floor(range: Range<usize>, text: &str) -> Range<usize> {
    let mut start = range.start.min(text.len());
    let mut end = range.end.min(text.len()).max(start);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    start..end
}

/// The runs to paint `text` with: `links` as the app's accent links (underlined), `marks` as
/// the caller's own (a search match), and the selection's background over whatever it covers.
///
/// GPUI paints a `StyledText` run by run, in order and never overlapping, so the lot is cut
/// into the pieces between every edge and each piece carries the styles covering it. Pushed in
/// that order, so where a selection covers a mark its background wins.
fn runs(
    text: &str,
    links: &[Range<usize>],
    marks: &[(Range<usize>, HighlightStyle)],
    selected: Option<Range<usize>>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let link = HighlightStyle {
        color: Some(crate::ui::themed(crate::ui::ACCENT).into()),
        underline: Some(UnderlineStyle { thickness: gpui::px(1.), ..Default::default() }),
        ..Default::default()
    };
    let mut spans: Vec<(Range<usize>, HighlightStyle)> =
        links.iter().cloned().map(|r| (r, link)).collect();
    spans.extend(marks.iter().cloned());
    if let Some(range) = selected.filter(|r| !r.is_empty()) {
        spans.push((range, HighlightStyle { background_color: Some(sel_bg()), ..Default::default() }));
    }
    if spans.len() < 2 {
        return spans;
    }
    let len = text.len();
    let mut edges: Vec<usize> = vec![0, len];
    edges.extend(spans.iter().flat_map(|(r, _)| [r.start, r.end]));
    edges.sort_unstable();
    edges.dedup();
    let mut runs = Vec::new();
    for pair in edges.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        // Every style covering this whole piece, merged; None fields let the earlier ones
        // stand (the selection sets only a background, a link only a color and underline).
        let mut style = HighlightStyle::default();
        let mut any = false;
        for (range, one) in &spans {
            if range.start <= start && end <= range.end {
                style.color = one.color.or(style.color);
                style.background_color = one.background_color.or(style.background_color);
                style.underline = one.underline.or(style.underline);
                style.font_weight = one.font_weight.or(style.font_weight);
                style.font_style = one.font_style.or(style.font_style);
                style.strikethrough = one.strikethrough.or(style.strikethrough);
                style.fade_out = one.fade_out.or(style.fade_out);
                any = true;
            }
        }
        if any {
            runs.push((start..end, style));
        }
    }
    runs
}

/// The selection's background: the accent at the same wash the text fields' selection uses.
fn sel_bg() -> gpui::Hsla {
    gpui::rgba((crate::ui::ACCENT << 8) | 0x66).into()
}

#[cfg(test)]
mod tests {
    use super::{floor, runs, sel_bg};
    use gpui::{HighlightStyle, Hsla};

    fn mark() -> HighlightStyle {
        HighlightStyle { background_color: Some(gpui::rgb(0x010203).into()), ..Default::default() }
    }

    /// A colour as the hex it paints, so the expectations below read as colours — `Hsla`
    /// itself isn't `PartialEq` (and its fields are hue, not red).
    fn hex(c: Option<Hsla>) -> Option<u32> {
        let c = gpui::Rgba::from(c?);
        let q = |v: f32| (v * 255.).round() as u32;
        Some(q(c.r) << 16 | q(c.g) << 8 | q(c.b))
    }

    /// What `runs` would paint, piece by piece: (range, colour, wash, underlined).
    fn pieces(
        text: &str,
        links: &[std::ops::Range<usize>],
        marks: &[(std::ops::Range<usize>, HighlightStyle)],
        sel: Option<std::ops::Range<usize>>,
    ) -> Vec<(std::ops::Range<usize>, Option<u32>, Option<u32>, bool)> {
        runs(text, links, marks, sel)
            .into_iter()
            .map(|(r, s)| (r, hex(s.color), hex(s.background_color), s.underline.is_some()))
            .collect()
    }

    #[test]
    fn floors_offsets_to_a_character() {
        // "aébc": a at 0, é at 1..3, b at 3, c at 4 — five bytes.
        let text = "aébc";
        assert_eq!(floor(0..5, text), 0..5);
        assert_eq!(floor(1..3, text), 1..3);
        // Mid-character ends grow apart rather than clip the character they're inside.
        assert_eq!(floor(2..4, text), 1..4);
        assert_eq!(floor(2..2, text), 1..3);
        // Past the end pulls back into it.
        assert_eq!(floor(9..9, text), 5..5);
    }

    #[test]
    fn selection_and_link_share_a_piece() {
        // A link's colour and the selection's wash both survive where they cover together,
        // and the runs abut: 0..2, 2..4, 4..6, nothing painted outside them.
        let accent = hex(Some(Hsla::from(crate::ui::themed(crate::ui::ACCENT)))).unwrap();
        let wash = hex(Some(sel_bg())).unwrap();
        assert_eq!(
            pieces("abcdef", &[0..4], &[], Some(2..6)),
            [
                (0..2, Some(accent), None, true),
                (2..4, Some(accent), Some(wash), true),
                (4..6, None, Some(wash), false),
            ]
        );
    }

    #[test]
    fn the_selection_covers_where_it_is() {
        // Nothing painted over a link-free block outside the selection.
        let wash = hex(Some(sel_bg())).unwrap();
        assert_eq!(pieces("hello world", &[], &[], Some(6..11)), [(6..11, None, Some(wash), false)]);
        assert!(pieces("hello world", &[], &[], None).is_empty());
        assert!(pieces("hello world", &[], &[], Some(6..6)).is_empty());
    }

    #[test]
    fn selection_beats_a_mark_it_covers() {
        // Both set a wash: the piece the selection covers shows the selection's, the piece it
        // doesn't shows the mark's, and the mark isn't painted twice.
        let wash = hex(Some(sel_bg())).unwrap();
        let wanted = hex(Some(Hsla::from(gpui::rgb(0x010203)))).unwrap();
        assert_eq!(
            pieces("hello world", &[], &[(5..7, mark())], Some(0..6)),
            [(0..5, None, Some(wash), false), (5..6, None, Some(wash), false), (6..7, None, Some(wanted), false)]
        );
    }
}
