use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use calloop::channel::Sender;
use chrono::Local;
use mpris::Event;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use smithay_client_toolkit::seat::pointer::PointerEvent;

use crate::{
    dbus::mpris::PlayerState,
    font::FontBook,
    ipc::events::IPCEvent,
    ui::{Component, UIState, UiEvent},
};

pub struct Clock {
    _update_interval: usize,
    time: Arc<Mutex<String>>,
}

impl Clock {
    pub fn new(sender: Sender<UiEvent>, screen_idx: usize, update_interval: usize) -> Self {
        let time = Arc::new(Mutex::new(String::new()));
        let time_clone = Arc::clone(&time);
        let interval = update_interval as u64;

        thread::spawn(move || loop {
            *time_clone.lock().unwrap() = Local::now().format("%H:%M:%S").to_string();
            let _ = sender.send(UiEvent::RequestRedraw(screen_idx));
            thread::sleep(Duration::from_millis(interval));
        });

        Self {
            _update_interval: update_interval,
            time,
        }
    }
}

impl Component for Clock {
    fn draw(&self, canvas: &Canvas, _state: &UIState, fonts: &FontBook) {
        let time = self.time.lock().unwrap().clone();
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::new(0., 0., 0., 1.), None);

        let font = fonts.sized("noto_sans", 24.);
        let metrics = font.metrics();

        canvas.draw_str_align(
            &time,
            (500.0, 10.0 - metrics.1.ascent),
            &font,
            &paint,
            Align::Left,
        );
    }

    fn on_cursor(&self, _: &PointerEvent) {}
    // fn on_key(&mut self, _: &KeyEvent, _: &crate::KeyTiming) {}
    fn on_ipc(&mut self, _: &IPCEvent) {}
    fn on_mpris(&mut self, _: &PlayerState, _: &Event) {}
}
