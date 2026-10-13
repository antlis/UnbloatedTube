use gpui::{div, px, prelude::*, rgb, Rgba, Transformation, radians, AnimationExt};

use crate::themed;

pub fn skeleton_thumb() -> impl gpui::IntoElement {
    gpui::div()
        .flex_none()
        .w(px(120.))
        .h(px(68.))
        .rounded_md()
        .bg(rgb(0x2a2a2a))
        .with_animation(
            "shimmer",
            gpui::Animation::new(std::time::Duration::from_millis(1500)).repeat(),
            |d, t| d.opacity(0.5 + 0.5 * (t * 2.0 * std::f32::consts::PI).sin()),
        )
}

/// Skeleton placeholder for a text line.
pub fn skeleton_line() -> impl gpui::IntoElement {
    gpui::div()
        .flex_1()
        .h(px(14.))
        .rounded(px(3.))
        .bg(rgb(0x2a2a2a))
        .with_animation(
            "shimmer",
            gpui::Animation::new(std::time::Duration::from_millis(1500)).repeat(),
            |d, t| d.opacity(0.5 + 0.5 * (t * 2.0 * std::f32::consts::PI).sin()),
        )
}

/// Skeleton for a group/channels row.
pub fn skeleton_group() -> impl gpui::IntoElement {
    gpui::div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            gpui::div()
                .flex_none()
                .w(px(32.))
                .h(px(32.))
                .rounded_full()
                .bg(rgb(0x2a2a2a))
                .with_animation(
                    "shimmer",
                    gpui::Animation::new(std::time::Duration::from_millis(1500)).repeat(),
                    |d, t| d.opacity(0.5 + 0.5 * (t * 2.0 * std::f32::consts::PI).sin()),
                ),
        )
        .child(
            gpui::div()
                .flex_1()
                .h(px(14.))
                .rounded(px(3.))
                .bg(rgb(0x2a2a2a))
                .with_animation(
                    "shimmer",
                    gpui::Animation::new(std::time::Duration::from_millis(1500)).repeat(),
                    |d, t| d.opacity(0.5 + 0.5 * (t * 2.0 * std::f32::consts::PI).sin()),
                ),
        )
}

/// Loading spinner.
pub fn spinner() -> impl gpui::IntoElement {
    gpui::div()
        .flex_none()
        .w(px(16.))
        .h(px(16.))
        .border_2()
        .border_color(themed(crate::ACCENT))
        .border_color(themed(crate::BORDER))
        .rounded_full()
        .with_animation(
            "spin",
            gpui::Animation::new(std::time::Duration::from_millis(600)).repeat(),
            |d, t| d.opacity(0.3 + 0.7 * (t * 2.0 * std::f32::consts::PI).sin()),
        )
}