//! The built-in vector icon set, shared by every widget that paints a glyph.
//!
//! Lives at the crate root rather than under `buttons/` because icons are not
//! a button-specific concept: icon buttons, segmented buttons and list items
//! (see `list_item.rs`'s `IconDrawer`) all draw from the same set.

use skia_safe::{BlendMode, Canvas, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, Rect};

use ui_core::scheme::color::Color;

/// Icons are drawn centered in a 24dp box, per MD3. Public because callers
/// place the box themselves.
pub const BOX_SIZE: f32 = 24.0;
const ICON_STROKE_WIDTH: f32 = 2.0;

/// What a battery icon says about the power supply, on top of its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryMark {
    None,
    /// Charging.
    Bolt,
    /// Plugged in and full.
    Check,
}

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
    Wifi,
    WifiOff,
    Ethernet,
    VolumeUp,
    VolumeOff,
    Bluetooth,
    BluetoothOff,
    /// `level` is a percentage; the fill tracks it.
    Battery {
        level: u8,
        mark: BatteryMark,
    },
    /// An empty battery outline with a slash: there is no battery to report on.
    BatteryMissing,
    Brightness,
    Shield,
    Play,
    Pause,
    SkipNext,
    SkipPrevious,
    MusicNote,
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
            Icon::Wifi => draw_wifi(canvas, box_rect, color),
            Icon::WifiOff => {
                draw_wifi(canvas, box_rect, color);
                draw_slash(canvas, box_rect, color);
            }
            Icon::Ethernet => draw_ethernet(canvas, box_rect, color),
            Icon::VolumeUp => {
                draw_speaker(canvas, box_rect, color);
                draw_sound_waves(canvas, box_rect, color);
            }
            Icon::VolumeOff => {
                draw_speaker(canvas, box_rect, color);
                draw_slash(canvas, box_rect, color);
            }
            Icon::Bluetooth => draw_bluetooth(canvas, box_rect, color),
            Icon::BluetoothOff => {
                draw_bluetooth(canvas, box_rect, color);
                draw_slash(canvas, box_rect, color);
            }
            Icon::Battery { level, mark } => {
                draw_battery_outline(canvas, box_rect, color);
                draw_battery_level(canvas, box_rect, color, level, mark);
            }
            Icon::BatteryMissing => {
                draw_battery_outline(canvas, box_rect, color);
                draw_slash(canvas, box_rect, color);
            }
            Icon::Brightness => draw_brightness(canvas, box_rect, color),
            Icon::Shield => draw_shield(canvas, box_rect, color),
            Icon::Play => draw_play(canvas, box_rect, color),
            Icon::Pause => draw_pause(canvas, box_rect, color),
            Icon::SkipNext => draw_skip(canvas, box_rect, color, false),
            Icon::SkipPrevious => draw_skip(canvas, box_rect, color, true),
            Icon::MusicNote => draw_music_note(canvas, box_rect, color),
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

/// A corner-to-corner stroke, marking an icon as off.
fn draw_slash(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    canvas.draw_line(at(4.0, 4.0), at(20.0, 20.0), &stroke_paint(color));
}

/// Three arcs fanning up from a dot.
fn draw_wifi(canvas: &Canvas, box_rect: Rect, color: Color) {
    let center = Point::new(box_rect.left + 12.0, box_rect.top + 18.5);
    let paint = stroke_paint(color);
    for radius in [5.0, 10.0, 15.0] {
        let oval = Rect::new(
            center.x - radius,
            center.y - radius,
            center.x + radius,
            center.y + radius,
        );
        canvas.draw_arc(oval, 225.0, 90.0, false, &paint);
    }
    canvas.draw_circle(center, 1.5, &fill_paint(color));
}

/// A box on top, branching down to three more: a network drop.
fn draw_ethernet(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let node = |x: f32, y: f32, size: f32| {
        Rect::from_xywh(box_rect.left + x, box_rect.top + y, size, size)
    };
    let paint = stroke_paint(color);
    let mut builder = PathBuilder::new();
    builder.move_to(at(12.0, 9.0)).line_to(at(12.0, 16.0));
    builder
        .move_to(at(5.0, 16.0))
        .line_to(at(5.0, 12.5))
        .line_to(at(19.0, 12.5))
        .line_to(at(19.0, 16.0));
    canvas.draw_path(&builder.detach(), &paint);
    canvas.draw_rect(node(9.0, 3.0, 6.0), &paint);
    for x in [3.0, 10.0, 17.0] {
        canvas.draw_rect(node(x, 16.0, 4.0), &paint);
    }
}

/// A filled speaker cone, on the icon's left.
fn draw_speaker(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(3.0, 9.0))
        .line_to(at(7.0, 9.0))
        .line_to(at(12.0, 4.5))
        .line_to(at(12.0, 19.5))
        .line_to(at(7.0, 15.0))
        .line_to(at(3.0, 15.0))
        .close();
    canvas.draw_path(&builder.detach(), &fill_paint(color));
}

/// Two arcs coming off [`draw_speaker`].
fn draw_sound_waves(canvas: &Canvas, box_rect: Rect, color: Color) {
    let center = Point::new(box_rect.left + 12.0, box_rect.top + 12.0);
    let paint = stroke_paint(color);
    for radius in [4.0, 8.0] {
        let oval = Rect::new(
            center.x - radius,
            center.y - radius,
            center.x + radius,
            center.y + radius,
        );
        canvas.draw_arc(oval, -50.0, 100.0, false, &paint);
    }
}

/// The Bluetooth rune.
fn draw_bluetooth(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(6.5, 7.5))
        .line_to(at(17.5, 16.5))
        .line_to(at(12.0, 21.0))
        .line_to(at(12.0, 3.0))
        .line_to(at(17.5, 7.5))
        .line_to(at(6.5, 16.5));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

fn draw_shield(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(12.0, 3.0))
        .line_to(at(19.5, 6.0))
        .line_to(at(19.5, 11.5))
        .quad_to(at(19.5, 17.5), at(12.0, 21.0))
        .quad_to(at(4.5, 17.5), at(4.5, 11.5))
        .line_to(at(4.5, 6.0))
        .close();
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

fn draw_play(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(8.0, 5.0))
        .line_to(at(19.0, 12.0))
        .line_to(at(8.0, 19.0))
        .close();
    canvas.draw_path(&builder.detach(), &fill_paint(color));
}

fn draw_pause(canvas: &Canvas, box_rect: Rect, color: Color) {
    let paint = fill_paint(color);
    for x in [6.0, 14.0] {
        let bar = Rect::from_xywh(box_rect.left + x, box_rect.top + 5.0, 4.0, 14.0);
        canvas.draw_rect(bar, &paint);
    }
}

fn draw_music_note(canvas: &Canvas, box_rect: Rect, color: Color) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    canvas.draw_circle(at(9.5, 17.0), 3.5, &fill_paint(color));
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(12.0, 17.0))
        .line_to(at(12.0, 4.0))
        .line_to(at(18.0, 7.0));
    canvas.draw_path(&builder.detach(), &stroke_paint(color));
}

/// A triangle against a bar, pointing left when `backward`.
fn draw_skip(canvas: &Canvas, box_rect: Rect, color: Color, backward: bool) {
    let at = |x: f32, y: f32| {
        let x = if backward { 24.0 - x } else { x };
        Point::new(box_rect.left + x, box_rect.top + y)
    };
    let mut builder = PathBuilder::new();
    builder
        .move_to(at(6.0, 6.0))
        .line_to(at(15.0, 12.0))
        .line_to(at(6.0, 18.0))
        .close();
    builder
        .move_to(at(16.0, 6.0))
        .line_to(at(18.0, 6.0))
        .line_to(at(18.0, 18.0))
        .line_to(at(16.0, 18.0))
        .close();
    canvas.draw_path(&builder.detach(), &fill_paint(color));
}

/// A horizontal cell with a terminal nub on its right.
fn draw_battery_outline(canvas: &Canvas, box_rect: Rect, color: Color) {
    let cell = Rect::from_xywh(box_rect.left + 2.0, box_rect.top + 7.0, 17.0, 10.0);
    canvas.draw_round_rect(cell, 2.5, 2.5, &stroke_paint(color));
    let nub = Rect::from_xywh(box_rect.left + 20.0, box_rect.top + 10.0, 2.0, 4.0);
    canvas.draw_rect(nub, &fill_paint(color));
}

/// Fills [`draw_battery_outline`]'s cell to `level`, with any `mark` over it. The mark
/// is carved out of the fill first, so it stays legible in one color at any level.
fn draw_battery_level(canvas: &Canvas, box_rect: Rect, color: Color, level: u8, mark: BatteryMark) {
    let at = |x: f32, y: f32| Point::new(box_rect.left + x, box_rect.top + y);
    let width = 13.0 * f32::from(level.min(100)) / 100.0;
    let fill = Rect::from_xywh(box_rect.left + 4.0, box_rect.top + 9.0, width, 6.0);

    let mut builder = PathBuilder::new();
    match mark {
        BatteryMark::None => {
            canvas.draw_rect(fill, &fill_paint(color));
            return;
        }
        BatteryMark::Bolt => builder
            .move_to(at(11.5, 8.0))
            .line_to(at(8.5, 12.5))
            .line_to(at(12.5, 11.5))
            .line_to(at(9.5, 16.0)),
        BatteryMark::Check => builder
            .move_to(at(7.5, 12.0))
            .line_to(at(9.5, 14.0))
            .line_to(at(13.5, 10.0)),
    };
    let glyph = builder.detach();

    canvas.save_layer_alpha_f(None, 1.0);
    canvas.draw_rect(fill, &fill_paint(color));
    let mut carve = stroke_paint(color);
    carve.set_blend_mode(BlendMode::Clear);
    carve.set_stroke_width(3.5);
    canvas.draw_path(&glyph, &carve);
    let mut stroke = stroke_paint(color);
    stroke.set_stroke_width(1.5);
    canvas.draw_path(&glyph, &stroke);
    canvas.restore();
}

/// A filled disc with eight rays.
fn draw_brightness(canvas: &Canvas, box_rect: Rect, color: Color) {
    let center = Point::new(box_rect.left + 12.0, box_rect.top + 12.0);
    canvas.draw_circle(center, 4.0, &fill_paint(color));
    let paint = stroke_paint(color);
    for ray in 0..8 {
        let (sin, cos) = (ray as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
        canvas.draw_line(
            Point::new(center.x + 7.0 * cos, center.y + 7.0 * sin),
            Point::new(center.x + 9.5 * cos, center.y + 9.5 * sin),
            &paint,
        );
    }
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
