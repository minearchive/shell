//! The notification on display, as a card that drops in from the top of the screen like
//! Android's heads-up notifications. It is not a bar `Component`: it draws on a surface
//! of its own along the top edge (`src/banner.rs`), and a click dismisses it.

use std::sync::Arc;
use std::time::Duration;

use calloop::channel::Sender;
use skia_safe::{BlurStyle, Canvas, Color, Color4f, Image, MaskFilter, Paint, Point, RRect};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use ui_core::{
    animation::{animation::easing::ease_out_cubic, parser::Easing},
    font::FontBook,
    geometry::LayoutRect,
    island::IslandMorph,
    pointer::button,
    scheme::ColorTheme,
    util::wrap_text,
};

use crate::{
    dbus::{
        notification::{NotificationStore, Urgency},
        notification_icon::ICON_SIZE,
    },
    ui::{Redraw, UIState, UiEvent},
};

const FONT: &str = "noto_sans";
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
/// How tall the surface along the top edge is. It always is; only the card takes input.
pub const SURFACE_HEIGHT: u32 = (MARGIN + MAX_CARD_HEIGHT + SHADOW_BLUR * 3.0) as u32;
const MORPH: Duration = Duration::from_millis(350);
pub const EASING_ID: &str = "notification_banner";

/// The notification laid out for the card. Kept after it closes, so it is still
/// there to draw while the card leaves.
struct Shown {
    id: u32,
    urgency: Urgency,
    icon: Option<Image>,
    app_name: String,
    summary: Vec<String>,
    body: Vec<String>,
    /// How many more notifications are waiting behind this one.
    others: usize,
}

/// Where the text starts, which is past the icon if there is one.
fn text_x(has_icon: bool) -> f32 {
    if has_icon {
        PADDING + ICON_SIZE as f32 + ICON_GAP
    } else {
        PADDING
    }
}

impl Shown {
    fn new(store: &NotificationStore, fonts: &FontBook, width: f32) -> Option<Self> {
        let current = store.current()?;
        let icon = current.data.icon.clone();
        let text_width = width - text_x(icon.is_some()) - PADDING;
        let wrap = |text: &str, size: f32, lines: usize| {
            wrap_text(text, text_width, lines, |s| fonts.measure(FONT, size, s))
        };
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
        let icon = if self.icon.is_some() {
            ICON_SIZE as f32
        } else {
            0.0
        };
        PADDING * 2.0 + text.max(icon)
    }

    /// The card's content, with the origin at its top-left corner.
    fn draw(&self, canvas: &Canvas, width: f32, fonts: &FontBook, theme: &ColorTheme) {
        let paint = |color| {
            let mut paint = Paint::new(Color4f::from(color), None);
            paint.set_anti_alias(true);
            paint
        };
        let secondary = paint(theme.on_surface_variant);
        let primary = paint(if self.urgency == Urgency::Critical {
            theme.error
        } else {
            theme.on_surface
        });

        if let Some(icon) = &self.icon {
            canvas.draw_image(icon, (PADDING, PADDING), None);
        }

        let x = text_x(self.icon.is_some());
        let mut y = PADDING;
        let mut line = |text: &str, text_size: f32, paint: &Paint| {
            let baseline = Point::new(x, y + text_size);
            fonts.draw(canvas, FONT, text_size, text, baseline, paint);
            y += text_size * LINE_HEIGHT;
        };

        line(&self.app_name, APP_SIZE, &secondary);
        for text in &self.summary {
            line(text, SUMMARY_SIZE, &primary);
        }
        for text in &self.body {
            line(text, BODY_SIZE, &secondary);
        }

        if self.others > 0 {
            let count = format!("+{}", self.others);
            let x = width - PADDING - fonts.measure(FONT, APP_SIZE, &count);
            let baseline = Point::new(x, PADDING + APP_SIZE);
            fonts.draw(canvas, FONT, APP_SIZE, &count, baseline, &secondary);
        }
    }
}

pub struct Banner {
    ui_tx: Sender<UiEvent>,
    shown: Option<Shown>,
    morph: IslandMorph,
    /// Where the card was on the last draw, for input.
    rect: Option<LayoutRect>,
}

impl Banner {
    pub fn new(ui_tx: Sender<UiEvent>, easing: Option<Easing>) -> Self {
        let easing = easing.unwrap_or_else(|| Arc::new(ease_out_cubic));
        Self {
            ui_tx,
            shown: None,
            morph: IslandMorph::new(MORPH, easing),
            rect: None,
        }
    }

    pub fn set_easing(&mut self, easing: Easing) {
        self.morph.set_easing(easing);
    }

    /// Whether there is nothing to show and nothing still leaving, so a repaint
    /// would paint the same transparent nothing again.
    pub fn is_blank(&self, state: &UIState) -> bool {
        state.notifications.current().is_none() && self.morph.rect().is_none()
    }

    /// Where the card is, in the surface's coordinates, while it is on screen.
    pub fn rect(&self) -> Option<LayoutRect> {
        self.rect
    }

    pub fn draw(
        &mut self,
        canvas: &Canvas,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
        width: f32,
    ) -> Redraw {
        canvas.clear(Color::TRANSPARENT);

        let card_width = WIDTH.min(width - MARGIN * 2.0);
        match Shown::new(&state.notifications, fonts, card_width) {
            Some(shown) => {
                let height = shown.height();
                let x = (width - card_width) / 2.0;
                let target = LayoutRect::new(x, MARGIN, card_width, height);
                // Up above the screen's edge, shadow and all.
                let rest = LayoutRect::new(x, -height - SHADOW_BLUR * 3.0, card_width, height);
                self.morph.morph_to(target, rest);
                self.shown = Some(shown);
            }
            None => self.morph.collapse(),
        }
        self.rect = self.morph.rect();
        if self.rect.is_none() {
            self.shown = None;
        }

        if let (Some(r), Some(shown)) = (self.rect, &self.shown) {
            let card = RRect::new_rect_xy(r.to_skia(), RADIUS, RADIUS);
            let mut shadow = Paint::new(Color4f::from(theme.shadow.with_alpha(0.3)), None);
            shadow.set_anti_alias(true);
            shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, SHADOW_BLUR / 2.0, None));
            canvas.draw_rrect(card.with_offset((0.0, SHADOW_DROP)), &shadow);
            let mut fill = Paint::new(Color4f::from(theme.surface_container_high), None);
            fill.set_anti_alias(true);
            canvas.draw_rrect(card, &fill);

            let saved = canvas.save();
            canvas.clip_rrect(card, None, true);
            canvas.translate((r.x, r.y));
            shown.draw(canvas, r.width, fonts, theme);
            canvas.restore_to_count(saved);
        }

        if self.morph.is_traveling() {
            Redraw::Animating
        } else {
            Redraw::None
        }
    }

    /// A click on the card dismisses it.
    pub fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        let PointerEventKind::Press { button, .. } = event.kind else {
            return Redraw::None;
        };
        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
        let on_card = self.rect.is_some_and(|r| r.contains(x, y));
        if let (true, true, Some(shown)) = (button == button::LEFT, on_card, &self.shown) {
            let _ = self.ui_tx.send(UiEvent::DismissNotification(shown.id));
        }
        Redraw::None
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Instant;

    use skia_safe::{surfaces, FontStyle};

    use super::*;
    use crate::dbus::notification::{NotificationEvent, NotificationSnapshot};

    #[test]
    fn the_card_drops_in_from_the_top_and_leaves_the_same_way() {
        let theme = crate::config::theme::Theme::default();
        let mut fonts = FontBook::new();
        fonts.register(FONT, "Noto Sans CJK JP", FontStyle::normal());
        let mut surface = surfaces::raster_n32_premul((800, SURFACE_HEIGHT as i32)).unwrap();
        let mut banner = Banner::new(calloop::channel::channel().0, None);
        let mut state = UIState::default();
        let mut draw = |banner: &mut Banner, state: &UIState, settle: bool| {
            let redraw = banner.draw(surface.canvas(), state, &fonts, theme.theme(), 800.0);
            if !settle {
                return redraw;
            }
            thread::sleep(MORPH + Duration::from_millis(20));
            banner.draw(surface.canvas(), state, &fonts, theme.theme(), 800.0)
        };

        assert!(banner.is_blank(&state));
        let data = NotificationSnapshot {
            summary: "a summary".into(),
            body: "a body that goes on long enough to wrap, and on, and then on some more \
                   until it has filled every line the card allows and then some more"
                .into(),
            expire_timeout: 0,
            ..Default::default()
        };
        let now = Instant::now();
        state
            .notifications
            .apply(&NotificationEvent::Posted { id: 1, data }, now);
        assert!(!banner.is_blank(&state));

        assert_eq!(draw(&mut banner, &state, true), Redraw::None);
        let rect = banner.rect().expect("on screen");
        assert_eq!((rect.x + rect.width / 2.0, rect.y), (400.0, MARGIN));
        assert!(
            rect.y + rect.height + SHADOW_BLUR * 3.0 <= SURFACE_HEIGHT as f32,
            "the tallest card and its shadow fit"
        );

        state.notifications.remove(1);
        let leaving = draw(&mut banner, &state, false);
        assert_eq!(leaving, Redraw::Animating);
        assert!(!banner.is_blank(&state), "still leaving");
        draw(&mut banner, &state, true);
        assert!(banner.rect().is_none() && banner.is_blank(&state));
    }
}
