//! Material 3 filled text field: single line, cursor only (no selection).

use skia_safe::{Canvas, ClipOp, Color4f, Contains, Paint, Point, RRect, Rect, Vector};

use ui_core::{
    font::FontBook,
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::Widget;

/// M3 filled text field default height.
const HEIGHT: f32 = 56.0;
const HORIZONTAL_PADDING: f32 = 16.0;
const TEXT_SIZE: f32 = 16.0;

/// Filled text fields round only the top corners; the bottom carries the
/// active indicator instead.
const TOP_RADIUS: f32 = 4.0;

/// Active indicator (the bottom border) thickness at rest and while focused.
const INDICATOR_HEIGHT: f32 = 1.0;
const INDICATOR_HEIGHT_FOCUSED: f32 = 2.0;

const CURSOR_WIDTH: f32 = 1.5;

/// Disabled treatments.
const DISABLED_CONTAINER_OPACITY: f32 = 0.04;
const DISABLED_CONTENT_OPACITY: f32 = 0.38;

pub struct TextField {
    text: String,
    /// Byte offset into `text`. Always on a char boundary.
    cursor: usize,
    origin: (f32, f32),
    width: f32,
    font_key: String,
    enabled: bool,
    hovered: bool,
    focused: bool,
    /// Horizontal scroll so the cursor stays visible when text overflows.
    scroll_offset: f32,
    /// Filled in by `draw`; zero until the first frame.
    bounds: Rect,
    on_change: Option<Box<dyn FnMut(String)>>,
}

impl TextField {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            origin: (0.0, 0.0),
            width: 200.0,
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            focused: false,
            scroll_offset: 0.0,
            bounds: Rect::new_empty(),
            on_change: None,
        }
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.origin = (x, y);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
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

    /// Starting text; cursor is placed at its end.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self.cursor = self.text.len();
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

    fn layout(&self) -> Rect {
        Rect::from_xywh(self.origin.0, self.origin.1, self.width, HEIGHT)
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
        Rect::from_ltrb(
            self.bounds.left + HORIZONTAL_PADDING,
            self.bounds.top,
            self.bounds.right - HORIZONTAL_PADDING,
            self.bounds.bottom - INDICATOR_HEIGHT_FOCUSED,
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

    /// Filters again rather than trusting the backend: a control character in
    /// the buffer is unrecoverable, and [`keyboard::insertable_text`] is the
    /// adapter's job to apply, not ours to assume.
    fn insert(&mut self, text: &str) -> bool {
        let Some(text) = keyboard::insertable_text(text) else {
            return false;
        };
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.changed();
        true
    }

    fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start = self.prev_char_boundary(self.cursor);
        self.text.drain(start..self.cursor);
        self.cursor = start;
        self.changed();
        true
    }

    fn delete_forward(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let end = self.next_char_boundary(self.cursor);
        self.text.drain(self.cursor..end);
        self.changed();
        true
    }

    fn move_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor = self.prev_char_boundary(self.cursor);
        true
    }

    fn move_right(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.cursor = self.next_char_boundary(self.cursor);
        true
    }

    fn move_home(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor = 0;
        true
    }

    fn move_end(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.cursor = self.text.len();
        true
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

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextField {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) {
        self.bounds = self.layout();
        let rrect = Self::container_rrect(self.bounds);

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
            self.bounds.left,
            self.bounds.bottom - indicator_height,
            self.bounds.right,
            self.bounds.bottom,
        );
        let mut indicator_paint = Paint::default();
        indicator_paint.set_anti_alias(true);
        indicator_paint.set_color4f(Color4f::from(self.indicator_color(theme)), None);
        canvas.draw_rect(indicator, &indicator_paint);

        let content = self.content_rect();
        self.update_scroll(fonts, content.width());

        let font = fonts.sized(&self.font_key, TEXT_SIZE);
        let metrics = font.metrics().1;
        let baseline = content.center_y() - (metrics.ascent + metrics.descent) / 2.0;

        canvas.save();
        canvas.clip_rect(content, ClipOp::Intersect, true);

        let text_x = content.left - self.scroll_offset;
        let mut text_paint = Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color4f(Color4f::from(self.text_color(theme)), None);
        canvas.draw_str(&self.text, Point::new(text_x, baseline), &font, &text_paint);

        if self.focused && self.enabled {
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
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.enabled {
            let dirty = self.hovered;
            self.hovered = false;
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
                let dirty = self.hovered;
                self.hovered = false;
                dirty
            }
            PointerEventKind::Press { button } if button == pointer::button::LEFT => inside,
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
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::BACKSPACE => self.backspace(),
                keyboard::key::DELETE => self.delete_forward(),
                keyboard::key::LEFT => self.move_left(),
                keyboard::key::RIGHT => self.move_right(),
                keyboard::key::HOME => self.move_home(),
                keyboard::key::END => self.move_end(),
                _ => false,
            },
            KeyboardEventKind::Release { .. } | KeyboardEventKind::Preedit { .. } => false,
        }
    }

    fn bounds(&self) -> Rect {
        self.bounds
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
        field.cursor = 0;
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
        field.cursor = "日".len();
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
}
