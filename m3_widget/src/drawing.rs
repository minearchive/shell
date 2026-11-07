//! Shared skia drawing primitives for the Material 3 widgets.

use skia_safe::{Canvas, Color4f, Paint, Point};

use ui_core::scheme::color::Color;

/// Fills a circle, skipping fully transparent or zero-radius draws (both of
/// which contribute nothing).
pub fn fill_circle(canvas: &Canvas, center: Point, radius: f32, color: Color) {
    if color.a <= 0.0 || radius <= 0.0 {
        return;
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::from(color), None);
    canvas.draw_circle(center, radius, &paint);
}

/// Strokes a circle outline of the given width, skipping fully transparent
/// draws.
pub fn stroke_circle(canvas: &Canvas, center: Point, radius: f32, width: f32, color: Color) {
    if color.a <= 0.0 {
        return;
    }
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::from(color), None);
    paint.set_stroke(true);
    paint.set_stroke_width(width);
    canvas.draw_circle(center, radius, &paint);
}
