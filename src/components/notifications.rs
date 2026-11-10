//! The notification on display, as a card that drops in from the top of the screen like
//! Android's heads-up notifications. It is not a bar `Part`: it is on a layer of its own along
//! the top edge, and a click dismisses it.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anti_webui::prelude::*;
use anti_webui::style::{BoxShadow, ImageSource, LineHeight};
use ui_core::{
    animation::animation::easing::ease_out_cubic, geometry::LayoutRect, island::IslandMorph,
    util::wrap_text,
};

use crate::{
    dbus::{
        notification::{NotificationStore, Urgency},
        notification_icon::ICON_SIZE,
    },
    state::State,
    ui::{color, measure},
};

const WIDTH: f32 = 460.0;
const PADDING: f32 = 18.0;
const ICON_GAP: f32 = 14.0;
const APP_SIZE: f32 = 12.0;
const SUMMARY_SIZE: f32 = 16.0;
const BODY_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 1.4;
const MAX_BODY_LINES: usize = 3;
const RADIUS: f32 = 24.0;
/// Between the card and the top and sides of the screen.
const MARGIN: f32 = 12.0;
const SHADOW_BLUR: f32 = 8.0;
const SHADOW_DROP: f32 = 3.0;
/// The tallest card, with its margin above and room for its shadow below.
const MAX_CARD_HEIGHT: f32 =
    PADDING * 2.0 + (APP_SIZE + SUMMARY_SIZE + BODY_SIZE * MAX_BODY_LINES as f32) * LINE_HEIGHT;
/// How tall the layer along the top edge is. It always is; only the card takes input.
pub const SURFACE_HEIGHT: f32 = MARGIN + MAX_CARD_HEIGHT + SHADOW_BLUR * 3.0;
const MORPH: Duration = Duration::from_millis(350);
const EASING_ID: &str = "notification_banner";

/// The notification laid out for the card. Kept after it closes, so it is still there to
/// draw while the card leaves.
#[derive(Clone, PartialEq)]
struct Shown {
    id: u32,
    urgency: Urgency,
    icon: Option<ImageSource>,
    app_name: String,
    summary: Vec<String>,
    body: Vec<String>,
    /// How many more notifications are waiting behind this one.
    others: usize,
}

impl Shown {
    fn new(store: &NotificationStore, width: f32) -> Option<Self> {
        let current = store.current()?;
        let icon = current.data.icon.clone();
        let text_width = width - text_x(icon.is_some()) - PADDING;
        let wrap = |text: &str, size: f32, lines: usize| wrap_text(text, text_width, lines, measure(size));
        Some(Self {
            id: current.id,
            urgency: current.data.urgency,
            icon,
            app_name: current.data.app_name.clone(),
            summary: wrap(&current.data.summary, SUMMARY_SIZE, 1),
            body: wrap(&current.data.body, BODY_SIZE, MAX_BODY_LINES),
            others: store.len() - 1,
        })
    }

    fn height(&self) -> f32 {
        let text = (APP_SIZE
            + SUMMARY_SIZE * self.summary.len() as f32
            + BODY_SIZE * self.body.len() as f32)
            * LINE_HEIGHT;
        let icon = if self.icon.is_some() { ICON_SIZE as f32 } else { 0.0 };
        PADDING * 2.0 + text.max(icon)
    }
}

/// Where the text starts, which is past the icon if there is one.
fn text_x(has_icon: bool) -> f32 {
    if has_icon { PADDING + ICON_SIZE as f32 + ICON_GAP } else { PADDING }
}

/// Where a card `height` tall rests on a screen `width` wide, and where it comes from: above
/// the screen's edge, shadow and all.
fn card_rects(width: f32, height: f32) -> (LayoutRect, LayoutRect) {
    let card_width = WIDTH.min(width - MARGIN * 2.0);
    let x = (width - card_width) / 2.0;
    (
        LayoutRect::new(x, MARGIN, card_width, height),
        LayoutRect::new(x, -height - SHADOW_BLUR * 3.0, card_width, height),
    )
}

/// Repaints every frame for as long as the card moves.
fn tick(frame: Signal<u64>, morph: Rc<RefCell<IslandMorph>>, ticking: Rc<Cell<bool>>) {
    request_frame(move |_| {
        frame.update(|f| *f += 1);
        if morph.borrow().is_traveling() {
            tick(frame, morph, ticking);
        } else {
            ticking.set(false);
        }
    });
}

/// The banner on a screen `width` wide and SURFACE_HEIGHT tall. `input` follows the card.
pub fn banner(state: &State, width: f32, input: Signal<Vec<RectI>>) -> Div {
    let morph = Rc::new(RefCell::new(IslandMorph::new(MORPH, Arc::new(ease_out_cubic))));
    let frame = signal(0u64);
    let ticking = Rc::new(Cell::new(false));
    let shown = signal(None::<Shown>);

    {
        let (morph, notifications) = (morph.clone(), state.notifications);
        let card_width = card_rects(width, 0.0).0.width;
        effect(move || {
            match notifications.with(|n| Shown::new(n, card_width)) {
                Some(next) => {
                    let (target, rest) = card_rects(width, next.height());
                    morph.borrow_mut().morph_to(target, rest);
                    untrack(|| shown.set(Some(next)));
                }
                None => morph.borrow_mut().collapse(),
            }
            if !ticking.replace(true) {
                tick(frame, morph.clone(), ticking.clone());
            }
        });
    }
    {
        let (morph, animation) = (morph.clone(), state.animation);
        effect(move || {
            if let Some(easing) = animation.with(|a| a.easings().get(EASING_ID).cloned()) {
                morph.borrow_mut().set_easing(easing);
            }
        });
    }

    let rect = memo(move || {
        frame.get();
        morph.borrow().rect()
    });
    effect(move || {
        let rects = rect.get().map(|r| {
            RectI::new(r.x as i32, r.y.max(0.0) as i32, r.width as i32, (r.y + r.height).max(0.0) as i32 - r.y.max(0.0) as i32)
        });
        input.set(rects.into_iter().collect());
    });

    let theme = state.theme;
    let state = state.clone();
    div().size_full().relative().select_none().child(dyn_view(move || {
        let Some(s) = shown.get() else {
            return div().into_view();
        };
        let id = s.id;
        let critical = s.urgency == Urgency::Critical;
        let secondary = move || theme.with(|t| color(t.theme().on_surface_variant));
        let primary = move || theme.with(|t| color(if critical { t.theme().error } else { t.theme().on_surface }));
        let line = |content: String, size: f32| {
            text(content)
                .text_size(size)
                .line_height(LineHeight::Relative(LINE_HEIGHT))
                .whitespace_nowrap()
        };
        let mut lines = div().flex().flex_col().flex_1().min_w(0.0).child(
            line(s.app_name.clone(), APP_SIZE).dyn_style(move |st| st.text_color(secondary())),
        );
        for summary in s.summary.clone() {
            lines = lines.child(line(summary, SUMMARY_SIZE).dyn_style(move |st| st.text_color(primary())));
        }
        for body in s.body.clone() {
            lines = lines.child(line(body, BODY_SIZE).dyn_style(move |st| st.text_color(secondary())));
        }

        let mut card = div()
            .absolute()
            .rounded(RADIUS)
            .overflow_hidden()
            .p(PADDING)
            .flex()
            .flex_row()
            .items_start()
            .gap(ICON_GAP)
            .dyn_style(move |st| {
                let Some(r) = rect.get() else {
                    return st.hidden();
                };
                theme.with(|t| {
                    let t = t.theme();
                    st.left(r.x).top(r.y).w(r.width).h(r.height).bg(color(t.surface_container_high)).shadow(
                        BoxShadow {
                            offset_y: SHADOW_DROP,
                            blur: SHADOW_BLUR,
                            color: color(t.shadow.with_alpha(0.3)),
                            ..Default::default()
                        },
                    )
                })
            })
            .on_click({
                let state = state.clone();
                move |_| state.dismiss_notification(id)
            });
        if let Some(icon) = s.icon.clone() {
            card = card.child(div().size(ICON_SIZE as f32).flex_none().bg_image(icon));
        }
        card = card.child(lines);
        if s.others > 0 {
            card = card.child(
                line(format!("+{}", s.others), APP_SIZE)
                    .flex_none()
                    .dyn_style(move |st| st.text_color(secondary())),
            );
        }
        card.into_view()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_rests_centered_below_the_top_and_starts_out_of_sight() {
        let (target, rest) = card_rects(800.0, MAX_CARD_HEIGHT);
        assert_eq!((target.x + target.width / 2.0, target.y), (400.0, MARGIN));
        assert!(target.y + target.height + SHADOW_DROP + SHADOW_BLUR <= SURFACE_HEIGHT, "the tallest card and its shadow fit");
        assert_eq!((rest.x, rest.width), (target.x, target.width));
        assert!(rest.y + rest.height + SHADOW_DROP + SHADOW_BLUR <= 0.0, "hidden, shadow and all");

        let (narrow, _) = card_rects(300.0, 100.0);
        assert_eq!((narrow.x, narrow.width), (MARGIN, 300.0 - MARGIN * 2.0));
    }
}
