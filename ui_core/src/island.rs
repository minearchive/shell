//! Shape and motion for "dynamic island" surfaces: a container that grows out
//! of a bar and stays one continuous silhouette with it.

use std::time::Duration;

use crate::{
    animation::{animation::Animation, parser::Easing},
    geometry::LayoutRect,
};

/// Animates one island between rectangles. It leaves from and returns to a resting
/// rectangle, where it counts as gone. All four edges share a duration and easing, so
/// an edge that stays put in both rectangles stays put throughout.
pub struct IslandMorph {
    x: Animation<f32>,
    y: Animation<f32>,
    width: Animation<f32>,
    height: Animation<f32>,
    rest: LayoutRect,
}

impl IslandMorph {
    pub fn new(duration: Duration, easing: Easing) -> Self {
        let animation = || Animation::new(0.0, 0.0, duration, easing.clone());
        Self {
            x: animation(),
            y: animation(),
            width: animation(),
            height: animation(),
            rest: LayoutRect::empty(),
        }
    }

    fn target(&self) -> LayoutRect {
        LayoutRect::new(self.x.to(), self.y.to(), self.width.to(), self.height.to())
    }

    fn set_target(&mut self, target: LayoutRect) {
        self.x.set_target(target.x);
        self.y.set_target(target.y);
        self.width.set_target(target.width);
        self.height.set_target(target.height);
    }

    /// Opens or moves the island to `target`, resting at `rest` when it collapses. Safe
    /// to call every frame: only a changed target restarts the motion.
    pub fn morph_to(&mut self, target: LayoutRect, rest: LayoutRect) {
        if self.rect().is_none() {
            // Nothing visible yet to travel from, so start from the resting rectangle.
            self.x.reset(rest.x, target.x);
            self.y.reset(rest.y, target.y);
            self.width.reset(rest.width, target.width);
            self.height.reset(rest.height, target.height);
        } else if self.target() != target {
            self.set_target(target);
        }
        self.rest = rest;
    }

    /// Safe to call every frame, like [`Self::morph_to`].
    pub fn collapse(&mut self) {
        if self.target() != self.rest {
            self.set_target(self.rest);
        }
    }

    /// The island's current rectangle, or `None` once it is at rest.
    pub fn rect(&self) -> Option<LayoutRect> {
        (self.is_traveling() || self.target() != self.rest).then(|| {
            LayoutRect::new(
                self.x.value(),
                self.y.value(),
                self.width.value(),
                self.height.value(),
            )
        })
    }

    pub fn is_traveling(&self) -> bool {
        self.x.is_traveling()
            || self.y.is_traveling()
            || self.width.is_traveling()
            || self.height.is_traveling()
    }

    pub fn set_easing(&mut self, easing: Easing) {
        self.x.set_easing(easing.clone());
        self.y.set_easing(easing.clone());
        self.width.set_easing(easing.clone());
        self.height.set_easing(easing);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::animation::animation::easing::linear;

    #[test]
    fn morph_appears_moves_and_disappears() {
        let mut morph = IslandMorph::new(Duration::ZERO, Arc::new(linear));
        let rest = LayoutRect::new(200.0, 100.0, 0.0, 0.0);
        let target = LayoutRect::new(150.0, 60.0, 100.0, 40.0);
        assert_eq!(morph.rect(), None);

        morph.morph_to(target, rest);
        assert_eq!(morph.rect(), Some(target));

        let moved = LayoutRect::new(250.0, 60.0, 100.0, 40.0);
        morph.morph_to(moved, rest);
        assert_eq!(morph.rect(), Some(moved));

        morph.collapse();
        assert_eq!(morph.rect(), None);
    }

    #[test]
    fn morph_grows_from_a_resting_rect_keeping_shared_edges_still() {
        let duration = Duration::from_millis(200);
        let mut morph = IslandMorph::new(duration, Arc::new(linear));
        let rest = LayoutRect::new(800.0, 300.0, 200.0, 60.0);
        let target = LayoutRect::new(600.0, 50.0, 400.0, 310.0);

        morph.morph_to(target, rest);
        let start = morph.rect().unwrap();
        assert!(start.width < 300.0 && start.height < 200.0, "{start:?}");
        std::thread::sleep(Duration::from_millis(50));

        let mid = morph.rect().unwrap();
        assert!(
            mid.width > rest.width && mid.width < target.width,
            "{mid:?}"
        );
        assert!(
            (mid.x + mid.width - 1000.0).abs() < 0.5,
            "right edge: {mid:?}"
        );
        assert!(
            (mid.y + mid.height - 360.0).abs() < 0.5,
            "bottom edge: {mid:?}"
        );

        morph.collapse();
        std::thread::sleep(duration + Duration::from_millis(20));
        assert_eq!(morph.rect(), None, "at rest counts as gone");
    }

    #[test]
    fn repeating_a_target_does_not_restart_the_motion() {
        let mut morph = IslandMorph::new(Duration::from_secs(60), Arc::new(linear));
        let rest = LayoutRect::new(200.0, 100.0, 0.0, 0.0);
        let target = LayoutRect::new(150.0, 60.0, 100.0, 40.0);
        morph.morph_to(target, rest);
        std::thread::sleep(Duration::from_millis(20));
        let before = morph.height.value();
        morph.morph_to(target, rest);
        assert!(morph.height.value() >= before);
        assert!(morph.is_traveling());
    }
}
