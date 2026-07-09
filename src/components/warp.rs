use std::sync::{Arc, RwLock};

use calloop::channel::Sender;
use skia_safe::{utils::text_utils::Align, Color4f, Paint};

use crate::{
    config::config::Configuration,
    ui::{Component, UiEvent},
};

pub struct Warp {
    sender: Sender<UiEvent>,
    config: Arc<RwLock<Configuration>>,
}

impl Warp {
    pub fn new(sender: Sender<UiEvent>, config: Arc<RwLock<Configuration>>) -> Self {
        Self { sender, config }
    }
}

impl Component for Warp {
    fn draw(
        &mut self,
        canvas: &skia_safe::Canvas,
        state: &crate::ui::UIState,
        fonts: &crate::font::FontBook,
    ) {
        let cfg = self.config.read().unwrap();
        let mut paint = Paint::default();

        if let Some(state) = &state.warp {
            if state.is_connected() {
                paint.set_color4f(
                    Color4f::new(
                        cfg.theme().primary.r,
                        cfg.theme().primary.g,
                        cfg.theme().primary.b,
                        cfg.theme().primary.a,
                    ),
                    None,
                );
            } else {
                paint.set_color4f(
                    Color4f::new(
                        cfg.theme().error.r,
                        cfg.theme().error.g,
                        cfg.theme().error.b,
                        cfg.theme().error.a,
                    ),
                    None,
                );
            }

            let font = fonts.sized("noto_sans", 32.);
            let metrics = font.metrics();

            canvas.draw_str_align(
                state.status.as_str(),
                (20., 20. + metrics.1.ascent),
                &font,
                &paint,
                Align::Left,
            );
        }
    }

    fn on_warp(&mut self, _status: &crate::dbus::warp::WarpStatus) {
        let _ = self.sender.send(UiEvent::RequestRedrawAll);
    }
}
