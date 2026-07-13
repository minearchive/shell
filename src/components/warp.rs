use std::sync::{Arc, RwLock};

use calloop::channel::Sender;
use skia_safe::{utils::text_utils::Align, Color4f, Paint};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::config::Configuration,
    dbus::warp::WarpCommand,
    ui::{Component, UiEvent},
};

pub struct Warp {
    sender: Sender<UiEvent>,
    config: Arc<RwLock<Configuration>>,
}

impl Warp {
    pub fn new(
        sender: Sender<UiEvent>,
        config: Arc<RwLock<Configuration>>,
        cmd: UnboundedSender<WarpCommand>,
    ) -> Self {
        let _ = cmd.send(WarpCommand::UpdateState);

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
        let Some(status) = &state.warp else { return };

        let cfg = self.config.read().unwrap();
        let theme = cfg.theme();
        let color = if status.is_connected() {
            theme.primary
        } else {
            theme.error
        };

        let mut paint = Paint::default();
        paint.set_color4f(Color4f::from(color), None);

        let font = fonts.sized("noto_sans", 32.);
        let metrics = font.metrics();

        canvas.draw_str_align(
            status.status.as_str(),
            (20., -metrics.1.ascent),
            &font,
            &paint,
            Align::Left,
        );
    }

    fn on_warp(&mut self, _status: &crate::dbus::warp::WarpStatus) {
        let _ = self.sender.send(UiEvent::RequestRedrawAll);
    }
}
