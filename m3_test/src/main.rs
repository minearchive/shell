use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use serde::Deserialize;
use skia_safe::{surfaces, Color4f, Contains, ImageInfo, Paint, Point};
use softbuffer::{Context, Surface};
use taffy::prelude::*;
use ui_core::font::FontBook;
use ui_core::keyboard::{self, KeyboardEvent, KeyboardEventKind};
use ui_core::pointer::{self, AxisScroll, PointerEvent, PointerEventKind};
use ui_core::scheme::ColorTheme;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use m3_widget::{
    switch, text_field, Button, ButtonSize, ButtonVariant, Slider, SliderSize, Switch, SwitchIcons,
    TextField, Widget,
};

fn button_code(button: MouseButton) -> Option<u32> {
    match button {
        MouseButton::Left => Some(pointer::button::LEFT),
        MouseButton::Right => Some(pointer::button::RIGHT),
        MouseButton::Middle => Some(pointer::button::MIDDLE),
        _ => None,
    }
}

/// Editing keys only — everything else (letters, digits, ...) arrives as text
/// via `KeyEvent::text` and is routed through `Commit` instead.
fn keysym_from_named(key: &NamedKey) -> Option<u32> {
    match key {
        NamedKey::Backspace => Some(keyboard::key::BACKSPACE),
        NamedKey::Tab => Some(keyboard::key::TAB),
        NamedKey::Enter => Some(keyboard::key::RETURN),
        NamedKey::Escape => Some(keyboard::key::ESCAPE),
        NamedKey::Home => Some(keyboard::key::HOME),
        NamedKey::ArrowLeft => Some(keyboard::key::LEFT),
        NamedKey::ArrowUp => Some(keyboard::key::UP),
        NamedKey::ArrowRight => Some(keyboard::key::RIGHT),
        NamedKey::ArrowDown => Some(keyboard::key::DOWN),
        NamedKey::End => Some(keyboard::key::END),
        NamedKey::Delete => Some(keyboard::key::DELETE),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct ThemeFile {
    is_dark: bool,
    dark: ColorTheme,
    light: ColorTheme,
}

impl Default for ThemeFile {
    fn default() -> Self {
        Self {
            is_dark: true,
            dark: ColorTheme::default(),
            light: ColorTheme::default(),
        }
    }
}

fn load_theme(path: &str) -> ColorTheme {
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<ThemeFile>(&contents) {
            Ok(tf) => {
                if tf.is_dark {
                    tf.dark
                } else {
                    tf.light
                }
            }
            Err(e) => {
                eprintln!("m3_test: failed to parse {path}: {e}; using default theme");
                ColorTheme::default()
            }
        },
        Err(e) => {
            eprintln!("m3_test: failed to read {path}: {e}; using default theme");
            ColorTheme::default()
        }
    }
}

// ---- gallery layout (taffy) ----

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

/// A gallery row/column, captioned just above its resolved top edge.
struct Section {
    caption: &'static str,
    node: NodeId,
}

/// A widget whose exact screen position is only known once taffy has resolved
/// its node's layout; `build` finishes construction with that position.
struct PendingWidget {
    node: NodeId,
    build: Box<dyn FnOnce(f32, f32) -> Box<dyn Widget>>,
}

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

fn row_style(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::FLEX_START),
        gap: Size {
            width: length(gap),
            height: length(0.0),
        },
        ..Default::default()
    }
}

fn column_style(gap: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        gap: Size {
            width: length(0.0),
            height: length(gap),
        },
        ..Default::default()
    }
}

fn item_style(width: f32, height: f32) -> Style {
    Style {
        size: Size {
            width: length(width),
            height: length(height),
        },
        ..Default::default()
    }
}

/// One button per variant, then one per size, then the disabled treatments.
fn button_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let variants = [
        ("Filled", ButtonVariant::Filled, 100.0),
        ("Tonal", ButtonVariant::FilledTonal, 100.0),
        ("Elevated", ButtonVariant::Elevated, 110.0),
        ("Outlined", ButtonVariant::Outlined, 110.0),
        ("Text", ButtonVariant::Text, 90.0),
    ];

    let sizes = [
        ("XS", ButtonSize::ExtraSmall, 90.0),
        ("S", ButtonSize::Small, 90.0),
        ("M", ButtonSize::Medium, 100.0),
        ("L", ButtonSize::Large, 110.0),
        ("XL", ButtonSize::ExtraLarge, 120.0),
    ];

    let disabled = [
        ("Filled", ButtonVariant::Filled, 100.0),
        ("Tonal", ButtonVariant::FilledTonal, 100.0),
        ("Outlined", ButtonVariant::Outlined, 110.0),
        ("Text", ButtonVariant::Text, 90.0),
    ];

    let mut sections = Vec::new();
    let mut pending = Vec::new();

    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
    for (label, variant, width) in variants {
        let leaf = tree
            .new_leaf(item_style(width, ButtonSize::default().height()))
            .unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |x, y| {
                Box::new(
                    Button::new(label)
                        .variant(variant)
                        .position(x, y)
                        .width(width)
                        .on_click(move || println!("clicked: {label}")),
                )
            }),
        });
    }
    sections.push(Section {
        caption: "Variants",
        node: row,
    });

    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
    for (label, size, width) in sizes {
        let leaf = tree.new_leaf(item_style(width, size.height())).unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |x, y| {
                Box::new(
                    Button::new(label)
                        .size(size)
                        .position(x, y)
                        .width(width)
                        .on_click(move || println!("clicked: size {label}")),
                )
            }),
        });
    }
    sections.push(Section {
        caption: "Sizes",
        node: row,
    });

    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();
    for (label, variant, width) in disabled {
        let leaf = tree
            .new_leaf(item_style(width, ButtonSize::default().height()))
            .unwrap();
        tree.add_child(row, leaf).unwrap();
        pending.push(PendingWidget {
            node: leaf,
            build: Box::new(move |x, y| {
                Box::new(
                    Button::new(label)
                        .variant(variant)
                        .position(x, y)
                        .width(width)
                        .enabled(false),
                )
            }),
        });
    }
    sections.push(Section {
        caption: "Disabled",
        node: row,
    });

    (sections, pending)
}

/// Continuous, discrete and disabled sliders, then one per size.
fn slider_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut sections = Vec::new();
    let mut pending = Vec::new();

    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    let leaf = tree
        .new_leaf(item_style(200.0, SliderSize::default().handle_height()))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                Slider::new(0.0, 100.0, 40.0)
                    .position(x, y)
                    .width(200.0)
                    .labeled(true)
                    .on_change(|v| println!("continuous: {v:.1}")),
            )
        }),
    });

    let leaf = tree
        .new_leaf(item_style(200.0, SliderSize::default().handle_height()))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                Slider::new(0.0, 10.0, 3.0)
                    .step(1.0)
                    .position(x, y)
                    .width(200.0)
                    .labeled(true)
                    .on_change(|v| println!("discrete: {v}")),
            )
        }),
    });

    let leaf = tree
        .new_leaf(item_style(200.0, SliderSize::default().handle_height()))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                Slider::new(0.0, 100.0, 60.0)
                    .position(x, y)
                    .width(200.0)
                    .enabled(false),
            )
        }),
    });

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
            build: Box::new(move |x, y| {
                Box::new(
                    Slider::new(0.0, 100.0, 50.0)
                        .size(size)
                        .position(x, y)
                        .width(280.0),
                )
            }),
        });
    }
    sections.push(Section {
        caption: "Slider sizes",
        node: column,
    });

    (sections, pending)
}

/// One plain field, one pre-filled field, one disabled field.
fn text_field_gallery(tree: &mut TaffyTree<()>) -> (Vec<Section>, Vec<PendingWidget>) {
    let mut pending = Vec::new();
    let row = tree.new_leaf(row_style(ITEM_GAP)).unwrap();

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                TextField::new()
                    .position(x, y)
                    .width(220.0)
                    .on_change(|text| println!("text: {text}")),
            )
        }),
    });

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                TextField::new()
                    .position(x, y)
                    .width(220.0)
                    .text("フォント入力テスト")
                    .on_change(|text| println!("text: {text}")),
            )
        }),
    });

    let leaf = tree
        .new_leaf(item_style(220.0, text_field::HEIGHT))
        .unwrap();
    tree.add_child(row, leaf).unwrap();
    pending.push(PendingWidget {
        node: leaf,
        build: Box::new(move |x, y| {
            Box::new(
                TextField::new()
                    .position(x, y)
                    .width(220.0)
                    .text("disabled")
                    .enabled(false),
            )
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

/// The three icon treatments in both states, then the disabled pair.
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
                build: Box::new(move |x, y| {
                    Box::new(
                        Switch::new(checked)
                            .icons(icons)
                            .position(x, y)
                            .on_change(move |v| println!("{label}: {v}")),
                    )
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
            build: Box::new(move |x, y| {
                Box::new(
                    Switch::new(checked)
                        .icons(SwitchIcons::Both)
                        .position(x, y)
                        .enabled(false),
                )
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

/// Accumulates absolute screen coordinates for every node, since
/// `Layout::location` is relative to the immediate parent.
fn resolve_absolute(
    tree: &TaffyTree<()>,
    node: NodeId,
    origin: (f32, f32),
    out: &mut HashMap<NodeId, (f32, f32)>,
) {
    let location = tree.layout(node).unwrap().location;
    let absolute = (origin.0 + location.x, origin.1 + location.y);
    out.insert(node, absolute);
    for child in tree.children(node).unwrap() {
        resolve_absolute(tree, child, absolute, out);
    }
}

/// Builds every gallery widget positioned by a taffy layout tree instead of
/// hand-tuned pixel constants, and the section captions that go with it.
fn build_gallery() -> (Vec<Box<dyn Widget>>, Vec<(&'static str, f32)>) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let root = tree.new_leaf(root_style()).unwrap();

    let mut sections = Vec::new();
    let mut pending = Vec::new();

    let (s, p) = button_gallery(&mut tree);
    sections.extend(s);
    pending.extend(p);
    let (s, p) = slider_gallery(&mut tree);
    sections.extend(s);
    pending.extend(p);
    let (s, p) = text_field_gallery(&mut tree);
    sections.extend(s);
    pending.extend(p);
    let (s, p) = switch_gallery(&mut tree);
    sections.extend(s);
    pending.extend(p);

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

    let mut positions = HashMap::new();
    resolve_absolute(&tree, root, (0.0, 0.0), &mut positions);

    let widgets = pending
        .into_iter()
        .map(|p| {
            let (x, y) = positions[&p.node];
            (p.build)(x, y)
        })
        .collect();

    let captions = sections
        .into_iter()
        .map(|s| (s.caption, positions[&s.node].1))
        .collect();

    (widgets, captions)
}

// ---- winit app ----

struct App {
    theme: ColorTheme,
    fonts: FontBook,
    widgets: Vec<Box<dyn Widget>>,
    /// Section captions with their taffy-resolved top-of-row y.
    captions: Vec<(&'static str, f32)>,
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    cursor: pointer::Point,
    modifiers: keyboard::Modifiers,
    focus: Option<usize>,
    /// `None` if the platform has no clipboard we can reach; paste is then a
    /// no-op rather than a hard failure.
    clipboard: Option<arboard::Clipboard>,
}

impl App {
    fn new(theme: ColorTheme, fonts: FontBook) -> Self {
        let (widgets, captions) = build_gallery();
        Self {
            theme,
            fonts,
            widgets,
            captions,
            window: None,
            surface: None,
            cursor: (0.0, 0.0),
            modifiers: keyboard::Modifiers::default(),
            focus: None,
            clipboard: arboard::Clipboard::new()
                .map_err(|e| eprintln!("m3_test: clipboard unavailable, paste disabled: {e}"))
                .ok(),
        }
    }

    /// Paste routes through `Commit`, the one text-insertion path, so widgets
    /// never learn what a clipboard is.
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
        for widget in &mut self.widgets {
            redraw |= widget.on_pointer(&event);
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
        let point = Point::new(self.cursor.0 as f32, self.cursor.1 as f32);
        let hit = self
            .widgets
            .iter()
            .enumerate()
            .rev()
            .find(|(_, w)| w.focusable() && w.bounds().contains(point))
            .map(|(i, _)| i);
        self.set_focus(hit)
    }

    fn set_focus(&mut self, new_focus: Option<usize>) -> bool {
        if new_focus == self.focus {
            return false;
        }
        if let Some(old) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
            old.set_focused(false);
            old.on_keyboard(&KeyboardEvent::new(KeyboardEventKind::Blur, self.modifiers));
        }
        self.focus = new_focus;
        if let Some(new) = self.focus.and_then(|i| self.widgets.get_mut(i)) {
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
            .widgets
            .get_mut(idx)
            .map(|w| w.on_keyboard(&event))
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
            if self.widgets[i as usize].focusable() {
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
        window.request_redraw();
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
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
                for (label, row) in &self.captions {
                    canvas.draw_str(
                        *label,
                        Point::new(ROOT_PADDING_X, row - CAPTION_OFFSET),
                        &caption,
                        &text_paint,
                    );
                }

                let mut redraw = false;

                // draw widgets
                for widget in &mut self.widgets {
                    redraw |= widget.draw(canvas, &self.theme, &self.fonts);
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
