//! Material 3 motion tokens: duration and easing.
//!
//! <https://m3.material.io/styles/motion/easing-and-duration/tokens-specs>

use ui_core::animation::parser::{cubic_bezier, Easing};

/// `md.sys.motion.duration.*` — the MD3 duration scale.
pub mod duration {
    use std::time::Duration;

    pub const SHORT1: Duration = Duration::from_millis(50);
    pub const SHORT2: Duration = Duration::from_millis(100);
    pub const SHORT3: Duration = Duration::from_millis(150);
    pub const SHORT4: Duration = Duration::from_millis(200);

    pub const MEDIUM1: Duration = Duration::from_millis(250);
    pub const MEDIUM2: Duration = Duration::from_millis(300);
    pub const MEDIUM3: Duration = Duration::from_millis(350);
    pub const MEDIUM4: Duration = Duration::from_millis(400);

    pub const LONG1: Duration = Duration::from_millis(450);
    pub const LONG2: Duration = Duration::from_millis(500);
    pub const LONG3: Duration = Duration::from_millis(550);
    pub const LONG4: Duration = Duration::from_millis(600);

    pub const EXTRA_LONG1: Duration = Duration::from_millis(700);
    pub const EXTRA_LONG2: Duration = Duration::from_millis(800);
    pub const EXTRA_LONG3: Duration = Duration::from_millis(900);
    pub const EXTRA_LONG4: Duration = Duration::from_millis(1000);
}

/// `md.sys.motion.easing.*` — the MD3 easing curves.
///
/// [`Easing`] is an `Arc<dyn Fn>`, so these build a fresh closure per call
/// rather than being `const`; call once per [`Animation`] constructed.
///
/// [`Animation`]: ui_core::animation::animation::Animation
pub mod easing {
    use super::{cubic_bezier, Easing};

    /// Micro-interactions that begin and end on screen: hover, press,
    /// toggles, state-layer fades.
    pub fn standard() -> Easing {
        cubic_bezier(0.2, 0.0, 0.0, 1.0)
    }

    /// Elements entering the screen.
    pub fn standard_decelerate() -> Easing {
        cubic_bezier(0.0, 0.0, 0.0, 1.0)
    }

    /// Elements leaving the screen.
    pub fn standard_accelerate() -> Easing {
        cubic_bezier(0.3, 0.0, 1.0, 1.0)
    }

    /// Large, expressive transitions that begin and end on screen. The MD3
    /// spec's true emphasized curve is a two-segment path; this
    /// cubic-bezier is the spec's own single-curve approximation of it.
    pub fn emphasized() -> Easing {
        cubic_bezier(0.2, 0.0, 0.0, 1.0)
    }

    /// Large elements entering the screen.
    pub fn emphasized_decelerate() -> Easing {
        cubic_bezier(0.05, 0.7, 0.1, 1.0)
    }

    /// Large elements leaving the screen.
    pub fn emphasized_accelerate() -> Easing {
        cubic_bezier(0.3, 0.0, 0.8, 0.15)
    }
}
