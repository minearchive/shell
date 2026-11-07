use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};

use ui_core::{font::FontBook, scheme::ColorTheme};

use crate::{
    dbus::warp::WarpStatus,
    ui::{bar_baseline, BarSlot, Component, Redraw, UIState},
};

const FONT_SIZE: f32 = 32.0;

pub struct Warp;

impl Component for Warp {
    fn bar_slot(&self) -> BarSlot {
        BarSlot::Left
    }

    fn bar_width(&self, state: &UIState, fonts: &FontBook) -> f32 {
        state.warp.as_ref().map_or(0.0, |status| {
            fonts
                .sized("noto_sans", FONT_SIZE)
                .measure_str(status.status.as_str(), None)
                .0
        })
    }

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

        let font = fonts.sized("noto_sans", FONT_SIZE);

        canvas.draw_str_align(
            status.status.as_str(),
            (0.0, bar_baseline(&font)),
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
