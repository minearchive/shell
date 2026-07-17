//! Material 3 slider: continuous or discrete, five sizes.

use skia_safe::{
    utils::text_utils::Align, Canvas, Color4f, Contains, Paint, Point, RRect, Rect, Vector,
};

use ui_core::{
    font::FontBook,
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::Widget;

/// Disabled treatments.
const DISABLED_TRACK_OPACITY: f32 = 0.12;
const DISABLED_CONTENT_OPACITY: f32 = 0.38;

/// Handle width at rest and while pressed — the handle narrows on press.
const HANDLE_WIDTH: f32 = 4.0;
const HANDLE_WIDTH_PRESSED: f32 = 2.0;

/// Gap between the handle and each track segment.
const HANDLE_GAP: f32 = 6.0;

/// The track corners that face the handle stay tight; the outer ones are `full`.
const INNER_CORNER_RADIUS: f32 = 2.0;

/// Stop indicator and tick mark diameter.
const STOP_INDICATOR_DIAMETER: f32 = 4.0;

/// Pointer target height, so the thinner sizes stay comfortably hittable.
const MIN_TOUCH_HEIGHT: f32 = 48.0;

/// Value indicator: Label Large text in a pill above the handle.
const VALUE_INDICATOR_TEXT_SIZE: f32 = 14.0;
const VALUE_INDICATOR_HEIGHT: f32 = 28.0;
const VALUE_INDICATOR_PADDING: f32 = 12.0;
/// Space between the value indicator and the handle.
const VALUE_INDICATOR_BOTTOM_SPACE: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliderSize {
    #[default]
    ExtraSmall,
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl SliderSize {
    pub fn track_height(self) -> f32 {
        match self {
            Self::ExtraSmall => 16.0,
            Self::Small => 24.0,
            Self::Medium => 40.0,
            Self::Large => 56.0,
            Self::ExtraLarge => 96.0,
        }
    }

    /// The handle overhangs the track, and never drops below 44dp so it stays
    /// grabbable at the thin sizes.
    pub fn handle_height(self) -> f32 {
        match self {
            Self::ExtraSmall | Self::Small => 44.0,
            Self::Medium => 52.0,
            Self::Large => 68.0,
            Self::ExtraLarge => 108.0,
        }
    }

    /// Tracks use the `full` shape token on their outer edges.
    pub fn corner_radius(self) -> f32 {
        self.track_height() / 2.0
    }
}

pub struct Slider {
    value: f32,
    min: f32,
    max: f32,
    /// `Some` makes the slider discrete: values snap and tick marks are drawn.
    step: Option<f32>,
    size: SliderSize,
    origin: (f32, f32),
    width: f32,
    font_key: String,
    labeled: bool,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// Widget rect (handle height, not track height), recomputed every `draw`.
    /// Hit testing uses it, so it only works once the slider has been laid out.
    bounds: Rect,
    on_change: Option<Box<dyn FnMut(f32)>>,
}

impl Slider {
    pub fn new(min: f32, max: f32, value: f32) -> Self {
        let mut slider = Self {
            value: 0.0,
            min,
            max,
            step: None,
            size: SliderSize::default(),
            origin: (0.0, 0.0),
            width: 200.0,
            font_key: "noto_sans".to_string(),
            labeled: false,
            enabled: true,
            hovered: false,
            pressed: false,
            bounds: Rect::new_empty(),
            on_change: None,
        };
        slider.value = slider.quantize(value);
        slider
    }

    pub fn size(mut self, size: SliderSize) -> Self {
        self.size = size;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.origin = (x, y);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Snaps values to `step` and draws tick marks.
    pub fn step(mut self, step: f32) -> Self {
        self.step = (step > 0.0).then_some(step);
        self.value = self.quantize(self.value);
        self
    }

    /// Key of a font registered in the [`FontBook`]; defaults to `noto_sans`.
    pub fn font(mut self, key: impl Into<String>) -> Self {
        self.font_key = key.into();
        self
    }

    /// Shows the value indicator above the handle while interacting.
    pub fn labeled(mut self, labeled: bool) -> Self {
        self.labeled = labeled;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Called with the new value on every change while dragging.
    pub fn on_change(mut self, callback: impl FnMut(f32) + 'static) -> Self {
        self.set_on_change(callback);
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Replaces the change callback, dropping any previous one.
    pub fn set_on_change(&mut self, callback: impl FnMut(f32) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    /// Sets the value without invoking `on_change`.
    pub fn set_value(&mut self, value: f32) {
        self.value = self.quantize(value);
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// The widget rect: as tall as the handle, so it spans the track overhang.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    fn quantize(&self, value: f32) -> f32 {
        let value = value.clamp(self.min, self.max);
        match self.step {
            Some(step) => {
                let snapped = self.min + ((value - self.min) / step).round() * step;
                snapped.clamp(self.min, self.max)
            }
            None => value,
        }
    }

    fn fraction(&self) -> f32 {
        let span = self.max - self.min;
        if span <= 0.0 {
            0.0
        } else {
            (self.value - self.min) / span
        }
    }

    fn layout(&self) -> Rect {
        Rect::from_xywh(
            self.origin.0,
            self.origin.1,
            self.width.max(self.size.track_height()),
            self.size.handle_height(),
        )
    }

    /// The track is centred in the bounds; the handle overhangs it either side.
    fn track_rect(&self) -> Rect {
        let height = self.size.track_height();
        Rect::from_xywh(
            self.bounds.left,
            self.bounds.center_y() - height / 2.0,
            self.bounds.width(),
            height,
        )
    }

    /// Padded bounds; the pointer target, not the paint area.
    fn hit_rect(&self) -> Rect {
        let pad = ((MIN_TOUCH_HEIGHT - self.bounds.height()) / 2.0).max(0.0);
        self.bounds.with_outset((0.0, pad))
    }

    /// Travel is inset by half a handle so the handle stays inside the bounds.
    fn handle_center_x(&self) -> f32 {
        let fraction = self.fraction();
        // A discrete handle lines up with its tick, which sits a track corner
        // further in — except at the ends, where the handle goes flush instead.
        if self.step.is_some() && fraction > 0.0 && fraction < 1.0 {
            return self.mark_x(fraction);
        }
        let left = self.bounds.left + HANDLE_WIDTH / 2.0;
        let right = self.bounds.right - HANDLE_WIDTH / 2.0;
        left + (right - left) * fraction
    }

    /// Marks are laid out over the track inset by a corner on each side, so the
    /// outermost ones sit inside the rounded caps rather than on them.
    fn mark_x(&self, fraction: f32) -> f32 {
        let corner = self.size.corner_radius();
        let left = self.bounds.left + corner;
        let right = self.bounds.right - corner;
        left + (right - left) * fraction
    }

    /// Inverse of [`Self::handle_center_x`]: pointer x to a quantized value.
    fn value_at(&self, x: f32) -> f32 {
        let left = self.bounds.left + HANDLE_WIDTH / 2.0;
        let right = self.bounds.right - HANDLE_WIDTH / 2.0;
        let span = right - left;
        let fraction = if span <= 0.0 {
            0.0
        } else {
            ((x - left) / span).clamp(0.0, 1.0)
        };
        self.quantize(self.min + fraction * (self.max - self.min))
    }

    fn active_track_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.primary
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        }
    }

    fn inactive_track_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.secondary_container
        } else {
            theme.on_surface.with_alpha(DISABLED_TRACK_OPACITY)
        }
    }

    fn handle_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.primary
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        }
    }

    fn format_value(&self) -> String {
        match self.step {
            Some(step) if step.fract() != 0.0 => format!("{:.1}", self.value),
            _ => format!("{:.0}", self.value),
        }
    }

    fn fill_rrect(canvas: &Canvas, rrect: RRect, color: Color) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(color), None);
        canvas.draw_rrect(rrect, &paint);
    }

    fn fill_circle(canvas: &Canvas, center: Point, radius: f32, color: Color) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(color), None);
        canvas.draw_circle(center, radius, &paint);
    }

    /// A track segment: full corners on `outer_side`, tight ones facing the handle.
    fn track_rrect(rect: Rect, outer_radius: f32, outer_side: Side) -> RRect {
        let outer = Vector::new(outer_radius, outer_radius);
        let inner = Vector::new(INNER_CORNER_RADIUS, INNER_CORNER_RADIUS);
        // Corner order: upper-left, upper-right, lower-right, lower-left.
        let radii = match outer_side {
            Side::Left => [outer, inner, inner, outer],
            Side::Right => [inner, outer, outer, inner],
        };
        RRect::new_rect_radii(rect, &radii)
    }

    /// The fractions carrying a mark: every step on a discrete slider, or just
    /// the far end on a continuous one, where the mark is the stop indicator.
    fn mark_fractions(&self) -> Vec<f32> {
        let span = self.max - self.min;
        match self.step {
            Some(step) if span > 0.0 => {
                let steps = (span / step).round() as i32;
                (0..=steps).map(|i| i as f32 * step / span).collect()
            }
            Some(_) => Vec::new(),
            None => vec![1.0],
        }
    }

    /// A mark takes the color of the track it does *not* sit on, and is dropped
    /// where the handle's gap would swallow it.
    fn draw_marks(&self, canvas: &Canvas, theme: &ColorTheme, handle_x: f32, handle_width: f32) {
        let on_active = self.inactive_track_color(theme);
        let on_inactive = self.active_track_color(theme);
        let radius = STOP_INDICATOR_DIAMETER / 2.0;
        let y = self.bounds.center_y();
        let gap = handle_width / 2.0 + HANDLE_GAP;

        for fraction in self.mark_fractions() {
            let x = self.mark_x(fraction);
            if (x - handle_x).abs() <= gap {
                continue;
            }
            let color = if x < handle_x { on_active } else { on_inactive };
            Self::fill_circle(canvas, Point::new(x, y), radius, color);
        }
    }

    /// A pill above the handle, centred on it and as wide as its label needs.
    fn draw_value_indicator(
        &self,
        canvas: &Canvas,
        theme: &ColorTheme,
        fonts: &FontBook,
        handle_x: f32,
    ) {
        let text = self.format_value();
        let font = fonts.sized(&self.font_key, VALUE_INDICATOR_TEXT_SIZE);
        let text_width = font.measure_str(&text, None).0;

        let width = (text_width + VALUE_INDICATOR_PADDING * 2.0).max(VALUE_INDICATOR_HEIGHT);
        let bottom = self.bounds.top - VALUE_INDICATOR_BOTTOM_SPACE;
        let rect = Rect::from_xywh(
            handle_x - width / 2.0,
            bottom - VALUE_INDICATOR_HEIGHT,
            width,
            VALUE_INDICATOR_HEIGHT,
        );

        let radius = VALUE_INDICATOR_HEIGHT / 2.0;
        Self::fill_rrect(
            canvas,
            RRect::new_rect_xy(rect, radius, radius),
            theme.inverse_surface,
        );

        let metrics = font.metrics().1;
        let baseline = rect.center_y() - (metrics.ascent + metrics.descent) / 2.0;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(theme.inverse_on_surface), None);
        canvas.draw_str_align(
            &text,
            Point::new(rect.center_x(), baseline),
            &font,
            &paint,
            Align::Center,
        );
    }

    /// Applies a pointer position, reporting whether the value moved.
    fn drag_to(&mut self, x: f32) -> bool {
        let value = self.value_at(x);
        if value == self.value {
            return false;
        }
        self.value = value;
        if let Some(callback) = self.on_change.as_mut() {
            callback(value);
        }
        true
    }
}

enum Side {
    Left,
    Right,
}

impl Widget for Slider {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) {
        self.bounds = self.layout();
        let track = self.track_rect();

        let radius = self.size.corner_radius();
        let handle_x = self.handle_center_x();
        let handle_width = if self.pressed {
            HANDLE_WIDTH_PRESSED
        } else {
            HANDLE_WIDTH
        };

        let active = Rect::new(
            track.left,
            track.top,
            (handle_x - handle_width / 2.0 - HANDLE_GAP).max(track.left),
            track.bottom,
        );
        let inactive = Rect::new(
            (handle_x + handle_width / 2.0 + HANDLE_GAP).min(track.right),
            track.top,
            track.right,
            track.bottom,
        );

        if active.width() > 0.0 {
            Self::fill_rrect(
                canvas,
                Self::track_rrect(active, radius, Side::Left),
                self.active_track_color(theme),
            );
        }
        if inactive.width() > 0.0 {
            Self::fill_rrect(
                canvas,
                Self::track_rrect(inactive, radius, Side::Right),
                self.inactive_track_color(theme),
            );
        }

        self.draw_marks(canvas, theme, handle_x, handle_width);

        let handle = Rect::from_xywh(
            handle_x - handle_width / 2.0,
            self.bounds.top,
            handle_width,
            self.bounds.height(),
        );
        let handle_radius = handle_width / 2.0;
        Self::fill_rrect(
            canvas,
            RRect::new_rect_xy(handle, handle_radius, handle_radius),
            self.handle_color(theme),
        );

        if self.labeled && self.enabled && (self.pressed || self.hovered) {
            self.draw_value_indicator(canvas, theme, fonts, handle_x);
        }
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
            self.hovered = false;
            self.pressed = false;
            return dirty;
        }

        let x = event.x() as f32;
        let inside = self.hit_rect().contains(Point::new(x, event.y() as f32));

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let changed = self.hovered != inside;
                self.hovered = inside;
                // A drag keeps tracking the pointer once it leaves the track.
                if self.pressed {
                    return self.drag_to(x) || changed;
                }
                changed
            }
            PointerEventKind::Leave => {
                let dirty = self.hovered || self.pressed;
                self.hovered = false;
                self.pressed = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                if inside {
                    self.hovered = true;
                    self.pressed = true;
                    self.drag_to(x);
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.pressed = false;
                was_pressed
            }
            _ => false,
        }
    }
}
