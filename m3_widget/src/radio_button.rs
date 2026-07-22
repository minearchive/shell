//! Material 3 radio button: a circular single-selection control. Grouping and
//! deselecting siblings is the caller's responsibility — this widget only
//! owns its own `selected` bool and reports when interaction turns it on.

use skia_safe::{Canvas, Point};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::LayoutRect,
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    animation::{duration, easing},
    drawing::{fill_circle, stroke_circle},
    tokens::{DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY, PRESSED_OPACITY},
    Widget,
};

/// The natural footprint of a radio button: also the 40dp minimum touch
/// target, so a layout system can just allocate a `SIZE` x `SIZE` node and
/// get a correctly hittable radio button with no extra padding logic. Public
/// so callers can size layout around a radio button without hardcoding it.
pub const SIZE: f32 = 40.0;

/// The painted ring itself, centered within the layout rect.
const RING_DIAMETER: f32 = 20.0;
const RING_STROKE_WIDTH: f32 = 2.0;

/// The inner dot painted when selected.
const DOT_DIAMETER: f32 = 10.0;
const DOT_RADIUS: f32 = DOT_DIAMETER / 2.0;

/// The state layer is a circle centred on the ring, larger than it.
const STATE_LAYER_DIAMETER: f32 = 40.0;

pub struct RadioButton {
    selected: bool,
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
    /// 0 unselected, 1 selected. Drives the inner dot's radius and the ring
    /// color lerp from `on_surface_variant` to `primary`.
    selection: Animation<f32>,
}

impl Animations {
    pub fn new(selected: bool) -> Self {
        let value = if selected { 1.0 } else { 0.0 };
        Self {
            selection: Animation::new(value, value, duration::MEDIUM1, easing::emphasized()),
        }
    }
}

impl RadioButton {
    pub fn new(selected: bool) -> Self {
        Self {
            selected,
            layout_rect: LayoutRect::empty(),
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            on_change: None,
            animations: Animations::new(selected),
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Called with `true` every time this radio button becomes selected via
    /// interaction. A radio button never reports `false` on its own —
    /// deselecting it is the caller's job via [`Self::set_selected`].
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

    pub fn selected(&self) -> bool {
        self.selected
    }

    /// Sets the selected state without invoking `on_change`; the dot still
    /// animates in or out. This is how a caller deselects the previously
    /// selected sibling in a group.
    pub fn set_selected(&mut self, selected: bool) {
        if self.selected == selected {
            return;
        }
        self.selected = selected;
        self.animations
            .selection
            .set_target(if selected { 1.0 } else { 0.0 });
    }

    /// Selects the radio button and reports it, animating the dot in. A
    /// no-op if it is already selected — a radio button does not toggle off
    /// by itself.
    fn select(&mut self) {
        if self.selected {
            return;
        }
        self.selected = true;
        self.animations.selection.set_target(1.0);
        if let Some(callback) = self.on_change.as_mut() {
            callback(true);
        }
    }

    /// The center of the ring/dot/state layer, in the layout rect. Hit
    /// testing works before the first frame since it reads the assigned
    /// layout rect rather than anything computed by `draw`.
    fn center(&self) -> Point {
        let layout = self.layout_rect.to_skia();
        Point::new(layout.center_x(), layout.center_y())
    }

    fn ring_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        if !self.enabled {
            return theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY);
        }
        theme.on_surface_variant.lerp(theme.primary, selection)
    }

    fn dot_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.primary
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        }
    }

    /// The state layer tints the area around the ring with the color of the
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

    fn state_layer_tint(&self, theme: &ColorTheme) -> Color {
        if self.selected {
            theme.primary
        } else {
            theme.on_surface
        }
    }
}

impl Widget for RadioButton {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, _fonts: &FontBook) -> bool {
        let selection = self.animations.selection.value();
        let center = self.center();

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let tint = self.state_layer_tint(theme);
            fill_circle(
                canvas,
                center,
                STATE_LAYER_DIAMETER / 2.0,
                tint.with_alpha(state_opacity),
            );
        }

        // Inset by half the stroke so the ring stays within its 20dp
        // footprint, matching the outline treatment in checkbox/switch.
        let ring_radius = RING_DIAMETER / 2.0 - RING_STROKE_WIDTH / 2.0;
        stroke_circle(
            canvas,
            center,
            ring_radius,
            RING_STROKE_WIDTH,
            self.ring_color(theme, selection),
        );

        if selection > 0.0 {
            fill_circle(
                canvas,
                center,
                DOT_RADIUS * selection,
                self.dot_color(theme),
            );
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
                // Selects only when press and release both land inside, and
                // only if not already selected.
                if was_pressed && inside {
                    self.select();
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
            KeyboardEventKind::Commit(text) => {
                if text == " " {
                    self.select();
                    true
                } else {
                    false
                }
            }
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::SPACE | keyboard::key::RETURN => {
                    self.select();
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

    fn pointer_at(radio: &mut RadioButton, x: f32, y: f32, kind: PointerEventKind) -> bool {
        radio.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A radio button laid out at the origin with its real ring footprint, as
    /// a layout system would assign before the first draw.
    fn placed_radio(selected: bool) -> RadioButton {
        let mut radio = RadioButton::new(selected);
        radio.set_layout_rect(LayoutRect::new(0.0, 0.0, RING_DIAMETER, RING_DIAMETER));
        radio
    }

    /// A press/release pair at the centre of the ring.
    fn click(radio: &mut RadioButton) {
        let center = radio.center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(radio, center.x, center.y, kind);
        }
    }

    fn press_key(radio: &mut RadioButton, keysym: u32) -> bool {
        radio.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    #[test]
    fn click_selects_and_reports_once() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut radio = placed_radio(false).on_change(move |v| sink.borrow_mut().push(v));

        click(&mut radio);
        assert!(radio.selected());
        assert_eq!(*seen.borrow(), vec![true]);
    }

    /// Clicking an already-selected radio button does nothing: no state
    /// change, no repeated callback.
    #[test]
    fn click_on_already_selected_does_not_report_again() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut radio = placed_radio(true).on_change(move |v| sink.borrow_mut().push(v));

        click(&mut radio);
        assert!(radio.selected());
        assert!(seen.borrow().is_empty());
    }

    /// The press must land inside too, so a drag off the ring cancels.
    #[test]
    fn release_outside_does_not_select() {
        let mut radio = placed_radio(false);
        let center = radio.center();

        pointer_at(
            &mut radio,
            center.x,
            center.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut radio,
            center.x + 400.0,
            center.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert!(!radio.selected());
    }

    #[test]
    fn disabled_radio_ignores_input() {
        let mut radio = placed_radio(false).enabled(false);
        click(&mut radio);
        assert!(!radio.selected());
        assert!(!press_key(&mut radio, keyboard::key::SPACE));
        assert!(!radio.selected());
    }

    #[test]
    fn space_selects_however_it_arrives() {
        let mut radio = RadioButton::new(false);
        assert!(press_key(&mut radio, keyboard::key::SPACE));
        assert!(radio.selected());

        // Already selected: Commit(" ") must not fire another report and
        // must leave it selected (radio buttons never self-deselect).
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        radio.set_on_change(move |v| sink.borrow_mut().push(v));
        assert!(radio.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(radio.selected());
        assert!(seen.borrow().is_empty());
    }

    /// A fresh, unselected radio button selecting via `Commit(" ")`.
    #[test]
    fn commit_space_selects_when_unselected() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut radio = RadioButton::new(false).on_change(move |v| sink.borrow_mut().push(v));

        assert!(radio.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(radio.selected());
        assert_eq!(*seen.borrow(), vec![true]);
    }

    /// `set_selected` is the programmatic path and must stay silent.
    #[test]
    fn set_selected_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut radio = RadioButton::new(false).on_change(move |v| sink.borrow_mut().push(v));

        radio.set_selected(true);

        assert!(radio.selected());
        assert!(seen.borrow().is_empty());
    }

    /// `set_selected` is also how a caller deselects the previous sibling in
    /// a group; it must be silent in that direction too.
    #[test]
    fn set_selected_false_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut radio = RadioButton::new(true).on_change(move |v| sink.borrow_mut().push(v));

        radio.set_selected(false);

        assert!(!radio.selected());
        assert!(seen.borrow().is_empty());
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut radio = RadioButton::new(false);
        assert_eq!(radio.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, RING_DIAMETER, RING_DIAMETER);
        radio.set_layout_rect(rect);
        assert_eq!(radio.layout_rect(), rect);
    }

    /// The 20px ring is smaller than the 40px minimum touch target, so
    /// `hit_rect` must pad it out in both dimensions.
    #[test]
    fn hit_rect_expands_ring_to_minimum_touch_target() {
        let radio = placed_radio(false);
        let hit = radio.hit_rect();

        assert_eq!(hit.width, SIZE);
        assert_eq!(hit.height, SIZE);
    }
}
