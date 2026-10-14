use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, ElementId, Hsla, SharedString, Stateful, UniformListScrollHandle, Window, div,
    prelude::*, px, relative, svg,
};

use super::*;
use crate::ui::icons;

pub(crate) enum Thumb {
    Ready(PathBuf),
    /// Downloading: shown as a skeleton.
    Pending,
    /// No image, or the download failed.
    Missing,
}

/// Shape of skeleton rows for a list that hasn't loaded yet.
#[derive(Clone, Copy)]
pub(crate) enum Rows {
    Videos,
    Groups,
    Comments,
}

/// Fade an element in and out, as a loading placeholder.
pub(crate) fn pulse(id: impl Into<ElementId>, el: gpui::Div) -> AnyElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(1400)).repeat().with_easing(gpui::pulsating_between(0.35, 0.9)),
        |d, t| d.opacity(t),
    )
    .into_any_element()
}

/// Grey rows shaped like the real ones, pulsing while a list loads.
pub(crate) fn skeleton(rows: Rows) -> AnyElement {
    let bar = |w: f32, h: f32| div().w(relative(w)).h(px(h)).rounded_sm().bg(themed(HOVER));
    let widths = [0.7, 0.55, 0.8, 0.45, 0.65, 0.6, 0.75, 0.5];
    let list = div().flex().flex_col().children((0..12).map(|i| {
        let w = widths[i % widths.len()];
        match rows {
            Rows::Videos => div()
                .h(px(ROW_H))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .child(div().w(px(96.)).h(px(54.)).flex_none().rounded(px(4.)).bg(themed(HOVER)))
                .child(div().flex_1().flex().flex_col().gap_2().child(bar(w, 12.)).child(bar(w * 0.45, 10.))),
            Rows::Comments => div().px_3().py_2().flex().flex_col().gap_2().child(bar(0.3, 10.)).child(bar(w, 12.)).child(bar(w * 0.6, 12.)),
            Rows::Groups => div()
                .h(px(44.))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .child(div().size(px(28.)).flex_none().rounded_full().bg(themed(HOVER)))
                .child(div().flex_1().child(bar(w * 0.5, 12.))),
        }
    }));
    pulse("skeleton", list)
}

/// Indeterminate progress: a short accent bar sweeping left to right.
pub(crate) fn loading_bar(id: &'static str) -> impl IntoElement {
    div().relative().w_full().h(px(2.)).overflow_hidden().child(
        div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.3))
            .bg(themed(ACCENT))
            .with_animation(id, Animation::new(Duration::from_millis(1200)).repeat(), |d, t| {
                d.left(relative(t * 1.3 - 0.3))
            }),
    )
}

/// Hover tooltip content.
/// `left`: opens to the left of the pointer, for things at the left column's right edge,
/// where a tooltip would run under the (always on top) video.
pub(crate) struct Tip(SharedString, bool);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tip = div()
            .whitespace_nowrap()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(themed(HOVER))
            .border_1()
            .border_color(themed(BORDER))
            .text_xs()
            .text_color(themed(TEXT))
            .child(self.0.clone());
        if self.1 {
            // A zero-width anchor at the pointer, with the tooltip hanging off to its left.
            div().relative().w_0().child(tip.absolute().right(px(8.)).top_0())
        } else {
            tip
        }
    }
}

pub(crate) fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone(), false)).into()
}

pub(crate) fn tip_left(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone(), true)).into()
}

/// Square icon button with a hover tooltip; `enabled: false` greys it out (add clicks only when enabled).
/// The red "LIVE" tag on live streams (small on thumbnails, `big` beside the playing video).
pub(crate) fn live_badge(big: bool) -> gpui::Div {
    div()
        .flex_none()
        .px(px(if big { 6. } else { 4. }))
        .py(px(1.))
        .rounded(px(3.))
        .bg(themed(ACCENT))
        .text_color(themed(ON_ACCENT))
        .text_size(px(if big { 11. } else { 9. }))
        .font_weight(gpui::FontWeight::BOLD)
        .child("LIVE")
}

pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    icon: &str,
    tooltip: impl Into<SharedString>,
    enabled: bool,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .mr_2()
        .p(px(7.))
        .rounded_md()
        .bg(themed(HOVER))
        .child(svg().path(icons::path(icon)).size(px(16.)).text_color(if enabled { themed(TEXT) } else { themed(BORDER) }))
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(themed(BORDER))))
        .tooltip(tip(tooltip))
}

/// First visible row and how many rows fit, for a uniform list of `row_h`-tall rows. (GPUI's own
/// `logical_scroll_top` always says 0 for uniform lists.) `fallback_h` is used until the list has
/// been laid out once.
pub(crate) fn scroll_window(handle: &UniformListScrollHandle, row_h: f32, fallback_h: f32) -> (usize, usize) {
    let state = handle.0.borrow();
    let top = (f32::from(-state.base_handle.offset().y) / row_h).max(0.).floor() as usize;
    let h = f32::from(state.base_handle.bounds().size.height);
    let visible = (if h > 0. { h } else { fallback_h } / row_h).floor().max(1.) as usize;
    (top, visible)
}

pub(crate) fn tab_button(label: impl Into<SharedString>, active: bool) -> Stateful<gpui::Div> {
    let label: SharedString = label.into();
    // Id from the label's first word, so "Up next (3)" keeps one id as its count changes.
    let id = SharedString::from(label.split(' ').next().unwrap_or_default().to_string());
    div()
        .id(ElementId::Name(id))
        .px_3()
        .py_2()
        .text_sm()
        .cursor_pointer()
        .border_b_2()
        .border_color(if active { themed(ACCENT).into() } else { Hsla::transparent_black() })
        .text_color(if active { themed(TEXT) } else { themed(MUTED) })
        .hover(|d| d.text_color(themed(TEXT)))
        .child(label)
}
