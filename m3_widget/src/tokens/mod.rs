//! Shared Material 3 tokens: the state-layer/disabled opacities below, and
//! the durations and easings in [`motion`].
//!
//! These are the values common to every interactive widget. Widgets with their
//! own deviations keep those locally — `text_field` uses a 0.04 disabled
//! container opacity, and `slider` names its disabled track opacity differently.

pub mod motion;

/// State-layer opacities: the tint painted over a component on hover, focus,
/// and press.
pub const HOVER_OPACITY: f32 = 0.08;
pub const FOCUS_OPACITY: f32 = 0.10;
pub const PRESSED_OPACITY: f32 = 0.10;

/// Disabled treatments: containers dim to 12%, content to 38%.
pub const DISABLED_CONTAINER_OPACITY: f32 = 0.12;
pub const DISABLED_CONTENT_OPACITY: f32 = 0.38;
