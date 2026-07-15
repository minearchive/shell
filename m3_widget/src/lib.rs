//! Material 3 widget crate — presentation-only, built on ui_core.

pub mod button;

pub use button::{Button, ButtonSize, ButtonVariant};

use ui_core::pointer::PointerEvent;

pub trait Widget {
    fn draw(
        &mut self,
        canvas: &skia_safe::Canvas,
        theme: &ui_core::scheme::ColorTheme,
        fonts: &ui_core::font::FontBook,
    );

    fn on_pointer(&mut self, _event: &PointerEvent) -> bool {
        false
    }
}
