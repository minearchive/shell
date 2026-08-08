//! Material 3 navigation rail: a fixed-width vertical destination bar for
//! medium/expanded window classes.
//!
//! Structurally this follows [`crate::buttons::segmented::SegmentedButton`]:
//! one [`Widget`] owning every destination rather than a `Row`/`Column` of
//! children, because the group shares one selection invariant (exactly one
//! destination selected) and one sliding active-indicator animation that
//! needs every destination's geometry at once. The optional leading menu
//! button is plain drawn content pinned to the top of the rail, not a widget
//! of its own — clicking it toggles the rail's own expanded/collapsed state
//! (see [`NavigationRail::set_expanded`]) and fires
//! [`NavigationRail::on_menu_click`] so the caller can react too.

use skia_safe::{Canvas, Color4f, Contains, Paint, Point, RRect, Rect};

use ui_core::{
    animation::animation::Animation,
    font::FontBook,
    geometry::{LayoutRect, Size},
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    icon::{Icon, BOX_SIZE as ICON_BOX_SIZE},
    tokens::motion::{duration, easing},
    tokens::{DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY, PRESSED_OPACITY},
    Widget,
};

/// The MD3 "standard" rail width. Public so callers can size layout around a
/// rail without hardcoding it.
pub const WIDTH: f32 = 80.0;

/// The MD3 expanded (wide) rail's `ContainerWidthMinimum`. `ContainerWidthMaximum`
/// (360dp) and the `Center`-arrangement/max-width-negotiation behavior it
/// implies are out of scope — this rail only ever opens to the minimum.
pub const EXPANDED_WIDTH: f32 = 220.0;

/// The active-indicator pill behind a selected (or hovered/pressed) icon.
const INDICATOR_WIDTH: f32 = 56.0;
const INDICATOR_HEIGHT: f32 = 32.0;
const INDICATOR_RADIUS: f32 = 16.0;
/// Top inset of the pill within a labeled item. An unlabeled item instead
/// centers the pill vertically — see [`NavigationRail::indicator_pill_rect`].
const INDICATOR_TOP_PADDING: f32 = 8.0;

/// Label Medium-ish.
const LABEL_SIZE: f32 = 12.0;
/// Gap between the indicator pill's bottom edge and the label.
const LABEL_GAP: f32 = 4.0;

/// Expanded (horizontal) item layout tokens —
/// `NavigationRailExpandedTokens`/`NavigationRailHorizontalItemTokens`. Icon
/// size and the indicator's corner shape (Full) match the collapsed rail;
/// what differs is the indicator spanning the item's full width instead of a
/// fixed 56dp pill.
///
/// Equal to `ActiveIndicatorHeight`: the horizontal item layout has no extra
/// vertical padding beyond the indicator itself, so the item's height is the
/// indicator's height.
const EXPANDED_ITEM_HEIGHT: f32 = 56.0;
const EXPANDED_INDICATOR_RADIUS: f32 = EXPANDED_ITEM_HEIGHT / 2.0;
/// Inset from the item's edges to the indicator (`LeadingSpace`/
/// `FullWidthTrailingSpace`), reused for the indicator's-left-edge-to-icon
/// gap too — the token spec uses the same 16dp value for both.
const EXPANDED_LEADING_SPACE: f32 = 16.0;
/// `IconLabelSpace`.
const EXPANDED_ICON_LABEL_SPACE: f32 = 8.0;
/// Label Large.
const EXPANDED_LABEL_SIZE: f32 = 14.0;
/// `ContainerVerticalSpace` — gap between items, expanded only. The
/// collapsed layout has no equivalent token; its items stack directly.
const EXPANDED_ITEM_GAP: f32 = 6.0;

/// Per-destination footprint: MD3 uses a shorter item when there's no label
/// and a taller one to fit the label line beneath the indicator.
const ITEM_HEIGHT_NO_LABEL: f32 = 56.0;
const ITEM_HEIGHT_WITH_LABEL: f32 = 72.0;

/// Touch target of the optional leading menu button, and the gap between it
/// and the destination group below.
const MENU_BUTTON_SIZE: f32 = 56.0;
const LEADING_GAP: f32 = 8.0;

const TOP_PADDING: f32 = 8.0;
const BOTTOM_PADDING: f32 = 8.0;

/// Linear interpolation, the one primitive every continuous-by-`t` layout
/// helper below builds on instead of the old collapsed/expanded boolean
/// snap.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Vertical placement of the destination group within the rail. The menu
/// button, when present, always stays pinned to the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavigationRailAlignment {
    #[default]
    Top,
    Bottom,
}

/// One destination in a [`NavigationRail`]. Carries only configuration — the
/// rail owns selection/hover/press/focus state for every destination, since a
/// single-select group can't let its members decide their own selection
/// independently of their siblings (same reasoning as `Segment`).
pub struct NavigationRailItem {
    icon: Icon,
    label: Option<String>,
    enabled: bool,
}

impl NavigationRailItem {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            label: None,
            enabled: true,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// What the pointer is over: a destination, or the menu button. Kept as one
/// value so `hovered`/`pressed` stay single fields instead of a bool per
/// region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Item(usize),
    Menu,
}

/// A vertical Material 3 navigation rail: a fixed-width column of
/// destinations, with an optional leading menu button. See the module docs
/// for why this is one [`Widget`] rather than a `Column` of children.
pub struct NavigationRail {
    items: Vec<NavigationRailItem>,
    /// Index into `items`. Always meaningful for a non-empty rail — MD3
    /// forbids an empty single-select, so a freshly constructed rail with
    /// items already has its first destination selected.
    selected: usize,
    alignment: NavigationRailAlignment,
    menu_icon: Option<Icon>,
    /// Whether the rail is in its expanded (wide) state. Unlike Compose's
    /// `WideNavigationRailState` (hoisted to the caller), the rail owns this
    /// itself — the built-in menu button is the intended open/close control
    /// here, so there is no separate state object to hoist.
    expanded: bool,
    enabled: bool,
    font_key: String,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
    hovered: Option<Hit>,
    pressed: Option<Hit>,
    focused: bool,
    /// The keyboard cursor: which destination Up/Down/Space act on. Distinct
    /// from `hovered`/`pressed`, which track the pointer. The menu button
    /// takes no keyboard focus.
    focused_index: usize,
    on_change: Option<Box<dyn FnMut(usize)>>,
    on_menu_click: Option<Box<dyn FnMut()>>,
    /// Tracks the selected index as a continuous value (e.g. `1.4` while
    /// traveling from destination 1 to destination 2), so the active
    /// indicator pill can be drawn by interpolating between the two
    /// destinations' pill positions. A large-ish transition — MEDIUM1 +
    /// emphasized, unlike the short/standard micro-interactions elsewhere in
    /// this crate.
    indicator: Animation<f32>,
    /// Animates the container between [`WIDTH`] and [`EXPANDED_WIDTH`] as
    /// `expanded` changes. See the constructor for why this is a tween
    /// rather than the spring Compose specs for this transition.
    width: Animation<f32>,
}

impl NavigationRail {
    /// Starts an empty, enabled rail aligned `Top` with no menu button.
    /// Callers add destinations with [`Self::item`].
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            alignment: NavigationRailAlignment::default(),
            menu_icon: None,
            expanded: false,
            enabled: true,
            font_key: "noto_sans".to_string(),
            layout_rect: LayoutRect::empty(),
            hovered: None,
            pressed: None,
            focused: false,
            focused_index: 0,
            on_change: None,
            on_menu_click: None,
            indicator: Animation::new(0.0, 0.0, duration::MEDIUM1, easing::emphasized()),
            // ponytail: Compose drives the width transition with a spring
            // (`MotionSchemeKeyTokens.DefaultSpatial`); this crate has no
            // spring primitive, so a fixed-duration eased tween
            // (MEDIUM4 + emphasized, the legacy "begin and end on screen"
            // transition) substitutes for it.
            width: Animation::new(WIDTH, WIDTH, duration::MEDIUM4, easing::emphasized()),
        }
    }

    pub fn item(mut self, item: NavigationRailItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn alignment(mut self, alignment: NavigationRailAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn set_alignment(&mut self, alignment: NavigationRailAlignment) {
        self.alignment = alignment;
    }

    /// Adds a menu button above the destination group, firing
    /// [`Self::on_menu_click`] on activation. The rail holds no open/close
    /// state of its own — swap the glyph with [`Self::set_menu_icon`] if the
    /// caller's state should change how it looks.
    pub fn menu_icon(mut self, icon: Icon) -> Self {
        self.menu_icon = Some(icon);
        self
    }

    pub fn set_menu_icon(&mut self, icon: Option<Icon>) {
        self.menu_icon = icon;
    }

    /// Construction-time initial expansion state. Snaps the width animation
    /// straight to its target instead of animating there — same idea as
    /// `SegmentedButton::selected` setting initial state directly rather
    /// than going through the animated activation path.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        let width = if expanded { EXPANDED_WIDTH } else { WIDTH };
        self.width.reset(width, width);
        self
    }

    /// Animates the rail open/closed. This is the same state the built-in
    /// menu button drives on click — call it directly when the caller wants
    /// another affordance (a keyboard shortcut, an adaptive-layout
    /// breakpoint) to open/close the rail too.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
        let width = if expanded { EXPANDED_WIDTH } else { WIDTH };
        self.width.set_target(width);
    }

    pub fn toggle_expanded(&mut self) {
        self.set_expanded(!self.expanded);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_item_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(item) = self.items.get_mut(index) {
            item.enabled = enabled;
        }
    }

    /// Key of a font registered in the [`FontBook`]; defaults to `noto_sans`.
    pub fn font(mut self, key: impl Into<String>) -> Self {
        self.font_key = key.into();
        self
    }

    /// Called with the new selected index every time activation (click or
    /// keyboard) changes the selection.
    pub fn on_change(mut self, callback: impl FnMut(usize) + 'static) -> Self {
        self.set_on_change(callback);
        self
    }

    pub fn set_on_change(&mut self, callback: impl FnMut(usize) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    /// Called every time the menu button is clicked. Typically toggles the
    /// caller's navigation drawer open/closed.
    pub fn on_menu_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.set_on_menu_click(callback);
        self
    }

    pub fn set_on_menu_click(&mut self, callback: impl FnMut() + 'static) {
        self.on_menu_click = Some(Box::new(callback));
    }

    pub fn clear_on_menu_click(&mut self) {
        self.on_menu_click = None;
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Programmatic path — never fires `on_change`. The indicator still
    /// travels to the new position, mirroring `CheckBox::set_checked`. An
    /// out-of-range or already-selected `index` is a silent no-op.
    pub fn set_selected(&mut self, index: usize) {
        if index >= self.items.len() || index == self.selected {
            return;
        }
        self.selected = index;
        self.indicator.set_target(index as f32);
    }

    fn first_enabled_index(&self) -> Option<usize> {
        self.items.iter().position(|i| i.enabled)
    }

    /// The single activation path shared by pointer release and
    /// Space/Return, so click and keyboard can never disagree about what
    /// "selecting destination N" means. Re-activating the already-selected
    /// destination is a no-op — MD3 forbids an empty single-select, and
    /// there is nothing to report either way.
    fn activate(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        if !self.enabled || !item.enabled || index == self.selected {
            return;
        }
        self.selected = index;
        self.indicator.set_target(index as f32);
        if let Some(callback) = self.on_change.as_mut() {
            callback(index);
        }
    }

    fn activate_focused(&mut self) {
        self.activate(self.focused_index);
    }

    /// Moves the keyboard cursor by `direction` (`-1` or `1`), skipping
    /// disabled destinations and clamping at the ends (no wraparound).
    /// Returns whether the cursor actually moved.
    fn move_focus(&mut self, direction: i32) -> bool {
        let len = self.items.len() as i32;
        let mut candidate = self.focused_index as i32;
        loop {
            candidate += direction;
            if candidate < 0 || candidate >= len {
                return false;
            }
            if self.items[candidate as usize].enabled {
                self.focused_index = candidate as usize;
                return true;
            }
        }
    }

    /// Per-destination height at transition progress `t`: the labelled or
    /// unlabelled collapsed footprint at `t=0`, lerped continuously toward
    /// [`EXPANDED_ITEM_HEIGHT`] as `t` approaches `1`.
    fn item_height(item: &NavigationRailItem, t: f32) -> f32 {
        let collapsed = if item.label.is_some() {
            ITEM_HEIGHT_WITH_LABEL
        } else {
            ITEM_HEIGHT_NO_LABEL
        };
        lerp(collapsed, EXPANDED_ITEM_HEIGHT, t)
    }

    /// How far through the collapsed→expanded width transition the rail
    /// currently is, `0.0` at [`WIDTH`] to `1.0` at [`EXPANDED_WIDTH`]. Every
    /// geometry helper below takes this as `t` and lerps continuously by it
    /// — there is no midpoint snap anywhere in the item layout.
    fn width_progress(&self) -> f32 {
        ((self.width.value() - WIDTH) / (EXPANDED_WIDTH - WIDTH)).clamp(0.0, 1.0)
    }

    /// The rail's current width clamped to its assigned slot, anchored to
    /// `layout_rect`'s left edge — the container never overdraws past what
    /// the layout system actually gave it, even mid-animation.
    fn effective_rect(&self) -> Rect {
        let assigned = self.layout_rect.to_skia();
        let width = self.width.value().clamp(0.0, assigned.width().max(0.0));
        Rect::from_xywh(assigned.left, assigned.top, width, assigned.height())
    }

    /// Sum of every destination's height at transition progress `t`,
    /// including the `ContainerVerticalSpace` gap lerped in alongside it (0
    /// at `t=0`, the full gap at `t=1` — collapsed items have no gap at
    /// all). Shared by [`Self::natural_height`] (measurement) and
    /// [`Self::compute_layout`] (placement) so the two formulas can never
    /// drift apart; both must lerp by the same `t` or a `Bottom`-aligned
    /// rail seams mid-transition.
    fn items_block_height(&self, t: f32) -> f32 {
        let gap = lerp(0.0, EXPANDED_ITEM_GAP, t);
        self.items
            .iter()
            .map(|item| Self::item_height(item, t))
            .sum::<f32>()
            + gap * self.items.len().saturating_sub(1) as f32
    }

    /// The rail's natural height: top/bottom padding plus the menu button
    /// (if any) plus the sum of every destination's height. No [`FontBook`]
    /// needed since every dimension here is fixed, not measured from text.
    fn natural_height(&self) -> f32 {
        let menu = if self.menu_icon.is_some() {
            MENU_BUTTON_SIZE + LEADING_GAP
        } else {
            0.0
        };
        let items = self.items_block_height(self.width_progress());
        TOP_PADDING + menu + items + BOTTOM_PADDING
    }

    /// The one geometry helper both `draw` and hit testing use, so they can
    /// never disagree about where a region's boundary sits. The menu button
    /// always pins to the top; only the destination group moves per
    /// [`Self::alignment`]. Uses [`Self::effective_rect`] rather than the
    /// raw assigned rect, so geometry always matches the animated width
    /// `draw` actually paints.
    fn compute_layout(&self) -> (Option<Rect>, Vec<Rect>) {
        let rect = self.effective_rect();
        let t = self.width_progress();
        let gap = lerp(0.0, EXPANDED_ITEM_GAP, t);

        let mut y = rect.top + TOP_PADDING;
        // Anchored to the constant `WIDTH`, not the animated `rect`'s
        // centre — the menu button never moves as the rail opens/closes.
        let menu = self.menu_icon.map(|_| {
            let r = Rect::from_xywh(
                rect.left + WIDTH / 2.0 - MENU_BUTTON_SIZE / 2.0,
                y,
                MENU_BUTTON_SIZE,
                MENU_BUTTON_SIZE,
            );
            y += MENU_BUTTON_SIZE + LEADING_GAP;
            r
        });

        let items_height = self.items_block_height(t);
        let mut item_y = match self.alignment {
            NavigationRailAlignment::Top => y,
            NavigationRailAlignment::Bottom => (rect.bottom - BOTTOM_PADDING - items_height).max(y),
        };

        let mut rects = Vec::with_capacity(self.items.len());
        for item in &self.items {
            let h = Self::item_height(item, t);
            rects.push(Rect::from_xywh(rect.left, item_y, rect.width(), h));
            item_y += h + gap;
        }
        (menu, rects)
    }

    /// The active-indicator pill for one destination, lerped continuously by
    /// `t` between the collapsed geometry — the 56x32 pill anchored to
    /// `item_rect.left + WIDTH / 2.0` (not the item rect's own, possibly
    /// wider, animated centre), top-aligned within a labeled item so the
    /// label can sit beneath it, vertically centered within an unlabeled
    /// one — and the expanded geometry: full item width inset by
    /// [`EXPANDED_LEADING_SPACE`] on both sides, vertically filling the item
    /// (whose own height already equals [`EXPANDED_ITEM_HEIGHT`] once
    /// `t=1`). Exactly the collapsed rect at `t=0.0` and the expanded rect
    /// at `t=1.0`, with nothing snapping in between.
    fn indicator_pill_rect(item_rect: Rect, item: &NavigationRailItem, t: f32) -> Rect {
        let collapsed_top = if item.label.is_some() {
            item_rect.top + INDICATOR_TOP_PADDING
        } else {
            item_rect.center_y() - INDICATOR_HEIGHT / 2.0
        };
        let collapsed = Rect::from_xywh(
            item_rect.left + WIDTH / 2.0 - INDICATOR_WIDTH / 2.0,
            collapsed_top,
            INDICATOR_WIDTH,
            INDICATOR_HEIGHT,
        );
        let expanded = Rect::from_xywh(
            item_rect.left + EXPANDED_LEADING_SPACE,
            item_rect.top,
            (item_rect.width() - EXPANDED_LEADING_SPACE * 2.0).max(0.0),
            item_rect.height(),
        );
        Rect::from_xywh(
            lerp(collapsed.left, expanded.left, t),
            lerp(collapsed.top, expanded.top, t),
            lerp(collapsed.width(), expanded.width(), t),
            lerp(collapsed.height(), expanded.height(), t),
        )
    }

    /// The single sliding selection pill, interpolated between the
    /// destinations bracketing the indicator animation's current (possibly
    /// fractional) value, at the rail's current width-transition progress
    /// `t`. `None` for an empty rail. Width/height interpolate across
    /// destinations too (rather than staying fixed at the collapsed pill's
    /// dimensions) so this stays correct in the expanded layout, where the
    /// pill spans the item's full width; using the same `t` as `draw`'s
    /// per-item loop keeps a selection change mid-width-transition
    /// consistent with everything else painted that frame.
    fn animated_indicator_rect(&self, rects: &[Rect]) -> Option<Rect> {
        if rects.is_empty() {
            return None;
        }
        let t = self.width_progress();
        let max_index = (rects.len() - 1) as f32;
        let value = self.indicator.value().clamp(0.0, max_index);
        let lower = value.floor() as usize;
        let upper = value.ceil() as usize;
        let lower_rect = Self::indicator_pill_rect(rects[lower], &self.items[lower], t);
        if lower == upper {
            return Some(lower_rect);
        }
        let upper_rect = Self::indicator_pill_rect(rects[upper], &self.items[upper], t);
        let frac = value - lower as f32;
        Some(Rect::from_xywh(
            lower_rect.left + (upper_rect.left - lower_rect.left) * frac,
            lower_rect.top + (upper_rect.top - lower_rect.top) * frac,
            lower_rect.width() + (upper_rect.width() - lower_rect.width()) * frac,
            lower_rect.height() + (upper_rect.height() - lower_rect.height()) * frac,
        ))
    }

    /// Which region (if any) contains `(x, y)`, filtered to enabled
    /// destinations — a disabled destination behaves as a gap for hit
    /// testing, exactly like a disabled segment in `SegmentedButton`.
    fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        let point = Point::new(x, y);
        if !self.effective_rect().contains(point) {
            return None;
        }
        let (menu, rects) = self.compute_layout();
        if menu.is_some_and(|r| r.contains(point)) {
            return Some(Hit::Menu);
        }
        rects
            .iter()
            .position(|r| r.contains(point))
            .filter(|&i| self.items[i].enabled)
            .map(Hit::Item)
    }

    /// Icon and label share every branch but the selected color, which is the
    /// indicator's `on_secondary_container` for the icon and plain
    /// `on_surface` for the label.
    fn item_content_color(&self, theme: &ColorTheme, index: usize, selected_color: Color) -> Color {
        if !self.enabled || !self.items[index].enabled {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        } else if self.selected == index {
            selected_color
        } else {
            theme.on_surface_variant
        }
    }

    /// The state layer tints a destination's own pill area with its content
    /// color, independent of the animated selection pill — hover/press
    /// feedback tracks the pointer, not the in-flight selection transition.
    fn item_state_layer_tint(&self, theme: &ColorTheme, index: usize) -> Color {
        if self.selected == index {
            theme.on_secondary_container
        } else {
            theme.on_surface_variant
        }
    }

    /// Precedence matches every other widget in this crate: pressed, then
    /// focus, then hover.
    fn item_state_layer_opacity(&self, index: usize) -> f32 {
        if !self.enabled || !self.items[index].enabled {
            return 0.0;
        }
        if self.pressed == Some(Hit::Item(index)) {
            PRESSED_OPACITY
        } else if self.focused && self.focused_index == index {
            FOCUS_OPACITY
        } else if self.hovered == Some(Hit::Item(index)) {
            HOVER_OPACITY
        } else {
            0.0
        }
    }

    /// The menu button takes no keyboard focus, so pressed/hover is the whole
    /// story.
    fn menu_state_layer_opacity(&self) -> f32 {
        if !self.enabled {
            0.0
        } else if self.pressed == Some(Hit::Menu) {
            PRESSED_OPACITY
        } else if self.hovered == Some(Hit::Menu) {
            HOVER_OPACITY
        } else {
            0.0
        }
    }

    fn centered_box(rect: Rect, size: f32) -> Rect {
        let half = size / 2.0;
        Rect::from_xywh(rect.center_x() - half, rect.center_y() - half, size, size)
    }
}

impl Default for NavigationRail {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for NavigationRail {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let assigned = self.layout_rect.to_skia();
        if assigned.width() <= 0.0 || assigned.height() <= 0.0 {
            return false;
        }

        // Container, painted at the animated width (clamped to the assigned
        // slot) rather than the full assigned rect — no rounding, no
        // elevation in either state, per the expanded token spec (CornerNone,
        // Level0), same as the collapsed rail already drew.
        let rect = self.effective_rect();
        {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(theme.surface), None);
            canvas.draw_rect(rect, &paint);
        }

        let t = self.width_progress();
        let indicator_radius = lerp(INDICATOR_RADIUS, EXPANDED_INDICATOR_RADIUS, t);
        let (menu, rects) = self.compute_layout();

        // Menu button: no container fill, a state-layer circle on
        // hover/press, icon always on_surface_variant (it's a plain
        // navigation affordance, not a selection control).
        if let (Some(icon), Some(menu_rect)) = (self.menu_icon, menu) {
            let opacity = self.menu_state_layer_opacity();
            if opacity > 0.0 {
                let radius = MENU_BUTTON_SIZE / 2.0;
                let rrect = RRect::new_rect_xy(menu_rect, radius, radius);
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(
                    Color4f::from(theme.on_surface_variant.with_alpha(opacity)),
                    None,
                );
                canvas.draw_rrect(rrect, &paint);
            }
            icon.draw(
                canvas,
                Self::centered_box(menu_rect, ICON_BOX_SIZE),
                theme.on_surface_variant,
            );
        }

        // The single sliding active-indicator pill, interpolated across
        // whichever destinations the selection transition is currently
        // between.
        if let Some(indicator_rect) = self.animated_indicator_rect(&rects) {
            let rrect = RRect::new_rect_xy(indicator_rect, indicator_radius, indicator_radius);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(theme.secondary_container), None);
            canvas.draw_rrect(rrect, &paint);
        }

        // Per-destination state layer, icon, label — every geometry below is
        // lerped continuously by `t` (icon centre, pill, label position and
        // size), so nothing here snaps or cross-fades between two states;
        // it just is somewhere between them.
        let label_size = lerp(LABEL_SIZE, EXPANDED_LABEL_SIZE, t);
        let font = fonts.sized(&self.font_key, label_size);
        let metrics = font.metrics().1;
        for (i, item_rect) in rects.iter().enumerate() {
            let item = &self.items[i];
            // The collapsed and expanded pills at this item's current rect,
            // needed on their own (not just their `t`-lerp) because the icon
            // centre and label anchors below are each their own lerp
            // between the two, not derived from the blended `pill`.
            let collapsed_pill = Self::indicator_pill_rect(*item_rect, item, 0.0);
            let expanded_pill = Self::indicator_pill_rect(*item_rect, item, 1.0);
            let pill = Self::indicator_pill_rect(*item_rect, item, t);

            let state_opacity = self.item_state_layer_opacity(i);
            if state_opacity > 0.0 {
                let tint = self.item_state_layer_tint(theme, i);
                let rrect = RRect::new_rect_xy(pill, indicator_radius, indicator_radius);
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(Color4f::from(tint.with_alpha(state_opacity)), None);
                canvas.draw_rrect(rrect, &paint);
            }

            let icon_color = self.item_content_color(theme, i, theme.on_secondary_container);
            let label_color = self.item_content_color(theme, i, theme.on_surface);

            // Icon centre is its own explicit lerp rather than derived from
            // `pill`: collapsed centres the icon in the collapsed pill,
            // expanded offsets it from the expanded pill's leading edge —
            // two structurally different formulas, so only the resulting
            // centre points blend cleanly.
            let collapsed_center = collapsed_pill.center();
            let expanded_center = Point::new(
                expanded_pill.left + EXPANDED_LEADING_SPACE + ICON_BOX_SIZE / 2.0,
                expanded_pill.center_y(),
            );
            let icon_center = Point::new(
                lerp(collapsed_center.x, expanded_center.x, t),
                lerp(collapsed_center.y, expanded_center.y, t),
            );
            let icon_rect = Rect::from_xywh(
                icon_center.x - ICON_BOX_SIZE / 2.0,
                icon_center.y - ICON_BOX_SIZE / 2.0,
                ICON_BOX_SIZE,
                ICON_BOX_SIZE,
            );
            item.icon.draw(canvas, icon_rect, icon_color);

            if let Some(label) = &item.label {
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(Color4f::from(label_color), None);

                // Both anchors reproduce a formerly-discrete formula
                // (centered beneath the collapsed pill vs. left-aligned
                // after the expanded icon) as a single left-aligned draw,
                // so the two can lerp continuously instead of cross-fading
                // between two different Skia text-alignment primitives.
                let (label_width, _) = font.measure_str(label, Some(&paint));
                let collapsed_x = item_rect.left + WIDTH / 2.0 - label_width / 2.0;
                let collapsed_baseline = collapsed_pill.bottom + LABEL_GAP - metrics.ascent;
                let expanded_x = expanded_pill.left
                    + EXPANDED_LEADING_SPACE
                    + ICON_BOX_SIZE
                    + EXPANDED_ICON_LABEL_SPACE;
                let expanded_baseline =
                    expanded_pill.center_y() - (metrics.ascent + metrics.descent) / 2.0;

                let label_x = lerp(collapsed_x, expanded_x, t);
                let baseline_y = lerp(collapsed_baseline, expanded_baseline, t);
                canvas.draw_str(label, Point::new(label_x, baseline_y), &font, &paint);
            }
        }

        self.indicator.is_traveling() || self.width.is_traveling()
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered.is_some() || self.pressed.is_some();
            self.hovered = None;
            self.pressed = None;
            return dirty;
        }

        let at = self.hit_test(event.x() as f32, event.y() as f32);

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let changed = self.hovered != at;
                self.hovered = at;
                changed
            }
            PointerEventKind::Leave => {
                let dirty = self.hovered.is_some() || self.pressed.is_some();
                self.hovered = None;
                self.pressed = None;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                self.hovered = at;
                self.pressed = at;
                at.is_some()
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was = self.pressed.take();

                // Activates only when press and release both land in the
                // same region, mirroring `SegmentedButton`'s rule.
                if let (Some(pressed), Some(released)) = (was, at) {
                    if pressed == released {
                        match pressed {
                            Hit::Item(i) => self.activate(i),
                            Hit::Menu => {
                                // The menu button is the rail's own
                                // open/close control (unlike Compose, where
                                // the caller hoists `WideNavigationRailState`
                                // and toggles it itself) — toggle first so
                                // `on_menu_click` observes the new state via
                                // `is_expanded` if it wants to.
                                self.toggle_expanded();
                                if let Some(callback) = self.on_menu_click.as_mut() {
                                    callback();
                                }
                            }
                        }
                    }
                }

                was.is_some()
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
                // Land the keyboard cursor on an enabled destination even if
                // `focused_index` (0 by default) happens to be disabled.
                let on_disabled = self
                    .items
                    .get(self.focused_index)
                    .is_none_or(|i| !i.enabled);
                if on_disabled {
                    if let Some(i) = self.first_enabled_index() {
                        self.focused_index = i;
                    }
                }
                true
            }
            KeyboardEventKind::Blur => {
                self.focused = false;
                true
            }
            // Space usually reaches a widget as committed text rather than as
            // a keysym, since backends route anything printable through
            // `Commit`.
            KeyboardEventKind::Commit(text) => {
                if text == " " {
                    self.activate_focused();
                    true
                } else {
                    false
                }
            }
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::SPACE | keyboard::key::RETURN => {
                    self.activate_focused();
                    true
                }
                keyboard::key::UP => self.move_focus(-1),
                keyboard::key::DOWN => self.move_focus(1),
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

    fn focusable(&self) -> bool {
        self.enabled && self.first_enabled_index().is_some()
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Width is the current animated width (between [`WIDTH`] and
    /// [`EXPANDED_WIDTH`] while a transition is in flight), so a layout
    /// system that re-measures mid-animation gets the right size; height
    /// sums every destination's fixed height at the matching layout (see
    /// [`Self::natural_height`]).
    fn measure(&self, _fonts: &FontBook) -> Size {
        Size::new(self.width.value(), self.natural_height())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// A rail laid out at the origin with `count` labeled destinations and a
    /// tall-enough footprint that the `Bottom` alignment test has room to
    /// show a difference, as a layout system would assign before the first
    /// draw.
    fn placed_rail(count: usize) -> NavigationRail {
        let mut rail = NavigationRail::new();
        for i in 0..count {
            rail = rail.item(NavigationRailItem::new(Icon::Check).label(format!("D{i}")));
        }
        rail.set_layout_rect(LayoutRect::new(0.0, 0.0, WIDTH, 600.0));
        rail
    }

    /// Like [`placed_rail`], but pre-expanded (snapped, not animating) and
    /// given a slot wide enough that [`EXPANDED_WIDTH`] isn't clamped down —
    /// `NavigationRail::effective_rect` clamps the animated width to the
    /// assigned rect, so an expanded-layout test needs a rect that wide.
    fn placed_expanded_rail(count: usize) -> NavigationRail {
        let mut rail = NavigationRail::new();
        for i in 0..count {
            rail = rail.item(NavigationRailItem::new(Icon::Check).label(format!("D{i}")));
        }
        let mut rail = rail.expanded(true);
        rail.set_layout_rect(LayoutRect::new(0.0, 0.0, EXPANDED_WIDTH, 600.0));
        rail
    }

    fn pointer_at(rail: &mut NavigationRail, x: f32, y: f32, kind: PointerEventKind) -> bool {
        rail.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A press/release pair at the centre of destination `index`, using the
    /// same `compute_layout` helper `draw`/hit-testing use.
    fn click_item(rail: &mut NavigationRail, index: usize) {
        let center = rail.compute_layout().1[index].center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(rail, center.x, center.y, kind);
        }
    }

    fn press_key(rail: &mut NavigationRail, keysym: u32) -> bool {
        rail.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    #[test]
    fn click_switches_selection_and_reports() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(3).on_change(move |i| sink.borrow_mut().push(i));

        click_item(&mut rail, 2);

        assert_eq!(rail.selected_index(), 2);
        assert_eq!(*seen.borrow(), vec![2]);
    }

    #[test]
    fn click_on_already_selected_reports_nothing() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(3).on_change(move |i| sink.borrow_mut().push(i));

        click_item(&mut rail, 0);

        assert_eq!(rail.selected_index(), 0);
        assert!(seen.borrow().is_empty());
    }

    /// The press must land in the same destination as the release, so a drag
    /// from one destination to another cancels.
    #[test]
    fn release_in_different_item_does_not_activate() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut rail = placed_rail(3).on_change(move |_| *sink.borrow_mut() += 1);

        let rects = rail.compute_layout().1;
        let press_point = rects[0].center();
        let release_point = rects[2].center();
        pointer_at(
            &mut rail,
            press_point.x,
            press_point.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut rail,
            release_point.x,
            release_point.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert_eq!(*seen.borrow(), 0);
        assert_eq!(rail.selected_index(), 0);
    }

    #[test]
    fn disabled_item_ignores_clicks_while_neighbours_still_work() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(3);
        rail.set_item_enabled(1, false);
        let mut rail = rail.on_change(move |i| sink.borrow_mut().push(i));

        click_item(&mut rail, 1);
        assert_eq!(rail.selected_index(), 0);

        click_item(&mut rail, 2);
        assert_eq!(rail.selected_index(), 2);
        assert_eq!(*seen.borrow(), vec![2]);
    }

    #[test]
    fn disabled_rail_ignores_clicks_and_keys() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut rail = placed_rail(3)
            .enabled(false)
            .on_change(move |_| *sink.borrow_mut() += 1);

        click_item(&mut rail, 1);
        assert_eq!(*seen.borrow(), 0);

        assert!(!press_key(&mut rail, keyboard::key::SPACE));
        assert_eq!(*seen.borrow(), 0);
        assert!(!rail.focusable());
    }

    /// `set_selected` is the programmatic path and must stay silent.
    #[test]
    fn set_selected_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(3).on_change(move |i| sink.borrow_mut().push(i));

        rail.set_selected(1);

        assert_eq!(rail.selected_index(), 1);
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn keyboard_navigation_skips_disabled_clamps_and_activates() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(3);
        rail.set_item_enabled(1, false);
        let mut rail = rail.on_change(move |i| sink.borrow_mut().push(i));

        rail.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Focus,
            keyboard::Modifiers::default(),
        ));
        assert_eq!(rail.focused_index, 0);

        // DOWN skips the disabled middle destination and lands on the last.
        assert!(press_key(&mut rail, keyboard::key::DOWN));
        assert_eq!(rail.focused_index, 2);
        // DOWN again: already at the end, no wraparound.
        assert!(!press_key(&mut rail, keyboard::key::DOWN));
        assert_eq!(rail.focused_index, 2);

        // SPACE activates the focused (not yet selected) last destination.
        assert!(press_key(&mut rail, keyboard::key::SPACE));
        assert_eq!(rail.selected_index(), 2);
        assert_eq!(*seen.borrow(), vec![2]);

        // UP skips the disabled middle destination on the way back.
        assert!(press_key(&mut rail, keyboard::key::UP));
        assert_eq!(rail.focused_index, 0);
        assert!(!press_key(&mut rail, keyboard::key::UP));
    }

    #[test]
    fn space_activates_however_it_arrives() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_rail(2).on_change(move |i| sink.borrow_mut().push(i));
        rail.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Focus,
            keyboard::Modifiers::default(),
        ));
        press_key(&mut rail, keyboard::key::DOWN);

        assert!(rail.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));

        assert_eq!(rail.selected_index(), 1);
        assert_eq!(*seen.borrow(), vec![1]);
    }

    #[test]
    fn menu_click_fires_its_own_callback_without_touching_selection() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut rail = placed_rail(2)
            .menu_icon(Icon::Menu)
            .on_menu_click(move || *sink.borrow_mut() += 1);

        let center = rail.compute_layout().0.unwrap().center();
        for kind in [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ] {
            pointer_at(&mut rail, center.x, center.y, kind);
        }

        assert_eq!(*seen.borrow(), 1);
        assert_eq!(rail.selected_index(), 0);
    }

    #[test]
    fn alignment_moves_the_destination_group() {
        let top_first = placed_rail(2).compute_layout().1[0].top;

        let bottom_first = placed_rail(2)
            .alignment(NavigationRailAlignment::Bottom)
            .compute_layout()
            .1[0]
            .top;

        assert!(top_first < bottom_first);
    }

    #[test]
    fn measure_reports_fixed_width_and_summed_height() {
        let fonts = FontBook::new();
        let rail = placed_rail(3);

        let size = rail.measure(&fonts);

        assert_eq!(size.width, WIDTH);
        assert_eq!(
            size.height,
            TOP_PADDING + ITEM_HEIGHT_WITH_LABEL * 3.0 + BOTTOM_PADDING
        );
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut rail = NavigationRail::new();
        assert_eq!(rail.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, WIDTH, 400.0);
        rail.set_layout_rect(rect);
        assert_eq!(rail.layout_rect(), rect);
    }

    #[test]
    fn disabled_items_make_rail_unfocusable() {
        let mut rail = placed_rail(2);
        rail.set_item_enabled(0, false);
        rail.set_item_enabled(1, false);

        assert!(!rail.focusable());
    }

    /// The expanded layout is horizontal (item height == the indicator
    /// height, not the taller labeled-collapsed footprint) and the
    /// indicator spans the item's width instead of a fixed 56dp pill.
    #[test]
    fn expanded_layout_geometry_differs_from_collapsed() {
        let collapsed = placed_rail(2);
        let collapsed_rects = collapsed.compute_layout().1;
        let collapsed_pill =
            NavigationRail::indicator_pill_rect(collapsed_rects[0], &collapsed.items[0], 0.0);

        let expanded = placed_expanded_rail(2);
        let expanded_rects = expanded.compute_layout().1;
        let expanded_pill =
            NavigationRail::indicator_pill_rect(expanded_rects[0], &expanded.items[0], 1.0);

        assert_eq!(collapsed_rects[0].height(), ITEM_HEIGHT_WITH_LABEL);
        assert_eq!(expanded_rects[0].height(), EXPANDED_ITEM_HEIGHT);
        assert_ne!(collapsed_rects[0].height(), expanded_rects[0].height());

        assert_eq!(collapsed_pill.width(), INDICATOR_WIDTH);
        assert_eq!(
            expanded_pill.width(),
            EXPANDED_WIDTH - EXPANDED_LEADING_SPACE * 2.0
        );
    }

    /// The built-in menu button is the open/close control the user asked
    /// for: clicking it toggles `expanded` in addition to firing
    /// `on_menu_click` (already covered by
    /// `menu_click_fires_its_own_callback_without_touching_selection`).
    #[test]
    fn menu_click_toggles_expansion() {
        let mut rail = placed_rail(2).menu_icon(Icon::Menu);
        assert!(!rail.is_expanded());

        let center = rail.compute_layout().0.unwrap().center();
        for kind in [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ] {
            pointer_at(&mut rail, center.x, center.y, kind);
        }

        assert!(rail.is_expanded());
    }

    /// `expanded()` is the construction-time builder and must snap;
    /// `set_expanded` is the runtime path and must animate — mirrors
    /// `SegmentedButton::selected` (snap) vs. activation (animate).
    #[test]
    fn expanded_builder_snaps_while_set_expanded_animates() {
        let rail = placed_rail(1).expanded(true);
        assert_eq!(rail.width.value(), EXPANDED_WIDTH);
        assert!(!rail.width.is_traveling());

        let mut rail = placed_rail(1);
        rail.set_expanded(true);
        assert_eq!(rail.width.to(), EXPANDED_WIDTH);
        assert!(rail.width.is_traveling());
    }

    #[test]
    fn measure_reports_expanded_width() {
        let fonts = FontBook::new();
        let rail = placed_rail(3).expanded(true);

        let size = rail.measure(&fonts);

        assert_eq!(size.width, EXPANDED_WIDTH);
    }

    #[test]
    fn click_destination_works_when_expanded() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut rail = placed_expanded_rail(3).on_change(move |i| sink.borrow_mut().push(i));

        click_item(&mut rail, 2);

        assert_eq!(rail.selected_index(), 2);
        assert_eq!(*seen.borrow(), vec![2]);
    }

    /// The menu button and the collapsed indicator pill are anchored to the
    /// constant [`WIDTH`], not the animated container centre — so opening
    /// the rail must not drag either sideways before the item layout itself
    /// has anything to say about it.
    #[test]
    fn collapsed_geometry_is_independent_of_animated_width() {
        let mut rail = placed_rail(1).menu_icon(Icon::Menu);
        rail.set_layout_rect(LayoutRect::new(0.0, 0.0, EXPANDED_WIDTH, 600.0));

        let expected_menu_left = rail.effective_rect().left + (WIDTH - MENU_BUTTON_SIZE) / 2.0;
        assert_eq!(rail.compute_layout().0.unwrap().left, expected_menu_left);

        let collapsed_pill_left_at_rest = {
            let rects = rail.compute_layout().1;
            NavigationRail::indicator_pill_rect(rects[0], &rail.items[0], 0.0).left
        };

        // Park the container mid-transition (not traveling, so no wall-clock
        // dependence). Anchoring to the animated `center_x()` would put both
        // the menu button and the collapsed pill at x=75 here instead of the
        // constant-`WIDTH` x=40 — this is what makes the assertions below a
        // real regression guard rather than a tautology.
        rail.width.reset(150.0, 150.0);
        assert_eq!(rail.effective_rect().center_x(), 75.0);

        assert_eq!(rail.compute_layout().0.unwrap().left, expected_menu_left);
        let collapsed_pill_left_mid = {
            let rects = rail.compute_layout().1;
            NavigationRail::indicator_pill_rect(rects[0], &rail.items[0], 0.0).left
        };
        assert_eq!(collapsed_pill_left_mid, collapsed_pill_left_at_rest);
    }

    /// The pill lerps continuously by `t`: a hair either side of the old
    /// midpoint snap must be nearly identical, not a jump, and the two ends
    /// of the lerp must reproduce the collapsed/expanded rects exactly.
    #[test]
    fn pill_geometry_is_continuous_across_the_transition() {
        let item = NavigationRailItem::new(Icon::Check).label("D");
        let item_rect = Rect::from_xywh(0.0, 0.0, WIDTH, ITEM_HEIGHT_WITH_LABEL);

        let at_0_49 = NavigationRail::indicator_pill_rect(item_rect, &item, 0.49);
        let at_0_51 = NavigationRail::indicator_pill_rect(item_rect, &item, 0.51);

        const EPS: f32 = 2.0;
        assert!((at_0_49.left - at_0_51.left).abs() < EPS);
        assert!((at_0_49.top - at_0_51.top).abs() < EPS);
        assert!((at_0_49.width() - at_0_51.width()).abs() < EPS);
        assert!((at_0_49.height() - at_0_51.height()).abs() < EPS);

        let expected_collapsed = Rect::from_xywh(
            item_rect.left + WIDTH / 2.0 - INDICATOR_WIDTH / 2.0,
            item_rect.top + INDICATOR_TOP_PADDING,
            INDICATOR_WIDTH,
            INDICATOR_HEIGHT,
        );
        let expected_expanded = Rect::from_xywh(
            item_rect.left + EXPANDED_LEADING_SPACE,
            item_rect.top,
            item_rect.width() - EXPANDED_LEADING_SPACE * 2.0,
            item_rect.height(),
        );
        assert_eq!(
            NavigationRail::indicator_pill_rect(item_rect, &item, 0.0),
            expected_collapsed
        );
        assert_eq!(
            NavigationRail::indicator_pill_rect(item_rect, &item, 1.0),
            expected_expanded
        );
    }

    /// `compute_layout` stacks items using the same lerped per-item height
    /// and gap that `items_block_height` sums — if the two ever disagreed,
    /// a `Bottom`-aligned rail would seam or overlap mid-transition.
    #[test]
    fn item_block_height_matches_stacked_items_at_partial_progress() {
        let mut rail = placed_rail(3);
        rail.set_layout_rect(LayoutRect::new(0.0, 0.0, EXPANDED_WIDTH, 600.0));
        // Not traveling (from == to == 150.0), so width_progress() reads a
        // deterministic 0.5 with no dependence on wall-clock timing.
        rail.width.reset(150.0, 150.0);

        let rects = rail.compute_layout().1;
        let stacked = rects.last().unwrap().bottom - rects.first().unwrap().top;

        assert_eq!(rail.items_block_height(0.5), stacked);
    }
}
