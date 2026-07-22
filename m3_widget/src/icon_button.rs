//! Material 3 icon button: a square, icon-only variant of [`crate::button::Button`].
//!
//! Structurally this mirrors `button.rs` (variants, container/icon color
//! logic, `on_pointer` state machine, `on_click`) but adds focus handling
//! (like `switch.rs`/`checkbox.rs`) and an optional toggle mode, since icon
//! buttons are commonly used as compact selection controls (e.g. a bookmark
//! or favorite toggle) as well as plain action buttons.

use skia_safe::{Canvas, Color4f, Paint, PaintCap, PaintJoin, PathBuilder, Point, RRect, Rect};

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
    tokens::{
        DISABLED_CONTAINER_OPACITY, DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY,
        PRESSED_OPACITY,
    },
    Widget,
};

/// The default 40dp square footprint (the `full` shape token, so the
/// painted container is a circle inscribed in it) and simultaneously the
/// minimum touch target, so a layout system can just allocate a `SIZE` x
/// `SIZE` node and get a correctly hittable button with no extra padding
/// logic. Public so callers can size layout around an icon button without
/// hardcoding it.
pub const SIZE: f32 = 40.0;

/// Icons are drawn centered in a 24dp box within the container, per MD3.
const ICON_BOX_SIZE: f32 = 24.0;
const ICON_STROKE_WIDTH: f32 = 2.0;

/// The pressed-state container corner radius, per MD3's press shape-morph
/// (round -> squarer). `IconButton`'s default `SIZE` (40dp) matches
/// `button.rs`'s `ButtonSize::Small::height()`, whose pressed corner token
/// (`ButtonSize::Small.corner_radius(pressed: true)`) is `8.0` — reused here
/// so the icon button morphs by the same amount as a same-sized button.
const PRESSED_CORNER_RADIUS: f32 = 8.0;

/// Emphasis order roughly follows `ButtonVariant`, minus `Elevated`/`Text`
/// (icon buttons have no elevated tier, and "no container, primary-colored
/// icon" isn't a distinct MD3 icon-button variant the way `Text` is for
/// `Button`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    /// No container fill; lowest emphasis.
    #[default]
    Standard,
    /// Filled container, highest emphasis.
    Filled,
    /// Tonal container, medium emphasis.
    FilledTonal,
    /// No fill, 1dp outline.
    Outlined,
}

/// A small, built-in vector icon set, drawn as strokes/fills in a 24x24 box
/// via [`skia_safe::PathBuilder`] — the same technique `switch.rs` uses for
/// its checkmark/cross. Callers needing something outside this set should
/// use [`IconButton::icon_fn`] instead.
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
    /// Renders the icon centered in `box_rect` (nominally [`ICON_BOX_SIZE`]
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

pub struct IconButton {
    variant: IconButtonVariant,
    icon: Option<Icon>,
    /// Overrides `icon` when present — set via [`Self::icon_fn`] for glyphs
    /// outside the built-in [`Icon`] set.
    icon_fn: Option<Box<dyn Fn(&Canvas, Rect, Color)>>,
    enabled: bool,
    /// Whether a click flips `selected`/fires `on_change` (true) or just
    /// fires `on_click` (false).
    toggle: bool,
    selected: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then, so hit testing only works once the button has been laid
    /// out.
    layout_rect: LayoutRect,
    on_click: Option<Box<dyn FnMut()>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
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

impl IconButton {
    /// Starts a `Standard`, non-toggle icon button with no icon set. Callers
    /// must follow up with [`Self::icon`] or [`Self::icon_fn`] — an icon
    /// button with neither draws its container/state layer but no glyph.
    pub fn new() -> Self {
        Self {
            variant: IconButtonVariant::default(),
            icon: None,
            icon_fn: None,
            enabled: true,
            toggle: false,
            selected: false,
            hovered: false,
            pressed: false,
            focused: false,
            layout_rect: LayoutRect::empty(),
            on_click: None,
            on_change: None,
            animations: Animations::new(),
        }
    }

    pub fn variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets a built-in icon. Overridden by [`Self::icon_fn`] if both are set.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Sets a custom icon-drawing closure, taking precedence over any
    /// built-in [`Icon`]. Called with the 24dp icon box (centered in the
    /// container) and the resolved icon color for the current state.
    pub fn icon_fn(mut self, f: impl Fn(&Canvas, Rect, Color) + 'static) -> Self {
        self.icon_fn = Some(Box::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Enables toggle behavior: a click flips [`Self::is_selected`] and
    /// fires `on_change` instead of `on_click`.
    pub fn toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
    }

    /// Sets the initial selected state at construction time.
    ///
    /// Named distinctly from the [`Self::is_selected`] getter: Rust doesn't
    /// allow two inherent methods sharing a name, so this builder-style
    /// setter and the query getter can't both be called `selected`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.set_on_click(callback);
        self
    }

    /// Called with the new selected state every time a toggle button is
    /// clicked (or activated via keyboard).
    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.set_on_change(callback);
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

    /// Replaces the change callback, dropping any previous one.
    pub fn set_on_change(&mut self, callback: impl FnMut(bool) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    /// Sets the selected state without invoking `on_change` — the
    /// programmatic path, as opposed to a user click/keypress.
    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    /// Flips `selected` and fires `on_change` (toggle mode), or fires
    /// `on_click` (plain mode). The single activation path shared by
    /// pointer release and Space/Return.
    fn activate(&mut self) {
        if self.toggle {
            self.selected = !self.selected;
            if let Some(callback) = self.on_change.as_mut() {
                callback(self.selected);
            }
        } else if let Some(callback) = self.on_click.as_mut() {
            callback();
        }
    }

    fn container_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            return match self.variant {
                IconButtonVariant::Filled | IconButtonVariant::FilledTonal => {
                    theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
                }
                IconButtonVariant::Standard | IconButtonVariant::Outlined => Color::default(),
            };
        }
        match self.variant {
            IconButtonVariant::Standard => Color::default(),
            // Toggle mode flips emphasis: the unselected resting state sits
            // on a neutral container so the selected state visibly reads as
            // "on". A non-toggle Filled button always uses the full-emphasis
            // primary container.
            IconButtonVariant::Filled => {
                if self.toggle && !self.selected {
                    theme.surface_container_highest
                } else {
                    theme.primary
                }
            }
            IconButtonVariant::FilledTonal => theme.secondary_container,
            IconButtonVariant::Outlined => Color::default(),
        }
    }

    fn icon_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            return theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY);
        }
        match self.variant {
            // Standard has no container to flip, so toggle emphasis moves to
            // the icon color instead: selected reads as primary-tinted,
            // unselected as the neutral on_surface_variant.
            IconButtonVariant::Standard => {
                if self.toggle && self.selected {
                    theme.primary
                } else {
                    theme.on_surface_variant
                }
            }
            IconButtonVariant::Filled => {
                if self.toggle && !self.selected {
                    theme.primary
                } else {
                    theme.on_primary
                }
            }
            IconButtonVariant::FilledTonal => theme.on_secondary_container,
            IconButtonVariant::Outlined => theme.on_surface_variant,
        }
    }

    fn outline_color(&self, theme: &ColorTheme) -> Option<Color> {
        if self.variant != IconButtonVariant::Outlined {
            return None;
        }
        Some(if self.enabled {
            theme.outline
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
        })
    }

    /// The state layer tints the container with the icon color. Focus reuses
    /// it rather than drawing a separate ring, as in `switch.rs`/`checkbox.rs`.
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
}

impl Default for IconButton {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for IconButton {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, _fonts: &FontBook) -> bool {
        let rect = self.layout_rect.to_skia();
        // The smaller dimension, so a non-square layout rect still yields a
        // circle inscribed within it rather than an ellipse.
        let resting_radius = rect.width().min(rect.height()) / 2.0;
        // MD3 Expressive press shape-morph: interpolate from the resting
        // circle toward the squarer `PRESSED_CORNER_RADIUS`, mirroring
        // `button.rs`'s `Widget::draw`.
        let radius = resting_radius
            + (PRESSED_CORNER_RADIUS - resting_radius) * self.animations.shape.value();

        let mut redraw = false;
        if self.animations.shape.is_traveling() {
            redraw = true;
        }

        let rrect = RRect::new_rect_xy(rect, radius, radius);

        let container = self.container_color(theme);
        if container.a > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(container), None);
            canvas.draw_rrect(rrect, &paint);
        }

        let icon_color = self.icon_color(theme);

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(icon_color.with_alpha(state_opacity)), None);
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

        let half = ICON_BOX_SIZE / 2.0;
        let icon_box = Rect::from_xywh(
            rect.center_x() - half,
            rect.center_y() - half,
            ICON_BOX_SIZE,
            ICON_BOX_SIZE,
        );

        if let Some(icon_fn) = self.icon_fn.as_ref() {
            icon_fn(canvas, icon_box, icon_color);
        } else if let Some(icon) = self.icon {
            icon.draw(canvas, icon_box, icon_color);
        }

        // The state layer and container colors switch instantly on state
        // change (per house convention, only the press shape-morph
        // animates), so `redraw` here reflects only the shape animation.
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
            KeyboardEventKind::Commit(text) => {
                if text == " " {
                    self.activate();
                    true
                } else {
                    false
                }
            }
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::SPACE | keyboard::key::RETURN => {
                    self.activate();
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

    // `hit_rect` uses the trait default (== `layout_rect`): the 40dp
    // footprint is already the minimum touch target, unlike e.g. Switch's
    // 32dp track.

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

    fn pointer_at(button: &mut IconButton, x: f32, y: f32, kind: PointerEventKind) -> bool {
        button.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// An icon button laid out at the origin with its real footprint, as a
    /// layout system would assign before the first draw.
    fn placed_button() -> IconButton {
        let mut button = IconButton::new().icon(Icon::Check);
        button.set_layout_rect(LayoutRect::new(0.0, 0.0, SIZE, SIZE));
        button
    }

    /// A press/release pair at the centre of the button.
    fn click(button: &mut IconButton) {
        let center = button.layout_rect().to_skia().center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(button, center.x, center.y, kind);
        }
    }

    fn press_key(button: &mut IconButton, keysym: u32) -> bool {
        button.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    #[test]
    fn click_fires_on_click_when_not_toggling() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut button = placed_button().on_click(move || *sink.borrow_mut() += 1);

        click(&mut button);

        assert_eq!(*seen.borrow(), 1);
        assert!(!button.is_selected());
    }

    #[test]
    fn toggle_click_flips_selected_and_reports() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut button = placed_button()
            .toggle(true)
            .on_change(move |v| sink.borrow_mut().push(v));

        click(&mut button);
        assert!(button.is_selected());
        click(&mut button);
        assert!(!button.is_selected());

        assert_eq!(*seen.borrow(), vec![true, false]);
    }

    /// The press must land inside too, so a drag off the button cancels.
    #[test]
    fn release_outside_does_not_activate() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut button = placed_button().on_click(move || *sink.borrow_mut() += 1);
        let center = button.layout_rect().to_skia().center();

        pointer_at(
            &mut button,
            center.x,
            center.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut button,
            center.x + 400.0,
            center.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert_eq!(*seen.borrow(), 0);
    }

    #[test]
    fn disabled_button_ignores_input() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut button = placed_button()
            .enabled(false)
            .on_click(move || *sink.borrow_mut() += 1);

        click(&mut button);
        assert_eq!(*seen.borrow(), 0);
        assert!(!press_key(&mut button, keyboard::key::SPACE));
        assert_eq!(*seen.borrow(), 0);
    }

    #[test]
    fn space_activates_however_it_arrives() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut button = IconButton::new()
            .icon(Icon::Check)
            .on_click(move || *sink.borrow_mut() += 1);

        assert!(press_key(&mut button, keyboard::key::SPACE));
        assert_eq!(*seen.borrow(), 1);

        assert!(button.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert_eq!(*seen.borrow(), 2);
    }

    /// `set_selected` is the programmatic path and must stay silent.
    #[test]
    fn set_selected_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut button = IconButton::new()
            .icon(Icon::Check)
            .toggle(true)
            .on_change(move |v| sink.borrow_mut().push(v));

        button.set_selected(true);

        assert!(button.is_selected());
        assert!(seen.borrow().is_empty());
    }

    /// The assigned rect must be readable before the first `draw`, since
    /// hit testing and layout queries can happen before then.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut button = IconButton::new().icon(Icon::Check);
        assert_eq!(button.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(10.0, 20.0, SIZE, SIZE);
        button.set_layout_rect(rect);
        assert_eq!(button.layout_rect(), rect);
    }
}
