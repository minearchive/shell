//! Material 3 buttons: every widget whose job is "press me".
//!
//! This groups the common (action) buttons, icon buttons, radio buttons and
//! segmented buttons, because they share a presentation vocabulary — state
//! layers, disabled treatment and press shape-morph — rather than any code
//! hierarchy. That shared logic lives outside this module on purpose:
//! interaction opacities in [`crate::tokens`], motion in
//! [`crate::animation`], and the glyph set in [`crate::icon`], so a
//! non-button widget can reuse them without depending on `buttons`.

pub mod common;
pub mod icon_button;
pub mod radio_button;
pub mod segmented;

pub use common::{Button, ButtonShape, ButtonSize, ButtonVariant};
pub use icon_button::{IconButton, IconButtonVariant};
pub use radio_button::RadioButton;
pub use segmented::{Segment, SegmentedButton, SelectionMode};
