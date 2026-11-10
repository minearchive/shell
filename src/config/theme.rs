use std::path::Path;

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

impl WatchableConfig for Theme {
    fn try_load(path: &Path) -> Option<Self> {
        read_toml(path)
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
