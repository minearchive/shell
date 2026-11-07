use calloop::channel::Sender;
use skia_safe::{Canvas, Color4f, Image, Paint, Point};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use ui_core::{font::FontBook, pointer::button, scheme::ColorTheme, util::wrap_text};

use crate::{
    dbus::{
        notification::{NotificationEvent, Urgency},
        notification_icon::ICON_SIZE,
    },
    ui::{Component, IslandPlacement, IslandRequest, Redraw, UIState, UiEvent, ISLAND_SIDE_MARGIN},
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

/// Above other islands when critical, below them otherwise.
fn priority(urgency: Urgency) -> u8 {
    match urgency {
        Urgency::Critical => 200,
        _ => 100,
    }
}

/// The notification laid out for the island. Kept after it closes, so it is still
/// there to draw while the island collapses.
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
}

pub struct Notifications {
    ui_tx: Sender<UiEvent>,
    shown: Option<Shown>,
}

impl Notifications {
    pub fn new(ui_tx: Sender<UiEvent>) -> Self {
        Self { ui_tx, shown: None }
    }
}

impl Component for Notifications {
    fn on_notification(&mut self, _event: &NotificationEvent) -> Redraw {
        Redraw::Now
    }

    fn island(
        &mut self,
        state: &UIState,
        fonts: &FontBook,
        bar_width: f32,
    ) -> Option<IslandRequest> {
        let current = state.notifications.current()?;
        let width = WIDTH.min(bar_width - ISLAND_SIDE_MARGIN * 2.0);
        let icon = current.data.icon.clone();
        let text_width = width - text_x(icon.is_some()) - PADDING;
        let wrap = |text: &str, size: f32, lines: usize| {
            wrap_text(text, text_width, lines, |s| fonts.measure(FONT, size, s))
        };

        let shown = Shown {
            id: current.id,
            urgency: current.data.urgency,
            icon,
            app_name: current.data.app_name.clone(),
            summary: wrap(&current.data.summary, SUMMARY_SIZE, 1),
            body: wrap(&current.data.body, BODY_SIZE, MAX_BODY_LINES),
            others: state.notifications.len() - 1,
        };
        let request = IslandRequest {
            width,
            height: shown.height(),
            placement: IslandPlacement::Joined { center_x: None },
            priority: priority(shown.urgency),
        };
        self.shown = Some(shown);
        Some(request)
    }

    fn draw_island(
        &mut self,
        canvas: &Canvas,
        size: (f32, f32),
        _state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let Some(shown) = &self.shown else {
            return Redraw::None;
        };

        let paint = |color| {
            let mut paint = Paint::new(Color4f::from(color), None);
            paint.set_anti_alias(true);
            paint
        };
        let secondary = paint(theme.on_surface_variant);
        let primary = paint(if shown.urgency == Urgency::Critical {
            theme.error
        } else {
            theme.on_surface
        });

        if let Some(icon) = &shown.icon {
            canvas.draw_image(icon, (PADDING, PADDING), None);
        }

        let x = text_x(shown.icon.is_some());
        let mut y = PADDING;
        let mut line = |text: &str, text_size: f32, paint: &Paint| {
            let baseline = Point::new(x, y + text_size);
            fonts.draw(canvas, FONT, text_size, text, baseline, paint);
            y += text_size * LINE_HEIGHT;
        };

        line(&shown.app_name, APP_SIZE, &secondary);
        for text in &shown.summary {
            line(text, SUMMARY_SIZE, &primary);
        }
        for text in &shown.body {
            line(text, BODY_SIZE, &secondary);
        }

        if shown.others > 0 {
            let count = format!("+{}", shown.others);
            let x = size.0 - PADDING - fonts.measure(FONT, APP_SIZE, &count);
            let baseline = Point::new(x, PADDING + APP_SIZE);
            fonts.draw(canvas, FONT, APP_SIZE, &count, baseline, &secondary);
        }

        Redraw::None
    }

    fn on_island_cursor(&mut self, event: &PointerEvent) -> Redraw {
        if let (Some(shown), PointerEventKind::Press { button, .. }) = (&self.shown, &event.kind) {
            if *button == button::LEFT {
                let _ = self.ui_tx.send(UiEvent::DismissNotification(shown.id));
            }
        }
        Redraw::None
    }
}
