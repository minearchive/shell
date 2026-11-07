use chrono::Local;
use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint};

use ui_core::{font::FontBook, scheme::ColorTheme};

use crate::ui::{bar_baseline, BarSlot, Component, Redraw, UIState};

const FORMAT: &str = "%H:%M:%S";
const FONT_SIZE: f32 = 32.0;

pub struct Clock;

impl Component for Clock {
    fn bar_slot(&self) -> BarSlot {
        BarSlot::Left
    }

    fn bar_width(&self, _state: &UIState, fonts: &FontBook) -> f32 {
        let time = Local::now().format(FORMAT).to_string();
        fonts
            .sized("noto_sans", FONT_SIZE)
            .measure_str(&time, None)
            .0
    }

    fn draw(
        &mut self,
        canvas: &Canvas,
        _state: &UIState,
        fonts: &FontBook,
        theme: &ColorTheme,
    ) -> Redraw {
        let time = Local::now().format(FORMAT).to_string();
        let mut paint = Paint::default();

        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(theme.primary), None);

        let font = fonts.sized("noto_sans", FONT_SIZE);
        canvas.draw_str_align(
            &time,
            (0.0, bar_baseline(&font)),
            &font,
            &paint,
            Align::Left,
        );

        Redraw::None
    }
}
