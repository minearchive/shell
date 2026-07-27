//! The [`Widget`] trait: the presentation/event abstraction every drawable
//! component implements.

use ui_core::font::FontBook;
use ui_core::geometry::LayoutRect;
use ui_core::keyboard::KeyboardEvent;
use ui_core::pointer::PointerEvent;
use ui_core::scheme::ColorTheme;

pub trait Widget {
    fn draw(&mut self, canvas: &skia_safe::Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool;

    fn on_pointer(&mut self, _event: &PointerEvent) -> bool {
        false
    }

    fn on_keyboard(&mut self, _event: &KeyboardEvent) -> bool {
        false
    }

    /// Assigns the rect a layout system (e.g. Taffy) allocated to this
    /// widget. The single source of truth for the widget's placement and
    /// size — widgets must not independently own position or width.
    fn set_layout_rect(&mut self, rect: LayoutRect);

    fn layout_rect(&self) -> LayoutRect;

    fn hit_rect(&self) -> LayoutRect {
        self.layout_rect()
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_focused(&mut self, _focused: bool) {}
}
