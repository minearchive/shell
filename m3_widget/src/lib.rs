//! Material 3 widget crate — presentation-only, built on ui_core.

pub mod animation;
pub mod buttons;
pub mod checkbox;
pub mod divider;
pub mod drawing;
pub mod icon;
pub mod list_item;
pub mod slider;
pub mod switch;
pub mod text_field;
pub mod tokens;

pub use buttons::{
    Button, ButtonShape, ButtonSize, ButtonVariant, IconButton, IconButtonVariant, RadioButton,
};
pub use checkbox::CheckBox;
pub use divider::{Divider, Orientation};
pub use icon::Icon;
pub use list_item::ListItem;
pub use slider::{Slider, SliderSize};
pub use switch::{Switch, SwitchIcons};
pub use text_field::TextField;

pub use ui_widget::{Column, CrossAlign, Row, ScrollableWidget, Widget};
