//! Material 3 slider: continuous or discrete, five sizes.

use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint, Point, RRect, Rect, Vector};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::LayoutRect,
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    tokens::motion::{duration, easing},
    Widget,
};

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
    font_key: String,
    labeled: bool,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// Widget rect (handle height, not track height), assigned by the layout
    /// system via [`Widget::set_layout_rect`]. Hit testing uses it, so it
    /// only works once the slider has been laid out.
    layout_rect: LayoutRect,
    on_change: Option<Box<dyn FnMut(f32)>>,
    animations: Animations,
}

pub struct Animations {
    /// 0 at rest (`HANDLE_WIDTH`), 1 pressed (`HANDLE_WIDTH_PRESSED`).
    handle_width: Animation<f32>,
    /// 0 hidden, 1 fully shown; drives the value indicator's fade.
    indicator: Animation<f32>,
}

impl Animations {
    pub fn new() -> Self {
        Self {
            handle_width: Animation::new(0., 0., duration::SHORT4, easing::standard()),
            indicator: Animation::new(0., 0., duration::SHORT4, easing::standard()),
        }
    }
}

impl Slider {
    pub fn new(min: f32, max: f32, value: f32) -> Self {
        let mut slider = Self {
            value: 0.0,
            min,
            max,
            step: None,
            size: SliderSize::default(),
            font_key: "noto_sans".to_string(),
            labeled: false,
            enabled: true,
            hovered: false,
            pressed: false,
            layout_rect: LayoutRect::empty(),
            on_change: None,
            animations: Animations::new(),
        };
        slider.value = slider.quantize(value);
        slider
    }

    pub fn size(mut self, size: SliderSize) -> Self {
        self.size = size;
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

    /// The widget rect assigned by the layout system, as a Skia rect for
    /// drawing and geometry math.
    fn rect(&self) -> Rect {
        self.layout_rect.to_skia()
    }

    /// The track is centred in the bounds; the handle overhangs it either side.
    fn track_rect(&self) -> Rect {
        let bounds = self.rect();
        let height = self.size.track_height();
        Rect::from_xywh(
            bounds.left,
            bounds.center_y() - height / 2.0,
            bounds.width(),
            height,
        )
    }

    /// Travel is inset by half a handle so the handle stays inside the bounds.
    fn handle_center_x(&self) -> f32 {
        let fraction = self.fraction();
        // A discrete handle lines up with its tick, which sits a track corner
        // further in — except at the ends, where the handle goes flush instead.
        if self.step.is_some() && fraction > 0.0 && fraction < 1.0 {
            return self.mark_x(fraction);
        }
        let bounds = self.rect();
        let left = bounds.left + HANDLE_WIDTH / 2.0;
        let right = bounds.right - HANDLE_WIDTH / 2.0;
        left + (right - left) * fraction
    }

    /// Marks are laid out over the track inset by a corner on each side, so the
    /// outermost ones sit inside the rounded caps rather than on them.
    fn mark_x(&self, fraction: f32) -> f32 {
        let bounds = self.rect();
        let corner = self.size.corner_radius();
        let left = bounds.left + corner;
        let right = bounds.right - corner;
        left + (right - left) * fraction
    }

    /// Exact inverse of [`Self::handle_center_x`]: pointer x to a quantized
    /// value. Uses the same inset — the track corner radius for discrete
    /// sliders (matching `mark_x`), half the handle width otherwise — so
    /// interior ticks round-trip exactly. The flush endpoints fall outside
    /// `[left, right]` under the corner inset; the clamp below pins those to
    /// min/max, which is exactly where `handle_center_x` puts the handle too.
    fn value_at(&self, x: f32) -> f32 {
        let bounds = self.rect();
        let inset = if self.step.is_some() {
            self.size.corner_radius()
        } else {
            HANDLE_WIDTH / 2.0
        };
        let left = bounds.left + inset;
        let right = bounds.right - inset;
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
        let y = self.rect().center_y();
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
    /// `progress` is the indicator animation's value: 0 invisible, 1 opaque.
    fn draw_value_indicator(
        &self,
        canvas: &Canvas,
        theme: &ColorTheme,
        fonts: &FontBook,
        handle_x: f32,
        progress: f32,
    ) {
        let text = self.format_value();
        let font = fonts.sized(&self.font_key, VALUE_INDICATOR_TEXT_SIZE);
        let text_width = font.measure_str(&text, None).0;

        let width = (text_width + VALUE_INDICATOR_PADDING * 2.0).max(VALUE_INDICATOR_HEIGHT);
        let bottom = self.rect().top - VALUE_INDICATOR_BOTTOM_SPACE;
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
            theme.inverse_surface.with_alpha(progress),
        );

        let metrics = font.metrics().1;
        let baseline = rect.center_y() - (metrics.ascent + metrics.descent) / 2.0;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(
            Color4f::from(theme.inverse_on_surface.with_alpha(progress)),
            None,
        );
        canvas.draw_str_align(
            &text,
            Point::new(rect.center_x(), baseline),
            &font,
            &paint,
            Align::Center,
        );
    }

    /// Retargets the indicator fade toward its current visibility, without
    /// restarting an already-running animation that's headed the same way.
    fn sync_indicator_target(&mut self) {
        let target = if self.hovered || self.pressed {
            1.0
        } else {
            0.0
        };
        if self.animations.indicator.to() != target {
            self.animations.indicator.set_target(target);
        }
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
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let track = self.track_rect();

        let radius = self.size.corner_radius();
        let handle_x = self.handle_center_x();
        let handle_width = HANDLE_WIDTH
            + (HANDLE_WIDTH_PRESSED - HANDLE_WIDTH) * self.animations.handle_width.value();

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

        let bounds = self.rect();
        let handle = Rect::from_xywh(
            handle_x - handle_width / 2.0,
            bounds.top,
            handle_width,
            bounds.height(),
        );
        let handle_radius = handle_width / 2.0;
        Self::fill_rrect(
            canvas,
            RRect::new_rect_xy(handle, handle_radius, handle_radius),
            self.handle_color(theme),
        );

        let indicator_progress = self.animations.indicator.value();
        if self.labeled && self.enabled && indicator_progress > 0.0 {
            self.draw_value_indicator(canvas, theme, fonts, handle_x, indicator_progress);
        }

        let mut redraw = false;
        if !self.animations.handle_width.is_done()
            && self.animations.handle_width.from() != self.animations.handle_width.to()
        {
            redraw = true;
        }
        if !self.animations.indicator.is_done()
            && self.animations.indicator.from() != self.animations.indicator.to()
        {
            redraw = true;
        }

        redraw
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
            self.animations.handle_width.set_target(0.0);
            self.hovered = false;
            self.pressed = false;
            self.sync_indicator_target();
            return dirty;
        }

        let x = event.x() as f32;
        let inside = self.hit_rect().contains(x, event.y() as f32);

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let changed = self.hovered != inside;
                self.hovered = inside;
                self.sync_indicator_target();
                // A drag keeps tracking the pointer once it leaves the track.
                if self.pressed {
                    return self.drag_to(x) || changed;
                }
                changed
            }
            PointerEventKind::Leave => {
                let dirty = self.hovered || self.pressed;
                self.animations.handle_width.set_target(0.0);
                self.hovered = false;
                self.pressed = false;
                self.sync_indicator_target();
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                if inside {
                    self.animations.handle_width.set_target(1.0);
                    self.hovered = true;
                    self.pressed = true;
                    self.sync_indicator_target();
                    self.drag_to(x);
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.animations.handle_width.set_target(0.0);
                self.pressed = false;
                self.sync_indicator_target();
                was_pressed
            }
            _ => false,
        }
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout_rect = rect;
    }

    /// The widget rect: as tall as the handle, so it spans the track overhang.
    fn layout_rect(&self) -> LayoutRect {
        self.layout_rect
    }

    /// Padded vertically to the minimum comfortable touch target; the
    /// pointer target, not the paint area. Does not include the temporary
    /// value-indicator pill, which is visual overflow, not a new hit target.
    fn hit_rect(&self) -> LayoutRect {
        let pad = ((MIN_TOUCH_HEIGHT - self.layout_rect.height) / 2.0).max(0.0);
        self.layout_rect.outset(0.0, pad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut slider = Slider::new(0.0, 100.0, 50.0);
        assert_eq!(slider.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(10.0, 20.0, 200.0, SliderSize::ExtraSmall.handle_height());
        slider.set_layout_rect(rect);
        assert_eq!(slider.layout_rect(), rect);
    }

    /// `ExtraSmall` slider handles (44dp) are already close to the 48dp
    /// minimum, so `hit_rect` should still pad them out a little.
    #[test]
    fn hit_rect_expands_to_minimum_touch_target() {
        let mut slider = Slider::new(0.0, 100.0, 50.0).size(SliderSize::ExtraSmall);
        let rect = LayoutRect::new(10.0, 20.0, 200.0, SliderSize::ExtraSmall.handle_height());
        slider.set_layout_rect(rect);

        let hit = slider.hit_rect();
        assert!(hit.height >= MIN_TOUCH_HEIGHT);
        assert_eq!(hit.width, rect.width);
        // A point just above the visual rect, but within the padded target,
        // must register as a hit.
        let just_above = rect.y - 1.0;
        assert!(hit.contains(rect.x + 1.0, just_above));
        assert!(!rect.contains(rect.x + 1.0, just_above));
    }

    /// `value_at` must be the exact inverse of `handle_center_x` at every
    /// interior tick of a discrete slider: feeding a tick's drawn x back in
    /// must return that same value, not a neighboring step. Regression test
    /// for a mismatched inset (corner radius vs. half handle width) that
    /// mis-rounded clicks on `ExtraLarge`, where the gap is largest.
    #[test]
    fn value_at_is_exact_inverse_of_handle_center_x_at_interior_ticks() {
        let mut slider = Slider::new(0.0, 100.0, 0.0)
            .size(SliderSize::ExtraLarge)
            .step(10.0);
        let rect = LayoutRect::new(0.0, 0.0, 600.0, SliderSize::ExtraLarge.handle_height());
        slider.set_layout_rect(rect);

        for step in 1..10 {
            let value = step as f32 * 10.0;
            slider.set_value(value);
            let x = slider.handle_center_x();
            assert_eq!(slider.value_at(x), value);
        }
    }

    /// The value indicator paints above the handle but must not be reachable
    /// as a hit target — only the padded track/handle area is.
    #[test]
    fn hit_rect_excludes_value_indicator_overflow() {
        let mut slider = Slider::new(0.0, 100.0, 50.0).labeled(true);
        let rect = LayoutRect::new(10.0, 100.0, 200.0, SliderSize::default().handle_height());
        slider.set_layout_rect(rect);

        // The value indicator paints well above the top of the rect
        // (VALUE_INDICATOR_HEIGHT + VALUE_INDICATOR_BOTTOM_SPACE), which is
        // farther up than the touch-target padding reaches.
        let indicator_y = rect.y - VALUE_INDICATOR_HEIGHT - VALUE_INDICATOR_BOTTOM_SPACE;
        assert!(!slider.hit_rect().contains(rect.x + 10.0, indicator_y));
    }
}
