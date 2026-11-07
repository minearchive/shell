//! Material 3 filled text field: single line, with keyboard and drag selection.

use skia_safe::{Canvas, ClipOp, Color4f, Font, Paint, Point, RRect, Rect, Vector};

use ui_core::{
    font::FontBook,
    geometry::LayoutRect,
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::Widget;

/// M3 filled text field default height. Public so callers can size layout
/// around a text field without hardcoding its footprint.
pub const HEIGHT: f32 = 56.0;
const HORIZONTAL_PADDING: f32 = 16.0;
const TEXT_SIZE: f32 = 16.0;

/// Filled text fields round only the top corners; the bottom carries the
/// active indicator instead.
const TOP_RADIUS: f32 = 4.0;

/// Active indicator (the bottom border) thickness at rest and while focused.
const INDICATOR_HEIGHT: f32 = 1.0;
const INDICATOR_HEIGHT_FOCUSED: f32 = 2.0;

const CURSOR_WIDTH: f32 = 1.5;
const SELECTION_OPACITY: f32 = 0.4;

/// Disabled treatments.
const DISABLED_CONTAINER_OPACITY: f32 = 0.04;
const DISABLED_CONTENT_OPACITY: f32 = 0.38;

pub struct TextField {
    text: String,
    /// Byte offset into `text`. Always on a char boundary.
    cursor: usize,
    /// The fixed end of the selection; equal to `cursor` when nothing is
    /// selected. Same invariant as `cursor`.
    anchor: usize,
    font_key: String,
    enabled: bool,
    hovered: bool,
    focused: bool,
    /// A left-button drag is extending the selection.
    dragging: bool,
    /// The font of the last `draw`: `on_pointer` has no [`FontBook`], but
    /// needs text metrics to turn a click into a cursor position.
    measure_font: Option<Font>,
    /// Horizontal scroll so the cursor stays visible when text overflows.
    scroll_offset: f32,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
    on_change: Option<Box<dyn FnMut(String)>>,
}

impl TextField {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            anchor: 0,
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            focused: false,
            dragging: false,
            measure_font: None,
            scroll_offset: 0.0,
            layout_rect: LayoutRect::empty(),
            on_change: None,
        }
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

    /// Starting text; cursor is placed at its end.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self
    }

    /// Called with the new text on every edit.
    pub fn on_change(mut self, callback: impl FnMut(String) + 'static) -> Self {
        self.set_on_change(callback);
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Replaces the change callback, dropping any previous one.
    pub fn set_on_change(&mut self, callback: impl FnMut(String) + 'static) {
        self.on_change = Some(Box::new(callback));
    }

    pub fn clear_on_change(&mut self) {
        self.on_change = None;
    }

    pub fn text_value(&self) -> &str {
        &self.text
    }

    /// The widget rect assigned by the layout system, as a Skia rect for
    /// drawing/geometry math.
    fn rect(&self) -> Rect {
        self.layout_rect.to_skia()
    }

    fn container_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            return theme.on_surface.with_alpha(DISABLED_CONTAINER_OPACITY);
        }
        theme.surface_container_highest
    }

    fn text_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        } else {
            theme.on_surface
        }
    }

    fn indicator_color(&self, theme: &ColorTheme) -> Color {
        if !self.enabled {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        } else if self.focused {
            theme.primary
        } else if self.hovered {
            theme.on_surface
        } else {
            theme.on_surface_variant
        }
    }

    fn container_rrect(rect: Rect) -> RRect {
        let top = Vector::new(TOP_RADIUS, TOP_RADIUS);
        let bottom = Vector::new(0.0, 0.0);
        RRect::new_rect_radii(rect, &[top, top, bottom, bottom])
    }

    /// Content area inside the horizontal padding, excluding the indicator.
    fn content_rect(&self) -> Rect {
        let bounds = self.rect();
        Rect::from_ltrb(
            bounds.left + HORIZONTAL_PADDING,
            bounds.top,
            bounds.right - HORIZONTAL_PADDING,
            bounds.bottom - INDICATOR_HEIGHT_FOCUSED,
        )
    }

    fn prev_char_boundary(&self, index: usize) -> usize {
        self.text[..index]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_char_boundary(&self, index: usize) -> usize {
        match self.text[index..].chars().next() {
            Some(c) => index + c.len_utf8(),
            None => index,
        }
    }

    /// The selected byte range, ordered; empty when nothing is selected.
    fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    /// Filters again rather than trusting the backend: a control character in
    /// the buffer is unrecoverable, and [`keyboard::insertable_text`] is the
    /// adapter's job to apply, not ours to assume.
    fn insert(&mut self, text: &str) -> bool {
        let Some(text) = keyboard::insertable_text(text) else {
            return false;
        };
        let (start, end) = self.selection();
        self.text.replace_range(start..end, &text);
        self.cursor = start + text.len();
        self.anchor = self.cursor;
        self.changed();
        true
    }

    fn delete_range(&mut self, start: usize, end: usize) -> bool {
        if start == end {
            return false;
        }
        self.text.drain(start..end);
        self.cursor = start;
        self.anchor = start;
        self.changed();
        true
    }

    fn backspace(&mut self) -> bool {
        let (start, end) = self.selection();
        if start != end {
            return self.delete_range(start, end);
        }
        self.delete_range(self.prev_char_boundary(self.cursor), self.cursor)
    }

    fn delete_forward(&mut self) -> bool {
        let (start, end) = self.selection();
        if start != end {
            return self.delete_range(start, end);
        }
        self.delete_range(self.cursor, self.next_char_boundary(self.cursor))
    }

    /// Moves the cursor, dragging the anchor along unless `extend`. Returns
    /// whether anything moved.
    fn move_to(&mut self, index: usize, extend: bool) -> bool {
        let before = (self.cursor, self.anchor);
        self.cursor = index;
        if !extend {
            self.anchor = index;
        }
        before != (self.cursor, self.anchor)
    }

    fn select_all(&mut self) -> bool {
        let before = (self.cursor, self.anchor);
        self.anchor = 0;
        self.cursor = self.text.len();
        before != (self.cursor, self.anchor)
    }

    /// The char boundary closest to a widget-space `x`. Before the first
    /// `draw` there are no metrics, so the cursor stays where it is.
    fn offset_at(&self, x: f32) -> usize {
        let Some(font) = &self.measure_font else {
            return self.cursor;
        };
        let text_x = x - self.content_rect().left + self.scroll_offset;
        nearest_boundary(&self.text, text_x, |prefix| {
            font.measure_str(prefix, None).0
        })
    }

    fn changed(&mut self) {
        if let Some(callback) = self.on_change.as_mut() {
            callback(self.text.clone());
        }
    }

    /// Keeps the cursor within the visible content width, scrolling the
    /// minimum amount necessary.
    fn update_scroll(&mut self, fonts: &FontBook, content_width: f32) {
        let font = fonts.sized(&self.font_key, TEXT_SIZE);
        let cursor_x = font.measure_str(&self.text[..self.cursor], None).0;
        if cursor_x < self.scroll_offset {
            self.scroll_offset = cursor_x;
        } else if cursor_x > self.scroll_offset + content_width {
            self.scroll_offset = cursor_x - content_width;
        }
        let total_width = font.measure_str(&self.text, None).0;
        self.scroll_offset = self
            .scroll_offset
            .clamp(0.0, (total_width - content_width).max(0.0));
    }
}

/// The char boundary of `text` whose prefix advance is closest to `x`.
///
// ponytail: re-measures every prefix, O(n²) per pointer event; fine at
// single-line lengths, cache the edges if a field ever holds long text.
fn nearest_boundary(text: &str, x: f32, measure: impl Fn(&str) -> f32) -> usize {
    text.char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .min_by(|&a, &b| {
            let distance = |i: usize| (measure(&text[..i]) - x).abs();
            distance(a).total_cmp(&distance(b))
        })
        .unwrap_or(0)
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextField {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let bounds = self.rect();
        let rrect = Self::container_rrect(bounds);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::from(self.container_color(theme)), None);
        canvas.draw_rrect(rrect, &paint);

        let indicator_height = if self.focused {
            INDICATOR_HEIGHT_FOCUSED
        } else {
            INDICATOR_HEIGHT
        };
        let indicator = Rect::from_ltrb(
            bounds.left,
            bounds.bottom - indicator_height,
            bounds.right,
            bounds.bottom,
        );
        let mut indicator_paint = Paint::default();
        indicator_paint.set_anti_alias(true);
        indicator_paint.set_color4f(Color4f::from(self.indicator_color(theme)), None);
        canvas.draw_rect(indicator, &indicator_paint);

        let content = self.content_rect();
        self.update_scroll(fonts, content.width());

        let font = fonts.sized(&self.font_key, TEXT_SIZE);
        self.measure_font = Some(font.clone());
        let metrics = font.metrics().1;
        let baseline = content.center_y() - (metrics.ascent + metrics.descent) / 2.0;

        canvas.save();
        canvas.clip_rect(content, ClipOp::Intersect, true);

        let text_x = content.left - self.scroll_offset;
        let (selection_start, selection_end) = self.selection();
        let has_selection = selection_start != selection_end;

        if self.focused && self.enabled && has_selection {
            let left = text_x + font.measure_str(&self.text[..selection_start], None).0;
            let right = text_x + font.measure_str(&self.text[..selection_end], None).0;
            let mut selection_paint = Paint::default();
            selection_paint.set_anti_alias(true);
            selection_paint.set_color4f(
                Color4f::from(theme.primary.with_alpha(SELECTION_OPACITY)),
                None,
            );
            canvas.draw_rect(
                Rect::from_ltrb(left, content.top, right, content.bottom),
                &selection_paint,
            );
        }

        let mut text_paint = Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color4f(Color4f::from(self.text_color(theme)), None);
        canvas.draw_str(&self.text, Point::new(text_x, baseline), &font, &text_paint);

        if self.focused && self.enabled && !has_selection {
            let cursor_x = text_x + font.measure_str(&self.text[..self.cursor], None).0;
            let cursor_rect = Rect::from_ltrb(
                cursor_x,
                content.top,
                cursor_x + CURSOR_WIDTH,
                content.bottom,
            );
            let mut cursor_paint = Paint::default();
            cursor_paint.set_anti_alias(true);
            cursor_paint.set_color4f(Color4f::from(theme.primary), None);
            canvas.draw_rect(cursor_rect, &cursor_paint);
        }

        canvas.restore();

        false
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered;
            self.hovered = false;
            self.dragging = false;
            return dirty;
        }

        let inside = self.hit_rect().contains(event.x() as f32, event.y() as f32);

        match event.kind {
            PointerEventKind::Enter | PointerEventKind::Motion => {
                let mut changed = self.hovered != inside;
                self.hovered = inside;
                if self.dragging {
                    changed |= self.move_to(self.offset_at(event.x() as f32), true);
                }
                changed
            }
            PointerEventKind::Leave => {
                let dirty = self.hovered;
                self.hovered = false;
                self.dragging = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => {
                if inside {
                    self.move_to(self.offset_at(event.x() as f32), false);
                    self.dragging = true;
                }
                inside
            }
            PointerEventKind::Release { button } if button == pointer::button::LEFT => {
                self.dragging = false;
                false
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
            KeyboardEventKind::Commit(text) => self.insert(text),
            KeyboardEventKind::Press { keysym, .. } => {
                let extend = event.modifiers.shift;
                let (start, end) = self.selection();
                // A plain arrow collapses a selection onto its edge rather
                // than stepping from the cursor.
                let collapse = start != end && !extend;
                match *keysym {
                    keyboard::key::BACKSPACE => self.backspace(),
                    keyboard::key::DELETE => self.delete_forward(),
                    keyboard::key::LEFT if collapse => self.move_to(start, false),
                    keyboard::key::RIGHT if collapse => self.move_to(end, false),
                    keyboard::key::LEFT => {
                        self.move_to(self.prev_char_boundary(self.cursor), extend)
                    }
                    keyboard::key::RIGHT => {
                        self.move_to(self.next_char_boundary(self.cursor), extend)
                    }
                    keyboard::key::HOME => self.move_to(0, extend),
                    keyboard::key::END => self.move_to(self.text.len(), extend),
                    keyboard::key::A if event.modifiers.ctrl => self.select_all(),
                    _ => false,
                }
            }
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
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection();
        (start != end).then(|| self.text[start..end].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(field: &mut TextField, text: &str) {
        field.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(text.to_string()),
            keyboard::Modifiers::default(),
        ));
    }

    fn press(field: &mut TextField, keysym: u32) -> bool {
        field.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        ))
    }

    /// xkb hands Backspace back as U+0008; a `Commit` carrying it must not
    /// reach the buffer even if an adapter forgets to filter.
    #[test]
    fn commit_of_control_char_is_rejected() {
        let mut field = TextField::new();
        commit(&mut field, "abc");

        let inserted = field.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit("\u{8}".to_string()),
            keyboard::Modifiers::default(),
        ));

        assert!(!inserted, "control char must not request a redraw");
        assert_eq!(field.text_value(), "abc");
    }

    #[test]
    fn commit_inserts_at_cursor() {
        let mut field = TextField::new();
        commit(&mut field, "ab");
        assert_eq!(field.text_value(), "ab");
        assert_eq!(field.cursor, 2);
    }

    #[test]
    fn commit_multibyte_advances_cursor_by_bytes_not_chars() {
        let mut field = TextField::new();
        commit(&mut field, "日本語");
        assert_eq!(field.text_value(), "日本語");
        assert_eq!(field.cursor, "日本語".len());
    }

    #[test]
    fn backspace_removes_one_char_not_one_byte() {
        let mut field = TextField::new();
        commit(&mut field, "日本語");
        assert!(press(&mut field, keyboard::key::BACKSPACE));
        assert_eq!(field.text_value(), "日本");
        assert_eq!(field.cursor, "日本".len());
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut field = TextField::new();
        assert!(!press(&mut field, keyboard::key::BACKSPACE));
        assert_eq!(field.text_value(), "");
    }

    #[test]
    fn backspace_repeated_does_not_panic_on_multibyte() {
        let mut field = TextField::new();
        commit(&mut field, "日本語abc");
        for _ in 0..10 {
            press(&mut field, keyboard::key::BACKSPACE);
        }
        assert_eq!(field.text_value(), "");
        assert_eq!(field.cursor, 0);
    }

    #[test]
    fn delete_forward_removes_one_char_after_cursor() {
        let mut field = TextField::new();
        commit(&mut field, "日本語");
        press(&mut field, keyboard::key::HOME);
        assert!(press(&mut field, keyboard::key::DELETE));
        assert_eq!(field.text_value(), "本語");
        assert_eq!(field.cursor, 0);
    }

    #[test]
    fn left_right_move_by_char_not_byte() {
        let mut field = TextField::new();
        commit(&mut field, "日本語");
        assert!(press(&mut field, keyboard::key::LEFT));
        assert_eq!(field.cursor, "日本".len());
        assert!(press(&mut field, keyboard::key::RIGHT));
        assert_eq!(field.cursor, "日本語".len());
    }

    #[test]
    fn home_end_move_to_bounds() {
        let mut field = TextField::new();
        commit(&mut field, "hello");
        assert!(press(&mut field, keyboard::key::HOME));
        assert_eq!(field.cursor, 0);
        assert!(press(&mut field, keyboard::key::END));
        assert_eq!(field.cursor, 5);
    }

    #[test]
    fn insert_in_middle_of_multibyte_text() {
        let mut field = TextField::new();
        commit(&mut field, "日本語");
        press(&mut field, keyboard::key::HOME);
        press(&mut field, keyboard::key::RIGHT);
        commit(&mut field, "X");
        assert_eq!(field.text_value(), "日X本語");
    }

    #[test]
    fn on_change_called_with_current_text() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let seen = Rc::new(RefCell::new(String::new()));
        let seen_clone = seen.clone();
        let mut field = TextField::new().on_change(move |text| {
            *seen_clone.borrow_mut() = text;
        });
        commit(&mut field, "hi");
        assert_eq!(*seen.borrow(), "hi");
    }

    #[test]
    fn disabled_field_ignores_keyboard() {
        let mut field = TextField::new().enabled(false);
        assert!(!commit_returns(&mut field, "x"));
        assert_eq!(field.text_value(), "");
    }

    fn commit_returns(field: &mut TextField, text: &str) -> bool {
        field.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(text.to_string()),
            keyboard::Modifiers::default(),
        ))
    }

    fn press_with(field: &mut TextField, keysym: u32, modifiers: keyboard::Modifiers) -> bool {
        field.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym,
                repeat: false,
            },
            modifiers,
        ))
    }

    const SHIFT: keyboard::Modifiers = keyboard::Modifiers {
        ctrl: false,
        alt: false,
        shift: true,
        logo: false,
        caps_lock: false,
        num_lock: false,
    };

    const CTRL: keyboard::Modifiers = keyboard::Modifiers {
        ctrl: true,
        alt: false,
        shift: false,
        logo: false,
        caps_lock: false,
        num_lock: false,
    };

    #[test]
    fn shift_arrows_extend_by_char_in_either_direction() {
        let mut field = TextField::new().text("日本語");
        assert!(press_with(&mut field, keyboard::key::LEFT, SHIFT));
        assert_eq!(field.selected_text().as_deref(), Some("語"));
        press_with(&mut field, keyboard::key::LEFT, SHIFT);
        assert_eq!(field.selected_text().as_deref(), Some("本語"));
        press_with(&mut field, keyboard::key::RIGHT, SHIFT);
        press_with(&mut field, keyboard::key::RIGHT, SHIFT);
        assert_eq!(field.selected_text(), None);
    }

    #[test]
    fn shift_home_end_select_to_bounds() {
        let mut field = TextField::new().text("hello");
        press_with(&mut field, keyboard::key::HOME, SHIFT);
        assert_eq!(field.selected_text().as_deref(), Some("hello"));
        press_with(&mut field, keyboard::key::END, SHIFT);
        assert_eq!(field.selected_text(), None);
    }

    #[test]
    fn plain_arrow_collapses_selection_onto_its_edge() {
        let mut field = TextField::new().text("hello");
        press_with(&mut field, keyboard::key::A, CTRL);
        assert!(press(&mut field, keyboard::key::LEFT));
        assert_eq!((field.cursor, field.selected_text()), (0, None));

        press_with(&mut field, keyboard::key::A, CTRL);
        assert!(press(&mut field, keyboard::key::RIGHT));
        assert_eq!((field.cursor, field.selected_text()), (5, None));
    }

    #[test]
    fn plain_a_without_ctrl_does_not_select() {
        let mut field = TextField::new().text("hello");
        assert!(!press(&mut field, keyboard::key::A));
        assert_eq!(field.selected_text(), None);
    }

    #[test]
    fn select_all_when_already_selected_requests_no_redraw() {
        let mut field = TextField::new().text("hello");
        assert!(press_with(&mut field, keyboard::key::A, CTRL));
        assert!(!press_with(&mut field, keyboard::key::A, CTRL));
    }

    #[test]
    fn commit_replaces_selection_and_reports_once() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = seen.clone();
        let mut field = TextField::new()
            .text("日本語")
            .on_change(move |text| seen_clone.borrow_mut().push(text));
        press_with(&mut field, keyboard::key::LEFT, SHIFT);
        commit(&mut field, "X");
        assert_eq!(field.text_value(), "日本X");
        assert_eq!(field.selected_text(), None);
        assert_eq!(*seen.borrow(), vec!["日本X".to_string()]);
    }

    #[test]
    fn rejected_commit_keeps_the_selection() {
        let mut field = TextField::new().text("abc");
        press_with(&mut field, keyboard::key::A, CTRL);
        commit(&mut field, "\u{8}");
        assert_eq!(field.text_value(), "abc");
        assert_eq!(field.selected_text().as_deref(), Some("abc"));
    }

    /// Backspace and Delete remove the whole selection, not a char beside it.
    /// A host implements cut as copy + Delete, so this is cut's contract too.
    #[test]
    fn backspace_and_delete_remove_only_the_selection() {
        for key in [keyboard::key::BACKSPACE, keyboard::key::DELETE] {
            let mut field = TextField::new().text("abcd");
            press(&mut field, keyboard::key::LEFT);
            press_with(&mut field, keyboard::key::LEFT, SHIFT);
            assert!(press(&mut field, key));
            assert_eq!(field.text_value(), "abd");
            assert_eq!((field.cursor, field.anchor), (2, 2));
        }
    }

    #[test]
    fn nearest_boundary_rounds_to_the_closest_char_edge() {
        // Every char 10px wide, so "日本語" has edges at 0, 10, 20, 30.
        let measure = |s: &str| s.chars().count() as f32 * 10.0;
        assert_eq!(nearest_boundary("日本語", -5.0, measure), 0);
        assert_eq!(nearest_boundary("日本語", 4.0, measure), 0);
        assert_eq!(nearest_boundary("日本語", 6.0, measure), "日".len());
        assert_eq!(nearest_boundary("日本語", 99.0, measure), "日本語".len());
        assert_eq!(nearest_boundary("", 5.0, measure), 0);
    }

    fn pointer(field: &mut TextField, x: f64, kind: PointerEventKind) -> bool {
        field.on_pointer(&PointerEvent::new((x, 30.0), kind))
    }

    const LEFT_PRESS: PointerEventKind = PointerEventKind::Press {
        button: pointer::button::LEFT,
    };
    const LEFT_RELEASE: PointerEventKind = PointerEventKind::Release {
        button: pointer::button::LEFT,
    };

    /// A real font is needed for positions, so this only drives the state
    /// machine: motion extends during a drag, and only during a drag.
    #[test]
    fn drag_state_ends_on_release_and_leave() {
        for end in [LEFT_RELEASE, PointerEventKind::Leave] {
            let mut field = TextField::new().text("hello");
            field.set_layout_rect(LayoutRect::new(0.0, 0.0, 200.0, HEIGHT));
            pointer(&mut field, 50.0, LEFT_PRESS);
            assert!(field.dragging);
            pointer(&mut field, 50.0, end);
            assert!(!field.dragging);
        }
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut field = TextField::new();
        assert_eq!(field.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(10.0, 20.0, 220.0, HEIGHT);
        field.set_layout_rect(rect);
        assert_eq!(field.layout_rect(), rect);
    }

    /// Hit testing uses the assigned layout rect, not any internally
    /// computed geometry, and works before the first draw.
    #[test]
    fn hit_testing_uses_assigned_layout_rect() {
        let mut field = TextField::new();
        field.set_layout_rect(LayoutRect::new(10.0, 20.0, 220.0, HEIGHT));

        let inside = pointer_event_at(&mut field, 50.0, 40.0);
        assert!(inside);

        let mut elsewhere = TextField::new();
        elsewhere.set_layout_rect(LayoutRect::new(10.0, 20.0, 220.0, HEIGHT));
        let outside = pointer_event_at(&mut elsewhere, 500.0, 500.0);
        assert!(!outside);
    }

    fn pointer_event_at(field: &mut TextField, x: f64, y: f64) -> bool {
        field.on_pointer(&PointerEvent::new(
            (x, y),
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        ))
    }
}
