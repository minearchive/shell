use std::collections::HashMap;

use serde::Deserialize;
use taffy::prelude::*;
use ui_core::geometry::LayoutRect;
use ui_core::keyboard;
use ui_core::pointer;
use ui_core::scheme::ColorTheme;
use winit::event::MouseButton;
use winit::keyboard::NamedKey;

use m3_widget::Widget;

pub fn button_code(button: MouseButton) -> Option<u32> {
    match button {
        MouseButton::Left => Some(pointer::button::LEFT),
        MouseButton::Right => Some(pointer::button::RIGHT),
        MouseButton::Middle => Some(pointer::button::MIDDLE),
        _ => None,
    }
}

/// Editing keys only — everything else (letters, digits, ...) arrives as text
/// via `KeyEvent::text` and is routed through `Commit` instead.
pub fn keysym_from_named(key: &NamedKey) -> Option<u32> {
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

pub fn load_theme(path: &str) -> ColorTheme {
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

/// A gallery row/column, captioned just above its resolved top edge.
pub struct Section {
    pub caption: &'static str,
    pub node: NodeId,
}

/// A widget whose exact screen rect is only known once taffy has resolved
/// its node's layout; `build` finishes construction with that rect.
pub struct PendingWidget {
    pub node: NodeId,
    pub build: Box<dyn FnOnce(LayoutRect) -> Box<dyn Widget>>,
}

pub fn row_style(gap: f32) -> Style {
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

pub fn column_style(gap: f32) -> Style {
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

pub fn item_style(width: f32, height: f32) -> Style {
    Style {
        size: Size {
            width: length(width),
            height: length(height),
        },
        ..Default::default()
    }
}

/// Accumulates each node's absolute `LayoutRect` (position and size), since
/// `Layout::location` is relative to the immediate parent. This is the one
/// place taffy's geometry is converted into `ui_core`'s layout-agnostic
/// `LayoutRect` — `ui_core` and `m3_widget` know nothing about taffy.
pub fn resolve_layout_rects(
    tree: &TaffyTree<()>,
    node: NodeId,
    origin: (f32, f32),
    out: &mut HashMap<NodeId, LayoutRect>,
) {
    let layout = tree.layout(node).unwrap();
    let x = origin.0 + layout.location.x;
    let y = origin.1 + layout.location.y;
    out.insert(
        node,
        LayoutRect::new(x, y, layout.size.width, layout.size.height),
    );
    for child in tree.children(node).unwrap() {
        resolve_layout_rects(tree, child, (x, y), out);
    }
}
