use serde::Deserialize;
use ui_core::keyboard;
use ui_core::pointer;
use ui_core::scheme::ColorTheme;
use winit::event::MouseButton;
use winit::keyboard::NamedKey;

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
