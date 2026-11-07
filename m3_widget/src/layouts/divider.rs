//! Material 3 divider: a thin rule that separates content, either a
//! full-bleed line or an "inset divider" indented from one or both ends.

use skia_safe::{Canvas, Color4f, Paint, Rect};

use ui_core::{font::FontBook, geometry::LayoutRect, scheme::ColorTheme};

use crate::Widget;

/// MD3's default rule thickness.
const DEFAULT_THICKNESS: f32 = 1.0;

/// Which axis the rule runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    /// A horizontal rule; insets apply to its left/right edges.
    #[default]
    Horizontal,
    /// A vertical rule; insets apply to its top/bottom edges.
    Vertical,
}

/// A thin, decorative rule separating content. Purely presentational: it
/// takes no pointer or keyboard input and is never focusable.
pub struct Divider {
    orientation: Orientation,
    leading_inset: f32,
    trailing_inset: f32,
    thickness: f32,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
}

impl Divider {
    pub fn new() -> Self {
        Self {
            orientation: Orientation::default(),
            leading_inset: 0.0,
            trailing_inset: 0.0,
            thickness: DEFAULT_THICKNESS,
            layout_rect: LayoutRect::empty(),
        }
    }

    /// Switches to a vertical rule.
    pub fn vertical(mut self) -> Self {
        self.orientation = Orientation::Vertical;
        self
    }

    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Inset from the start edge along the main axis (left for horizontal,
    /// top for vertical) — e.g. 16dp to align with list item text.
    pub fn leading_inset(mut self, inset: f32) -> Self {
        self.leading_inset = inset;
        self
    }

    /// Inset from the end edge along the main axis (right for horizontal,
    /// bottom for vertical).
    pub fn trailing_inset(mut self, inset: f32) -> Self {
        self.trailing_inset = inset;
        self
    }

    /// Overrides the default 1dp rule thickness.
    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    pub fn orientation_value(&self) -> Orientation {
        self.orientation
    }

    pub fn leading_inset_value(&self) -> f32 {
        self.leading_inset
    }

    pub fn trailing_inset_value(&self) -> f32 {
        self.trailing_inset
    }

    pub fn thickness_value(&self) -> f32 {
        self.thickness
    }

    /// The rule rect, as a Skia rect for drawing. Available before the first
    /// `draw` since it reads the assigned layout rect rather than anything
    /// computed by `draw`.
    fn rule_rect(&self) -> Rect {
        let rect = self.layout_rect.to_skia();
        match self.orientation {
            Orientation::Horizontal => {
                let half = self.thickness / 2.0;
                Rect::new(
                    rect.left + self.leading_inset,
                    rect.center_y() - half,
                    rect.right - self.trailing_inset,
                    rect.center_y() + half,
                )
            }
            Orientation::Vertical => {
                let half = self.thickness / 2.0;
                Rect::new(
                    rect.center_x() - half,
                    rect.top + self.leading_inset,
                    rect.center_x() + half,
                    rect.bottom - self.trailing_inset,
                )
            }
        }
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Divider {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, _fonts: &FontBook) -> bool {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(theme.outline_variant), None);
        canvas.draw_rect(self.rule_rect(), &paint);

        false
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout_rect = rect;
    }

    fn layout_rect(&self) -> LayoutRect {
        self.layout_rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut divider = Divider::new();
        assert_eq!(divider.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, 200.0, 1.0);
        divider.set_layout_rect(rect);
        assert_eq!(divider.layout_rect(), rect);
    }

    /// Defaults: horizontal, no insets, 1dp thickness.
    #[test]
    fn defaults_match_md3_full_bleed_divider() {
        let divider = Divider::new();
        assert_eq!(divider.orientation_value(), Orientation::Horizontal);
        assert_eq!(divider.leading_inset_value(), 0.0);
        assert_eq!(divider.trailing_inset_value(), 0.0);
        assert_eq!(divider.thickness_value(), DEFAULT_THICKNESS);
    }

    /// The builder chain sets orientation, insets, and thickness.
    #[test]
    fn builder_sets_fields() {
        let divider = Divider::new()
            .vertical()
            .leading_inset(16.0)
            .trailing_inset(8.0)
            .thickness(2.0);

        assert_eq!(divider.orientation_value(), Orientation::Vertical);
        assert_eq!(divider.leading_inset_value(), 16.0);
        assert_eq!(divider.trailing_inset_value(), 8.0);
        assert_eq!(divider.thickness_value(), 2.0);
    }

    /// `.orientation()` is equivalent to `.vertical()` for the vertical case,
    /// and can also be used to explicitly select horizontal.
    #[test]
    fn orientation_setter_matches_convenience_method() {
        let a = Divider::new().vertical();
        let b = Divider::new().orientation(Orientation::Vertical);
        assert_eq!(a.orientation_value(), b.orientation_value());

        let h = Divider::new().orientation(Orientation::Horizontal);
        assert_eq!(h.orientation_value(), Orientation::Horizontal);
    }

    /// Horizontal rule spans the layout rect minus insets, centered on the
    /// rect's vertical midpoint, `thickness` tall.
    #[test]
    fn horizontal_rule_rect_is_inset_and_centered() {
        let mut divider = Divider::new().leading_inset(16.0).trailing_inset(4.0);
        divider.set_layout_rect(LayoutRect::new(0.0, 0.0, 100.0, 20.0));

        let rect = divider.rule_rect();
        assert_eq!(rect.left, 16.0);
        assert_eq!(rect.right, 96.0);
        assert_eq!(rect.top, 10.0 - DEFAULT_THICKNESS / 2.0);
        assert_eq!(rect.bottom, 10.0 + DEFAULT_THICKNESS / 2.0);
    }

    /// Vertical rule spans the layout rect minus insets, centered on the
    /// rect's horizontal midpoint, `thickness` wide.
    #[test]
    fn vertical_rule_rect_is_inset_and_centered() {
        let mut divider = Divider::new()
            .vertical()
            .leading_inset(4.0)
            .trailing_inset(8.0)
            .thickness(3.0);
        divider.set_layout_rect(LayoutRect::new(0.0, 0.0, 20.0, 100.0));

        let rect = divider.rule_rect();
        assert_eq!(rect.top, 4.0);
        assert_eq!(rect.bottom, 92.0);
        assert_eq!(rect.left, 10.0 - 1.5);
        assert_eq!(rect.right, 10.0 + 1.5);
    }

    /// `draw` never requests an animation frame — the divider is static.
    #[test]
    fn draw_returns_false() {
        let mut divider = Divider::new();
        divider.set_layout_rect(LayoutRect::new(0.0, 0.0, 100.0, 20.0));

        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 20)).expect("surface");
        let theme = ColorTheme::default();
        let fonts = FontBook::new();

        assert!(!divider.draw(surface.canvas(), &theme, &fonts));
    }
}
