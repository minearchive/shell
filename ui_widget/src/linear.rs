//! Generic linear layout containers: [`Row`] (main axis horizontal) and
//! [`Column`] (main axis vertical). Both share one implementation
//! ([`Linear`]) parameterized on [`Axis`]; `Row`/`Column` are thin newtypes
//! over it so callers get distinct, self-documenting constructors
//! (`Row::new()` / `Column::new()`) while the layout logic lives in one place.

use skia_safe::Canvas;

use ui_core::font::FontBook;
use ui_core::geometry::{LayoutRect, Size};
use ui_core::pointer::PointerEvent;
use ui_core::scheme::ColorTheme;

use crate::Widget;

/// How children are positioned/sized along the cross axis (the axis
/// perpendicular to the container's main axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossAlign {
    #[default]
    Start,
    Center,
    End,
    /// Flush to the cross-axis start, sized to fill the container's entire
    /// cross extent.
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    fn main_of(self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        }
    }

    fn cross_of(self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.height,
            Axis::Vertical => size.width,
        }
    }

    fn child_rect(
        self,
        main_pos: f32,
        main_size: f32,
        cross_pos: f32,
        cross_size: f32,
    ) -> LayoutRect {
        match self {
            Axis::Horizontal => LayoutRect::new(main_pos, cross_pos, main_size, cross_size),
            Axis::Vertical => LayoutRect::new(cross_pos, main_pos, cross_size, main_size),
        }
    }
}

/// Shared implementation behind [`Row`] and [`Column`]. Not exported —
/// callers use the two public newtypes so the axis is fixed at the type
/// level instead of being a runtime field they could mismatch.
struct Linear {
    axis: Axis,
    layout: LayoutRect,
    children: Vec<Box<dyn Widget>>,
    gap: f32,
    cross_align: CrossAlign,
}

impl Linear {
    fn new(axis: Axis) -> Self {
        Self {
            axis,
            layout: LayoutRect::empty(),
            children: Vec::new(),
            gap: 0.0,
            cross_align: CrossAlign::Start,
        }
    }

    fn push(&mut self, child: Box<dyn Widget>) {
        self.children.push(child);
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn child_mut(&mut self, i: usize) -> Option<&mut Box<dyn Widget>> {
        self.children.get_mut(i)
    }

    /// The container's cross-axis start coordinate and extent (y/height for
    /// a `Row`, x/width for a `Column`).
    fn container_cross(&self) -> (f32, f32) {
        match self.axis {
            Axis::Horizontal => (self.layout.y, self.layout.height),
            Axis::Vertical => (self.layout.x, self.layout.width),
        }
    }

    /// The container's main-axis start coordinate (x for a `Row`, y for a
    /// `Column`).
    fn container_main_start(&self) -> f32 {
        match self.axis {
            Axis::Horizontal => self.layout.x,
            Axis::Vertical => self.layout.y,
        }
    }

    fn measure(&self, fonts: &FontBook) -> Size {
        let axis = self.axis;
        let mut main_total = 0.0f32;
        let mut cross_max = 0.0f32;
        for (i, child) in self.children.iter().enumerate() {
            let size = child.measure(fonts);
            if i > 0 {
                main_total += self.gap;
            }
            main_total += axis.main_of(size);
            cross_max = cross_max.max(axis.cross_of(size));
        }
        match axis {
            Axis::Horizontal => Size::new(main_total, cross_max),
            Axis::Vertical => Size::new(cross_max, main_total),
        }
    }

    /// Lays out and draws every child in a single pass: layout needs `fonts`
    /// to measure children, and only `draw` receives it (this is an
    /// immediate-mode container, not a retained layout tree).
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        // Copied into locals so the loop below can mutably borrow
        // `self.children` while still reading these (`Axis`/`CrossAlign` are
        // `Copy`, so this isn't a borrow of `self` at all).
        let axis = self.axis;
        let cross_align = self.cross_align;
        let gap = self.gap;
        let (cross_start, cross_extent) = self.container_cross();
        let mut main_pos = self.container_main_start();

        let mut redraw = false;
        for child in &mut self.children {
            let natural = child.measure(fonts);
            let main_size = axis.main_of(natural);
            let natural_cross = axis.cross_of(natural);

            let (cross_pos, cross_size) = match cross_align {
                CrossAlign::Start => (cross_start, natural_cross),
                CrossAlign::Center => (
                    cross_start + (cross_extent - natural_cross) / 2.0,
                    natural_cross,
                ),
                CrossAlign::End => (cross_start + cross_extent - natural_cross, natural_cross),
                CrossAlign::Stretch => (cross_start, cross_extent),
            };

            child.set_layout_rect(axis.child_rect(main_pos, main_size, cross_pos, cross_size));
            main_pos += main_size + gap;

            redraw |= child.draw(canvas, theme, fonts);
        }
        redraw
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        let mut redraw = false;
        for child in &mut self.children {
            redraw |= child.on_pointer(event);
        }
        redraw
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.layout = rect;
    }

    fn layout_rect(&self) -> LayoutRect {
        self.layout
    }
}

pub struct Row(Linear);

impl Row {
    pub fn new() -> Self {
        Self(Linear::new(Axis::Horizontal))
    }

    /// Sets the gap between adjacent children, in main-axis pixels.
    pub fn gap(mut self, gap: f32) -> Self {
        self.0.gap = gap;
        self
    }

    /// Sets how children are aligned/sized along the cross (vertical) axis.
    pub fn cross_align(mut self, align: CrossAlign) -> Self {
        self.0.cross_align = align;
        self
    }

    pub fn push(&mut self, child: Box<dyn Widget>) {
        self.0.push(child);
    }

    /// Builder-style variant of [`Self::push`], for constructing a `Row`
    /// inline.
    pub fn child(mut self, child: Box<dyn Widget>) -> Self {
        self.push(child);
        self
    }

    pub fn children(&self) -> &[Box<dyn Widget>] {
        self.0.children()
    }

    pub fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        self.0.children_mut()
    }

    pub fn child_mut(&mut self, i: usize) -> Option<&mut Box<dyn Widget>> {
        self.0.child_mut(i)
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Row {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        self.0.draw(canvas, theme, fonts)
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        self.0.on_pointer(event)
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.0.set_layout_rect(rect);
    }

    fn layout_rect(&self) -> LayoutRect {
        self.0.layout_rect()
    }

    fn measure(&self, fonts: &FontBook) -> Size {
        self.0.measure(fonts)
    }
}

/// A container that arranges children top-to-bottom (main axis vertical,
/// cross axis horizontal).
pub struct Column(Linear);

impl Column {
    pub fn new() -> Self {
        Self(Linear::new(Axis::Vertical))
    }

    /// Sets the gap between adjacent children, in main-axis pixels.
    pub fn gap(mut self, gap: f32) -> Self {
        self.0.gap = gap;
        self
    }

    /// Sets how children are aligned/sized along the cross (horizontal) axis.
    pub fn cross_align(mut self, align: CrossAlign) -> Self {
        self.0.cross_align = align;
        self
    }

    pub fn push(&mut self, child: Box<dyn Widget>) {
        self.0.push(child);
    }

    /// Builder-style variant of [`Self::push`], for constructing a `Column`
    /// inline.
    pub fn child(mut self, child: Box<dyn Widget>) -> Self {
        self.push(child);
        self
    }

    pub fn children(&self) -> &[Box<dyn Widget>] {
        self.0.children()
    }

    pub fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        self.0.children_mut()
    }

    pub fn child_mut(&mut self, i: usize) -> Option<&mut Box<dyn Widget>> {
        self.0.child_mut(i)
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Column {
    fn draw(&mut self, canvas: &Canvas, theme: &ColorTheme, fonts: &FontBook) -> bool {
        self.0.draw(canvas, theme, fonts)
    }

    fn on_pointer(&mut self, event: &PointerEvent) -> bool {
        self.0.on_pointer(event)
    }

    fn set_layout_rect(&mut self, rect: LayoutRect) {
        self.0.set_layout_rect(rect);
    }

    fn layout_rect(&self) -> LayoutRect {
        self.0.layout_rect()
    }

    fn measure(&self, fonts: &FontBook) -> Size {
        self.0.measure(fonts)
    }
}
