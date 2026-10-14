use super::*;

pub(crate) fn divider(id: &'static str, split: Split, cx: &mut Context<Unbloated>) -> Stateful<gpui::Div> {
    let bar = div().id(id).flex_none().bg(themed(BORDER)).hover(|d| d.bg(themed(MUTED)));
    let bar = match split {
        Split::Columns => bar.w(px(5.)).h_full().cursor(CursorStyle::ResizeLeftRight),
        Split::Player | Split::Continue => bar.h(px(5.)).w_full().cursor(CursorStyle::ResizeUpDown),
    };
    bar.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, e: &gpui::MouseDownEvent, _, _| {
            this.dragging = Some(split);
            this.drag_from = (f32::from(e.position.y), this.settings.continue_height);
        }),
    )
}
