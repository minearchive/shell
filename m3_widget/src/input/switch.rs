//! Material 3 switch: a two-state toggle, optionally carrying icons.

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
    tokens::{
        DISABLED_CONTAINER_OPACITY, DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY,
        PRESSED_OPACITY,
    },
    Widget,
};

/// Track geometry. The track uses the `full` shape token, so its radius is half
/// its height. Public so callers can size layout around a switch without
/// hardcoding its footprint.
pub const TRACK_WIDTH: f32 = 52.0;
pub const TRACK_HEIGHT: f32 = 32.0;
const TRACK_OUTLINE_WIDTH: f32 = 2.0;

/// Handle diameters. The unselected handle grows when it has to hold an icon.
const HANDLE_UNSELECTED: f32 = 16.0;
const HANDLE_UNSELECTED_WITH_ICON: f32 = 24.0;
const HANDLE_SELECTED: f32 = 24.0;
const HANDLE_PRESSED: f32 = 28.0;

/// Both end positions sit this far in from their own edge of the track — 8dp of
/// margin around the 16dp unselected handle, 4dp around the 24dp selected one.
const HANDLE_INSET: f32 = 16.0;

/// The state layer is a circle centred on the handle, larger than either.
const STATE_LAYER_DIAMETER: f32 = 40.0;

const ICON_SIZE: f32 = 16.0;
const ICON_STROKE_WIDTH: f32 = 2.0;

/// Pointer target height, so the 32dp track stays comfortably hittable.
const MIN_TOUCH_HEIGHT: f32 = 48.0;

/// Which states draw an icon inside the handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitchIcons {
    /// No icons in either state.
    #[default]
    None,
    /// A checkmark when selected, nothing when unselected.
    Selected,
    /// A checkmark when selected, a cross when unselected.
    Both,
}

impl SwitchIcons {
    fn on_unselected(self) -> bool {
        self == Self::Both
    }

    fn on_selected(self) -> bool {
        self != Self::None
    }
}

pub struct Switch {
    checked: bool,
    icons: SwitchIcons,
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
    /// 0 unselected, 1 selected. Drives the handle's travel and size, the track
    /// color and outline, and the icon crossfade.
    selection: Animation<f32>,
    /// 0 at rest, 1 pressed; grows the handle to [`HANDLE_PRESSED`].
    press: Animation<f32>,
}

impl Animations {
    pub fn new(checked: bool) -> Self {
        let selected = if checked { 1.0 } else { 0.0 };
        Self {
            // Selection is the expressive move here; the press morph is a
            // micro-interaction, so it takes the standard curve.
            selection: Animation::new(selected, selected, duration::MEDIUM1, easing::emphasized()),
            press: Animation::new(0., 0., duration::SHORT4, easing::standard()),
        }
    }
}

impl Switch {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            icons: SwitchIcons::default(),
            layout_rect: LayoutRect::empty(),
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            on_change: None,
            animations: Animations::new(checked),
        }
    }

    pub fn icons(mut self, icons: SwitchIcons) -> Self {
        self.icons = icons;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Called with the new state every time the switch toggles.
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

    /// Sets the state without invoking `on_change`; the handle still animates.
    pub fn set_checked(&mut self, checked: bool) {
        if self.checked == checked {
            return;
        }
        self.checked = checked;
        self.animations
            .selection
            .set_target(if checked { 1.0 } else { 0.0 });
    }

    /// Flips the state and reports it, animating the handle across.
    fn toggle(&mut self) {
        self.checked = !self.checked;
        self.animations
            .selection
            .set_target(if self.checked { 1.0 } else { 0.0 });
        if let Some(callback) = self.on_change.as_mut() {
            callback(self.checked);
        }
    }

    /// The track rect, as a Skia rect for drawing/geometry math. Hit testing
    /// works before the first frame since it reads the assigned layout rect
    /// rather than anything computed by `draw`.
    fn track_rect(&self) -> Rect {
        self.layout_rect.to_skia()
    }

    fn handle_diameter(&self, selection: f32, press: f32) -> f32 {
        let unselected = if self.icons.on_unselected() {
            HANDLE_UNSELECTED_WITH_ICON
        } else {
            HANDLE_UNSELECTED
        };
        let resting = lerp(unselected, HANDLE_SELECTED, selection);
        lerp(resting, HANDLE_PRESSED, press)
    }

    fn handle_center(&self, selection: f32) -> Point {
        let track = self.track_rect();
        Point::new(
            lerp(
                track.left + HANDLE_INSET,
                track.right - HANDLE_INSET,
                selection,
            ),
            track.center_y(),
        )
    }

    fn track_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        if !self.enabled {
            let unselected = theme
                .surface_container_highest
                .with_alpha(DISABLED_CONTAINER_OPACITY);
            let selected = theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY);
            return unselected.lerp(selected, selection);
        }
        theme
            .surface_container_highest
            .lerp(theme.primary, selection)
    }

    /// The outline belongs to the unselected track only, so it fades out as the
    /// switch travels.
    fn track_outline_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        let color = if self.enabled {
            theme.outline
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
        };
        color.with_alpha(1.0 - selection)
    }

    fn handle_color(&self, theme: &ColorTheme, selection: f32) -> Color {
        if !self.enabled {
            // The disabled selected handle stays opaque so it reads against the
            // dimmed track.
            let unselected = theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY);
            return unselected.lerp(theme.surface, selection);
        }
        theme.outline.lerp(theme.on_primary, selection)
    }

    fn icon_color(&self, theme: &ColorTheme, selected: bool) -> Color {
        match (self.enabled, selected) {
            (true, true) => theme.on_primary_container,
            (true, false) => theme.surface_container_highest,
            (false, true) => theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY),
            (false, false) => theme
                .surface_container_highest
                .with_alpha(DISABLED_CONTENT_OPACITY),
        }
    }

    /// The state layer tints the area around the handle with the color of the
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

    /// A checkmark or a cross, drawn as strokes in a [`ICON_SIZE`] box centred
    /// on the handle.
    fn draw_icon(canvas: &Canvas, center: Point, selected: bool, color: Color) {
        if color.a <= 0.0 {
            return;
        }
        let half = ICON_SIZE / 2.0;
        // Coordinates below are in a 16x16 box with its origin at the corner.
        let at = |x: f32, y: f32| Point::new(center.x - half + x, center.y - half + y);

        let mut builder = PathBuilder::new();
        if selected {
            builder
                .move_to(at(3.0, 8.5))
                .line_to(at(6.5, 12.0))
                .line_to(at(13.0, 4.5));
        } else {
            builder.move_to(at(4.0, 4.0)).line_to(at(12.0, 12.0));
            builder.move_to(at(12.0, 4.0)).line_to(at(4.0, 12.0));
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

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

impl Widget for Switch {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, _fonts: &FontBook) -> bool {
        let selection = self.animations.selection.value();
        let press = self.animations.press.value();

        let track = self.track_rect();
        let radius = track.height() / 2.0;
        let rrect = RRect::new_rect_xy(track, radius, radius);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(self.track_color(theme, selection)), None);
        canvas.draw_rrect(rrect, &paint);

        let outline = self.track_outline_color(theme, selection);
        if outline.a > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(outline), None);
            paint.set_stroke(true);
            paint.set_stroke_width(TRACK_OUTLINE_WIDTH);
            // Inset by half the stroke so the outline stays within the track.
            let inset = TRACK_OUTLINE_WIDTH / 2.0;
            let inner = track.with_inset((inset, inset));
            canvas.draw_rrect(
                RRect::new_rect_xy(inner, radius - inset, radius - inset),
                &paint,
            );
        }

        let center = self.handle_center(selection);
        let diameter = self.handle_diameter(selection, press);

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let color = theme.on_surface.lerp(theme.primary, selection);
            // The layer is wider than the track, so it is clipped to the track's
            // outer edge instead of spilling past it.
            canvas.save();
            canvas.clip_rrect(rrect, None, true);
            fill_circle(
                canvas,
                center,
                STATE_LAYER_DIAMETER / 2.0,
                color.with_alpha(state_opacity),
            );
            canvas.restore();
        }

        fill_circle(
            canvas,
            center,
            diameter / 2.0,
            self.handle_color(theme, selection),
        );

        // The two icons crossfade through the travel; either may be absent.
        if self.icons.on_unselected() && selection < 1.0 {
            let color = self.icon_color(theme, false).with_alpha(1.0 - selection);
            Self::draw_icon(canvas, center, false, color);
        }
        if self.icons.on_selected() && selection > 0.0 {
            let color = self.icon_color(theme, true).with_alpha(selection);
            Self::draw_icon(canvas, center, true, color);
        }

        self.animations.selection.is_traveling() || self.animations.press.is_traveling()
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered || self.pressed;
            self.animations.press.set_target(0.0);
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
                    self.animations.press.set_target(0.0);
                }
                self.hovered = false;
                self.pressed = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                if inside {
                    self.animations.press.set_target(1.0);
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
                    self.animations.press.set_target(0.0);
                }
                // Toggles only when press and release both land inside.
                if was_pressed && inside {
                    self.toggle();
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

    /// Expands the track vertically to the 48px minimum touch target; the
    /// pointer target, not the paint area.
    fn hit_rect(&self) -> LayoutRect {
        let pad = ((MIN_TOUCH_HEIGHT - self.layout_rect.height) / 2.0).max(0.0);
        self.layout_rect.outset(0.0, pad)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// The track's natural footprint. Unlike checkbox/radio, this is smaller
    /// than the 48dp minimum touch target reported by [`Self::hit_rect`] —
    /// `measure` reports the visual/layout footprint, not the padded hit
    /// area.
    fn measure(&self, _fonts: &FontBook) -> Size {
        Size::new(TRACK_WIDTH, TRACK_HEIGHT)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn pointer_at(switch: &mut Switch, x: f32, y: f32, kind: PointerEventKind) -> bool {
        switch.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A switch laid out at the origin with its real track footprint, as a
    /// layout system would assign before the first draw.
    fn placed_switch(checked: bool) -> Switch {
        let mut switch = Switch::new(checked);
        switch.set_layout_rect(LayoutRect::new(0.0, 0.0, TRACK_WIDTH, TRACK_HEIGHT));
        switch
    }

    /// A press/release pair at the centre of the track.
    fn click(switch: &mut Switch) {
        let center = switch.track_rect().center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(switch, center.x, center.y, kind);
        }
    }

    fn press_key(switch: &mut Switch, keysym: u32) -> bool {
        switch.on_keyboard(&KeyboardEvent::new(
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
        let mut switch = placed_switch(false).on_change(move |v| sink.borrow_mut().push(v));

        click(&mut switch);
        assert!(switch.checked());
        click(&mut switch);
        assert!(!switch.checked());

        assert_eq!(*seen.borrow(), vec![true, false]);
    }

    /// The press must land inside too, so a drag off the track cancels.
    #[test]
    fn release_outside_does_not_toggle() {
        let mut switch = placed_switch(false);
        let center = switch.track_rect().center();

        pointer_at(
            &mut switch,
            center.x,
            center.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut switch,
            center.x + 400.0,
            center.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert!(!switch.checked());
    }

    #[test]
    fn disabled_switch_ignores_input() {
        let mut switch = placed_switch(false).enabled(false);
        click(&mut switch);
        assert!(!switch.checked());
        assert!(!press_key(&mut switch, keyboard::key::SPACE));
        assert!(!switch.checked());
    }

    #[test]
    fn space_toggles_however_it_arrives() {
        let mut switch = Switch::new(false);

        assert!(press_key(&mut switch, keyboard::key::SPACE));
        assert!(switch.checked());

        assert!(switch.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(!switch.checked());
    }

    /// `set_checked` is the programmatic path and must stay silent.
    #[test]
    fn set_checked_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut switch = Switch::new(false).on_change(move |v| sink.borrow_mut().push(v));

        switch.set_checked(true);

        assert!(switch.checked());
        assert!(seen.borrow().is_empty());
    }

    /// Both resting positions keep the handle inside the track: 8dp of margin
    /// around the small handle, 4dp around the large one.
    #[test]
    fn handle_stays_within_the_track() {
        let switch = placed_switch(false);
        let track = switch.track_rect();

        for selection in [0.0, 0.5, 1.0] {
            let center = switch.handle_center(selection);
            let radius = switch.handle_diameter(selection, 1.0) / 2.0;
            assert!(center.x - radius >= track.left);
            assert!(center.x + radius <= track.right);
        }
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut switch = Switch::new(false);
        assert_eq!(switch.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, TRACK_WIDTH, TRACK_HEIGHT);
        switch.set_layout_rect(rect);
        assert_eq!(switch.layout_rect(), rect);
    }

    /// The 32px track is shorter than the 48px minimum touch target, so
    /// `hit_rect` must pad it out vertically without changing its width.
    #[test]
    fn hit_rect_expands_track_to_minimum_touch_target() {
        let switch = placed_switch(false);
        let hit = switch.hit_rect();

        assert_eq!(hit.width, TRACK_WIDTH);
        assert_eq!(hit.height, MIN_TOUCH_HEIGHT);
        assert!(hit.contains(
            TRACK_WIDTH / 2.0,
            -((MIN_TOUCH_HEIGHT - TRACK_HEIGHT) / 2.0) + 1.0
        ));
    }

    /// A click just above the visual track, but inside the padded touch
    /// target, still toggles the switch.
    #[test]
    fn click_within_padded_touch_target_toggles() {
        let mut switch = placed_switch(false);
        let track = switch.track_rect();
        let just_above_track = track.top - 2.0;

        pointer_at(
            &mut switch,
            track.center_x(),
            just_above_track,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut switch,
            track.center_x(),
            just_above_track,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert!(switch.checked());
    }
}
