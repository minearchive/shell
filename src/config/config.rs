use std::fs;
use std::io;

use log::info;
use serde::{Deserialize, Serialize};

use crate::ui::UiEvent;

use super::watchable::WatchableConfig;

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Configuration {
    pub name: String,
}

impl PartialEq for Configuration {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl WatchableConfig for Configuration {
    fn try_load(path: &str) -> Option<Self> {
        let src = fs::read_to_string(path)
            .map_err(|e| info!("Config read failed: {e}"))
            .ok()?;

        if src.trim().is_empty() {
            return None;
        }

        toml::from_str(&src)
            .map_err(|e| info!("Config parse failed:\n{e}"))
            .ok()
    }

    fn write(&self, path: &str) -> io::Result<()> {
        let content =
            toml::to_string(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, content)
    }

    fn is_changed(&self, new: &Self) -> bool {
        self != new
    }

    fn reload_event(&self) -> UiEvent {
        UiEvent::RequestRedrawAll
    }
}
