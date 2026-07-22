//! Material 3 list item: a single row of headline/supporting text, with
//! optional overline, leading/trailing icons, and trailing supporting text.

use skia_safe::{utils::text_utils::Align, Canvas, Color4f, Paint, Point, Rect};

use ui_core::{
    font::FontBook,
    geometry::LayoutRect,
    keyboard::{self, KeyboardEvent, KeyboardEventKind},
    pointer::{self, PointerEvent, PointerEventKind},
    scheme::{color::Color, ColorTheme},
};

use crate::Widget;

/// State layer opacities.
const HOVER_OPACITY: f32 = 0.08;
const FOCUS_OPACITY: f32 = 0.10;
const PRESSED_OPACITY: f32 = 0.10;

/// Disabled treatment: all content collapses to `on_surface` at this alpha,
/// regardless of what color it would otherwise be.
const DISABLED_CONTENT_OPACITY: f32 = 0.38;

/// Horizontal padding at both edges of the row.
const HORIZONTAL_PADDING: f32 = 16.0;
/// Leading/trailing icons are drawn into a 24dp box.
const ICON_SIZE: f32 = 24.0;
/// Gap between an icon and the adjacent text.
const ICON_TEXT_GAP: f32 = 16.0;

/// Body Large.
const HEADLINE_SIZE: f32 = 16.0;
/// Body Medium.
const SUPPORTING_SIZE: f32 = 14.0;
/// Label Small.
const OVERLINE_SIZE: f32 = 11.0;
/// Label Small.
const TRAILING_TEXT_SIZE: f32 = 11.0;

/// A leading or trailing icon drawer: given the canvas, a 24x24 box to draw
/// into, and the content color to use, paints the icon.
type IconDrawer = Box<dyn Fn(&Canvas, Rect, Color)>;

pub struct ListItem {
    headline: String,
    supporting: Option<String>,
    overline: Option<String>,
    trailing_text: Option<String>,
    leading: Option<IconDrawer>,
    trailing: Option<IconDrawer>,
    font_key: String,
    enabled: bool,
    hovered: bool,
    pressed: bool,
    focused: bool,
    /// Assigned by the layout system via [`Widget::set_layout_rect`]; zero
    /// until then.
    layout_rect: LayoutRect,
    on_click: Option<Box<dyn FnMut()>>,
}

impl ListItem {
    pub fn new(headline: impl Into<String>) -> Self {
        Self {
            headline: headline.into(),
            supporting: None,
            overline: None,
            trailing_text: None,
            leading: None,
            trailing: None,
            font_key: "noto_sans".to_string(),
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            layout_rect: LayoutRect::empty(),
            on_click: None,
        }
    }

    /// Second line, below the headline.
    pub fn supporting(mut self, text: impl Into<String>) -> Self {
        self.supporting = Some(text.into());
        self
    }

    /// Small text drawn above the headline.
    pub fn overline(mut self, text: impl Into<String>) -> Self {
        self.overline = Some(text.into());
        self
    }

    /// Small supporting text drawn on the right side of the row.
    pub fn trailing_text(mut self, text: impl Into<String>) -> Self {
        self.trailing_text = Some(text.into());
        self
    }

    /// Draws an icon into a 24x24 box on the left, using the given content
    /// color.
    pub fn leading(mut self, drawer: impl Fn(&Canvas, Rect, Color) + 'static) -> Self {
        self.leading = Some(Box::new(drawer));
        self
    }

    /// Draws an icon into a 24x24 box on the right, using the given content
    /// color.
    pub fn trailing(mut self, drawer: impl Fn(&Canvas, Rect, Color) + 'static) -> Self {
        self.trailing = Some(Box::new(drawer));
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

    /// Makes the item interactive: adds a state layer and focusability.
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

    /// Interactive items get a state layer, hit-test input, and can take
    /// focus; items without an `on_click` are inert, presentational rows.
    fn interactive(&self) -> bool {
        self.on_click.is_some() && self.enabled
    }

    /// Disabled content collapses to `on_surface` at
    /// [`DISABLED_CONTENT_OPACITY`], regardless of `base`.
    fn content_color(&self, theme: &ColorTheme, base: Color) -> Color {
        if self.enabled {
            base
        } else {
            theme.on_surface.with_alpha(DISABLED_CONTENT_OPACITY)
        }
    }

    /// The state layer tints the whole row with `on_surface`.
    fn state_layer_opacity(&self) -> f32 {
        if !self.interactive() {
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

impl Widget for ListItem {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        let rect = self.layout_rect.to_skia();

        let state_opacity = self.state_layer_opacity();
        if state_opacity > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(
                Color4f::from(theme.on_surface.with_alpha(state_opacity)),
                None,
            );
            canvas.draw_rect(rect, &paint);
        }

        let icon_top = rect.center_y() - ICON_SIZE / 2.0;
        let icon_color = self.content_color(theme, theme.on_surface_variant);

        // Leading icon reserves space on the left; the text block starts
        // after it.
        let mut text_left = rect.left + HORIZONTAL_PADDING;
        if let Some(leading) = &self.leading {
            let icon_rect = Rect::from_xywh(text_left, icon_top, ICON_SIZE, ICON_SIZE);
            leading(canvas, icon_rect, icon_color);
            text_left += ICON_SIZE + ICON_TEXT_GAP;
        }

        // Trailing icon and trailing text stack from the right edge inward;
        // whichever is present narrows the space left for the text block.
        // The text block itself is left-aligned and not clipped to this
        // boundary.
        let mut content_right = rect.right - HORIZONTAL_PADDING;
        if let Some(trailing) = &self.trailing {
            let icon_x = content_right - ICON_SIZE;
            let icon_rect = Rect::from_xywh(icon_x, icon_top, ICON_SIZE, ICON_SIZE);
            trailing(canvas, icon_rect, icon_color);
            content_right = icon_x - ICON_TEXT_GAP;
        }

        if let Some(trailing_text) = &self.trailing_text {
            let font = fonts.sized(&self.font_key, TRAILING_TEXT_SIZE);
            let metrics = font.metrics().1;
            let baseline = rect.center_y() - (metrics.ascent + metrics.descent) / 2.0;
            let color = self.content_color(theme, theme.on_surface_variant);

            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(color), None);
            canvas.draw_str_align(
                trailing_text,
                Point::new(content_right, baseline),
                &font,
                &paint,
                Align::Right,
            );
        }

        // Headline (always present), plus optional overline above and
        // supporting text below, stacked and centered as a group.
        struct Line {
            text: String,
            size: f32,
            color: Color,
        }

        let mut lines = Vec::with_capacity(3);
        if let Some(overline) = &self.overline {
            lines.push(Line {
                text: overline.clone(),
                size: OVERLINE_SIZE,
                color: self.content_color(theme, theme.on_surface_variant),
            });
        }
        lines.push(Line {
            text: self.headline.clone(),
            size: HEADLINE_SIZE,
            color: self.content_color(theme, theme.on_surface),
        });
        if let Some(supporting) = &self.supporting {
            lines.push(Line {
                text: supporting.clone(),
                size: SUPPORTING_SIZE,
                color: self.content_color(theme, theme.on_surface_variant),
            });
        }

        let rendered: Vec<_> = lines
            .iter()
            .map(|line| {
                let font = fonts.sized(&self.font_key, line.size);
                let metrics = font.metrics().1;
                (font, metrics, line)
            })
            .collect();

        let total_height: f32 = rendered
            .iter()
            .map(|(_, metrics, _)| metrics.descent - metrics.ascent)
            .sum();

        let mut top = rect.center_y() - total_height / 2.0;
        for (font, metrics, line) in &rendered {
            let baseline = top - metrics.ascent;
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(line.color), None);
            canvas.draw_str(&line.text, Point::new(text_left, baseline), font, &paint);
            top += metrics.descent - metrics.ascent;
        }

        false
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        if !self.interactive() {
            return false;
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
                // Fires only when press and release both land inside.
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

    fn on_keyboard(&mut self, event: &KeyboardEvent) -> bool {
        if !self.interactive() {
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
            // Space usually reaches a widget as committed text rather than as
            // a keysym, since backends route anything printable through
            // `Commit`.
            KeyboardEventKind::Commit(text) => {
                if text == " " {
                    if let Some(callback) = self.on_click.as_mut() {
                        callback();
                    }
                    true
                } else {
                    false
                }
            }
            KeyboardEventKind::Press { keysym, .. } => match *keysym {
                keyboard::key::SPACE | keyboard::key::RETURN => {
                    if let Some(callback) = self.on_click.as_mut() {
                        callback();
                    }
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

    fn focusable(&self) -> bool {
        self.interactive()
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

    fn pointer_at(item: &mut ListItem, x: f32, y: f32, kind: PointerEventKind) -> bool {
        item.on_pointer(&PointerEvent::new((x as f64, y as f64), kind))
    }

    /// A list item laid out at the origin with a representative row footprint.
    fn placed_item(headline: &str) -> ListItem {
        let mut item = ListItem::new(headline);
        item.set_layout_rect(LayoutRect::new(0.0, 0.0, 300.0, 56.0));
        item
    }

    /// A press/release pair at the centre of the row.
    fn click(item: &mut ListItem) {
        let center = item.layout_rect().to_skia().center();
        let kinds = [
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        ];
        for kind in kinds {
            pointer_at(item, center.x, center.y, kind);
        }
    }

    /// The assigned rect must be readable before the first `draw`.
    #[test]
    fn layout_rect_is_returned_before_first_draw() {
        let mut item = ListItem::new("Headline");
        assert_eq!(item.layout_rect(), LayoutRect::empty());

        let rect = LayoutRect::new(5.0, 6.0, 300.0, 56.0);
        item.set_layout_rect(rect);
        assert_eq!(item.layout_rect(), rect);
    }

    #[test]
    fn interactive_item_fires_on_click_when_pressed_and_released_inside() {
        let calls = Rc::new(RefCell::new(0));
        let sink = calls.clone();
        let mut item = placed_item("Headline").on_click(move || *sink.borrow_mut() += 1);

        click(&mut item);

        assert_eq!(*calls.borrow(), 1);
    }

    /// A drag off the row cancels the click.
    #[test]
    fn release_outside_does_not_fire_on_click() {
        let calls = Rc::new(RefCell::new(0));
        let sink = calls.clone();
        let mut item = placed_item("Headline").on_click(move || *sink.borrow_mut() += 1);

        let center = item.layout_rect().to_skia().center();
        pointer_at(
            &mut item,
            center.x,
            center.y,
            PointerEventKind::Press {
                button: pointer::button::LEFT,
            },
        );
        pointer_at(
            &mut item,
            center.x + 1000.0,
            center.y,
            PointerEventKind::Release {
                button: pointer::button::LEFT,
            },
        );

        assert_eq!(*calls.borrow(), 0);
    }

    /// An item with no `on_click` is a presentational row: pointer input is
    /// ignored entirely.
    #[test]
    fn non_interactive_item_ignores_pointer() {
        let mut item = placed_item("Headline");

        let dirty = pointer_at(&mut item, 10.0, 10.0, PointerEventKind::Enter);
        assert!(!dirty);

        click(&mut item);
        assert!(!item.focusable());
    }

    #[test]
    fn disabled_item_does_not_fire_on_click() {
        let calls = Rc::new(RefCell::new(0));
        let sink = calls.clone();
        let mut item = placed_item("Headline")
            .on_click(move || *sink.borrow_mut() += 1)
            .enabled(false);

        click(&mut item);

        assert_eq!(*calls.borrow(), 0);
        assert!(!item.focusable());
    }

    #[test]
    fn space_and_return_fire_on_click_when_focusable() {
        let calls = Rc::new(RefCell::new(0));
        let sink = calls.clone();
        let mut item = placed_item("Headline").on_click(move || *sink.borrow_mut() += 1);
        assert!(item.focusable());

        assert!(item.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Commit(" ".to_string()),
            keyboard::Modifiers::default(),
        )));
        assert!(item.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Press {
                keysym: keyboard::key::RETURN,
                repeat: false,
            },
            keyboard::Modifiers::default(),
        )));

        assert_eq!(*calls.borrow(), 2);
    }

    #[test]
    fn focus_and_blur_track_focused_state() {
        let mut item = placed_item("Headline").on_click(|| {});

        assert!(item.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Focus,
            keyboard::Modifiers::default(),
        )));
        assert!(item.focused);

        assert!(item.on_keyboard(&KeyboardEvent::new(
            KeyboardEventKind::Blur,
            keyboard::Modifiers::default(),
        )));
        assert!(!item.focused);
    }
}
