use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;

use calloop::channel::Sender;
use log::info;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use skia_safe::Color4f;

use crate::ui::UiEvent;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(try_from = "String")]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }
    }
}

impl TryFrom<String> for Color {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let hex = s.trim_start_matches('#');
        let parse = |slice: &str| {
            u8::from_str_radix(slice, 16)
                .map(|v| v as f32 / 255.0)
                .map_err(|e| e.to_string())
        };
        match hex.len() {
            6 => Ok(Color {
                r: parse(&hex[0..2])?,
                g: parse(&hex[2..4])?,
                b: parse(&hex[4..6])?,
                a: 1.0,
            }),
            8 => Ok(Color {
                r: parse(&hex[0..2])?,
                g: parse(&hex[2..4])?,
                b: parse(&hex[4..6])?,
                a: parse(&hex[6..8])?,
            }),
            _ => Err(format!("invalid color: {s}")),
        }
    }
}

impl From<Color> for Color4f {
    fn from(val: Color) -> Self {
        Color4f {
            r: val.r,
            g: val.g,
            b: val.b,
            a: val.a,
        }
    }
}

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Configuration {
    pub name: String,
    pub is_dark: bool,
    pub dark: ColorTheme,
    pub light: ColorTheme,
}

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ColorTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub inverse_primary: Color,
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,
    pub source_color: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub surface_dim: Color,
    pub surface: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub surface_variant: Color,
    pub background: Color,
    pub on_background: Color,
    pub shadow: Color,
    pub scrim: Color,
}

impl Configuration {
    fn try_load(path: &str) -> Option<Self> {
        let src = fs::read_to_string(path)
            .map_err(|e| info!("Config read failed: {e}"))
            .ok()?;
        toml::from_str(&src)
            .map_err(|e| info!("Config parse failed:\n{e}"))
            .ok()
    }

    pub fn load(path: &str) -> Self {
        Self::try_load(path).unwrap_or_default()
    }

    pub fn load_and_watch(path: &str, ui_tx: Sender<UiEvent>) -> Arc<RwLock<Self>> {
        let config = Arc::new(RwLock::new(Self::load(path)));
        let config_ref = Arc::clone(&config);
        let path = path.to_string();

        thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();
            watcher
                .watch(Path::new(&path), RecursiveMode::NonRecursive)
                .unwrap();

            for event in rx.into_iter().flatten() {
                if event.kind.is_modify() {
                    if let Some(new_cfg) = Self::try_load(&path) {
                        *config_ref.write().unwrap() = new_cfg;
                        info!("Config reloaded.");
                        let _ = ui_tx.send(UiEvent::RequestRedrawAll);
                    }
                }
            }
        });

        config
    }

    pub fn theme(&self) -> &ColorTheme {
        if self.is_dark {
            &self.dark
        } else {
            &self.light
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_parse_6digit() {
        let c = Color::try_from("#6750A4".to_string()).unwrap();
        assert!((c.r - 103.0 / 255.0).abs() < 1e-4);
        assert!((c.g - 80.0 / 255.0).abs() < 1e-4);
        assert!((c.b - 164.0 / 255.0).abs() < 1e-4);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn test_color_parse_8digit() {
        let c = Color::try_from("#6750A480".to_string()).unwrap();
        assert!((c.a - 128.0 / 255.0).abs() < 1e-4);
    }

    #[test]
    fn test_color_invalid() {
        assert!(Color::try_from("#ZZZ".to_string()).is_err());
    }

    #[test]
    fn test_color_default_is_transparent() {
        let c = Color::default();
        assert_eq!(c.a, 0.0);
    }
}
