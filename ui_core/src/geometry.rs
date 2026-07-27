//! Allocated layout geometry: the rectangle a layout system (e.g. Taffy)
//! assigns to a widget.
//!
//! This is deliberately distinct from [`crate::util::BoundingBox`], which
//! measures ink/text extents rather than allocated space — `BoundingBox`
//! stays the text-measurement type used by `Clock`, while [`LayoutRect`] is
//! the single source of truth for where a widget was placed and how big it
//! was made.

/// A rectangle allocated to a widget by a layout system.
///
/// `ui_core` intentionally has no dependency on any particular layout engine
/// (e.g. Taffy) — callers convert their layout engine's output into a
/// `LayoutRect` at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A zero-sized rect at the origin; the default before a layout system
    /// has assigned real geometry.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        self.x <= x && x < self.x + self.width && self.y <= y && y < self.y + self.height
    }

    /// Shrinks the rect toward its center by `horizontal` on each side and
    /// `vertical` on each side. Width/height are clamped at zero rather than
    /// going negative.
    pub fn inset(self, horizontal: f32, vertical: f32) -> Self {
        Self {
            x: self.x + horizontal,
            y: self.y + vertical,
            width: (self.width - horizontal * 2.0).max(0.0),
            height: (self.height - vertical * 2.0).max(0.0),
        }
    }

    /// Grows the rect away from its center by `horizontal` on each side and
    /// `vertical` on each side.
    pub fn outset(self, horizontal: f32, vertical: f32) -> Self {
        Self {
            x: self.x - horizontal,
            y: self.y - vertical,
            width: self.width + horizontal * 2.0,
            height: self.height + vertical * 2.0,
        }
    }

    pub fn to_skia(self) -> skia_safe::Rect {
        skia_safe::Rect::from_xywh(self.x, self.y, self.width, self.height)
    }
}

impl From<LayoutRect> for skia_safe::Rect {
    fn from(rect: LayoutRect) -> Self {
        rect.to_skia()
    }
}

/// A widget's intrinsic (natural) width/height, independent of any
/// [`LayoutRect`] a layout system may have assigned it. See
/// `ui_widget::Widget::measure`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `contains` treats the rect as half-open: the left/top edges are inside,
    /// the right/bottom edges are not. Two rects sharing an edge therefore
    /// never both claim a point on it.
    #[test]
    fn contains_is_half_open() {
        let rect = LayoutRect::new(10.0, 20.0, 100.0, 50.0);
        // Left/top edges and the interior are inside.
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(60.0, 45.0));
        // Just inside the far edges.
        assert!(rect.contains(109.9, 69.9));
        // Right/bottom edges (x == x+w, y == y+h) are outside.
        assert!(!rect.contains(110.0, 70.0));
        assert!(!rect.contains(110.0, 45.0));
        assert!(!rect.contains(60.0, 70.0));
        // Just outside the left/top edges.
        assert!(!rect.contains(9.9, 45.0));
        assert!(!rect.contains(60.0, 19.9));
    }

    #[test]
    fn inset_shrinks_toward_center() {
        let rect = LayoutRect::new(10.0, 20.0, 100.0, 50.0);
        let inset = rect.inset(5.0, 10.0);
        assert_eq!(inset, LayoutRect::new(15.0, 30.0, 90.0, 30.0));
    }

    #[test]
    fn inset_clamps_width_and_height_at_zero() {
        let rect = LayoutRect::new(0.0, 0.0, 10.0, 10.0);
        let inset = rect.inset(20.0, 20.0);
        assert_eq!(inset.width, 0.0);
        assert_eq!(inset.height, 0.0);
    }

    #[test]
    fn outset_grows_away_from_center() {
        let rect = LayoutRect::new(10.0, 20.0, 100.0, 50.0);
        let outset = rect.outset(5.0, 10.0);
        assert_eq!(outset, LayoutRect::new(5.0, 10.0, 110.0, 70.0));
    }

    #[test]
    fn inset_and_outset_are_inverses() {
        let rect = LayoutRect::new(3.0, 4.0, 40.0, 20.0);
        assert_eq!(rect.inset(5.0, 2.0).outset(5.0, 2.0), rect);
    }
}
