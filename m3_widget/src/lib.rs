//! Material 3 widget crate — presentation-only, built on ui_core.
//!
//! Widgets are grouped by role: [`input`] for anything that captures a value
//! or an action, [`layouts`] for the structural widgets that arrange or
//! separate content, and [`tokens`] for the shared interaction and motion
//! constants both draw on. [`drawing`] and [`icon`] are the shared paint
//! helpers, used from either group.
//!
//! Every widget type and every leaf widget module is re-exported at the crate
//! root, so consumers write `m3_widget::CheckBox` and `m3_widget::checkbox::SIZE`
//! without spelling out which group a widget happens to sit in — the grouping
//! is an internal organising device, and moving a widget between groups is not
//! a breaking change.

mod input;
mod layouts;

pub mod drawing;
pub mod icon;
pub mod tokens;

pub use input::{buttons, checkbox, slider, switch, text_field};
pub use layouts::{divider, list_item, navigation_rail};

pub use buttons::{
    Button, ButtonShape, ButtonSize, ButtonVariant, IconButton, IconButtonVariant, RadioButton,
    Segment, SegmentedButton, SelectionMode,
};
pub use checkbox::CheckBox;
pub use divider::{Divider, Orientation};
pub use icon::Icon;
pub use list_item::ListItem;
pub use navigation_rail::{NavigationRail, NavigationRailAlignment, NavigationRailItem};
pub use slider::{Slider, SliderSize};
pub use switch::{Switch, SwitchIcons};
pub use text_field::TextField;

pub use ui_widget::{Column, CrossAlign, Row, ScrollableWidget, Widget};
