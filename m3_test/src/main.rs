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

use m3_widget::{
    checkbox, icon_button, radio_button, switch, text_field, Button, ButtonSize, ButtonVariant,
    CheckBox, Divider, Icon, IconButton, IconButtonVariant, ListItem, RadioButton, Slider,
    SliderSize, Switch, SwitchIcons, TextField, Widget,
};
use util::{
    button_code, column_style, item_style, keysym_from_named, load_theme, resolve_layout_rects,
    row_style, PendingWidget, Section,
};

/// Left/right/top/bottom breathing room around the whole gallery. Top clears
/// the two header lines drawn separately in `RedrawRequested`.
const ROOT_PADDING_X: f32 = 24.0;
const ROOT_PADDING_TOP: f32 = 90.0;
const ROOT_PADDING_BOTTOM: f32 = 24.0;
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

fn root_style() -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        padding: Rect {
            left: length(ROOT_PADDING_X),
            right: length(ROOT_PADDING_X),
            top: length(ROOT_PADDING_TOP),
            bottom: length(ROOT_PADDING_BOTTOM),
        },
        gap: Size {
            width: length(0.0),
            height: length(SECTION_GAP),
        },
        ..Default::default()
    }
}

fn button_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
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

    let mut sections = Vec::new();
    let mut pending = Vec::new();

    let all = [
        ("Variants", variants),
        ("Sizes", sizes),
        ("Disabled", disabled),
    ];

    for (title, variant) in all {
        let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
        for (label, size, variant, enabled, width) in variant {
            let leaf = tree.new_leaf(item_style(width, size.height())).unwrap();
            tree.add_child(row, leaf).unwrap();
            pending.push(PendingWidget {
                node: leaf,
                build: Box::new(move |rect| {
                    let mut widget = Button::new(label)
                        .size(size)
                        .variant(variant)
                        .enabled(enabled)
                        .on_click(move || println!("clicked: {label}"));
                    widget.set_layout_rect(rect);
                    Box::new(widget)
                }),
            });
        }
        sections.push(Section {
            caption: title,
            node: row,
        });
    }

    (sections, pending)
}

fn slider_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut sections = Vec::new();
    let mut pending = Vec::new();

    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    let types = [(true, 0.), (true, 10.), (false, 0.)];

    for (enabled, step) in types {
        let leaf = tree
            .new_leaf(item_style(200.0, SliderSize::default().handle_height()))
            .unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |rect| {
                let mut widget = Slider::new(0.0, 100.0, 40.0)
                    .labeled(true)
                    .enabled(enabled)
                    .step(step)
                    .on_change(|v| println!("continuous: {v:.1}"));
                widget.set_layout_rect(rect);
                Box::new(widget)
            }),
        });
    }

    sections.push(Section {
        caption: "Sliders",
        node: row,
    });

    let sizes = [
        SliderSize::ExtraSmall,
        SliderSize::Small,
        SliderSize::Medium,
    ];
    let column = tree.new_leaf(column_style(COLUMN_ITEM_GAP)).unwrap();
    for size in sizes {
        let leaf = tree
            .new_leaf(item_style(280.0, size.handle_height()))
            .unwrap();
        tree.add_child(column, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |rect| {
                let mut widget = Slider::new(0.0, 100.0, 50.0).size(size);
                widget.set_layout_rect(rect);
                Box::new(widget)
            }),
        });
    }
    sections.push(Section {
        caption: "Slider sizes",
        node: column,
    });

    (sections, pending)
}

fn text_field_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut pending = Vec::new();
    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = TextField::new().on_change(|text| println!("text: {text}"));
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = TextField::new()
                .text("フォント入力テスト")
                .on_change(|text| println!("text: {text}"));
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = TextField::new().text("disabled").enabled(false);
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    (
        vec![Section {
            caption: "Text fields",
            node: row,
        }],
        pending,
    )
}

fn switch_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let variants = [
        ("plain", SwitchIcons::None),
        ("selected-icon", SwitchIcons::Selected),
        ("both-icons", SwitchIcons::Both),
    ];

    let mut pending = Vec::new();
    let outer = tree.new_leaf(row_style(GROUP_GAP)).unwrap();

    for (label, icons) in variants {
        let group = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
        tree.add_child(outer, group).unwrap();
        for checked in [false, true] {
            let leaf = tree
                .new_leaf(item_style(switch::TRACK_WIDTH, switch::TRACK_HEIGHT))
                .unwrap();
            tree.add_child(group, leaf).unwrap();
            pending.push(PendingWidget {
                node: leaf,
                build: Box::new(move |rect| {
                    let mut widget = Switch::new(checked)
                        .icons(icons)
                        .on_change(move |v| println!("{label}: {v}"));
                    widget.set_layout_rect(rect);
                    Box::new(widget)
                }),
            });
        }
    }

    let disabled_group = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
    tree.add_child(outer, disabled_group).unwrap();
    for checked in [false, true] {
        let leaf = tree
            .new_leaf(item_style(switch::TRACK_WIDTH, switch::TRACK_HEIGHT))
            .unwrap();
        tree.add_child(disabled_group, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |rect| {
                let mut widget = Switch::new(checked).icons(SwitchIcons::Both).enabled(false);
                widget.set_layout_rect(rect);
                Box::new(widget)
            }),
        });
    }

    (
        vec![Section {
            caption: "Switches",
            node: outer,
        }],
        pending,
    )
}

fn checkbox_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let variants = [
        ("unchecked", false, false, true),
        ("checked", true, false, true),
        ("indeterminate", false, true, true),
        ("disabled-unchecked", false, false, false),
        ("disabled-checked", true, false, false),
    ];

    let mut pending = Vec::new();
    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    for (label, checked, indeterminate, enabled) in variants {
        let leaf = tree
            .new_leaf(item_style(checkbox::SIZE, checkbox::SIZE))
            .unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |rect| {
                let mut widget = CheckBox::new(checked)
                    .indeterminate(indeterminate)
                    .enabled(enabled)
                    .on_change(move |v| println!("{label}: {v}"));
                widget.set_layout_rect(rect);
                Box::new(widget)
            }),
        });
    }

    (
        vec![Section {
            caption: "Checkboxes",
            node: row,
        }],
        pending,
    )
}

fn radio_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    // One radio group: only "Option A" starts selected. Real deselection of
    // the previously selected sibling when another option is picked (via
    // `RadioButton::set_selected`) is the app's job, not wired up here.
    let variants = [
        ("Option A", true, true),
        ("Option B", false, true),
        ("Option C", false, true),
        ("Disabled", false, false),
    ];

    let mut pending = Vec::new();
    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    for (label, selected, enabled) in variants {
        let leaf = tree
            .new_leaf(item_style(radio_button::SIZE, radio_button::SIZE))
            .unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |rect| {
                let mut widget = RadioButton::new(selected)
                    .enabled(enabled)
                    .on_change(move |v| println!("{label}: {v}"));
                widget.set_layout_rect(rect);
                Box::new(widget)
            }),
        });
    }

    (
        vec![Section {
            caption: "Radio buttons",
            node: row,
        }],
        pending,
    )
}

fn icon_button_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
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

    let mut pending = Vec::new();
    let outer = tree.new_leaf(row_style(GROUP_GAP)).unwrap();

    for (variant_label, variant) in variants {
        let group = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
        tree.add_child(outer, group).unwrap();
        for (icon_label, icon) in icons {
            let leaf = tree
                .new_leaf(item_style(icon_button::SIZE, icon_button::SIZE))
                .unwrap();
            tree.add_child(group, leaf).unwrap();
            pending.push(PendingWidget {
                node: leaf,
                build: Box::new(move |rect| {
                    let mut widget = IconButton::new()
                        .variant(variant)
                        .icon(icon)
                        .on_click(move || println!("clicked: {variant_label} {icon_label}"));
                    widget.set_layout_rect(rect);
                    Box::new(widget)
                }),
            });
        }
    }

    // Disabled and toggle examples share a final group.
    let extra_group = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
    tree.add_child(outer, extra_group).unwrap();

    let leaf = tree
        .new_leaf(item_style(icon_button::SIZE, icon_button::SIZE))
        .unwrap();
    tree.add_child(extra_group, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = IconButton::new()
                .variant(IconButtonVariant::Filled)
                .icon(Icon::Close)
                .enabled(false);
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree
        .new_leaf(item_style(icon_button::SIZE, icon_button::SIZE))
        .unwrap();
    tree.add_child(extra_group, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = IconButton::new()
                .variant(IconButtonVariant::Standard)
                .icon(Icon::Favorite)
                .toggle(true)
                .selected(true)
                .on_change(|v| println!("toggle favorite: {v}"));
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    (
        vec![Section {
            caption: "Icon buttons",
            node: outer,
        }],
        pending,
    )
}

fn list_item_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut pending = Vec::new();
    let column = tree.new_leaf(column_style(COLUMN_ITEM_GAP)).unwrap();

    let leaf = tree.new_leaf(item_style(320.0, 56.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = ListItem::new("Headline only");
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree.new_leaf(item_style(320.0, 72.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = ListItem::new("Headline").supporting("Supporting text");
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree.new_leaf(item_style(320.0, 72.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = ListItem::new("Headline")
                .supporting("Supporting text")
                .trailing_text("12:34");
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree.new_leaf(item_style(320.0, 56.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = ListItem::new("Tap me").on_click(|| println!("list item clicked"));
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree.new_leaf(item_style(320.0, 72.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = ListItem::new("Favorite item")
                .supporting("With leading/trailing icons")
                .leading(|canvas, rect, color| Icon::Favorite.draw(canvas, rect, color))
                .trailing(|canvas, rect, color| Icon::Close.draw(canvas, rect, color));
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    (
        vec![Section {
            caption: "List items",
            node: column,
        }],
        pending,
    )
}

fn divider_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut pending = Vec::new();
    let column = tree.new_leaf(column_style(COLUMN_ITEM_GAP)).unwrap();

    let leaf = tree.new_leaf(item_style(320.0, 16.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = Divider::new();
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    let leaf = tree.new_leaf(item_style(320.0, 16.0)).unwrap();
    tree.add_child(column, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |rect| {
            let mut widget = Divider::new().leading_inset(16.0);
            widget.set_layout_rect(rect);
            Box::new(widget)
        }),
    });

    (
        vec![Section {
            caption: "Dividers",
            node: column,
        }],
        pending,
    )
}

/// A widget paired with the taffy node driving its layout, so a resize can
/// push fresh geometry into existing widget state (text, value, focus,
/// callbacks, animations, ...) instead of rebuilding it.
struct PositionedWidget {
    node: NodeId,
    widget: Box<dyn Widget>,
}

/// A section caption; its screen position is re-derived from its node's
/// current layout on every relayout, including resize.
struct Caption {
    label: &'static str,
    node: NodeId,
}

/// Builds every gallery widget positioned by a taffy layout tree instead of
/// hand-tuned pixel constants, and the section captions that go with it.
fn build_gallery() -> (
    Vec<PositionedWidget>,
    Vec<Caption>,
    HashMap<NodeId, LayoutRect>,
    TaffyTree,
    NodeId,
) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let root = tree.new_leaf(root_style()).unwrap();

    let mut sections = Vec::new();
    let mut pending = Vec::new();

    for (s, p) in [
        button_gallery(&mut tree),
        slider_gallery(&mut tree),
        text_field_gallery(&mut tree),
        switch_gallery(&mut tree),
        checkbox_gallery(&mut tree),
        radio_gallery(&mut tree),
        icon_button_gallery(&mut tree),
        list_item_gallery(&mut tree),
        divider_gallery(&mut tree),
    ] {
        sections.extend(s);
        pending.extend(p);
    }

    for section in &sections {
        tree.add_child(root, section.node).unwrap();
    }

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

    let widgets = pending
        .into_iter()
        .map(|p| {
            let rect = rects[&p.node];
            PositionedWidget {
                node: p.node,
                widget: (p.build)(rect),
            }
        })
        .collect();

    let captions = sections
        .into_iter()
        .map(|s| Caption {
            label: s.caption,
            node: s.node,
        })
        .collect();

    (widgets, captions, rects, tree, root)
}

struct App {
    theme: ColorTheme,
    fonts: FontBook,
    widgets: Vec<PositionedWidget>,
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
        Self {
            theme,
            fonts,
            widgets,
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

    /// Resolves every node's current rect from the taffy tree and pushes it
    /// into its widget via `set_layout_rect`, without touching any other
    /// widget state (text, value, focus, callbacks, animations, ...).
    /// Captions read their position out of `layout_rects` lazily at draw
    /// time, so refreshing it here is enough to move them too.
    fn relayout(&mut self) {
        let mut rects = HashMap::new();
        resolve_layout_rects(&self.tree, self.root, (0.0, 0.0), &mut rects);
        for positioned in &mut self.widgets {
            if let Some(rect) = rects.get(&positioned.node) {
                positioned.widget.set_layout_rect(*rect);
            }
        }
        self.layout_rects = rects;
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
        let mut redraw = false;
        for positioned in &mut self.widgets {
            redraw |= positioned.widget.on_pointer(&event);
        }
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
    /// clicking empty space or a non-focusable widget clears it.
    fn update_focus_from_click(&mut self) -> bool {
        let hit = self
            .widgets
            .iter()
            .enumerate()
            .rev()
            .find(|(_, positioned)| {
                let widget = &positioned.widget;
                widget.focusable()
                    && widget
                        .hit_rect()
                        .contains(self.cursor.0 as f32, self.cursor.1 as f32)
            })
            .map(|(i, _)| i);
        self.set_focus(hit)
    }

    fn set_focus(&mut self, new_focus: Option<usize>) -> bool {
        if new_focus == self.focus {
            return false;
        }
        if let Some(old) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
            old.widget.set_focused(false);
            old.widget
                .on_keyboard(&KeyboardEvent::new(KeyboardEventKind::Blur, self.modifiers));
        }
        self.focus = new_focus;
        if let Some(new) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
            new.widget.set_focused(true);
            new.widget.on_keyboard(&KeyboardEvent::new(
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
            .widgets
            .get_mut(idx)
            .map(|positioned| positioned.widget.on_keyboard(&event))
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
        let count = self.widgets.len();
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
            if self.widgets[i as usize].widget.focusable() {
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
                    height: AvailableSpace::Definite(height),
                },
            )
            .expect("Failed to recalculate layout");

        self.relayout();
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

                let caption = self.fonts.sized("noto_sans", 13.0);
                for entry in &self.captions {
                    let Some(rect) = self.layout_rects.get(&entry.node) else {
                        continue;
                    };
                    canvas.draw_str(
                        entry.label,
                        Point::new(ROOT_PADDING_X, rect.y - CAPTION_OFFSET),
                        &caption,
                        &text_paint,
                    );
                }

                let mut redraw = false;

                // draw widgets
                for positioned in &mut self.widgets {
                    redraw |= positioned.widget.draw(canvas, &self.theme, &self.fonts);
                }

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
