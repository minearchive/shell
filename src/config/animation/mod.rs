mod easings;

use std::collections::HashMap;
use std::fs;
use std::io;

use log::warn;

use crate::animation::parser::{css_to_easing, Easing};
use crate::ui::UiEvent;

use self::easings::RawAnimationConfig;
use super::watchable::WatchableConfig;

pub struct AnimationConfig {
    raw: HashMap<String, String>,
    easings: HashMap<String, Easing>,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            raw: HashMap::new(),
            easings: HashMap::new(),
        }
    }
}

impl WatchableConfig for AnimationConfig {
    fn try_load(path: &str) -> Option<Self> {
        let src = fs::read_to_string(path)
            .map_err(|e| warn!("AnimationConfig read failed: {e}"))
            .ok()?;

        if src.trim().is_empty() {
            return None;
        }

        let raw_cfg: RawAnimationConfig = toml::from_str(&src)
            .map_err(|e| warn!("AnimationConfig parse failed:\n{e}"))
            .ok()?;

        let mut easings = HashMap::new();
        for (name, css) in &raw_cfg.easings {
            match css_to_easing(css) {
                Ok(easing) => {
                    easings.insert(name.clone(), easing);
                }
                Err(e) => warn!("easing {name:?}: {e}"),
            }
        }

        Some(Self {
            raw: raw_cfg.easings,
            easings,
        })
    }

    fn write(&self, path: &str) -> io::Result<()> {
        let raw_cfg = RawAnimationConfig {
            easings: self.raw.clone(),
        };
        let content =
            toml::to_string(&raw_cfg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, content)
    }

    fn is_changed(&self, new: &Self) -> bool {
        self.raw != new.raw
    }

    fn reload_event(&self) -> UiEvent {
        UiEvent::AnimationUpdated(self.easings.clone())
    }
}

#[allow(unused)]
impl AnimationConfig {
    pub fn add_easing(&mut self, name: String, easing: Easing) {
        self.easings.insert(name, easing);
    }

    pub fn get_easing(&self, name: &str) -> Option<&Easing> {
        self.easings.get(name)
    }
}
