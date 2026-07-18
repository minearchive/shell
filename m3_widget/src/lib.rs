//! Material 3 widget crate — presentation-only, built on ui_core.

pub mod animation;
pub mod button;
pub mod slider;
pub mod text_field;

pub use button::{Button, ButtonSize, ButtonVariant};
pub use slider::{Slider, SliderSize};
pub use text_field::TextField;

use ui_core::keyboard::KeyboardEvent;
use ui_core::pointer::PointerEvent;

pub trait Widget {
    fn draw(
        &mut self,
        canvas: &skia_safe::Canvas,
        theme: &ui_core::scheme::ColorTheme,
        fonts: &ui_core::font::FontBook,
    ) -> bool;

    fn on_pointer(&mut self, _event: &PointerEvent) -> bool {
        false
    }

    /// Returns: needs redraw.
    fn on_keyboard(&mut self, _event: &KeyboardEvent) -> bool {
        false
    }

    fn bounds(&self) -> skia_safe::Rect {
        skia_safe::Rect::new_empty()
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_focused(&mut self, _focused: bool) {}
}
