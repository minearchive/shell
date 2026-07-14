use std::{
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use calloop::channel::Sender;
use chrono::Local;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::pointer::PointerEvent;

use ui_core::{
    animation::{
        animation::{easing::ease_out_bounce, Animation},
        parser::Easing,
    },
    font::FontBook,
    scheme::ColorTheme,
    util::BoundingBox,
};

use crate::{
    config::animation::AnimationConfig,
    ui::{Component, Redraw, UIState, UiEvent},
};

pub struct Clock {
    animation: Animation<f32>,
    time: Arc<Mutex<String>>,
    destination: f32,
    bounding: BoundingBox,
}

impl Clock {
    pub fn new(
        sender: Sender<UiEvent>,
        screen_idx: usize,
        update_interval: usize,
        animation: Arc<RwLock<AnimationConfig>>,
    ) -> Self {
        let time = Arc::new(Mutex::new(String::new()));
        let time_clone = Arc::clone(&time);
        let animation = Animation::new(
            0.,
            1.,
            Duration::from_millis(1000),
            animation
                .read()
                .unwrap()
                .get_easing("a")
                .cloned()
                .unwrap_or(Arc::new(ease_out_bounce)),
        );
        let interval = update_interval as u64;

        // Time is a data source, not a widget animation: the clock thread owns
        // waking the loop on each tick. Widget-driven repaints go through the
        // `Redraw` return values instead.
        thread::spawn(move || {
            let mut first = true;

            loop {
                if !first {
                    thread::sleep(Duration::from_millis(interval));
                }
                *time_clone.lock().unwrap() = Local::now().format("%H:%M:%S").to_string();
                let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
                first = false;
            }
        });

        Self {
            time,
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
        let time = self.time.lock().unwrap().clone();
        let mut paint = Paint::default();

        paint.set_anti_alias(true);
        paint.set_color4f(
            Color4f::new(
                theme.primary.r,
                theme.primary.g,
                theme.primary.b,
                theme.primary.a,
            ),
            None,
        );

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
        if let smithay_client_toolkit::seat::pointer::PointerEventKind::Press { button, .. } =
            event.kind
        {
            if self
                .bounding
                .is_cover(event.position.0 as f32, event.position.1 as f32)
                && button == 272
            {
                self.destination = 1. - self.destination;
                self.animation.set_target(self.destination);
                return Redraw::Now;
            }
        }
        Redraw::None
    }

    fn on_easing_updated(&mut self, id: String, easing: &Easing) -> Redraw {
        if id == "a" {
            self.animation.set_easing(easing.clone());
            Redraw::Now
        } else {
            Redraw::None
        }
    }
}
