use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::scheme::ColorTheme,
    dbus::warp::{WarpCommand, WarpStatus},
    font::FontBook,
    ui::{Component, Redraw, UIState},
};

pub struct Warp;

impl Warp {
    pub fn new(cmd: UnboundedSender<WarpCommand>) -> Self {
        let _ = cmd.send(WarpCommand::UpdateState);

        Self
    }
}

impl Component for Warp {
    fn draw(
        &mut self,
        canvas: &Canvas,
        state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let Some(status) = &state.warp else {
            return Redraw::None;
        };

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

        Redraw::None
    }

    fn on_warp(&mut self, _status: &WarpStatus) -> Redraw {
        Redraw::Now
    }
}
