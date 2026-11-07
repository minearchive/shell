//! Material 3 chips: compact actions (assist) and toggles (filter).

use skia_safe::{Canvas, Color4f, Paint, Point, RRect, Rect};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::{LayoutRect, Size},
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    icon::{self, Icon},
    tokens::motion::{duration, easing},
    tokens::{
        DISABLED_CONTAINER_OPACITY, DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY,
        PRESSED_OPACITY,
    },
    Widget,
};

/// Public so callers can size layout around a chip without hardcoding it.
pub const HEIGHT: f32 = 32.0;
const CORNER_RADIUS: f32 = 8.0;
const OUTLINE_WIDTH: f32 = 1.0;
const LABEL_SIZE: f32 = 14.0;
const ICON_SIZE: f32 = 18.0;
const ICON_GAP: f32 = 8.0;
/// Horizontal padding beside a label, and beside a leading icon.
const PADDING: f32 = 16.0;
const ICON_PADDING: f32 = 8.0;

pub struct Chip {
    label: String,
    icon: Option<Icon>,
    /// A filter chip toggles `selected`; an assist chip is a plain action.
    filter: bool,
    selected: bool,
    font_key: String,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
    on_click: Option<Box<dyn FnMut()>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
    animations: Animations,
}

pub struct Animations {
    /// 0 unselected, 1 selected. Drives the container and outline fades and
    /// the check reveal.
    selection: Animation<f32>,
}

impl Animations {
    pub fn new(selected: bool) -> Self {
        let value = if selected { 1.0 } else { 0.0 };
        Self {
            selection: Animation::new(value, value, duration::SHORT4, easing::standard()),
        }
    }
}

impl Chip {
    fn new(label: String, filter: bool, selected: bool) -> Self {
        Self {
            label,
            icon: None,
            filter,
            selected,
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            layout_rect: LayoutRect::empty(),
            on_click: None,
            on_change: None,
            animations: Animations::new(selected),
        }
    }

    /// A one-off action; report it with [`Self::on_click`].
    pub fn assist(label: impl Into<String>) -> Self {
        Self::new(label.into(), false, false)
    }

    /// A toggle; report it with [`Self::on_change`].
    pub fn filter(label: impl Into<String>, selected: bool) -> Self {
        Self::new(label.into(), true, selected)
    }

    /// Leading icon. On a filter chip the check replaces it while selected.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
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
        self.on_click = Some(Box::new(callback));
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Programmatic update: animates, but does not fire `on_change`. No-op on
    /// an assist chip, which has no selected state.
    pub fn set_selected(&mut self, selected: bool) {
        if self.filter {
            self.selected = selected;
            self.animations
                .selection
                .set_target(if selected { 1.0 } else { 0.0 });
        }
    }

    pub fn selected(&self) -> bool {
        self.selected
    }

    fn activate(&mut self) {
        if self.filter {
            self.set_selected(!self.selected);
            if let Some(callback) = self.on_change.as_mut() {
                callback(self.selected);
            }
        }
        if let Some(callback) = self.on_click.as_mut() {
            callback();
        }
    }

    /// Whether `measure` holds room for a leading icon or check. A filter chip
    /// always does, so selecting it can't reflow its neighbours.
    fn reserves_icon_slot(&self) -> bool {
        self.filter || self.icon.is_some()
    }

    fn state_layer_opacity(&self) -> f32 {
        if !self.enabled {
            0.0
        } else if self.pressed {
            PRESSED_OPACITY
        } else if self.focused {
            FOCUS_OPACITY
        } else if self.hovered {
            HOVER_OPACITY
        } else {
            0.0
        }
    }

    fn label_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        if !self.enabled {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        } else if self.filter {
            theme
                .on_surface_variant
                .lerp(theme.on_secondary_container, selection)
        } else {
            theme.on_surface
        }
    }

    fn draw_leading(&self, canvas: &Canvas, theme: &ColorTheme, icon_rect: Rect, selection: f32) {
        if let Some(icon) = self.icon {
            let color = if self.enabled {
                theme.primary
            } else {
                theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
            };
            draw_icon(canvas, icon, icon_rect, color.with_alpha(1.0 - selection));
        }
        let color = if self.enabled {
            theme.on_secondary_container
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        };
        draw_icon(canvas, Icon::Check, icon_rect, color.with_alpha(selection));
    }
}

/// [`Icon`] draws at a fixed 24dp, so a chip's 18dp icon scales the canvas
/// (stroke included).
fn draw_icon(canvas: &Canvas, icon: Icon, rect: Rect, color: Color) {
    let scale = rect.width() / icon::BOX_SIZE;
    canvas.save();
    canvas.translate((rect.left, rect.top));
    canvas.scale((scale, scale));
    icon.draw(canvas, Rect::from_wh(icon::BOX_SIZE, icon::BOX_SIZE), color);
    canvas.restore();
}

impl Widget for Chip {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let rect = self.layout_rect.to_skia();
        let rrect = RRect::new_rect_xy(rect, CORNER_RADIUS, CORNER_RADIUS);
        let selection = self.animations.selection.value();

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        let (fill, outline) = if self.enabled {
            (theme.secondary_container, theme.outline_variant)
        } else {
            let disabled = theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY);
            (disabled, disabled)
        };
        paint.set_color4f(Color4f::from(fill.with_alpha(selection)), None);
        canvas.draw_rrect(rrect, &paint);

        let label_color = self.label_color(theme, selection);

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            paint.set_color4f(Color4f::from(label_color.with_alpha(state_opacity)), None);
            canvas.draw_rrect(rrect, &paint);
        }

        let mut stroke = paint.clone();
        stroke.set_stroke(true);
        stroke.set_stroke_width(OUTLINE_WIDTH);
        stroke.set_color4f(Color4f::from(outline.with_alpha(1.0 - selection)), None);
        // Inset by half the stroke so the outline stays within the bounds.
        let inset = OUTLINE_WIDTH / 2.0;
        canvas.draw_rrect(
            RRect::new_rect_xy(
                rect.with_inset((inset, inset)),
                CORNER_RADIUS - inset,
                CORNER_RADIUS - inset,
            ),
            &stroke,
        );

        // How much of the leading slot the content occupies: all of it with
        // an icon, otherwise opening as the check fades in.
        let slot = if self.icon.is_some() { 1.0 } else { selection };
        // ponytail: slides inside a fixed width rather than animating
        // `measure`; animate the width if chips ever sit in a reflowing wrap.
        let label_width = fonts.measure(&self.font_key, LABEL_SIZE, &self.label);
        let content_width = label_width + (ICON_SIZE + ICON_GAP) * slot;
        // The slot's padding is narrower than the label's, so a full slot
        // sits left of centre by half the difference.
        let left = rect.center_x() - content_width / 2.0 - (PADDING - ICON_PADDING) / 2.0 * slot;

        if slot > 0.0 {
            let icon_rect = Rect::from_xywh(
                left,
                rect.center_y() - ICON_SIZE / 2.0,
                ICON_SIZE,
                ICON_SIZE,
            );
            self.draw_leading(canvas, theme, icon_rect, selection);
        }

        let metrics = fonts.sized(&self.font_key, LABEL_SIZE).metrics().1;
        let baseline = rect.center_y() - (metrics.ascent + metrics.descent) / 2.0;
        let mut label_paint = Paint::default();
        label_paint.set_anti_alias(true);
        label_paint.set_color4f(Color4f::from(label_color), None);
        fonts.draw(
            canvas,
            &self.font_key,
            LABEL_SIZE,
            &self.label,
            Point::new(left + (ICON_SIZE + ICON_GAP) * slot, baseline),
            &label_paint,
        );

        self.animations.selection.is_traveling()
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
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
                self.hovered = false;
                self.pressed = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                self.pressed = inside;
                inside
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.pressed = false;
                // Activates only when press and release both land inside.
                if was_pressed && inside {
                    self.activate();
                }
                was_pressed
            }
            _ => false,
        }
    }

    fn on_keyboard(&mut self, event: &KeyboardEvent) -> bool {
        if !self.enabled {
            return false;
        }

        match &event.kind {
            KeyboardEventKind::Focus => {
                self.focused = true;
                true
            }
            KeyboardEventKind::Blur => {
                self.focused = false;
                true
            }
            // Space usually reaches a widget as committed text rather than as a
            // keysym, since backends route anything printable through `Commit`.
            KeyboardEventKind::Commit(text) if text == " " => {
                self.activate();
                true
            }
            KeyboardEventKind::Press { keysym, .. }
                if matches!(*keysym, keyboard::key::SPACE | keyboard::key::RETURN) =>
            {
                self.activate();
                true
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

    fn measure(&self, fonts: &FontBook) -> Size {
        let label_width = fonts.measure(&self.font_key, LABEL_SIZE, &self.label);
        let leading = if self.reserves_icon_slot() {
            ICON_PADDING + ICON_SIZE + ICON_GAP
        } else {
            PADDING
        };
        Size::new(leading + label_width + PADDING, HEIGHT)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn placed(chip: Chip) -> Chip {
        let mut chip = chip;
        chip.set_layout_rect(LayoutRect::new(0.0, 0.0, 100.0, HEIGHT));
        chip
    }

    fn press_at(chip: &mut Chip, x: f64) -> bool {
        chip.on_pointer(&PointerEvent::new(
            (x, 10.0),
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        ))
    }

    fn release_at(chip: &mut Chip, x: f64) -> bool {
        chip.on_pointer(&PointerEvent::new(
            (x, 10.0),
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ))
    }

    fn click(chip: &mut Chip) {
        press_at(chip, 10.0);
        release_at(chip, 10.0);
    }

    #[test]
    fn filter_click_toggles_and_reports() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = seen.clone();
        let mut chip =
            placed(Chip::filter("a", false).on_change(move |v| seen_clone.borrow_mut().push(v)));
        click(&mut chip);
        click(&mut chip);
        assert_eq!(*seen.borrow(), vec![true, false]);
        assert!(!chip.selected());
    }

    #[test]
    fn assist_click_fires_without_gaining_selection() {
        let clicks = Rc::new(Cell::new(0));
        let clicks_clone = clicks.clone();
        let mut chip =
            placed(Chip::assist("a").on_click(move || clicks_clone.set(clicks_clone.get() + 1)));
        click(&mut chip);
        chip.set_selected(true);
        assert_eq!(clicks.get(), 1);
        assert!(!chip.selected());
    }

    #[test]
    fn release_outside_does_not_activate() {
        let mut chip = placed(Chip::filter("a", false));
        press_at(&mut chip, 10.0);
        release_at(&mut chip, 500.0);
        assert!(!chip.selected());
    }

    #[test]
    fn set_selected_does_not_report() {
        let fired = Rc::new(Cell::new(false));
        let fired_clone = fired.clone();
        let mut chip = placed(Chip::filter("a", false).on_change(move |_| fired_clone.set(true)));
        chip.set_selected(true);
        assert!(chip.selected());
        assert!(!fired.get());
    }

    #[test]
    fn disabled_ignores_pointer_and_keyboard() {
        let mut chip = placed(Chip::filter("a", false).enabled(false));
        click(&mut chip);
        chip.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        ));
        assert!(!chip.selected());
        assert!(!chip.focusable());
    }

    #[test]
    fn space_and_return_activate() {
        let mut chip = placed(Chip::filter("a", false));
        chip.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        ));
        assert!(chip.selected());
        chip.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym: keyboard::key::RETURN,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ));
        assert!(!chip.selected());
    }

    #[test]
    fn other_commit_does_not_activate() {
        let mut chip = placed(Chip::filter("a", false));
        assert!(!chip.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit("x".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(!chip.selected());
    }
}
