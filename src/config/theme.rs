use std::path::Path;

use log::warn;
use serde::Deserialize;

use ui_core::scheme::ColorTheme;

use super::watchable::{read_toml, WatchableConfig};

#[derive(Clone, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Theme {
    pub is_dark: bool,
    pub dark: ColorTheme,
    pub light: ColorTheme,
}

/// The colors used while `theme.toml` is missing or broken. `theme.toml` is generated per
/// machine (matugen, from `example/template.toml`) and not checked in, and the derived
/// default is all transparent, which shows nothing.
const FALLBACK: &str = include_str!("default_theme.toml");

impl WatchableConfig for Theme {
    fn try_load(path: &Path) -> Option<Self> {
        read_toml(path)
    }

    fn load(path: &Path) -> Self {
        Self::try_load(path).unwrap_or_else(|| {
            warn!("{}: using the built-in theme", path.display());
            toml::from_str(FALLBACK).expect("default_theme.toml parses")
        })
    }

    fn is_changed(&self, new: &Self) -> bool {
        self != new
    }
}

impl Theme {
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
    fn a_missing_file_falls_back_to_visible_colors() {
        let theme = Theme::load(Path::new("/nonexistent/theme.toml"));
        assert!(theme != Theme::default());
    }
}
