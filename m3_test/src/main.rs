mod layout;
mod util;

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use skia_safe::{surfaces, Color4f, ImageInfo, Paint, Point};
use softbuffer::{Context, Surface};
use taffy::prelude::*;
use ui_core::font::FontBook;
use ui_core::geometry::LayoutRect;
use ui_core::keyboard::{self, KeyboardEvent, KeyboardEventKind};
use ui_core::pointer::{self, AxisScroll, PointerEvent, PointerEventKind};
use ui_core::scheme::ColorTheme;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::layout::{build, leaf, resolve_layout_rects, Caption, LayoutNode, PendingWidget};
use m3_widget::{
    buttons::{icon_button, radio_button, segmented},
    checkbox, navigation_rail, switch, text_field, Button, ButtonSize, ButtonVariant, CheckBox,
    Column, CrossAlign, Divider, Icon, IconButton, IconButtonVariant, ListItem, NavigationRail,
    NavigationRailAlignment, NavigationRailItem, RadioButton, Row, ScrollableWidget, Segment,
    SegmentedButton, SelectionMode, Slider, SliderSize, Switch, SwitchIcons, TextField, Widget,
};
use util::{button_code, keysym_from_named, load_theme};

/// Left/right/top/bottom breathing room around the whole gallery. Top clears
/// the two header lines drawn separately in `RedrawRequested`.
const ROOT_PADDING_X: f32 = 24.0;
const ROOT_PADDING_TOP: f32 = 24.0;
const ROOT_PADDING_BOTTOM: f32 = 24.0;
/// Height of the fixed header drawn above the scrollable viewport.
const HEADER_HEIGHT: f32 = 80.0;
/// Gap between gallery sections (rows/columns), stacked in a column.
const SECTION_GAP: f32 = 32.0;
/// Gap between items within one row.
const ITEM_GAP: f32 = 16.0;
/// Gap between switch variant groups.
const GROUP_GAP: f32 = 24.0;
/// Gap between stacked items in the slider-sizes column.
const COLUMN_ITEM_GAP: f32 = 8.0;
/// Baseline offset from a section's top to its caption, drawn just above it.
const CAPTION_OFFSET: f32 = 8.0;

fn root_padding() -> Rect<LengthPercentage> {
    Rect {
        left: length(ROOT_PADDING_X),
        right: length(ROOT_PADDING_X),
        top: length(ROOT_PADDING_TOP),
        bottom: length(ROOT_PADDING_BOTTOM),
    }
}

fn button_gallery() -> Vec<LayoutNode> {
    let variants = [
        (
            "Filled",
            ButtonSize::Medium,
            ButtonVariant::Filled,
            true,
            100.0,
        ),
        (
            "Tonal",
            ButtonSize::Medium,
            ButtonVariant::FilledTonal,
            true,
            100.0,
        ),
        (
            "Elevated",
            ButtonSize::Medium,
            ButtonVariant::Elevated,
            true,
            110.0,
        ),
        (
            "Outlined",
            ButtonSize::Medium,
            ButtonVariant::Outlined,
            true,
            110.0,
        ),
        ("Text", ButtonSize::Medium, ButtonVariant::Text, true, 90.0),
    ];

    let sizes = [
        (
            "XS",
            ButtonSize::ExtraSmall,
            ButtonVariant::Filled,
            true,
            90.0,
        ),
        ("S", ButtonSize::Small, ButtonVariant::Filled, true, 90.0),
        ("M", ButtonSize::Medium, ButtonVariant::Filled, true, 100.0),
        ("L", ButtonSize::Large, ButtonVariant::Filled, true, 110.0),
        (
            "XL",
            ButtonSize::ExtraLarge,
            ButtonVariant::Filled,
            true,
            120.0,
        ),
    ];

    let disabled = [
        (
            "Filled",
            ButtonSize::Medium,
            ButtonVariant::Filled,
            false,
            100.0,
        ),
        (
            "Tonal",
            ButtonSize::Medium,
            ButtonVariant::FilledTonal,
            false,
            100.0,
        ),
        (
            "Elevated",
            Default::default(),
            ButtonVariant::Elevated,
            false,
            110.0,
        ),
        (
            "Outlined",
            ButtonSize::Medium,
            ButtonVariant::Outlined,
            false,
            110.0,
        ),
        ("Text", ButtonSize::Medium, ButtonVariant::Text, false, 90.0),
    ];

    let all = [
        ("Variants", variants),
        ("Sizes", sizes),
        ("Disabled", disabled),
    ];

    all.into_iter()
        .map(|(title, group)| {
            LayoutNode::row(
                group
                    .into_iter()
                    .map(|(label, size, variant, enabled, width)| {
                        leaf(
                            width,
                            size.height(),
                            Button::new(label)
                                .size(size)
                                .variant(variant)
                                .enabled(enabled)
                                .on_click(move || println!("clicked: {label}")),
                        )
                    })
                    .collect(),
            )
            .gap(ITEM_GAP)
            .caption(title)
        })
        .collect()
}

/// A fixed per-segment width estimate for sizing taffy leaves. Not
/// `SegmentedButton::measure` itself — that needs a `FontBook`, which isn't
/// available while the layout tree is still being built (widgets aren't
/// constructed until layout is resolved) — but comfortably wide enough for a
/// short label plus its reserved icon slot, matching the widths used
/// elsewhere in this file for buttons of similar label length.
const SEGMENTED_SEGMENT_WIDTH: f32 = 110.0;

fn segmented_gallery() -> Vec<LayoutNode> {
    vec![
        // Single-select: three label-only segments, "Day" starts selected.
        row![leaf(
            SEGMENTED_SEGMENT_WIDTH * 3.0,
            segmented::HEIGHT,
            SegmentedButton::new()
                .segment(Segment::new("Day"))
                .segment(Segment::new("Week"))
                .segment(Segment::new("Month"))
                .selected(0)
                .on_change(|i, v| println!("segmented (single): {i} -> {v}")),
        )]
        .gap(ITEM_GAP)
        .caption("Segmented (single)"),
        // Multi-select: three icon+label segments, two starting selected.
        row![leaf(
            SEGMENTED_SEGMENT_WIDTH * 3.0,
            segmented::HEIGHT,
            SegmentedButton::new()
                .segment(Segment::new("Bold").icon(Icon::Check))
                .segment(Segment::new("Italic").icon(Icon::Add))
                .segment(Segment::new("Underline").icon(Icon::Menu))
                .mode(SelectionMode::Multi)
                .selected(0)
                .selected(2)
                .on_change(|i, v| println!("segmented (multi): {i} -> {v}")),
        )]
        .gap(ITEM_GAP)
        .caption("Segmented (multi)"),
        // Disabled: one control with a disabled middle segment (its
        // neighbours stay interactive), and one control disabled entirely.
        row![
            leaf(
                SEGMENTED_SEGMENT_WIDTH * 3.0,
                segmented::HEIGHT,
                SegmentedButton::new()
                    .segment(Segment::new("List"))
                    .segment(Segment::new("Grid").enabled(false))
                    .segment(Segment::new("Table"))
                    .selected(0)
                    .on_change(|i, v| println!("segmented (disabled segment): {i} -> {v}")),
            ),
            leaf(
                SEGMENTED_SEGMENT_WIDTH * 2.0,
                segmented::HEIGHT,
                SegmentedButton::new()
                    .segment(Segment::new("On"))
                    .segment(Segment::new("Off"))
                    .selected(0)
                    .enabled(false)
                    .on_change(|i, v| println!("segmented (disabled control): {i} -> {v}")),
            ),
        ]
        .gap(ITEM_GAP)
        .caption("Segmented (disabled)"),
    ]
}

/// Demonstrates `ui_widget`'s generic `Row`/`Column` containers, used
/// directly (not via taffy) to arrange a mix of M3 widgets inside a single
/// fixed-size taffy leaf. `Column` stacks two stretched buttons above a
/// centered `Row` of fixed-size controls.
///
/// Note these are *not* the `row!`/`column!` macros: `ui_widget::Row` lays out
/// and paints its children itself in one immediate-mode pass, whereas `row!`
/// only describes taffy nodes. The two never interleave — the `Column` below
/// owns everything inside its single leaf.
fn layout_gallery() -> Vec<LayoutNode> {
    // Comfortably fits: two Medium buttons (48 each) + a 48-tall control row,
    // plus two 8px gaps between them (96 + 48 + 16 = 160, rounded up).
    const LEAF_WIDTH: f32 = 240.0;
    const LEAF_HEIGHT: f32 = 176.0;

    let mut col = Column::new().gap(8.0).cross_align(CrossAlign::Stretch);
    col.push(Box::new(
        Button::new("One")
            .size(ButtonSize::Medium)
            .variant(ButtonVariant::Filled)
            .on_click(|| println!("One")),
    ));
    col.push(Box::new(
        Button::new("Two")
            .size(ButtonSize::Medium)
            .variant(ButtonVariant::FilledTonal)
            .on_click(|| println!("Two")),
    ));

    let mut controls = Row::new().gap(8.0).cross_align(CrossAlign::Center);
    controls.push(Box::new(CheckBox::new(true)));
    controls.push(Box::new(Switch::new(true)));
    controls.push(Box::new(
        IconButton::new()
            .variant(IconButtonVariant::Filled)
            .icon(Icon::Favorite),
    ));
    col.push(Box::new(controls));

    vec![row![leaf(LEAF_WIDTH, LEAF_HEIGHT, col)]
        .gap(ITEM_GAP)
        .caption("Row / Column")]
}

fn slider_gallery() -> Vec<LayoutNode> {
    let types = [(true, 0.), (true, 10.), (false, 0.)];
    let sizes = [
        SliderSize::ExtraSmall,
        SliderSize::Small,
        SliderSize::Medium,
    ];

    vec![
        LayoutNode::row(
            types
                .into_iter()
                .map(|(enabled, step)| {
                    leaf(
                        200.0,
                        SliderSize::default().handle_height(),
                        Slider::new(0.0, 100.0, 40.0)
                            .labeled(true)
                            .enabled(enabled)
                            .step(step)
                            .on_change(|v| println!("continuous: {v:.1}")),
                    )
                })
                .collect(),
        )
        .gap(ITEM_GAP)
        .caption("Sliders"),
        LayoutNode::column(
            sizes
                .into_iter()
                .map(|size| {
                    leaf(
                        280.0,
                        size.handle_height(),
                        Slider::new(0.0, 100.0, 50.0).size(size),
                    )
                })
                .collect(),
        )
        .gap(COLUMN_ITEM_GAP)
        .caption("Slider sizes"),
    ]
}

fn text_field_gallery() -> Vec<LayoutNode> {
    vec![row![
        leaf(
            220.0,
            text_field::HEIGHT,
            TextField::new().on_change(|text| println!("text: {text}")),
        ),
        leaf(
            220.0,
            text_field::HEIGHT,
            TextField::new()
                .text("フォント入力テスト")
                .on_change(|text| println!("text: {text}")),
        ),
        leaf(
            220.0,
            text_field::HEIGHT,
            TextField::new().text("disabled").enabled(false),
        ),
    ]
    .gap(ITEM_GAP)
    .caption("Text fields")]
}

fn switch_gallery() -> Vec<LayoutNode> {
    let variants = [
        ("plain", SwitchIcons::None),
        ("selected-icon", SwitchIcons::Selected),
        ("both-icons", SwitchIcons::Both),
    ];

    let mut groups: Vec<LayoutNode> = variants
        .into_iter()
        .map(|(label, icons)| {
            LayoutNode::row(
                [false, true]
                    .into_iter()
                    .map(|checked| {
                        leaf(
                            switch::TRACK_WIDTH,
                            switch::TRACK_HEIGHT,
                            Switch::new(checked)
                                .icons(icons)
                                .on_change(move |v| println!("{label}: {v}")),
                        )
                    })
                    .collect(),
            )
            .gap(ITEM_GAP)
        })
        .collect();

    groups.push(
        LayoutNode::row(
            [false, true]
                .into_iter()
                .map(|checked| {
                    leaf(
                        switch::TRACK_WIDTH,
                        switch::TRACK_HEIGHT,
                        Switch::new(checked).icons(SwitchIcons::Both).enabled(false),
                    )
                })
                .collect(),
        )
        .gap(ITEM_GAP),
    );

    vec![LayoutNode::row(groups).gap(GROUP_GAP).caption("Switches")]
}

fn checkbox_gallery() -> Vec<LayoutNode> {
    let variants = [
        ("unchecked", false, false, true),
        ("checked", true, false, true),
        ("indeterminate", false, true, true),
        ("disabled-unchecked", false, false, false),
        ("disabled-checked", true, false, false),
    ];

    vec![LayoutNode::row(
        variants
            .into_iter()
            .map(|(label, checked, indeterminate, enabled)| {
                leaf(
                    checkbox::SIZE,
                    checkbox::SIZE,
                    CheckBox::new(checked)
                        .indeterminate(indeterminate)
                        .enabled(enabled)
                        .on_change(move |v| println!("{label}: {v}")),
                )
            })
            .collect(),
    )
    .gap(ITEM_GAP)
    .caption("Checkboxes")]
}

fn radio_gallery() -> Vec<LayoutNode> {
    // One radio group: only "Option A" starts selected. Real deselection of
    // the previously selected sibling when another option is picked (via
    // `RadioButton::set_selected`) is the app's job, not wired up here.
    let variants = [
        ("Option A", true, true),
        ("Option B", false, true),
        ("Option C", false, true),
        ("Disabled", false, false),
    ];

    vec![LayoutNode::row(
        variants
            .into_iter()
            .map(|(label, selected, enabled)| {
                leaf(
                    radio_button::SIZE,
                    radio_button::SIZE,
                    RadioButton::new(selected)
                        .enabled(enabled)
                        .on_change(move |v| println!("{label}: {v}")),
                )
            })
            .collect(),
    )
    .gap(ITEM_GAP)
    .caption("Radio buttons")]
}

fn icon_button_gallery() -> Vec<LayoutNode> {
    let icons = [
        ("favorite", Icon::Favorite),
        ("add", Icon::Add),
        ("settings", Icon::Settings),
        ("menu", Icon::Menu),
    ];

    let variants = [
        ("Standard", IconButtonVariant::Standard),
        ("Filled", IconButtonVariant::Filled),
        ("Tonal", IconButtonVariant::FilledTonal),
        ("Outlined", IconButtonVariant::Outlined),
    ];

    let mut groups: Vec<LayoutNode> = variants
        .into_iter()
        .map(|(variant_label, variant)| {
            LayoutNode::row(
                icons
                    .into_iter()
                    .map(|(icon_label, icon)| {
                        leaf(
                            icon_button::SIZE,
                            icon_button::SIZE,
                            IconButton::new()
                                .variant(variant)
                                .icon(icon)
                                .on_click(move || {
                                    println!("clicked: {variant_label} {icon_label}")
                                }),
                        )
                    })
                    .collect(),
            )
            .gap(ITEM_GAP)
        })
        .collect();

    // Disabled and toggle examples share a final group.
    groups.push(
        row![
            leaf(
                icon_button::SIZE,
                icon_button::SIZE,
                IconButton::new()
                    .variant(IconButtonVariant::Filled)
                    .icon(Icon::Close)
                    .enabled(false),
            ),
            leaf(
                icon_button::SIZE,
                icon_button::SIZE,
                IconButton::new()
                    .variant(IconButtonVariant::Standard)
                    .icon(Icon::Favorite)
                    .toggle(true)
                    .selected(true)
                    .on_change(|v| println!("toggle favorite: {v}")),
            ),
        ]
        .gap(ITEM_GAP),
    );

    vec![LayoutNode::row(groups)
        .gap(GROUP_GAP)
        .caption("Icon buttons")]
}

fn list_item_gallery() -> Vec<LayoutNode> {
    vec![column![
        leaf(320.0, 56.0, ListItem::new("Headline only")),
        leaf(
            320.0,
            72.0,
            ListItem::new("Headline").supporting("Supporting text"),
        ),
        leaf(
            320.0,
            72.0,
            ListItem::new("Headline")
                .supporting("Supporting text")
                .trailing_text("12:34"),
        ),
        leaf(
            320.0,
            56.0,
            ListItem::new("Tap me").on_click(|| println!("list item clicked")),
        ),
        leaf(
            320.0,
            72.0,
            ListItem::new("Favorite item")
                .supporting("With leading/trailing icons")
                .leading(|canvas, rect, color| Icon::Favorite.draw(canvas, rect, color))
                .trailing(|canvas, rect, color| Icon::Close.draw(canvas, rect, color)),
        ),
    ]
    .gap(COLUMN_ITEM_GAP)
    .caption("List items")]
}

fn divider_gallery() -> Vec<LayoutNode> {
    vec![column![
        leaf(320.0, 16.0, Divider::new()),
        leaf(320.0, 16.0, Divider::new().leading_inset(16.0)),
    ]
    .gap(COLUMN_ITEM_GAP)
    .caption("Dividers")]
}

fn navigation_rail_gallery() -> Vec<LayoutNode> {
    const RAIL_HEIGHT: f32 = 400.0;

    // Minimal rail: unlabeled destinations, bottom-aligned.
    let mut minimal = NavigationRail::new()
        .alignment(NavigationRailAlignment::Bottom)
        .item(NavigationRailItem::new(Icon::Favorite))
        .item(NavigationRailItem::new(Icon::Settings))
        .on_change(|i| println!("nav rail (minimal): {i}"));
    minimal.set_selected(1);

    vec![row![
        // Menu button plus three labeled, top-aligned destinations, one
        // disabled. Given the expanded width up front so clicking the menu
        // button has room to actually widen into — the rail clamps itself to
        // its assigned slot.
        leaf(
            navigation_rail::EXPANDED_WIDTH,
            RAIL_HEIGHT,
            NavigationRail::new()
                .menu_icon(Icon::Menu)
                .on_menu_click(|| println!("nav rail: menu toggled"))
                .item(NavigationRailItem::new(Icon::Favorite).label("Home"))
                .item(NavigationRailItem::new(Icon::Settings).label("Settings"))
                .item(
                    NavigationRailItem::new(Icon::More)
                        .label("More")
                        .enabled(false),
                )
                .on_change(|i| println!("nav rail: {i}")),
        ),
        leaf(navigation_rail::WIDTH, RAIL_HEIGHT, minimal),
    ]
    .gap(ITEM_GAP)
    .caption("Navigation rail")]
}

/// Builds every gallery widget positioned by a taffy layout tree instead of
/// hand-tuned pixel constants, and the section captions that go with it.
/// Widgets are returned paired with the taffy node driving their layout, so a
/// resize can push fresh geometry into existing widget state (text, value,
/// focus, callbacks, animations, ...) instead of rebuilding it.
fn build_gallery() -> (
    Vec<PendingWidget>,
    Vec<Caption>,
    HashMap<NodeId, LayoutRect>,
    TaffyTree,
    NodeId,
) {
    let sections: Vec<LayoutNode> = [
        button_gallery(),
        segmented_gallery(),
        layout_gallery(),
        slider_gallery(),
        text_field_gallery(),
        switch_gallery(),
        checkbox_gallery(),
        radio_gallery(),
        icon_button_gallery(),
        list_item_gallery(),
        divider_gallery(),
        navigation_rail_gallery(),
    ]
    .into_iter()
    .flatten()
    .collect();

    let (widgets, captions, mut tree, root) = build(
        LayoutNode::column(sections)
            .gap(SECTION_GAP)
            .padding(root_padding()),
    );

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();

    let mut rects = HashMap::new();
    resolve_layout_rects(&tree, root, (0.0, 0.0), &mut rects);

    (widgets, captions, rects, tree, root)
}

struct App {
    theme: ColorTheme,
    fonts: FontBook,
    /// The scrollable viewport owning every gallery widget as a child.
    scroll: ScrollableWidget,
    /// Taffy node for each child, in the same order as `scroll.children()`,
    /// so a relayout can push fresh geometry into each widget.
    child_nodes: Vec<NodeId>,
    /// Section captions, paired with the taffy node their position is
    /// re-derived from on every relayout.
    captions: Vec<Caption>,
    /// Every node's taffy-resolved rect, refreshed by `relayout` after
    /// initial construction and after every resize.
    layout_rects: HashMap<NodeId, LayoutRect>,
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    cursor: pointer::Point,
    modifiers: keyboard::Modifiers,
    focus: Option<usize>,
    /// `None` if the platform has no clipboard we can reach; paste is then a
    /// no-op rather than a hard failure.
    clipboard: Option<arboard::Clipboard>,
    tree: TaffyTree,
    /// root node of tree
    root: NodeId,
}

impl App {
    fn new(theme: ColorTheme, fonts: FontBook) -> Self {
        let (widgets, captions, layout_rects, tree, root) = build_gallery();
        let mut scroll = ScrollableWidget::new();
        let mut child_nodes = Vec::with_capacity(widgets.len());
        for pending in widgets {
            child_nodes.push(pending.node);
            scroll.push(pending.widget);
        }
        Self {
            theme,
            fonts,
            scroll,
            child_nodes,
            captions,
            layout_rects,
            window: None,
            surface: None,
            cursor: (0.0, 0.0),
            modifiers: keyboard::Modifiers::default(),
            focus: None,
            clipboard: arboard::Clipboard::new()
                .map_err(|e| eprintln!("m3_test: clipboard unavailable, paste disabled: {e}"))
                .ok(),
            tree,
            root,
        }
    }

    /// Resolves every node's current rect from the taffy tree, sets the
    /// scroll viewport/content height, and pushes fresh geometry into each
    /// child via `set_layout_rect`, without touching any other widget state
    /// (text, value, focus, callbacks, animations, ...). Also rebuilds the
    /// caption overlay so captions keep tracking their section's top edge.
    fn relayout(&mut self, width: f32, height: f32) {
        let mut rects = HashMap::new();
        resolve_layout_rects(&self.tree, self.root, (0.0, 0.0), &mut rects);

        self.scroll.set_layout_rect(LayoutRect::new(
            0.0,
            HEADER_HEIGHT,
            width,
            (height - HEADER_HEIGHT).max(0.0),
        ));
        let content_h = self.tree.layout(self.root).unwrap().size.height;
        self.scroll.set_content_height(content_h);

        for (node, child) in self.child_nodes.iter().zip(self.scroll.children_mut()) {
            if let Some(rect) = rects.get(node) {
                child.set_layout_rect(*rect);
            }
        }
        self.layout_rects = rects;

        self.update_caption_overlay();
    }

    /// Rebuilds the scroll widget's content overlay from the current
    /// `layout_rects`, so section captions are drawn (and therefore scroll
    /// and clip) along with the content.
    fn update_caption_overlay(&mut self) {
        let caption_data: Vec<(&'static str, f32)> = self
            .captions
            .iter()
            .filter_map(|c| {
                self.layout_rects
                    .get(&c.node)
                    .map(|r| (c.label, r.y - CAPTION_OFFSET))
            })
            .collect();
        self.scroll
            .set_content_overlay(move |canvas, theme, fonts| {
                let mut paint = Paint::default();
                paint.set_color4f(Color4f::from(theme.on_surface), None);
                let font = fonts.sized("noto_sans", 13.0);
                for (label, y) in &caption_data {
                    canvas.draw_str(label, Point::new(ROOT_PADDING_X, *y), &font, &paint);
                }
            });
    }

    fn paste(&mut self) {
        let Some(clipboard) = self.clipboard.as_mut() else {
            return;
        };
        match clipboard.get_text() {
            Ok(text) => {
                if let Some(text) = keyboard::insertable_text(&text) {
                    self.dispatch_keyboard(KeyboardEventKind::Commit(text));
                }
            }
            Err(e) => eprintln!("m3_test: clipboard read failed: {e}"),
        }
    }

    fn dispatch_pointer(&mut self, kind: PointerEventKind) {
        let event = PointerEvent::new(self.cursor, kind);
        let mut redraw = self.scroll.on_pointer(&event);
        if let PointerEventKind::Press { button } = kind {
            if button == pointer::button::LEFT {
                redraw |= self.update_focus_from_click();
            }
        }
        if redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Topmost (last-drawn) focusable widget under the cursor gets focus;
    /// clicking empty space or a non-focusable widget clears it. Hit-testing
    /// happens in content space, and only when the cursor is inside the
    /// scroll viewport.
    fn update_focus_from_click(&mut self) -> bool {
        let hit = if self.scroll.viewport_contains(self.cursor) {
            let cp = self.scroll.content_point(self.cursor);
            self.scroll
                .children()
                .iter()
                .enumerate()
                .rev()
                .find(|(_, w)| w.focusable() && w.hit_rect().contains(cp.0 as f32, cp.1 as f32))
                .map(|(i, _)| i)
        } else {
            None
        };
        self.set_focus(hit)
    }

    fn set_focus(&mut self, new_focus: Option<usize>) -> bool {
        if new_focus == self.focus {
            return false;
        }
        if let Some(old) = self.focus.and_then(|i| self.scroll.child_mut(i)) {
            old.set_focused(false);
            old.on_keyboard(&KeyboardEvent::new(KeyboardEventKind::Blur, self.modifiers));
        }
        self.focus = new_focus;
        if let Some(new) = self.focus.and_then(|i| self.scroll.child_mut(i)) {
            new.set_focused(true);
            new.on_keyboard(&KeyboardEvent::new(
                KeyboardEventKind::Focus,
                self.modifiers,
            ));
        }
        true
    }

    /// Delivers only to the focused widget — keyboard events have no
    /// coordinates, so broadcasting would type into every field at once.
    fn dispatch_keyboard(&mut self, kind: KeyboardEventKind) {
        let Some(idx) = self.focus else {
            return;
        };
        let event = KeyboardEvent::new(kind, self.modifiers);
        let redraw = self
            .scroll
            .child_mut(idx)
            .map(|widget| widget.on_keyboard(&event))
            .unwrap_or(false);
        if redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Tab / Shift+Tab move focus to the next/previous focusable widget,
    /// wrapping around. Does nothing if there are no focusable widgets.
    fn cycle_focus(&mut self, backward: bool) {
        let count = self.scroll.children().len();
        if count == 0 {
            return;
        }
        let start = self.focus.map(|i| i as isize).unwrap_or(-1);
        let mut i = start;
        for _ in 0..count {
            i = if backward {
                (i - 1).rem_euclid(count as isize)
            } else {
                (i + 1).rem_euclid(count as isize)
            };
            if self.scroll.children()[i as usize].focusable() {
                self.set_focus(Some(i as usize));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
        }
        // No focusable widget found; clear focus if one was set.
        if self.set_focus(None) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn layout_for_viewport(&mut self, width: f32, height: f32) {
        self.tree
            .compute_layout(
                self.root,
                Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::MaxContent,
                },
            )
            .expect("Failed to recalculate layout");

        self.relayout(width, height);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Rc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("m3_test")
                        .with_inner_size(LogicalSize::new(800.0_f64, 780.0_f64)),
                )
                .expect("failed to create window"),
        );
        let context = Context::new(window.clone()).expect("failed to create softbuffer context");
        let surface =
            Surface::new(&context, window.clone()).expect("failed to create softbuffer surface");

        self.layout_for_viewport(
            window.inner_size().width as f32,
            window.inner_size().height as f32,
        );

        window.request_redraw();
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                self.layout_for_viewport(physical_size.width as f32, physical_size.height as f32);

                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::CursorEntered { .. } => self.dispatch_pointer(PointerEventKind::Enter),
            WindowEvent::CursorLeft { .. } => self.dispatch_pointer(PointerEventKind::Leave),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.dispatch_pointer(PointerEventKind::Motion);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = button_code(button) {
                    let kind = match state {
                        ElementState::Pressed => PointerEventKind::Press { button },
                        ElementState::Released => PointerEventKind::Release { button },
                    };
                    self.dispatch_pointer(kind);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (horizontal, vertical) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        AxisScroll {
                            discrete: x as i32,
                            absolute: x as f64,
                            stop: false,
                        },
                        AxisScroll {
                            discrete: y as i32,
                            absolute: y as f64,
                            stop: false,
                        },
                    ),
                    MouseScrollDelta::PixelDelta(pos) => (
                        AxisScroll {
                            absolute: pos.x,
                            discrete: 0,
                            stop: false,
                        },
                        AxisScroll {
                            absolute: pos.y,
                            discrete: 0,
                            stop: false,
                        },
                    ),
                };
                self.dispatch_pointer(PointerEventKind::Axis {
                    horizontal,
                    vertical,
                });
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = keyboard::Modifiers {
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    shift: state.shift_key(),
                    logo: state.super_key(),
                    ..Default::default()
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let is_tab_press =
                    pressed && matches!(&event.logical_key, Key::Named(NamedKey::Tab));
                let is_paste = pressed
                    && self.modifiers.ctrl
                    && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v"));

                if is_tab_press {
                    self.cycle_focus(self.modifiers.shift);
                } else if is_paste {
                    self.paste();
                } else {
                    if let Key::Named(named) = &event.logical_key {
                        if let Some(keysym) = keysym_from_named(named) {
                            let kind = match event.state {
                                ElementState::Pressed => KeyboardEventKind::Press {
                                    keysym,
                                    repeat: event.repeat,
                                },
                                ElementState::Released => KeyboardEventKind::Release { keysym },
                            };
                            self.dispatch_keyboard(kind);
                        }
                    }
                    // `KeyEvent::text` is not affected by Ctrl — winit hands
                    // back "v" for Ctrl+V — so without this a shortcut would
                    // type its own letter. Shift and Caps Lock are what produce
                    // the character in the first place, so they must not count.
                    let shortcut = self.modifiers.ctrl || self.modifiers.alt || self.modifiers.logo;
                    if pressed && !shortcut {
                        if let Some(text) =
                            event.text.as_deref().and_then(keyboard::insertable_text)
                        {
                            self.dispatch_keyboard(KeyboardEventKind::Commit(text));
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut())
                else {
                    return;
                };

                let size = window.inner_size();
                let (w, h) = (size.width, size.height);
                if w == 0 || h == 0 {
                    return;
                }

                surface
                    .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
                    .expect("failed to resize surface");

                let mut buffer = surface.buffer_mut().expect("failed to get buffer");

                let bytes: &mut [u8] = unsafe {
                    std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 4)
                };

                let info = ImageInfo::new_n32_premul((w as i32, h as i32), None);
                let stride = (w * 4) as usize;
                let mut skia_surface = surfaces::wrap_pixels(&info, bytes, stride, None).unwrap();
                let canvas = skia_surface.canvas();

                canvas.clear(Color4f::from(self.theme.surface));

                let redraw = self.scroll.draw(canvas, &self.theme, &self.fonts);

                let mut text_paint = Paint::default();
                text_paint.set_color4f(Color4f::from(self.theme.on_surface), None);
                let font = self.fonts.sized("noto_sans", 24.0);
                canvas.draw_str(
                    "m3_test — ui_core smoke render",
                    Point::new(24.0, 40.0),
                    &font,
                    &text_paint,
                );
                canvas.draw_str(
                    "フォント描画テスト",
                    Point::new(24.0, 68.0),
                    &font,
                    &text_paint,
                );

                if redraw {
                    window.request_redraw();
                }

                // drop skia surface to release the bytes borrow before presenting
                drop(skia_surface);

                buffer.present().expect("failed to present buffer");
            }
            _ => {}
        }
    }
}

fn main() {
    let theme_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "example/theme.toml".to_string());
    let theme = load_theme(&theme_path);

    let mut fonts = FontBook::new();
    fonts.register(
        "noto_sans",
        "Noto Sans CJK JP",
        skia_safe::FontStyle::normal(),
    );

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(theme, fonts);
    event_loop.run_app(&mut app).unwrap();
}
