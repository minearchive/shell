//! Material 3 button: five variants ordered by emphasis, five sizes.

use skia_safe::{
    utils::text_utils::Align, BlurStyle, Canvas, Color4f, MaskFilter, Paint, Point, RRect,
};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::{LayoutRect, Size},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    animation::{duration, easing},
    tokens::{
        DISABLED_CONTAINER_OPACITY, DISABLED_CONTENT_OPACITY, HOVER_OPACITY, PRESSED_OPACITY,
    },
    Widget,
};

/// Emphasis order: Filled > FilledTonal > Elevated > Outlined > Text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Primary action, highest emphasis.
    #[default]
    Filled,
    /// Medium emphasis, softer than filled.
    FilledTonal,
    /// Medium emphasis; the one variant that carries a shadow, for use on
    /// colored backgrounds where a tonal button would blend in.
    Elevated,
    /// Medium emphasis, neutral.
    Outlined,
    /// Lowest emphasis.
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonShape {
    // Rounded Type, full corner radius
    #[default]
    Round,
    // Squared type, less corner radius
    Square,
}

impl ButtonShape {
    pub fn corner_radius(self, size: ButtonSize, pressed: bool) -> f32 {
        match self {
            ButtonShape::Round => size.corner_radius(pressed),
            ButtonShape::Square => match size {
                ButtonSize::ExtraSmall => 12.0,
                ButtonSize::Small => 12.0,
                ButtonSize::Medium => 16.0,
                ButtonSize::Large => 28.0,
                ButtonSize::ExtraLarge => 28.0,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    ExtraSmall,
    #[default]
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl ButtonSize {
    pub fn height(self) -> f32 {
        match self {
            Self::ExtraSmall => 32.0,
            Self::Small => 40.0,
            Self::Medium => 48.0,
            Self::Large => 56.0,
            Self::ExtraLarge => 64.0,
        }
    }

    pub fn horizontal_padding(self) -> f32 {
        match self {
            Self::ExtraSmall => 12.0,
            Self::Small => 16.0,
            Self::Medium | Self::Large => 24.0,
            Self::ExtraLarge => 32.0,
        }
    }

    /// Label Large at the two smaller sizes, scaling up from there.
    pub fn label_size(self) -> f32 {
        match self {
            Self::ExtraSmall | Self::Small => 14.0,
            Self::Medium | Self::Large => 16.0,
            Self::ExtraLarge => 18.0,
        }
    }

    /// Buttons use the `full` shape token, so the radius tracks the height.
    pub fn corner_radius(self, pressed: bool) -> f32 {
        if pressed {
            match self {
                ButtonSize::ExtraSmall => 8.0,
                ButtonSize::Small => 8.0,
                ButtonSize::Medium => 12.0,
                ButtonSize::Large => 16.0,
                ButtonSize::ExtraLarge => 16.0,
            }
        } else {
            self.height() / 2.0
        }
    }
}

pub struct Button {
    label: String,
    variant: ButtonVariant,
    shape: ButtonShape,
    size: ButtonSize,
    font_key: String,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then, so hit testing only works once the button has been laid
    /// out.
    layout_rect: LayoutRect,
    on_click: Option<Box<dyn FnMut()>>,
    animations: Animations,
}

pub struct Animations {
    shape: Animation<f32>,
}

impl Animations {
    pub fn new() -> Self {
        Self {
            // Starts at rest (round); `set_target(1.0)` on press morphs it in.
            shape: Animation::new(0., 0., duration::SHORT4, easing::standard()),
        }
    }
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::default(),
            shape: ButtonShape::default(),
            size: ButtonSize::default(),
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            pressed: false,
            layout_rect: LayoutRect::empty(),
            on_click: None,
            animations: Animations::new(),
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn shape(mut self, shape: ButtonShape) -> Self {
        self.shape = shape;
        self
    }

    /// Visual styling only — the layout system decides the actual rect, so
    /// callers must size the node from [`ButtonSize::height`] themselves.
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Key of a font registered in the [`FontBook`]; defaults to `noto_sans`.
    pub fn font(mut self, key: impl Into<String>) -> Self {
        self.font_key = key.into();
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.set_on_click(callback);
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Replaces the click callback, dropping any previous one.
    pub fn set_on_click(&mut self, callback: impl FnMut() + 'static) {
        self.on_click = Some(Box::new(callback));
    }

    pub fn clear_on_click(&mut self) {
        self.on_click = None;
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    fn container_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            return match self.variant {
                ButtonVariant::Filled | ButtonVariant::FilledTonal | ButtonVariant::Elevated => {
                    theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
                }
                ButtonVariant::Outlined | ButtonVariant::Text => Color::default(),
            };
        }
        match self.variant {
            ButtonVariant::Filled => theme.primary,
            ButtonVariant::FilledTonal => theme.secondary_container,
            ButtonVariant::Elevated => theme.surface_container_low,
            ButtonVariant::Outlined | ButtonVariant::Text => Color::default(),
        }
    }

    fn label_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            return theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY);
        }
        match self.variant {
            ButtonVariant::Filled => theme.on_primary,
            ButtonVariant::FilledTonal => theme.on_secondary_container,
            ButtonVariant::Elevated | ButtonVariant::Outlined | ButtonVariant::Text => {
                theme.primary
            }
        }
    }

    fn outline_color(&self, theme: &ColorTheme) -> Option<Color> {
        if self.variant != ButtonVariant::Outlined {
            return None;
        }
        Some(if self.enabled {
            theme.outline
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
        })
    }

    /// The state layer tints the container with the content color.
    fn state_layer_opacity(&self) -> f32 {
        if !self.enabled {
            0.0
        } else if self.pressed {
            PRESSED_OPACITY
        } else if self.hovered {
            HOVER_OPACITY
        } else {
            0.0
        }
    }

    fn draw_shadow(&self, canvas: &Canvas, theme: &ColorTheme, rrect: &RRect) {
        // Elevation level 1 at rest, level 2 on hover.
        let (dy, sigma) = if self.hovered && !self.pressed {
            (2.0, 3.0)
        } else {
            (1.0, 1.5)
        };
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(theme.shadow.with_alpha(0.3)), None);
        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, sigma, None));
        canvas.draw_rrect(rrect.with_offset((0.0, dy)), &paint);
    }
}

impl Widget for Button {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let rect = self.layout_rect.to_skia();
        let mut redraw = false;

        let radius = self.shape.corner_radius(self.size, false)
            + (self.shape.corner_radius(self.size, true)
                - self.shape.corner_radius(self.size, false))
                * self.animations.shape.value();

        if self.animations.shape.is_traveling() {
            redraw = true;
        }

        let rrect = RRect::new_rect_xy(rect, radius, radius);

        if self.enabled && self.variant == ButtonVariant::Elevated {
            self.draw_shadow(canvas, theme, &rrect);
        }

        let container = self.container_color(theme);
        if container.a > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(container), None);
            canvas.draw_rrect(rrect, &paint);
        }

        let label_color = self.label_color(theme);

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(label_color.with_alpha(state_opacity)), None);
            canvas.draw_rrect(rrect, &paint);
        }

        if let Some(outline) = self.outline_color(theme) {
            const STROKE_WIDTH: f32 = 1.0;
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(outline), None);
            paint.set_stroke(true);
            paint.set_stroke_width(STROKE_WIDTH);
            // Inset by half the stroke so the outline stays within the bounds.
            let inset = STROKE_WIDTH / 2.0;
            let inner = rect.with_inset((inset, inset));
            canvas.draw_rrect(
                RRect::new_rect_xy(inner, radius - inset, radius - inset),
                &paint,
            );
        }

        let font = fonts.sized(&self.font_key, self.size.label_size());
        let metrics = font.metrics().1;
        let baseline = rect.center_y() - (metrics.ascent + metrics.descent) / 2.0;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(label_color), None);
        canvas.draw_str_align(
            &self.label,
            Point::new(rect.center_x(), baseline),
            &font,
            &paint,
            Align::Center,
        );

        redraw
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
            if self.pressed {
                self.animations.shape.set_target(0.0);
            }
            self.hovered = false;
            self.pressed = false;
            return dirty;
        }

        let inside = self.hit_rect().contains(event.x() as f32, event.y() as f32);

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let changed = self.hovered != inside;
                self.hovered = inside;
                changed
            }
            PointerEventKind::Leave => {
                let dirty = self.hovered || self.pressed;
                if self.pressed {
                    self.animations.shape.set_target(0.0);
                }
                self.hovered = false;
                self.pressed = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                if inside {
                    self.animations.shape.set_target(1.0);
                    self.hovered = true;
                    self.pressed = true;
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.pressed = false;
                if was_pressed {
                    self.animations.shape.set_target(0.0);
                }
                // Activates only when press and release both land inside.
                if was_pressed && inside {
                    if let Some(callback) = self.on_click.as_mut() {
                        callback();
                    }
                }
                was_pressed
            }
            _ => false,
        }
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout_rect = rect;
    }

    fn layout_rect(&self) -> LayoutRect {
        self.layout_rect
    }

    /// Height comes straight from [`ButtonSize::height`]; width is the label
    /// measured at the same font/size `draw` uses, plus horizontal padding on
    /// both sides (buttons here carry no icon/leading content to add a gap
    /// for).
    fn measure(&self, fonts: &FontBook) -> Size {
        let font = fonts.sized(&self.font_key, self.size.label_size());
        let label_width = font.measure_str(&self.label, None).0;
        let width = label_width + self.size.horizontal_padding() * 2.0;
        Size::new(width, self.size.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assigned rect must be readable before the first `draw`, since
    /// hit testing and layout queries can happen before then.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut button = Button::new("Click me");
        assert_eq!(button.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(10.0, 20.0, 100.0, 40.0);
        button.set_layout_rect(rect);
        assert_eq!(button.layout_rect(), rect);
    }

    /// `size` is a visual token, not geometry: it must never write back into
    /// the layout rect, or the widget becomes a second source of truth that
    /// silently fights whatever the layout system assigned.
    #[test]
    fn size_does_not_mutate_the_layout_rect() {
        let rect = LayoutRect::new(10.0, 20.0, 100.0, ButtonSize::ExtraLarge.height());
        let mut button = Button::new("Click me");
        button.set_layout_rect(rect);

        let button = button.size(ButtonSize::ExtraSmall);
        assert_eq!(button.layout_rect(), rect);
    }

    /// Hit testing uses the assigned layout rect, not any internally
    /// computed geometry.
    #[test]
    fn hit_testing_uses_assigned_layout_rect() {
        let mut button = Button::new("Click me").on_click(|| {});
        button.set_layout_rect(LayoutRect::new(10.0, 20.0, 100.0, 40.0));

        // Inside the assigned rect: pressing then releasing should register
        // as "was pressed".
        let was_pressed = button.on_pointer(&PointerEvent::new(
            (50.0, 40.0),
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        ));
        assert!(was_pressed);

        // Outside the assigned rect: no hit.
        let mut other = Button::new("Elsewhere");
        other.set_layout_rect(LayoutRect::new(10.0, 20.0, 100.0, 40.0));
        let hit = other.on_pointer(&PointerEvent::new(
            (500.0, 500.0),
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        ));
        assert!(!hit);
    }
}
