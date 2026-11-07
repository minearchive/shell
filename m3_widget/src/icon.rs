//! The built-in vector icon set, shared by every widget that paints a glyph.
//!
//! Lives at the crate root rather than under `buttons/` because icons are not
//! a button-specific concept: icon buttons, segmented buttons and list items
//! (see `list_item.rs`'s `IconDrawer`) all draw from the same set.

use skia_safe::{Canvas, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, Rect};

use ui_core::scheme::color::Color;

/// Icons are drawn centered in a 24dp box, per MD3. Public because callers
/// place the box themselves.
pub const BOX_SIZE: f32 = 24.0;
const ICON_STROKE_WIDTH: f32 = 2.0;

/// A small, built-in vector icon set, drawn as strokes/fills in a 24x24 box
/// via [`skia_safe::PathBuilder`] — the same technique `switch.rs` uses for
/// its checkmark/cross. Callers needing something outside this set should
/// use [`crate::IconButton::icon_fn`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Check,
    Close,
    Add,
    /// Three horizontal lines ("hamburger" menu).
    Menu,
    Favorite,
    /// A simplified gear glyph: a ring, a hub, and radial teeth ticks —
    /// legible at 24dp without needing a full involute-gear path.
    Settings,
    /// Three dots (an overflow/"more" affordance).
    More,
    ArrowBack,
}

impl Icon {
    /// Renders the icon centered in `box_rect` (nominally [`BOX_SIZE`]
    /// square). No-op if `color` is fully transparent.
    pub fn draw(self, canvas: &Canvas, box_rect: Rect, color: Color) {
        if color.a <= 0.0 {
            return;
        }
        match self {
            Icon::Check => draw_check(canvas, box_rect, color),
            Icon::Close => draw_close(canvas, box_rect, color),
            Icon::Add => draw_add(canvas, box_rect, color),
            Icon::Menu => draw_menu(canvas, box_rect, color),
            Icon::Favorite => draw_favorite(canvas, box_rect, color),
            Icon::Settings => draw_settings(canvas, box_rect, color),
            Icon::More => draw_more(canvas, box_rect, color),
            Icon::ArrowBack => draw_arrow_back(canvas, box_rect, color),
        }
    }
}

fn stroke_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::from(color), None);
    paint.set_stroke(true);
    paint.set_stroke_width(ICON_STROKE_WIDTH);
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint
}

fn fill_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(Color4f::from(color), None);
    paint
}

/// A checkmark, in the manner of `switch.rs`'s `draw_icon`.
fn draw_check(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(5.0, 12.5))
        .line_to(at(10.0, 17.5))
        .line_to(at(19.0, 7.0));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// A diagonal cross.
fn draw_close(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder.move_to(at(6.0, 6.0)).line_to(at(18.0, 18.0));
    builder.move_to(at(18.0, 6.0)).line_to(at(6.0, 18.0));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// A plus.
fn draw_add(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder.move_to(at(12.0, 5.0)).line_to(at(12.0, 19.0));
    builder.move_to(at(5.0, 12.0)).line_to(at(19.0, 12.0));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// Three horizontal lines.
fn draw_menu(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    for y in [6.0, 12.0, 18.0] {
        builder.move_to(at(4.0, y)).line_to(at(20.0, y));
    }
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// Three filled dots.
fn draw_more(canvas: &Canvas, box_rect: Rect, color: Color) {
    const DOT_RADIUS: f32 = 1.6;
    let cy = box_rect.center_y();
    let paint = fill_paint(color);
    for x in [
        box_rect.left + 6.0,
        box_rect.center_x(),
        box_rect.right - 6.0,
    ] {
        canvas.draw_circle(Point::new(x, cy), DOT_RADIUS, &paint);
    }
}

/// A leftward-pointing arrow: a shaft plus a chevron head.
fn draw_arrow_back(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder.move_to(at(20.0, 12.0)).line_to(at(4.0, 12.0));
    builder
        .move_to(at(11.0, 5.0))
        .line_to(at(4.0, 12.0))
        .line_to(at(11.0, 19.0));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// A filled heart, transcribed from the standard Material "favorite" glyph
/// (its SVG path data maps directly onto our 24x24 icon box).
fn draw_favorite(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(12.0, 21.35))
        .line_to(at(10.55, 20.03))
        .cubic_to(at(5.4, 15.36), at(2.0, 12.28), at(2.0, 8.5))
        .cubic_to(at(2.0, 5.42), at(4.42, 3.0), at(7.5, 3.0))
        .cubic_to(at(9.24, 3.0), at(10.91, 3.81), at(12.0, 5.09))
        .cubic_to(at(13.09, 3.81), at(14.76, 3.0), at(16.5, 3.0))
        .cubic_to(at(19.58, 3.0), at(22.0, 5.42), at(22.0, 8.5))
        .cubic_to(at(22.0, 12.28), at(18.6, 15.36), at(13.45, 20.04))
        .close();
    canvas.draw_path(&builder.detach(), &fill_paint(color));
}

/// A simplified gear: an outer ring, a hub, and radial teeth ticks.
fn draw_settings(canvas: &Canvas, box_rect: Rect, color: Color) {
    const OUTER_RADIUS: f32 = 6.5;
    const INNER_RADIUS: f32 = 2.5;
    const TOOTH_LENGTH: f32 = 2.5;
    const TEETH: usize = 8;

    let center = Point::new(box_rect.center_x(), box_rect.center_y());
    let mut builder = PathBuilder::new();
    for i in 0..TEETH {
        let angle = i as f32 * std::f32::consts::TAU / TEETH as f32;
        let (sin, cos) = angle.sin_cos();
        let inner = Point::new(center.x + cos * OUTER_RADIUS, center.y + sin * OUTER_RADIUS);
        let outer = Point::new(
            center.x + cos * (OUTER_RADIUS + TOOTH_LENGTH),
            center.y + sin * (OUTER_RADIUS + TOOTH_LENGTH),
        );
        builder.move_to(inner).line_to(outer);
    }

    let paint = stroke_paint(color);
    canvas.draw_path(&builder.detach(), &paint);
    canvas.draw_circle(center, OUTER_RADIUS, &paint);
    canvas.draw_circle(center, INNER_RADIUS, &paint);
}
