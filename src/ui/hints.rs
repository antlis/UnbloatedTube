use gpui::{
    canvas, div, prelude::*, Bounds as GBounds, IntoElement, Pixels as GPixels, Rgba, SharedString,
    Stateful, px, AnimationExt,
};
use std::rc::Rc;
use std::sync::Arc;

use crate::{themed};

/// Letters used for hint labels (home row first).
#[allow(dead_code)]
const HINT_CHARS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// A click target for `f` hints: where it is on screen and what clicking it does.
#[allow(dead_code)]
pub type HintTarget = (crate::GBounds<crate::GPixels>, Rc<dyn Fn(&mut crate::Unbloated, &mut crate::Window, &mut crate::Context<crate::Unbloated>)>);

/// Registry for clickable hint targets.
#[allow(dead_code)]
#[derive(Clone)]
pub struct HintReg {
    targets: Arc<std::sync::Mutex<Vec<HintTarget>>>,
}

impl Default for HintReg {
    fn default() -> Self {
        Self {
            targets: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl HintReg {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        self.targets.lock().unwrap().clear();
    }

    #[allow(dead_code)]
    pub fn targets(&self) -> Vec<HintTarget> {
        self.targets.lock().unwrap().clone()
    }
}

/// Labels for `n` targets: single letters when they fit, else two letters for all (so no label
/// is a prefix of another).
#[allow(dead_code)]
pub fn hint_label_list(n: usize) -> Vec<String> {
    let chars: Vec<char> = HINT_CHARS.chars().collect();
    if n <= chars.len() {
        chars.iter().take(n).map(|c| c.to_string()).collect()
    } else {
        chars
            .iter()
            .flat_map(|a| chars.iter().map(move |b| format!("{a}{b}")))
            .take(n)
            .collect()
    }
}

/// `on_click` that also registers the element for `f` hints and Tab focus, which run the same handler.
#[allow(dead_code)]
pub trait OnClickHinted: gpui::StatefulInteractiveElement + gpui::ParentElement + gpui::Styled + Sized {
    fn on_click_hinted(
        self,
        h: &crate::ui::hints::HintReg,
        cx: &mut crate::Context<crate::Unbloated>,
        f: impl Fn(&mut crate::Unbloated, &gpui::ClickEvent, &mut crate::Window, &mut crate::Context<crate::Unbloated>) + 'static,
    ) -> Self {
        let f = std::rc::Rc::new(f);
        let click = f.clone();
        let el = self.on_click(cx.listener(move |this, ev, window, cx| click(this, ev, window, cx)));
        let targets = h.targets.clone();
        let action: Rc<dyn Fn(&mut crate::Unbloated, &mut crate::Window, &mut crate::Context<crate::Unbloated>)> =
            std::rc::Rc::new(move |this, window, cx| f(this, &gpui::ClickEvent::default(), window, cx));
        el.relative().child(
            canvas(
                move |bounds, _, _| targets.lock().unwrap().push((bounds, action.clone())),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl OnClickHinted for gpui::Stateful<gpui::Div> {}

/// Labels for `n` targets: single letters when they fit, else two letters for all (so no label
/// is a prefix of another).

/// Hint mode overlay: a yellow label on each target still matching what was typed.
#[allow(dead_code)]
pub fn hint_labels(
    targets: &[HintTarget],
    typed: &str,
) -> gpui::Div {
    let labels = hint_label_list(targets.len());
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .children(
            targets
                .iter()
                .zip(labels)
                .filter(|(_, l)| l.starts_with(typed))
                .map(|((b, _), label)| {
                    div()
                        .absolute()
                        .left(b.origin.x)
                        .top(b.origin.y)
                        .px_1()
                        .py(px(1.))
                        .rounded_sm()
                        .bg(themed(crate::ACCENT))
                        .text_color(themed(crate::ON_ACCENT))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(label)
                }),
        )
}