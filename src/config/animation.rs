use std::collections::HashMap;
use std::path::Path;

use log::warn;
use serde::Deserialize;

use ui_core::animation::parser::{css_to_easing, Easing};

use crate::ui::UiEvent;

use super::watchable::{read_toml, WatchableConfig};

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAnimationConfig {
    easings: HashMap<String, String>,
}

#[derive(Default)]
pub struct AnimationConfig {
    raw: HashMap<String, String>,
    easings: HashMap<String, Easing>,
}

impl WatchableConfig for AnimationConfig {
    fn try_load(path: &Path) -> Option<Self> {
        let raw_cfg: RawAnimationConfig = read_toml(path)?;

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

    fn is_changed(&self, new: &Self) -> bool {
        self.raw != new.raw
    }

    fn reload_event(&self) -> UiEvent {
        UiEvent::AnimationUpdated(self.easings.clone())
    }
}

impl AnimationConfig {
    pub fn get_easing(&self, name: &str) -> Option<&Easing> {
        self.easings.get(name)
    }
}
