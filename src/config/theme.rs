use std::fs;
use std::io;

use log::info;
use serde::{Deserialize, Serialize};

use crate::ui::UiEvent;

use super::scheme::ColorTheme;
use super::watchable::WatchableConfig;

#[allow(unused)]
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Theme {
    pub is_dark: bool,
    pub dark: ColorTheme,
    pub light: ColorTheme,
}

impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        self.is_dark == other.is_dark && self.dark == other.dark && self.light == other.light
    }
}

impl WatchableConfig for Theme {
    fn try_load(path: &str) -> Option<Self> {
        let src = fs::read_to_string(path)
            .map_err(|e| info!("Theme read failed: {e}"))
            .ok()?;

        if src.trim().is_empty() {
            return None;
        }

        toml::from_str(&src)
            .map_err(|e| info!("Theme parse failed:\n{e}"))
            .ok()
    }

    fn write(&self, path: &str) -> io::Result<()> {
        let content = toml::to_string(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, content)
    }

    fn is_changed(&self, new: &Self) -> bool {
        self != new
    }

    fn reload_event(&self) -> UiEvent {
        UiEvent::RequestRedrawAll
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
