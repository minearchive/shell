//! Material 3 widget crate — presentation-only, built on ui_core.

pub mod animation;
pub mod button;
pub mod checkbox;
pub mod divider;
pub mod drawing;
pub mod icon_button;
pub mod list_item;
pub mod radio_button;
pub mod scrollable;
pub mod slider;
pub mod switch;
pub mod text_field;
pub mod tokens;

pub use button::{Button, ButtonSize, ButtonVariant};
pub use checkbox::CheckBox;
pub use divider::{Divider, Orientation};
pub use icon_button::{Icon, IconButton, IconButtonVariant};
pub use list_item::ListItem;
pub use radio_button::RadioButton;
pub use scrollable::ScrollableWidget;
pub use slider::{Slider, SliderSize};
pub use switch::{Switch, SwitchIcons};
pub use text_field::TextField;

use ui_core::geometry::LayoutRect;
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

    /// Assigns the rect a layout system (e.g. Taffy) allocated to this
    /// widget. The single source of truth for the widget's placement and
    /// size — widgets must not independently own position or width.
    fn set_layout_rect(&mut self, rect: LayoutRect);

    /// The rect last assigned via [`Self::set_layout_rect`].
    fn layout_rect(&self) -> LayoutRect;

    /// The rect used for pointer hit testing. Defaults to [`Self::layout_rect`];
    /// widgets whose visual footprint is smaller than the minimum comfortable
    /// touch target (e.g. `Switch`, `Slider`) override this to expand it,
    /// without treating the expansion as part of paint or layout bounds.
    fn hit_rect(&self) -> LayoutRect {
        self.layout_rect()
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_focused(&mut self, _focused: bool) {}
}
