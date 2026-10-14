use super::*;

/// A click target for `f` hints: where it is on screen and what clicking it does.
pub(crate) type HintTarget = (GBounds<GPixels>, Rc<dyn Fn(&mut Unbloated, &mut Window, &mut Context<Unbloated>)>);

#[derive(Clone)]
pub(crate) struct HintReg {
    pub(crate) targets: Rc<RefCell<Vec<HintTarget>>>,
}

/// `on_click` that also registers the element for `f` hints and Tab focus, which run the same handler.
pub(crate) trait OnClickHinted: StatefulInteractiveElement + ParentElement + Styled + Sized {
    fn on_click_hinted(
        self,
        h: &HintReg,
        cx: &mut Context<Unbloated>,
        f: impl Fn(&mut Unbloated, &gpui::ClickEvent, &mut Window, &mut Context<Unbloated>) + 'static,
    ) -> Self {
        let f = Rc::new(f);
        let click = f.clone();
        let el = self.on_click(cx.listener(move |this, ev, window, cx| click(this, ev, window, cx)));
        let targets = h.targets.clone();
        let action: Rc<dyn Fn(&mut Unbloated, &mut Window, &mut Context<Unbloated>)> =
            Rc::new(move |this, window, cx| f(this, &gpui::ClickEvent::default(), window, cx));
        el.relative().child(
            canvas(move |bounds, _, _| targets.borrow_mut().push((bounds, action.clone())), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
    }
}

impl OnClickHinted for Stateful<gpui::Div> {}

/// Letters used for hint labels (home row first).
pub(crate) const HINT_CHARS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// Labels for `n` targets: single letters when they fit, else two letters for all (so no label
/// is a prefix of another).
pub(crate) fn hint_label_list(n: usize) -> Vec<String> {
    let chars: Vec<char> = HINT_CHARS.chars().collect();
    if n <= chars.len() {
        chars.iter().take(n).map(|c| c.to_string()).collect()
    } else {
        chars.iter().flat_map(|a| chars.iter().map(move |b| format!("{a}{b}"))).take(n).collect()
    }
}

/// Hint mode overlay: a yellow label on each target still matching what was typed.
pub(crate) fn hint_labels(targets: &[HintTarget], typed: &str) -> gpui::Div {
    let labels = hint_label_list(targets.len());
    div().absolute().top_0().left_0().size_full().children(targets.iter().zip(labels).filter(|(_, l)| l.starts_with(typed)).map(
        |((b, _), label)| {
            div()
                .absolute()
                .left(b.origin.x)
                .top(b.origin.y)
                .px_1()
                .rounded_sm()
                .bg(rgb(0xffd54a))
                .border_1()
                .border_color(rgb(0x8a6d00))
                .text_xs()
                .text_color(rgb(0x1a1a1a))
                .child(label.to_uppercase())
        },
    ))
}
