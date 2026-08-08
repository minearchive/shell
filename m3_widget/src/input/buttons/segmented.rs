//! Material 3 segmented button: a row of mutually-adjacent choices sharing
//! one outline.
//!
//! Unlike the other widgets in [`crate::buttons`], this is a single [`Widget`]
//! that owns every segment rather than a `Row` of child widgets. The control
//! paints one shared outline, internal dividers and per-segment hit rects
//! derived from its one assigned [`ui_core::geometry::LayoutRect`] —
//! splitting it into children would duplicate that geometry and make the
//! shared outline impossible to draw correctly (each child would need to
//! agree on where the others' edges are).

use skia_safe::{Canvas, Color4f, Paint, Point, RRect, Rect, Vector};

use ui_core::{
    font::FontBook,
    geometry::{LayoutRect, Size},
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::{
    icon::{Icon, BOX_SIZE as ICON_BOX_SIZE},
    tokens::{
        DISABLED_CONTAINER_OPACITY, DISABLED_CONTENT_OPACITY, FOCUS_OPACITY, HOVER_OPACITY,
        PRESSED_OPACITY,
    },
    Widget,
};

/// The MD3 segmented-button height.
pub const HEIGHT: f32 = 40.0;
/// The floor every segment's measured width is clamped to, so a single-glyph
/// label (or an icon-only segment) still gets a comfortable touch target.
pub const MIN_SEGMENT_WIDTH: f32 = 48.0;

/// Horizontal padding on each side of a segment's content.
const SEGMENT_PADDING: f32 = 12.0;
/// Gap between a segment's icon and its label.
const ICON_LABEL_GAP: f32 = 8.0;
/// Label Large.
const LABEL_SIZE: f32 = 14.0;
/// The shared outline and the internal dividers.
const STROKE_WIDTH: f32 = 1.0;

/// One choice in a [`SegmentedButton`]. Carries only configuration — the
/// control owns selection/hover/press/focus state for every segment, since a
/// single-select group can't let its members decide their own selection
/// independently of their siblings.
pub struct Segment {
    label: Option<String>,
    icon: Option<Icon>,
    enabled: bool,
}

impl Segment {
    /// A label-only segment.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            icon: None,
            enabled: true,
        }
    }

    /// A segment with no label, identified by icon alone.
    pub fn icon_only(icon: Icon) -> Self {
        Self {
            label: None,
            icon: Some(icon),
            enabled: true,
        }
    }

    /// Adds a leading icon to a label segment (or replaces one on an
    /// icon-only segment).
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// How many segments can be selected at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Exactly one segment selected once a selection exists — MD3 forbids an
    /// empty single-select, so activating the sole selected segment again is
    /// a no-op rather than clearing it.
    #[default]
    Single,
    /// Any number of segments selected independently.
    Multi,
}

/// A row of segments sharing one container outline. See the module docs for
/// why this is one [`Widget`] rather than a `Row` of children.
pub struct SegmentedButton {
    segments: Vec<Segment>,
    mode: SelectionMode,
    /// Parallel to `segments`; kept as a flat `Vec<bool>` rather than a field
    /// on `Segment` because selection is a property of the group (especially
    /// in `Single` mode, where selecting one implies deselecting another),
    /// not of an individual segment.
    selected: Vec<bool>,
    show_check: bool,
    enabled: bool,
    font_key: String,
    layout_rect: LayoutRect,
    hovered: Option<usize>,
    pressed: Option<usize>,
    focused: bool,
    /// The keyboard cursor: which segment LEFT/RIGHT/SPACE act on. Distinct
    /// from `hovered`/`pressed`, which track the pointer.
    focused_index: usize,
    /// Cached from the last [`Widget::draw`] call, which has [`FontBook`]
    /// access; [`Widget::on_pointer`] has none, so hit testing reuses this
    /// rather than risking a second, possibly-differing computation. `None`
    /// before the first draw (or after the segment set changes), in which
    /// case [`Self::segment_rects`] falls back to equal division.
    measured_widths: Option<Vec<f32>>,
    on_change: Option<Box<dyn FnMut(usize, bool)>>,
}

impl SegmentedButton {
    /// Starts an empty, enabled, `Single`-select control. Callers add
    /// segments with [`Self::segment`]/[`Self::push_segment`].
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            mode: SelectionMode::default(),
            selected: Vec::new(),
            show_check: true,
            enabled: true,
            font_key: "noto_sans".to_string(),
            layout_rect: LayoutRect::empty(),
            hovered: None,
            pressed: None,
            focused: false,
            focused_index: 0,
            measured_widths: None,
            on_change: None,
        }
    }

    pub fn segment(mut self, segment: Segment) -> Self {
        self.push_segment(segment);
        self
    }

    pub fn push_segment(&mut self, segment: Segment) {
        self.segments.push(segment);
        self.selected.push(false);
        // The segment count (and therefore every proportional width) just
        // changed, so a stale cache would misdraw/mis-hit-test.
        self.measured_widths = None;
    }

    pub fn mode(mut self, mode: SelectionMode) -> Self {
        self.mode = mode;
        if mode == SelectionMode::Single {
            // Collapse to at most one selected, keeping the first, so the
            // invariant (never more than one selected in `Single` mode)
            // holds immediately even if segments were pre-selected before
            // switching modes.
            let mut kept_one = false;
            for selected in self.selected.iter_mut() {
                if *selected {
                    if kept_one {
                        *selected = false;
                    }
                    kept_one = true;
                }
            }
        }
        self
    }

    /// Sets the initial selection. Must be called after the segments it
    /// refers to have been added — like [`Self::set_selected`], an
    /// out-of-range index is silently ignored rather than panicking.
    pub fn selected(mut self, index: usize) -> Self {
        self.set_selected(index, true);
        self
    }

    /// Whether selected segments show a leading check, replacing the
    /// segment's own icon while selected (default `true`, per MD3).
    pub fn show_check(mut self, show: bool) -> Self {
        self.show_check = show;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Key of a font registered in the [`FontBook`]; defaults to `noto_sans`.
    pub fn font(mut self, key: impl Into<String>) -> Self {
        self.font_key = key.into();
        self
    }

    /// Called with `(index, now_selected)` every time activation (click or
    /// keyboard) changes a segment's selection.
    pub fn on_change(mut self, callback: impl FnMut(usize, bool) + 'static) -> Self {
        self.set_on_change(callback);
        self
    }

    /// Replaces the change callback, dropping any previous one.
    pub fn set_on_change(&mut self, callback: impl FnMut(usize, bool) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_segment_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(segment) = self.segments.get_mut(index) {
            segment.enabled = enabled;
        }
    }

    /// Programmatic path — never fires `on_change`.
    ///
    /// In [`SelectionMode::Single`], selecting a segment silently deselects
    /// any other (mirroring activation's exclusivity). Deselecting
    /// (`selected == false`) is a no-op in `Single` mode: the single-select
    /// invariant — never zero selected once a selection exists — applies
    /// here too, not just to interactive activation, so there is no way to
    /// explicitly clear a `Single`-mode selection; select a different
    /// segment instead. An out-of-range `index` is silently ignored.
    pub fn set_selected(&mut self, index: usize, selected: bool) {
        if index >= self.segments.len() {
            return;
        }
        match self.mode {
            SelectionMode::Single => {
                if selected {
                    for (i, slot) in self.selected.iter_mut().enumerate() {
                        *slot = i == index;
                    }
                }
            }
            SelectionMode::Multi => {
                self.selected[index] = selected;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.get(index).copied().unwrap_or(false)
    }

    /// The first selected segment, in index order.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected.iter().position(|&s| s)
    }

    pub fn selected_indices(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter(|(_, &s)| s)
            .map(|(i, _)| i)
            .collect()
    }

    fn first_enabled_index(&self) -> Option<usize> {
        self.segments.iter().position(|s| s.enabled)
    }

    /// The single activation path shared by pointer release and
    /// Space/Return, so click and keyboard can never disagree about what
    /// "activating segment N" means.
    fn activate(&mut self, index: usize) {
        let Some(segment) = self.segments.get(index) else {
            return;
        };
        if !self.enabled || !segment.enabled {
            return;
        }

        match self.mode {
            SelectionMode::Single => {
                // Re-activating the already-selected segment is a no-op:
                // MD3 forbids an empty single-select, so there is nothing to
                // do (and nothing to report) here.
                if self.selected[index] {
                    return;
                }
                let previous = self.selected_index();
                for (i, slot) in self.selected.iter_mut().enumerate() {
                    *slot = i == index;
                }
                if let Some(previous) = previous {
                    if let Some(callback) = self.on_change.as_mut() {
                        callback(previous, false);
                    }
                }
                if let Some(callback) = self.on_change.as_mut() {
                    callback(index, true);
                }
            }
            SelectionMode::Multi => {
                self.selected[index] = !self.selected[index];
                let now_selected = self.selected[index];
                if let Some(callback) = self.on_change.as_mut() {
                    callback(index, now_selected);
                }
            }
        }
    }

    fn activate_focused(&mut self) {
        self.activate(self.focused_index);
    }

    /// Moves the keyboard cursor by `direction` (`-1` or `1`), skipping
    /// disabled segments and clamping at the ends (no wraparound). Returns
    /// whether the cursor actually moved.
    fn move_focus(&mut self, direction: i32) -> bool {
        let len = self.segments.len() as i32;
        let mut candidate = self.focused_index as i32;
        loop {
            candidate += direction;
            if candidate < 0 || candidate >= len {
                return false;
            }
            if self.segments[candidate as usize].enabled {
                if candidate as usize == self.focused_index {
                    return false;
                }
                self.focused_index = candidate as usize;
                return true;
            }
        }
    }

    /// Every segment's ideal (unclamped-by-layout) width: the icon slot (if
    /// the segment has an icon, or if `show_check` means selecting it later
    /// would show one) plus the measured label, padded and floored at
    /// [`MIN_SEGMENT_WIDTH`]. Reserving the icon slot regardless of whether
    /// the segment is *currently* selected keeps every width stable when the
    /// selection moves, instead of the whole control resizing on select.
    ///
    /// Pure (`&self`, no caching) so both [`Widget::measure`] (no `&mut
    /// self`) and `draw` (which does cache the result) can call it.
    fn segment_widths(&self, fonts: &FontBook) -> Vec<f32> {
        let font = fonts.sized(&self.font_key, LABEL_SIZE);
        self.segments
            .iter()
            .map(|segment| {
                let reserves_icon_slot = segment.icon.is_some() || self.show_check;
                let mut content = if reserves_icon_slot {
                    ICON_BOX_SIZE
                } else {
                    0.0
                };
                if let Some(label) = &segment.label {
                    if reserves_icon_slot {
                        content += ICON_LABEL_GAP;
                    }
                    content += font.measure_str(label, None).0;
                }
                (content + SEGMENT_PADDING * 2.0).max(MIN_SEGMENT_WIDTH)
            })
            .collect()
    }

    /// The one geometry helper both `draw` and hit testing use, so they can
    /// never disagree about where a segment's boundary sits. Distributes the
    /// assigned layout rect's width over the segments proportionally to
    /// [`Self::measured_widths`] (falling back to equal division if the
    /// control has never been drawn with fonts yet); the last segment
    /// absorbs the rounding remainder so the segments always exactly tile
    /// the assigned rect.
    fn segment_rects(&self) -> Vec<Rect> {
        let rect = self.layout_rect.to_skia();
        let count = self.segments.len();
        if count == 0 {
            return Vec::new();
        }

        let equal_share = rect.width() / count as f32;
        let widths: Vec<f32> = match &self.measured_widths {
            Some(widths) if widths.len() == count => widths.clone(),
            _ => vec![equal_share; count],
        };
        let total: f32 = widths.iter().sum();

        let mut rects = Vec::with_capacity(count);
        let mut x = rect.left;
        for (i, width) in widths.iter().enumerate() {
            let is_last = i + 1 == count;
            let segment_width = if is_last {
                rect.right - x
            } else if total > 0.0 {
                rect.width() * (width / total)
            } else {
                equal_share
            };
            rects.push(Rect::from_xywh(x, rect.top, segment_width, rect.height()));
            x += segment_width;
        }
        rects
    }

    /// Which segment (if any) contains `(x, y)`, filtered to enabled
    /// segments only — a disabled segment behaves as a gap for hit testing,
    /// exactly like a disabled leaf widget ignores pointer input elsewhere
    /// in this crate.
    fn segment_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.layout_rect.contains(x, y) {
            return None;
        }
        self.segment_rects()
            .into_iter()
            .position(|r| r.left <= x && x < r.right)
            .filter(|&i| self.segments[i].enabled)
    }

    /// The icon actually drawn for a segment: a check when selected (and
    /// `show_check`) replaces its configured icon, per MD3.
    fn segment_icon(&self, index: usize) -> Option<Icon> {
        if self.show_check && self.is_selected(index) {
            Some(Icon::Check)
        } else {
            self.segments[index].icon
        }
    }

    fn container_color(&self, theme: &ColorTheme, index: usize) -> Color {
        if !self.is_selected(index) {
            return Color::default();
        }
        if self.enabled && self.segments[index].enabled {
            theme.secondary_container
        } else {
            // Mirrors `Button`/`IconButton`'s disabled treatment for
            // variants that normally carry a container: dim to on_surface
            // rather than keep the full-emphasis fill.
            theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
        }
    }

    fn content_color(&self, theme: &ColorTheme, index: usize) -> Color {
        if !self.enabled || !self.segments[index].enabled {
            return theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY);
        }
        if self.is_selected(index) {
            theme.on_secondary_container
        } else {
            theme.on_surface
        }
    }

    /// The shared outline and the internal dividers — one control-wide
    /// color, since disabling a single segment doesn't make sense to render
    /// as a disabled *outline* (the outline isn't that segment's alone).
    fn outline_color(&self, theme: &ColorTheme) -> Color {
        if self.enabled {
            theme.outline
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY)
        }
    }

    /// The state layer tints with the segment's selected/unselected content
    /// color, ignoring disabled dimming — disabled segments never reach this
    /// because [`Self::state_layer_opacity`] returns `0.0` for them first.
    fn state_layer_tint(&self, theme: &ColorTheme, index: usize) -> Color {
        if self.is_selected(index) {
            theme.on_secondary_container
        } else {
            theme.on_surface
        }
    }

    /// Precedence matches every other widget in this crate: pressed, then
    /// focus, then hover.
    fn state_layer_opacity(&self, index: usize) -> f32 {
        if !self.enabled || !self.segments[index].enabled {
            return 0.0;
        }
        if self.pressed == Some(index) {
            PRESSED_OPACITY
        } else if self.focused && self.focused_index == index {
            FOCUS_OPACITY
        } else if self.hovered == Some(index) {
            HOVER_OPACITY
        } else {
            0.0
        }
    }

    /// An `RRect` for `rect` with `radius` applied only to the corners on its
    /// rounded side(s): the whole control's shared outline rounds both ends
    /// (`round_left && round_right`), while a middle segment's selected-fill
    /// rounds neither — segments share one outline, so an individual
    /// segment's own corners must stay square wherever they're not also the
    /// control's outer edge, or the fill would visually tear away from it.
    fn corner_rrect(rect: Rect, round_left: bool, round_right: bool, radius: f32) -> RRect {
        let round = Vector::new(radius, radius);
        let square = Vector::new(0.0, 0.0);
        // Corner order: upper-left, upper-right, lower-right, lower-left.
        let radii = [
            if round_left { round } else { square },
            if round_right { round } else { square },
            if round_right { round } else { square },
            if round_left { round } else { square },
        ];
        RRect::new_rect_radii(rect, &radii)
    }
}

impl Default for SegmentedButton {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SegmentedButton {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        // Recompute and cache so `on_pointer`'s hit testing (which has no
        // `FontBook`) reuses exactly what was just painted.
        self.measured_widths = Some(self.segment_widths(fonts));

        let rect = self.layout_rect.to_skia();
        let rects = self.segment_rects();
        if rects.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return false;
        }
        let control_radius = rect.height() / 2.0;
        let last = rects.len() - 1;

        // Container fills (selected segments only) — background layer.
        for (i, segment_rect) in rects.iter().enumerate() {
            let container = self.container_color(theme, i);
            if container.a > 0.0 {
                let rrect = Self::corner_rrect(*segment_rect, i == 0, i == last, control_radius);
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(Color4f::from(container), None);
                canvas.draw_rrect(rrect, &paint);
            }
        }

        // State layers, same shape as the container fill they sit over.
        for (i, segment_rect) in rects.iter().enumerate() {
            let opacity = self.state_layer_opacity(i);
            if opacity > 0.0 {
                let tint = self.state_layer_tint(theme, i);
                let rrect = Self::corner_rrect(*segment_rect, i == 0, i == last, control_radius);
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(Color4f::from(tint.with_alpha(opacity)), None);
                canvas.draw_rrect(rrect, &paint);
            }
        }

        // Shared outline: rounded at the two outer ends of the *whole*
        // control, square everywhere else (there is no "everywhere else" on
        // a single rect's own corners, so this is simply a fully-rounded
        // stroke around `rect`).
        {
            let outline = self.outline_color(theme);
            let inset = STROKE_WIDTH / 2.0;
            let inner = rect.with_inset((inset, inset));
            let rrect = Self::corner_rrect(inner, true, true, control_radius - inset);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(outline), None);
            paint.set_stroke(true);
            paint.set_stroke_width(STROKE_WIDTH);
            canvas.draw_rrect(rrect, &paint);
        }

        // Internal dividers, one per boundary between segments.
        {
            let outline = self.outline_color(theme);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(outline), None);
            paint.set_stroke(true);
            paint.set_stroke_width(STROKE_WIDTH);
            for segment_rect in &rects[..last] {
                let x = segment_rect.right;
                canvas.draw_line(Point::new(x, rect.top), Point::new(x, rect.bottom), &paint);
            }
        }

        // Icon (or check) + label, centered as one group per segment, drawn
        // last so it always sits above the outline/dividers.
        let font = fonts.sized(&self.font_key, LABEL_SIZE);
        let metrics = font.metrics().1;
        let baseline_offset = (metrics.ascent + metrics.descent) / 2.0;
        for (i, segment_rect) in rects.iter().enumerate() {
            let color = self.content_color(theme, i);
            let icon = self.segment_icon(i);
            let label = self.segments[i].label.as_deref();

            let label_width = label.map_or(0.0, |text| font.measure_str(text, None).0);
            let has_icon = icon.is_some();
            let content_width = if has_icon { ICON_BOX_SIZE } else { 0.0 }
                + if has_icon && label.is_some() {
                    ICON_LABEL_GAP
                } else {
                    0.0
                }
                + label_width;

            let mut cursor = segment_rect.center_x() - content_width / 2.0;
            if let Some(icon) = icon {
                let icon_top = segment_rect.center_y() - ICON_BOX_SIZE / 2.0;
                let icon_rect = Rect::from_xywh(cursor, icon_top, ICON_BOX_SIZE, ICON_BOX_SIZE);
                icon.draw(canvas, icon_rect, color);
                cursor += ICON_BOX_SIZE + if label.is_some() { ICON_LABEL_GAP } else { 0.0 };
            }
            if let Some(label) = label {
                let baseline = segment_rect.center_y() - baseline_offset;
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_color4f(Color4f::from(color), None);
                canvas.draw_str(label, Point::new(cursor, baseline), &font, &paint);
            }
        }

        // No press shape-morph, unlike `Button`/`IconButton`: segments share
        // one container outline, so morphing an individual segment's corners
        // on press would tear it away from the shared outline and misalign
        // the dividers. Nothing here animates, so there is never another
        // frame to request.
        false
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered.is_some() || self.pressed.is_some();
            self.hovered = None;
            self.pressed = None;
            return dirty;
        }

        let at = self.segment_at(event.x() as f32, event.y() as f32);

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
                if let Some(i) = at {
                    self.hovered = Some(i);
                    self.pressed = Some(i);
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                let was_pressed = self.pressed;
                self.pressed = None;
                // Activates only when press and release both land in the
                // same segment, mirroring `Button`'s inside-press +
                // inside-release rule.
                if let (Some(pressed), Some(released)) = (was_pressed, at) {
                    if pressed == released {
                        self.activate(pressed);
                    }
                }
                was_pressed.is_some()
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
                // Land the keyboard cursor on an enabled segment even if
                // `focused_index` (0 by default) happens to be disabled.
                let on_disabled = self
                    .segments
                    .get(self.focused_index)
                    .is_none_or(|s| !s.enabled);
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
                keyboard::key::LEFT => self.move_focus(-1),
                keyboard::key::RIGHT => self.move_focus(1),
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

    /// Width sums every segment's ideal width (see [`Self::segment_widths`]);
    /// height is the fixed [`HEIGHT`].
    fn measure(&self, fonts: &FontBook) -> Size {
        let total_width: f32 = self.segment_widths(fonts).iter().sum();
        Size::new(total_width, HEIGHT)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// A control laid out at the origin with `count` label segments and a
    /// real footprint, as a layout system would assign before the first
    /// draw. Never drawn with a `FontBook`, so `segment_rects` falls back to
    /// equal division — exactly the fallback path these tests exercise.
    fn placed_control(count: usize) -> SegmentedButton {
        let mut control = SegmentedButton::new();
        for i in 0..count {
            control = control.segment(Segment::new(format!("S{i}")));
        }
        control.set_layout_rect(LayoutRect::new(0.0, 0.0, 300.0, HEIGHT));
        control
    }

    fn pointer_at(control: &mut SegmentedButton, x: f32, y: f32, kind: PointerEventKind) -> bool {
        control.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A press/release pair at the centre of segment `index`, using the same
    /// `segment_rects` helper `draw`/hit-testing use.
    fn click_segment(control: &mut SegmentedButton, index: usize) {
        let center = control.segment_rects()[index].center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(control, center.x, center.y, kind);
        }
    }

    fn press_key(control: &mut SegmentedButton, keysym: u32) -> bool {
        control.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    #[test]
    fn single_select_click_switches_selection_and_reports_both_changes() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3)
            .selected(0)
            .on_change(move |i, v| sink.borrow_mut().push((i, v)));

        click_segment(&mut control, 1);

        assert!(!control.is_selected(0));
        assert!(control.is_selected(1));
        assert_eq!(*seen.borrow(), vec![(0, false), (1, true)]);
    }

    #[test]
    fn single_select_click_on_already_selected_reports_nothing() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3)
            .selected(1)
            .on_change(move |i, v| sink.borrow_mut().push((i, v)));

        click_segment(&mut control, 1);

        assert!(control.is_selected(1));
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn multi_select_clicks_toggle_independently() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3)
            .mode(SelectionMode::Multi)
            .on_change(move |i, v| sink.borrow_mut().push((i, v)));

        click_segment(&mut control, 0);
        click_segment(&mut control, 2);
        click_segment(&mut control, 0);

        assert!(!control.is_selected(0));
        assert!(control.is_selected(2));
        assert_eq!(*seen.borrow(), vec![(0, true), (2, true), (0, false)]);
    }

    /// The press must land in the same segment as the release, so a drag
    /// from one segment to another cancels — mirrors `Button`'s
    /// release-outside rule, generalized to "outside this segment".
    #[test]
    fn release_in_different_segment_does_not_activate() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut control = placed_control(3).on_change(move |_, _| *sink.borrow_mut() += 1);

        let rects = control.segment_rects();
        let press_point = rects[0].center();
        let release_point = rects[2].center();
        pointer_at(
            &mut control,
            press_point.x,
            press_point.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut control,
            release_point.x,
            release_point.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert_eq!(*seen.borrow(), 0);
        assert!(control.selected_index().is_none());
    }

    #[test]
    fn disabled_segment_ignores_clicks_while_neighbours_still_work() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3);
        control.set_segment_enabled(1, false);
        let mut control = control.on_change(move |i, v| sink.borrow_mut().push((i, v)));

        click_segment(&mut control, 1);
        assert!(control.selected_index().is_none());

        click_segment(&mut control, 2);
        assert!(control.is_selected(2));
        assert_eq!(*seen.borrow(), vec![(2, true)]);
    }

    #[test]
    fn disabled_control_ignores_clicks_and_keys() {
        let seen = Rc::new(RefCell::new(0));
        let sink = seen.clone();
        let mut control = placed_control(3)
            .enabled(false)
            .on_change(move |_, _| *sink.borrow_mut() += 1);

        click_segment(&mut control, 0);
        assert_eq!(*seen.borrow(), 0);

        assert!(!press_key(&mut control, keyboard::key::SPACE));
        assert_eq!(*seen.borrow(), 0);
        assert!(!control.focusable());
    }

    /// `set_selected` is the programmatic path and must stay silent.
    #[test]
    fn set_selected_does_not_report() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3).on_change(move |i, v| sink.borrow_mut().push((i, v)));

        control.set_selected(1, true);

        assert!(control.is_selected(1));
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn keyboard_navigation_skips_disabled_clamps_and_activates() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut control = placed_control(3);
        control.set_segment_enabled(1, false);
        let mut control = control.on_change(move |i, v| sink.borrow_mut().push((i, v)));

        control.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Focus,
            keyboard::Modifiers::default(),
        ));
        assert_eq!(control.focused_index, 0);

        // RIGHT skips the disabled middle segment and lands on the last one.
        assert!(press_key(&mut control, keyboard::key::RIGHT));
        assert_eq!(control.focused_index, 2);
        // RIGHT again: already at the end, no wraparound.
        assert!(!press_key(&mut control, keyboard::key::RIGHT));
        assert_eq!(control.focused_index, 2);

        // LEFT skips the disabled middle segment on the way back.
        assert!(press_key(&mut control, keyboard::key::LEFT));
        assert_eq!(control.focused_index, 0);
        assert!(!press_key(&mut control, keyboard::key::LEFT));

        assert!(press_key(&mut control, keyboard::key::SPACE));
        assert!(control.is_selected(0));
        assert_eq!(*seen.borrow(), vec![(0, true)]);
    }

    #[test]
    fn measure_reports_min_widths_and_fixed_height() {
        let mut fonts = FontBook::new();
        fonts.register(
            "noto_sans",
            "Noto Sans CJK JP",
            skia_safe::FontStyle::normal(),
        );
        let control = placed_control(3);

        let size = control.measure(&fonts);

        assert!(size.width >= MIN_SEGMENT_WIDTH * 3.0);
        assert_eq!(size.height, HEIGHT);
    }
}
