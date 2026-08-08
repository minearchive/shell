//! Material 3 checkbox: a small square selection control, with an optional
//! third "indeterminate" state.

use skia_safe::{Canvas, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, RRect, Rect};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::{LayoutRect, Size},
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    drawing::fill_circle,
    tokens::motion::{duration, easing},
    tokens::{DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY, PRESSED_OPACITY},
    Widget,
};

/// The natural footprint of a checkbox: also the 40dp minimum touch target,
/// so a layout system can just allocate a `SIZE` x `SIZE` node and get a
/// correctly hittable checkbox with no extra padding logic. Public so
/// callers can size layout around a checkbox without hardcoding it.
pub const SIZE: f32 = 40.0;

/// The painted box itself, centered within the layout rect.
const BOX_SIZE: f32 = 18.0;
const BOX_RADIUS: f32 = 2.0;
const BOX_OUTLINE_WIDTH: f32 = 2.0;

/// The state layer is a circle centred on the box, larger than it.
const STATE_LAYER_DIAMETER: f32 = 40.0;

const ICON_SIZE: f32 = 14.0;
const ICON_STROKE_WIDTH: f32 = 2.0;

pub struct CheckBox {
    checked: bool,
    /// Indeterminate takes visual precedence over `checked`: the box paints
    /// filled with a dash instead of a checkmark, regardless of `checked`.
    indeterminate: bool,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
    on_change: Option<Box<dyn FnMut(bool)>>,
    animations: Animations,
}

pub struct Animations {
    /// 0 unselected, 1 selected (checked or indeterminate). Drives the box
    /// fill fade, the outline fade, and the checkmark/dash reveal.
    selection: Animation<f32>,
}

impl Animations {
    pub fn new(selected: bool) -> Self {
        let value = if selected { 1.0 } else { 0.0 };
        Self {
            // Unlike the switch's handle travel, this is a small alpha/color
            // fade over an 18dp box — a toggle micro-interaction, not a
            // large expressive transition, so it takes the standard curve
            // at a short duration (matching the button family's press morph).
            selection: Animation::new(value, value, duration::SHORT4, easing::standard()),
        }
    }
}

impl CheckBox {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            indeterminate: false,
            layout_rect: LayoutRect::empty(),
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            on_change: None,
            animations: Animations::new(checked),
        }
    }

    /// Sets the indeterminate flag. This is a construction-time builder, so
    /// it snaps the selection animation to its resting value rather than
    /// animating — the animated path is `toggle`/`set_checked` after the
    /// checkbox is already live.
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        let value = if self.is_selected() { 1.0 } else { 0.0 };
        self.animations.selection.reset(value, value);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Called with the new checked state every time the checkbox toggles.
    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.set_on_change(callback);
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Replaces the change callback, dropping any previous one.
    pub fn set_on_change(&mut self, callback: impl FnMut(bool) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    pub fn indeterminate_value(&self) -> bool {
        self.indeterminate
    }

    /// Sets the checked state without invoking `on_change`; the box still
    /// animates. Also clears `indeterminate`, since an explicit checked
    /// state is unambiguous.
    pub fn set_checked(&mut self, checked: bool) {
        if self.checked == checked {
            return;
        }
        self.checked = checked;
        self.indeterminate = false;
        self.animations
            .selection
            .set_target(if checked { 1.0 } else { 0.0 });
    }

    fn is_selected(&self) -> bool {
        self.checked || self.indeterminate
    }

    /// Flips the checked state and reports it, clearing indeterminate and
    /// animating the box across.
    fn toggle(&mut self) {
        self.checked = !self.checked;
        self.indeterminate = false;
        self.animations
            .selection
            .set_target(if self.checked { 1.0 } else { 0.0 });
        if let Some(callback) = self.on_change.as_mut() {
            callback(self.checked);
        }
    }

    /// The box rect, an 18dp square centered within the layout rect. Hit
    /// testing works before the first frame since it reads the assigned
    /// layout rect rather than anything computed by `draw`.
    fn box_rect(&self) -> Rect {
        let layout = self.layout_rect.to_skia();
        let half = BOX_SIZE / 2.0;
        Rect::from_xywh(
            layout.center_x() - half,
            layout.center_y() - half,
            BOX_SIZE,
            BOX_SIZE,
        )
    }

    fn box_fill_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        let target = if self.enabled {
            theme.primary
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        };
        target.with_alpha(selection)
    }

    /// The outline belongs to the unselected box only, so it fades out as
    /// the box fills in.
    fn box_outline_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        let color = if self.enabled {
            theme.on_surface_variant
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        };
        color.with_alpha(1.0 - selection)
    }

    fn icon_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.on_primary
        } else {
            theme.surface
        }
    }

    /// The state layer tints the area around the box with the color of the
    /// state it is in. Focus reuses it rather than drawing a separate ring.
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

    fn state_layer_tint(&self, theme: &ColorTheme, selection: f32) -> Color {
        theme.on_surface.lerp(theme.primary, selection)
    }

    /// A checkmark or, for the indeterminate state, a horizontal dash;
    /// drawn as a stroke in an [`ICON_SIZE`] box centred on the checkbox.
    fn draw_icon(canvas: &Canvas, center: Point, indeterminate: bool, color: Color) {
        if color.a <= 0.0 {
            return;
        }
        let half = ICON_SIZE / 2.0;
        // Coordinates below are in a 14x14 box with its origin at the corner.
        let at = |x: f32, y: f32| Point::new(center.x - half + x, center.y - half + y);

        let mut builder = PathBuilder::new();
        if indeterminate {
            builder.move_to(at(3.0, 7.0)).line_to(at(11.0, 7.0));
        } else {
            builder
                .move_to(at(3.0, 7.5))
                .line_to(at(6.0, 10.5))
                .line_to(at(11.5, 4.0));
        }

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(color), None);
        paint.set_stroke(true);
        paint.set_stroke_width(ICON_STROKE_WIDTH);
        paint.set_stroke_cap(PaintCap::Round);
        paint.set_stroke_join(PaintJoin::Round);
        canvas.draw_path(&builder.detach(), &paint);
    }
}

impl Widget for CheckBox {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, _fonts: &FontBook) -> bool {
        let selection = self.animations.selection.value();

        let rect = self.box_rect();
        let center = Point::new(rect.center_x(), rect.center_y());
        let rrect = RRect::new_rect_xy(rect, BOX_RADIUS, BOX_RADIUS);

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let tint = self.state_layer_tint(theme, selection);
            fill_circle(
                canvas,
                center,
                STATE_LAYER_DIAMETER / 2.0,
                tint.with_alpha(state_opacity),
            );
        }

        let fill = self.box_fill_color(theme, selection);
        if fill.a > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(fill), None);
            canvas.draw_rrect(rrect, &paint);
        }

        let outline = self.box_outline_color(theme, selection);
        if outline.a > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(outline), None);
            paint.set_stroke(true);
            paint.set_stroke_width(BOX_OUTLINE_WIDTH);
            // Inset by half the stroke so the outline stays within the box.
            let inset = BOX_OUTLINE_WIDTH / 2.0;
            let inner = rect.with_inset((inset, inset));
            let radius = (BOX_RADIUS - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(inner, radius, radius), &paint);
        }

        if selection > 0.0 {
            let color = self.icon_color(theme).with_alpha(selection);
            Self::draw_icon(canvas, center, self.indeterminate, color);
        }

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
                // Toggles only when press and release both land inside.
                if was_pressed && inside {
                    self.toggle();
                }
                was_pressed
            }
            _ => false,
        }
    }

    /// The 40dp minimum touch target, matching [`Self::hit_rect`] — the whole
    /// footprint a layout system should allocate for a checkbox.
    fn measure(&self, _fonts: &FontBook) -> Size {
        Size::new(SIZE, SIZE)
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
            KeyboardEventKind::Commit(text) => {
                if text == " " {
                    self.toggle();
                    true
                } else {
                    false
                }
            }
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::SPACE | keyboard::key::RETURN => {
                    self.toggle();
                    true
                }
                _ => false,
            },
            KeyboardEventKind::Release { .. } | KeyboardEventKind::Preedit { .. } => false,
        }
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout_rect = rect;
    }

    fn layout_rect(&self) -> LayoutRect {
        self.layout_rect
    }

    /// Expands the layout rect to the 40dp minimum touch target in both
    /// dimensions; the pointer target, not the paint area.
    fn hit_rect(&self) -> LayoutRect {
        let pad_x = ((SIZE - self.layout_rect.width) / 2.0).max(0.0);
        let pad_y = ((SIZE - self.layout_rect.height) / 2.0).max(0.0);
        self.layout_rect.outset(pad_x, pad_y)
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
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn pointer_at(checkbox: &mut CheckBox, x: f32, y: f32, kind: PointerEventKind) -> bool {
        checkbox.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A checkbox laid out at the origin with its real box footprint, as a
    /// layout system would assign before the first draw.
    fn placed_checkbox(checked: bool) -> CheckBox {
        let mut checkbox = CheckBox::new(checked);
        checkbox.set_layout_rect(LayoutRect::new(0.0, 0.0, BOX_SIZE, BOX_SIZE));
        checkbox
    }

    /// A press/release pair at the centre of the box.
    fn click(checkbox: &mut CheckBox) {
        let center = checkbox.box_rect().center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(checkbox, center.x, center.y, kind);
        }
    }

    fn press_key(checkbox: &mut CheckBox, keysym: u32) -> bool {
        checkbox.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    #[test]
    fn click_toggles_and_reports() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut checkbox = placed_checkbox(false).on_change(move |v| sink.borrow_mut().push(v));

        click(&mut checkbox);
        assert!(checkbox.checked());
        click(&mut checkbox);
        assert!(!checkbox.checked());

        assert_eq!(*seen.borrow(), vec![true, false]);
    }

    /// The press must land inside too, so a drag off the box cancels.
    #[test]
    fn release_outside_does_not_toggle() {
        let mut checkbox = placed_checkbox(false);
        let center = checkbox.box_rect().center();

        pointer_at(
            &mut checkbox,
            center.x,
            center.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut checkbox,
            center.x + 400.0,
            center.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert!(!checkbox.checked());
    }

    #[test]
    fn disabled_checkbox_ignores_input() {
        let mut checkbox = placed_checkbox(false).enabled(false);
        click(&mut checkbox);
        assert!(!checkbox.checked());
        assert!(!press_key(&mut checkbox, keyboard::key::SPACE));
        assert!(!checkbox.checked());
    }

    #[test]
    fn space_toggles_however_it_arrives() {
        let mut checkbox = CheckBox::new(false);

        assert!(press_key(&mut checkbox, keyboard::key::SPACE));
        assert!(checkbox.checked());

        assert!(checkbox.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(!checkbox.checked());
    }

    /// `set_checked` is the programmatic path and must stay silent.
    #[test]
    fn set_checked_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut checkbox = CheckBox::new(false).on_change(move |v| sink.borrow_mut().push(v));

        checkbox.set_checked(true);

        assert!(checkbox.checked());
        assert!(seen.borrow().is_empty());
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut checkbox = CheckBox::new(false);
        assert_eq!(checkbox.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, BOX_SIZE, BOX_SIZE);
        checkbox.set_layout_rect(rect);
        assert_eq!(checkbox.layout_rect(), rect);
    }

    /// The 18px box is smaller than the 40px minimum touch target, so
    /// `hit_rect` must pad it out in both dimensions.
    #[test]
    fn hit_rect_expands_box_to_minimum_touch_target() {
        let checkbox = placed_checkbox(false);
        let hit = checkbox.hit_rect();

        assert_eq!(hit.width, SIZE);
        assert_eq!(hit.height, SIZE);
    }

    /// Toggling clears indeterminate.
    #[test]
    fn toggle_clears_indeterminate() {
        let mut checkbox = placed_checkbox(false).indeterminate(true);
        assert!(checkbox.indeterminate_value());

        click(&mut checkbox);
        assert!(!checkbox.indeterminate_value());
        assert!(checkbox.checked());
    }
}
