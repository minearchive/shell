use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::Local;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use ui_core::{
    animation::{
        animation::{easing::ease_out_bounce, Animation},
        parser::Easing,
    },
    font::FontBook,
    pointer::button,
    scheme::ColorTheme,
    util::BoundingBox,
};

use crate::{
    config::animation::AnimationConfig,
    ui::{Component, Redraw, UIState},
};

const EASING_ID: &str = "a";

pub struct Clock {
    animation: Animation<f32>,
    destination: f32,
    bounding: BoundingBox,
}

impl Clock {
    pub fn new(animation: &RwLock<AnimationConfig>) -> Self {
        let animation = Animation::new(
            0.,
            1.,
            Duration::from_millis(1000),
            animation
                .read()
                .unwrap()
                .get_easing(EASING_ID)
                .cloned()
                .unwrap_or(Arc::new(ease_out_bounce)),
        );

        Self {
            animation,
            destination: 1.,
            bounding: BoundingBox::zero(),
        }
    }
}

impl Component for Clock {
    fn draw(
        &mut self,
        canvas: &Canvas,
        _state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let pos = 10. + self.animation.value() * 200.;
        let time = Local::now().format("%H:%M:%S").to_string();
        let mut paint = Paint::default();

        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(theme.primary), None);

        let font = fonts.sized("noto_sans", 32.);
        let metrics = font.metrics();

        canvas.draw_str_align(
            &time,
            (10.0 + pos, 10.0 - metrics.1.ascent),
            &font,
            &paint,
            Align::Left,
        );

        self.bounding = BoundingBox::from_text(10.0 + pos, 10.0 - metrics.1.ascent, &time, &font);

        if self.animation.is_done() {
            Redraw::None
        } else {
            Redraw::Animating
        }
    }

    fn on_cursor(&mut self, event: &PointerEvent) -> Redraw {
        if let PointerEventKind::Press { button, .. } = event.kind {
            if self
                .bounding
                .is_cover(event.position.0 as f32, event.position.1 as f32)
                && button == button::LEFT
            {
                self.destination = 1. - self.destination;
                self.animation.set_target(self.destination);
                return Redraw::Now;
            }
        }
        Redraw::None
    }

    fn on_easing_updated(&mut self, id: &str, easing: &Easing) -> Redraw {
        if id == EASING_ID {
            self.animation.set_easing(easing.clone());
            Redraw::Now
        } else {
            Redraw::None
        }
    }
}
