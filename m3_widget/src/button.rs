//! Material 3 button: five variants ordered by emphasis, five sizes.

use skia_safe::{
    utils::text_utils::Align, BlurStyle, Canvas, Color4f, Contains, MaskFilter, Paint, Point,
    RRect, Rect,
};

use ui_core::{
    font::FontBook,
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::Widget;

/// State layer opacities.
const HOVER_OPACITY: f32 = 0.08;
const PRESSED_OPACITY: f32 = 0.10;

/// Disabled treatments.
const DISABLED_CONTAINER_OPACITY: f32 = 0.12;
const DISABLED_CONTENT_OPACITY: f32 = 0.38;

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
    pub fn corner_radius(self) -> f32 {
        self.height() / 2.0
    }
}

pub struct Button {
    label: String,
    variant: ButtonVariant,
    size: ButtonSize,
    origin: (f32, f32),
    width: Option<f32>,
    font_key: String,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// Filled in by `draw`; zero until the first frame, so hit testing only
    /// works once the button has been laid out against a real font.
    bounds: Rect,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            origin: (0.0, 0.0),
            width: None,
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            pressed: false,
            bounds: Rect::new_empty(),
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.origin = (x, y);
        self
    }

    /// Fixes the width instead of sizing to the label.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
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

    fn layout(&self, fonts: &FontBook) -> Rect {
        let font = fonts.sized(&self.font_key, self.size.label_size());
        let text_width = font.measure_str(&self.label, None).0;
        let height = self.size.height();
        let width = self
            .width
            .unwrap_or(text_width + self.size.horizontal_padding() * 2.0)
            .max(height);
        Rect::from_xywh(self.origin.0, self.origin.1, width, height)
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
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) {
        let rect = self.layout(fonts);
        self.bounds = rect;

        let radius = self.size.corner_radius();
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
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
            self.hovered = false;
            self.pressed = false;
            return dirty;
        }

        let inside = self
            .bounds
            .contains(Point::new(event.x() as f32, event.y() as f32));

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let changed = self.hovered != inside;
                self.hovered = inside;
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
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.pressed = false;
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

    fn bounds(&self) -> Rect {
        self.bounds
    }
}
