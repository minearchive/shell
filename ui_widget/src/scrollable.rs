//! A vertically scrollable container widget.

use std::time::Instant;

use skia_safe::{Canvas, ClipOp, Color4f, Paint};

use crate::Widget;
use ui_core::font::FontBook;
use ui_core::geometry::LayoutRect;
use ui_core::pointer::{self, PointerEvent, PointerEventKind};
use ui_core::scheme::ColorTheme;

/// Distance scrolled per discrete wheel "click".
const SCROLL_LINE: f32 = 48.0;
/// Width of the scrollbar thumb.
const BAR_WIDTH: f32 = 4.0;
/// Gap between the scrollbar thumb and the viewport's right edge.
const BAR_MARGIN: f32 = 2.0;
/// Minimum thumb height, regardless of content/viewport ratio.
const MIN_THUMB: f32 = 24.0;
/// Seconds the scrollbar stays fully visible after the last scroll input.
const BAR_HOLD: f32 = 0.8;
/// Seconds the scrollbar takes to fade out after `BAR_HOLD` elapses.
const BAR_FADE: f32 = 0.3;
/// Maximum alpha (fully visible) the scrollbar thumb ever reaches.
const BAR_MAX_ALPHA: f32 = 0.5;

/// A container widget that lays out children in a vertical scrolling content
/// area, clipped to a viewport.
pub struct ScrollableWidget {
    /// The viewport, in window space.
    layout: LayoutRect,
    children: Vec<Box<dyn Widget>>,
    /// Total scrollable content height, in content space.
    content_height: f32,
    /// Current vertical scroll offset; always clamped to `[0, max_offset()]`.
    offset: f32,
    /// Optional decoration drawn in content space (after children), so it
    /// scrolls and clips along with the content. Used e.g. to draw section
    /// captions.
    content_overlay: Option<Box<dyn FnMut(&Canvas, &ColorTheme, &FontBook)>>,
    /// Timestamp of the last scroll input; drives the scrollbar auto-fade.
    last_scroll: Option<Instant>,
}

impl ScrollableWidget {
    pub fn new() -> Self {
        Self {
            layout: LayoutRect::empty(),
            children: Vec::new(),
            content_height: 0.0,
            offset: 0.0,
            content_overlay: None,
            last_scroll: None,
        }
    }

    pub fn push(&mut self, child: Box<dyn Widget>) {
        self.children.push(child);
    }

    pub fn set_content_height(&mut self, h: f32) {
        self.content_height = h;
        self.clamp_offset();
    }

    pub fn set_content_overlay(
        &mut self,
        f: impl FnMut(&Canvas, &ColorTheme, &FontBook) + 'static,
    ) {
        self.content_overlay = Some(Box::new(f));
    }

    pub fn offset(&self) -> f32 {
        self.offset
    }

    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    pub fn child_mut(&mut self, i: usize) -> Option<&mut Box<dyn Widget>> {
        self.children.get_mut(i)
    }

    fn max_offset(&self) -> f32 {
        (self.content_height - self.layout.height).max(0.0)
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }

    /// Converts a window coordinate into content space.
    pub fn content_point(&self, window: pointer::Point) -> pointer::Point {
        (
            window.0 - self.layout.x as f64,
            window.1 - (self.layout.y - self.offset) as f64,
        )
    }

    /// Whether `window` (window-space coordinates) falls inside the
    /// viewport.
    pub fn viewport_contains(&self, window: pointer::Point) -> bool {
        self.layout.contains(window.0 as f32, window.1 as f32)
    }

    /// Current scrollbar thumb opacity, from 0 (hidden) to 1 (fully
    /// visible), driven by time since the last scroll input.
    fn bar_alpha(&self) -> f32 {
        if self.max_offset() == 0.0 {
            return 0.0;
        }
        let Some(last_scroll) = self.last_scroll else {
            return 0.0;
        };
        let e = last_scroll.elapsed().as_secs_f32();
        if e < BAR_HOLD {
            1.0
        } else if e < BAR_HOLD + BAR_FADE {
            1.0 - (e - BAR_HOLD) / BAR_FADE
        } else {
            0.0
        }
    }

    /// Whether the scrollbar is still mid hold/fade and needs further
    /// redraws to animate.
    fn bar_animating(&self) -> bool {
        self.max_offset() > 0.0
            && self
                .last_scroll
                .is_some_and(|t| t.elapsed().as_secs_f32() < BAR_HOLD + BAR_FADE)
    }

    fn draw_scrollbar(&self, canvas: &Canvas, theme: &ColorTheme) {
        let max_offset = self.max_offset();
        if max_offset <= 0.0 {
            return;
        }
        let alpha = self.bar_alpha();
        if alpha <= 0.0 {
            return;
        }

        let track_h = self.layout.height;
        let thumb_h = (track_h * track_h / self.content_height).clamp(MIN_THUMB, track_h);
        let thumb_y = self.layout.y + (self.offset / max_offset) * (track_h - thumb_h);
        let x = self.layout.x + self.layout.width - BAR_MARGIN - BAR_WIDTH;

        let rect = skia_safe::Rect::from_xywh(x, thumb_y, BAR_WIDTH, thumb_h);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(
            Color4f::from(theme.on_surface_variant.with_alpha(BAR_MAX_ALPHA * alpha)),
            None,
        );
        canvas.draw_round_rect(rect, BAR_WIDTH / 2.0, BAR_WIDTH / 2.0, &paint);
    }
}

impl Default for ScrollableWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollableWidget {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        canvas.save();
        canvas.clip_rect(self.layout.to_skia(), ClipOp::Intersect, true);
        canvas.translate((self.layout.x, self.layout.y - self.offset));

        let mut redraw = false;
        for c in &mut self.children {
            redraw |= c.draw(canvas, theme, fonts);
        }
        if let Some(overlay) = &mut self.content_overlay {
            overlay(canvas, theme, fonts);
        }

        canvas.restore();

        self.draw_scrollbar(canvas, theme);

        redraw || self.bar_animating()
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        match event.kind {
            PointerEventKind::Axis { vertical, .. } => {
                if self.viewport_contains(event.position) && self.max_offset() > 0.0 {
                    let dy = if vertical.discrete != 0 {
                        vertical.discrete as f32 * SCROLL_LINE
                    } else {
                        vertical.absolute as f32
                    };
                    self.offset -= dy;
                    self.clamp_offset();
                    self.last_scroll = Some(Instant::now());
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Leave => {
                let mut redraw = false;
                for c in &mut self.children {
                    redraw |=
                        c.on_pointer(&PointerEvent::new(event.position, PointerEventKind::Leave));
                }
                redraw
            }
            _ => {
                let mut redraw = false;
                if self.viewport_contains(event.position) {
                    let translated =
                        PointerEvent::new(self.content_point(event.position), event.kind);
                    for c in &mut self.children {
                        redraw |= c.on_pointer(&translated);
                    }
                } else {
                    let synthetic = PointerEvent::new(event.position, PointerEventKind::Leave);
                    for c in &mut self.children {
                        redraw |= c.on_pointer(&synthetic);
                    }
                }
                redraw
            }
        }
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout = rect;
        self.clamp_offset();
    }

    fn layout_rect(&self) -> LayoutRect {
        self.layout
    }
}
