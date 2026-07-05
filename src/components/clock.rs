use std::{
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use calloop::channel::Sender;
use chrono::Local;
use mpris::Event;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::pointer::PointerEvent;

use crate::{
    animation::{
        animation::{easing::ease_out_bounce, Animation},
        parser::css_to_easing,
    },
    config::config::Configuration,
    dbus::mpris::PlayerState,
    font::FontBook,
    ipc::events::IPCEvent,
    ui::{Component, UIState, UiEvent},
    util::BoundingBox,
};

pub struct Clock {
    config: Arc<RwLock<Configuration>>,
    animation: Animation<f32>,
    sender: Sender<UiEvent>,
    time: Arc<Mutex<String>>,
    destination: f32,
    bounding: BoundingBox,
}

impl Clock {
    pub fn new(
        sender: Sender<UiEvent>,
        screen_idx: usize,
        update_interval: usize,
        config: Arc<RwLock<Configuration>>,
    ) -> Self {
        let time = Arc::new(Mutex::new(String::new()));
        let time_clone = Arc::clone(&time);
        let animation = Animation::new(
            0.,
            1.,
            Duration::from_millis(1000),
            css_to_easing("cubic-bezier(0.68, -0.6, 0.32, 1.6)")
                .unwrap_or_else(|_| Box::new(ease_out_bounce)),
        );
        let sender_clone = sender.clone();
        let interval = update_interval as u64;

        thread::spawn(move || {
            let mut first = true;

            loop {
                if !first {
                    thread::sleep(Duration::from_millis(interval));
                }
                *time_clone.lock().unwrap() = Local::now().format("%H:%M:%S").to_string();
                let _ = sender_clone.send(UiEvent::RequestRedraw(screen_idx));
                first = false;
            }
        });

        Self {
            time,
            animation,
            sender,
            config,
            destination: 1.,
            bounding: BoundingBox::zero(),
        }
    }
}

impl Component for Clock {
    fn draw(&mut self, canvas: &Canvas, _state: &UIState, fonts: &FontBook) {
        let cfg = self.config.read().unwrap();
        let pos = 10. + self.animation.value() * 200.;
        let time = self.time.lock().unwrap().clone();
        let mut paint = Paint::default();

        paint.set_anti_alias(true);
        paint.set_color4f(
            Color4f::new(
                cfg.theme().primary.r,
                cfg.theme().primary.g,
                cfg.theme().primary.b,
                cfg.theme().primary.a,
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

        if !self.animation.is_done() {
            let _ = self.sender.send(UiEvent::RequestRedrawAll);
        }
    }

    fn on_cursor(&mut self, event: &PointerEvent) {
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
                let _ = self.sender.send(UiEvent::RequestRedrawAll);
            }
        }
    }

    fn on_ipc(&mut self, _: &IPCEvent) {}
    fn on_mpris(&mut self, _: &PlayerState, _: &Event) {}
}
